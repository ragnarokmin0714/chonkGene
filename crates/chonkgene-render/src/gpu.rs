//! Ray-marches a pet on the GPU, every frame (PLAN §3.1).
//!
//! The CPU path (`sprites`) can only afford a few poses per pet, rendered
//! once; everything in between is 2D warping, which is why turning, lying
//! down and the shadow looked flat. Here each frame is the real 3D pet in
//! whatever pose the animation has blended to, squashed, leaned and lifted
//! in 3D, with the contact shadow left on the floor. `pet.wgsl` is a port of
//! the CPU ray-marcher, so the two look alike.
//!
//! Frames are read back into a [`Pixmap`]: the pet windows stay the same
//! layered/ARGB windows, and no GPU surface ever has to be transparent.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chonkgene_rig::math::V3;
use chonkgene_rig::sdf::Shape;
use chonkgene_rig::{Look, Scene};
use tiny_skia::Pixmap;

use crate::raymarch::Camera;

/// Parts and marks a canvas holds before its buffers grow.
const PARTS: usize = 32;
const MARKS: usize = 32;
const PALETTE: usize = 16;

/// What a frame does to the posed pet as a whole, about the point under it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    /// > 0 flattens and widens, < 0 stretches.
    pub squash: f32,
    /// −1.0 … 1.0 over a breath.
    pub breath: f32,
    /// Lean about the feet, degrees (positive leans to the viewer's left).
    pub lean: f32,
    /// Height above the floor, pixels at 100% (the shadow stays put).
    pub lift: f32,
}

impl Frame {
    /// World → posed-pet space, as `to_pet` in pet.wgsl: undo the lift,
    /// the lean about the line of sight through `anchor`, then the squash.
    pub(crate) fn pet_space(self, p: V3, anchor: V3, fwd: V3) -> V3 {
        let mut q = p
            - anchor
            - V3 {
                x: 0.0,
                y: self.lift / crate::raymarch::PPU,
                z: 0.0,
            };
        let (s, c) = (-self.lean.to_radians()).sin_cos();
        let cross = V3 {
            x: fwd.y * q.z - fwd.z * q.y,
            y: fwd.z * q.x - fwd.x * q.z,
            z: fwd.x * q.y - fwd.y * q.x,
        };
        q = q * c + cross * s + fwd * (fwd.dot(q) * (1.0 - c));
        let sx = 1.0 + self.squash;
        let sy = (1.0 - self.squash) * (1.0 + 0.015 * self.breath);
        V3 {
            x: q.x / sx,
            y: q.y / sy,
            z: q.z / sx,
        } + anchor
    }

    /// Posed-pet space → world: where the frame puts a point of the pet.
    fn world_space(self, p: V3, anchor: V3, fwd: V3) -> V3 {
        let sx = 1.0 + self.squash;
        let sy = (1.0 - self.squash) * (1.0 + 0.015 * self.breath);
        let q = p - anchor;
        let q = V3 {
            x: q.x * sx,
            y: q.y * sy,
            z: q.z * sx,
        };
        let (s, c) = self.lean.to_radians().sin_cos();
        let cross = V3 {
            x: fwd.y * q.z - fwd.z * q.y,
            y: fwd.z * q.x - fwd.x * q.z,
            z: fwd.x * q.y - fwd.y * q.x,
        };
        let q = q * c + cross * s + fwd * (fwd.dot(q) * (1.0 - c));
        q + anchor
            + V3 {
                x: 0.0,
                y: self.lift / crate::raymarch::PPU,
                z: 0.0,
            }
    }

    /// The sphere around the pet once the frame has moved it: the pose's
    /// sphere carried along and grown by the most the squash stretches.
    pub(crate) fn bound(self, center: V3, radius: f32, anchor: V3, fwd: V3) -> (V3, f32) {
        let sx = (1.0 + self.squash).abs();
        let sy = ((1.0 - self.squash) * (1.0 + 0.015 * self.breath)).abs();
        (
            self.world_space(center, anchor, fwd),
            radius * sx.max(sy) * 1.01,
        )
    }

    /// How much `pet_space` stretches distances at most.
    pub(crate) fn stretch(self) -> f32 {
        (1.0 + self.squash).min((1.0 - self.squash) * (1.0 + 0.015 * self.breath))
    }
}

/// The share of GPU time pets may use unless `CHONKGENE_GPU_BUDGET` (a
/// percentage) says otherwise.
const DEFAULT_BUDGET: f32 = 0.10;
/// GPU seconds the budget may save up for a burst (a few frames at once).
const BURST: f32 = 0.05;
/// A frame's GPU time before it has been measured, at 100% (Roger's UHD
/// 770 took ~4 ms of drawing; err high), scaled by area.
const GUESS: f32 = 0.005;
const GUESS_PIXELS: f32 = 227.0 * 201.0;

/// Keeps the pets' share of GPU time at or under `share` (PLAN §5): GPU
/// time builds up at `share` seconds a second, and every frame sent spends
/// what it is expected to cost; a frame it cannot pay for waits.
struct Budget {
    share: f32,
    saved: f32,
    at: Instant,
}

impl Budget {
    fn try_spend(&mut self, cost: f32) -> bool {
        let now = Instant::now();
        let secs = (now - self.at).as_secs_f32();
        self.at = now;
        self.saved = (self.saved + secs * self.share).min(BURST);
        if self.share < 1.0 && self.saved < cost {
            return false;
        }
        self.saved -= cost;
        true
    }
}

/// The device and the two pipelines, shared by every pet.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Pass 1: one ray through every pixel centre, with where it hit.
    centre: wgpu::RenderPipeline,
    /// Pass 2: more rays only where the silhouette or the depth jumps.
    edges: wgpu::RenderPipeline,
    name: String,
    software: bool,
    /// Nanoseconds a GPU timestamp tick, if the GPU can time its passes.
    tick_ns: Option<f32>,
    budget: Mutex<Budget>,
}

/// One pet's render targets and buffers.
pub struct Canvas {
    scale: f32,
    w: u32,
    h: u32,
    centre_color: wgpu::TextureView,
    centre_depth: wgpu::TextureView,
    output: wgpu::Texture,
    output_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    row: u32,
    uniforms: wgpu::Buffer,
    parts: wgpu::Buffer,
    marks: wgpu::Buffer,
    bind_centre: wgpu::BindGroup,
    bind_edges: wgpu::BindGroup,
    part_cap: usize,
    mark_cap: usize,
    /// Set while a frame is on its way; turns true when it can be read.
    pending: Option<Arc<AtomicBool>>,
    /// Timing the two passes, when the GPU can: timestamps land in the
    /// readback buffer after the picture.
    timer: Option<(wgpu::QuerySet, wgpu::Buffer)>,
    /// Expected GPU seconds a frame, what the frame on its way was charged,
    /// and all GPU seconds measured so far.
    cost: f32,
    charged: f32,
    measured: f64,
}

pub(crate) type Vec4 = [f32; 4];

pub(crate) fn v4(v: V3, w: f32) -> Vec4 {
    [v.x, v.y, v.z, w]
}

const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;

impl Gpu {
    /// Find a GPU (a software one counts) and build the pipelines.
    pub fn new() -> Result<Self, String> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|e| format!("no GPU: {e}"))?;
        let info = adapter.get_info();
        let name = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
        let software = info.device_type == wgpu::DeviceType::Cpu;
        let features = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: features,
            ..Default::default()
        }))
        .map_err(|e| format!("GPU device: {e}"))?;
        let tick_ns = (!features.is_empty()).then(|| queue.get_timestamp_period());
        let share = std::env::var("CHONKGENE_GPU_BUDGET")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map_or(DEFAULT_BUDGET, |pct| (pct / 100.0).clamp(0.01, 1.0));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pet"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("pet.wgsl"), include_str!("paint.wgsl")).into(),
            ),
        });
        let pipeline = |entry: &str, targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets,
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let centre = pipeline("fs_centre", &[Some(COLOR.into()), Some(DEPTH.into())]);
        let edges = pipeline("fs_edges", &[Some(COLOR.into())]);
        Ok(Self {
            device,
            queue,
            centre,
            edges,
            name,
            software,
            tick_ns,
            budget: Mutex::new(Budget {
                share,
                saved: BURST,
                at: Instant::now(),
            }),
        })
    }

    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Nanoseconds a GPU timestamp tick, if the GPU can time its passes.
    pub(crate) fn tick_ns(&self) -> Option<f32> {
        self.tick_ns
    }

    /// The adapter in use, for logs.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Is this the CPU pretending to be a GPU (WARP, llvmpipe)? Real-time
    /// frames would cost more than the pre-rendered sprites then.
    pub fn is_software(&self) -> bool {
        self.software
    }

    /// Can the GPU time its frames (else costs are guesses)?
    pub fn times_frames(&self) -> bool {
        self.tick_ns.is_some()
    }

    /// The share of GPU time pets may use (1.0: no limit).
    pub fn set_budget(&self, share: f32) {
        if let Ok(mut b) = self.budget.lock() {
            b.share = share.clamp(0.01, 1.0);
        }
    }

    pub fn budget(&self) -> f32 {
        self.budget.lock().map_or(DEFAULT_BUDGET, |b| b.share)
    }

    fn texture(
        &self,
        w: u32,
        h: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    /// A canvas for a pet drawn at `scale` (1.0 = 100%).
    pub fn canvas(&self, scale: f32) -> Canvas {
        let cam = Camera::new(scale);
        let (w, h) = (cam.w, cam.h);
        let between = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let view = |t: wgpu::Texture| t.create_view(&Default::default());
        let centre_color = view(self.texture(w, h, COLOR, between));
        let centre_depth = view(self.texture(w, h, DEPTH, between));
        let output = self.texture(
            w,
            h,
            COLOR,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let output_view = output.create_view(&Default::default());
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let timer = self.tick_ns.map(|_| {
            let set = self.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: None,
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            });
            let resolve = self.buffer(
                16,
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            );
            (set, resolve)
        });
        let readback = self.buffer(
            u64::from(row * h) + if timer.is_some() { 16 } else { 0 },
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        );
        let uniforms = self.buffer(
            (UNIFORM_VECS * 16) as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let (parts, marks) = (
            self.storage(PARTS * PART_VECS),
            self.storage(MARKS * MARK_VECS),
        );
        let (bind_centre, bind_edges) =
            self.binds(&uniforms, &parts, &marks, &centre_color, &centre_depth);
        Canvas {
            scale,
            w,
            h,
            centre_color,
            centre_depth,
            output,
            output_view,
            readback,
            row,
            uniforms,
            parts,
            marks,
            bind_centre,
            bind_edges,
            part_cap: PARTS,
            mark_cap: MARKS,
            pending: None,
            timer,
            cost: GUESS * (w * h) as f32 / GUESS_PIXELS,
            charged: 0.0,
            measured: 0.0,
        }
    }

    fn buffer(&self, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage,
            mapped_at_creation: false,
        })
    }

    fn storage(&self, vecs: usize) -> wgpu::Buffer {
        self.buffer(
            (vecs * 16) as u64,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        )
    }

    /// Bind groups for the two passes; the second also reads the first's
    /// results.
    fn binds(
        &self,
        uniforms: &wgpu::Buffer,
        parts: &wgpu::Buffer,
        marks: &wgpu::Buffer,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) -> (wgpu::BindGroup, wgpu::BindGroup) {
        let buffers = [
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: parts.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: marks.as_entire_binding(),
            },
        ];
        let centre = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.centre.get_bind_group_layout(0),
            entries: &buffers,
        });
        let mut all = buffers.to_vec();
        all.push(wgpu::BindGroupEntry {
            binding: 3,
            resource: wgpu::BindingResource::TextureView(color),
        });
        all.push(wgpu::BindGroupEntry {
            binding: 4,
            resource: wgpu::BindingResource::TextureView(depth),
        });
        let edges = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.edges.get_bind_group_layout(0),
            entries: &all,
        });
        (centre, edges)
    }

    /// Draw `scene` into `out` (the canvas's size, premultiplied RGBA),
    /// waiting for it. `shadow`: draw the contact shadow.
    pub fn render(
        &self,
        canvas: &mut Canvas,
        scene: &Scene,
        frame: &Frame,
        shadow: bool,
        out: &mut Pixmap,
    ) -> Result<(), String> {
        self.submit(canvas, scene, frame, shadow)?;
        self.collect(canvas, out, true)?;
        Ok(())
    }

    /// [`Gpu::submit`] if the budget can pay for the frame now; false (and
    /// nothing sent) if it has to wait.
    pub fn try_submit(
        &self,
        canvas: &mut Canvas,
        scene: &Scene,
        frame: &Frame,
        shadow: bool,
    ) -> Result<bool, String> {
        let paid = self
            .budget
            .lock()
            .map_or(true, |mut b| b.try_spend(canvas.cost));
        if !paid {
            return Ok(false);
        }
        canvas.charged = canvas.cost;
        self.submit(canvas, scene, frame, shadow)?;
        Ok(true)
    }

    /// Start drawing `scene` on `canvas` without waiting; [`Gpu::collect`]
    /// picks the picture up. A canvas draws one frame at a time.
    pub fn submit(
        &self,
        canvas: &mut Canvas,
        scene: &Scene,
        frame: &Frame,
        shadow: bool,
    ) -> Result<(), String> {
        if canvas.pending.is_some() {
            return Err("a frame is already on its way".into());
        }
        let (parts, marks) = pack(scene);
        if parts.len() / PART_VECS > canvas.part_cap || marks.len() / MARK_VECS > canvas.mark_cap {
            canvas.part_cap = canvas
                .part_cap
                .max(parts.len() / PART_VECS)
                .next_power_of_two();
            canvas.mark_cap = canvas
                .mark_cap
                .max(marks.len() / MARK_VECS)
                .next_power_of_two();
            canvas.parts = self.storage(canvas.part_cap * PART_VECS);
            canvas.marks = self.storage(canvas.mark_cap * MARK_VECS);
            (canvas.bind_centre, canvas.bind_edges) = self.binds(
                &canvas.uniforms,
                &canvas.parts,
                &canvas.marks,
                &canvas.centre_color,
                &canvas.centre_depth,
            );
        }
        let cam = Camera::new(canvas.scale);
        let uniforms = uniforms(scene, &cam, frame, shadow, parts.len() / PART_VECS);
        self.queue
            .write_buffer(&canvas.uniforms, 0, bytemuck::cast_slice(&uniforms));
        self.queue
            .write_buffer(&canvas.parts, 0, bytemuck::cast_slice(&parts));
        if !marks.is_empty() {
            self.queue
                .write_buffer(&canvas.marks, 0, bytemuck::cast_slice(&marks));
        }
        let attach = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let mut enc = self.device.create_command_encoder(&Default::default());
        // The first pass stamps its start, the second its end.
        let stamp = |begin: Option<u32>, end: Option<u32>| {
            canvas
                .timer
                .as_ref()
                .map(|(set, _)| wgpu::RenderPassTimestampWrites {
                    query_set: set,
                    beginning_of_pass_write_index: begin,
                    end_of_pass_write_index: end,
                })
        };
        for (pipeline, bind, targets, timestamp_writes) in [
            (
                &self.centre,
                &canvas.bind_centre,
                vec![attach(&canvas.centre_color), attach(&canvas.centre_depth)],
                stamp(Some(0), None),
            ),
            (
                &self.edges,
                &canvas.bind_edges,
                vec![attach(&canvas.output_view)],
                stamp(None, Some(1)),
            ),
        ] {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &targets,
                timestamp_writes,
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
        enc.copy_texture_to_buffer(
            canvas.output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &canvas.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(canvas.row),
                    rows_per_image: None,
                },
            },
            canvas.output.size(),
        );
        if let Some((set, resolve)) = &canvas.timer {
            enc.resolve_query_set(set, 0..2, resolve, 0);
            enc.copy_buffer_to_buffer(
                resolve,
                0,
                &canvas.readback,
                u64::from(canvas.row * canvas.h),
                16,
            );
        }
        self.queue.submit([enc.finish()]);
        let arrived = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&arrived);
        canvas
            .readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                flag.store(r.is_ok(), Ordering::Release);
            });
        canvas.pending = Some(arrived);
        Ok(())
    }

    /// Is a frame on its way to `canvas`?
    pub fn pending(&self, canvas: &Canvas) -> bool {
        canvas.pending.is_some()
    }

    /// Copy the frame on its way into `out` if it has arrived (`wait`: wait
    /// for it). True if `out` now holds it.
    pub fn collect(
        &self,
        canvas: &mut Canvas,
        out: &mut Pixmap,
        wait: bool,
    ) -> Result<bool, String> {
        let Some(arrived) = canvas.pending.clone() else {
            return Ok(false);
        };
        if out.width() != canvas.w || out.height() != canvas.h {
            return Err("canvas and pixmap sizes differ".into());
        }
        let poll = if wait {
            wgpu::PollType::wait_indefinitely()
        } else {
            wgpu::PollType::Poll
        };
        self.device.poll(poll).map_err(|e| format!("GPU: {e}"))?;
        if !arrived.load(Ordering::Acquire) {
            if wait {
                // Waited and it still failed to map: drop the frame.
                canvas.pending = None;
                return Err("GPU readback failed".into());
            }
            return Ok(false);
        }
        canvas.pending = None;
        {
            let slice = canvas.readback.slice(..);
            let data = slice
                .get_mapped_range()
                .map_err(|e| format!("GPU readback: {e}"))?;
            let (w, h, row) = (
                canvas.w as usize * 4,
                canvas.h as usize,
                canvas.row as usize,
            );
            for (y, dst) in out.data_mut().chunks_exact_mut(w).take(h).enumerate() {
                dst.copy_from_slice(&data[y * row..y * row + w]);
            }
            if let Some(ns) = self.tick_ns {
                // What the frame really cost: settle the budget and learn.
                let at = row * h;
                let stamp = |i: usize| {
                    let b: [u8; 8] = data[at + i * 8..at + i * 8 + 8]
                        .try_into()
                        .unwrap_or([0; 8]);
                    u64::from_le_bytes(b)
                };
                let ticks = stamp(1).saturating_sub(stamp(0));
                let secs = (ticks as f64 * f64::from(ns) * 1e-9) as f32;
                if secs > 0.0 && secs < 1.0 {
                    canvas.measured += f64::from(secs);
                    canvas.cost = 0.8 * canvas.cost + 0.2 * secs;
                    if let Ok(mut b) = self.budget.lock() {
                        b.saved += canvas.charged - secs;
                    }
                }
            }
        }
        canvas.readback.unmap();
        Ok(true)
    }
}

impl Canvas {
    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// GPU seconds measured on this canvas so far (0 if the GPU cannot
    /// time its frames).
    pub fn gpu_seconds(&self) -> f64 {
        self.measured
    }
}

const PART_VECS: usize = 7;
const MARK_VECS: usize = 3;
const UNIFORM_VECS: usize = 13 + PALETTE;

fn look_index(look: Look) -> f32 {
    match look {
        Look::Open => 0.0,
        Look::Half => 1.0,
        Look::Shut => 2.0,
        Look::Happy => 3.0,
        Look::Laugh => 4.0,
        Look::FedUp => 5.0,
        Look::Pout => 6.0,
        Look::Asleep => 7.0,
        Look::Yawn => 8.0,
    }
}

/// Parts and marks as `pet.wgsl` reads them.
fn pack(scene: &Scene) -> (Vec<Vec4>, Vec<Vec4>) {
    let (mut parts, mut marks) = (Vec::new(), Vec::new());
    for pv in scene.parts() {
        let (kind, shape) = match pv.shape {
            Shape::Ellipsoid(r) => (0.0, v4(r, 0.0)),
            Shape::Capsule { to, r } => (1.0, v4(to, r)),
            Shape::Torus { major, minor } => (2.0, [major, minor, 0.0, 0.0]),
            Shape::Cone { r1, r2, h, flat } => (3.0, [r1, r2, h, flat]),
        };
        let part = pv.part;
        let first = marks.len() / MARK_VECS;
        for m in &part.marks {
            marks.push(v4(m.at, m.soft));
            marks.push(v4(m.radii, m.slot as f32));
            marks.push([if m.mirror { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0]);
        }
        parts.extend([
            v4(pv.a.r[0], pv.k),
            v4(pv.a.r[1], pv.blend),
            v4(pv.a.r[2], pv.radius),
            v4(pv.b, kind),
            v4(pv.center, part.gloss),
            shape,
            [
                part.fuzz,
                part.slot as f32,
                first as f32,
                part.marks.len() as f32,
            ],
        ]);
    }
    (parts, marks)
}

pub(crate) fn uniforms(
    scene: &Scene,
    cam: &Camera,
    frame: &Frame,
    shadow: bool,
    n: usize,
) -> Vec<Vec4> {
    let face = &scene.model().face;
    let (eyes, mouth) = scene.face_parts();
    let index = |i: Option<usize>| i.map_or(-1.0, |i| i as f32);
    let slot = |s: Option<usize>| s.map_or(-1.0, |s| s as f32);
    let brows = face
        .brows
        .map_or([0.0; 4], |b| [b.at.0, b.at.1, b.r.0, b.r.1]);
    let mut u = vec![
        [cam.w as f32, cam.h as f32, cam.ppu, cam.floor_px],
        v4(cam.right, 0.0),
        v4(cam.up, 0.0),
        v4(cam.fwd, 0.0),
        v4(cam.anchor, n as f32),
        {
            let (c, r) = frame.bound(scene.center, scene.radius, cam.anchor, cam.fwd);
            v4(c, r)
        },
        [
            index(eyes),
            index(mouth),
            look_index(scene.look()),
            face.ink as f32,
        ],
        [face.eye_at.0, face.eye_at.1, face.eye_r.0, face.eye_r.1],
        [
            slot(face.iris.map(|(s, _)| s)),
            face.iris.map_or(1.0, |(_, k)| k),
            if face.whiskers { 1.0 } else { 0.0 },
            if shadow { 1.0 } else { 0.0 },
        ],
        brows,
        [
            slot(face.brows.map(|b| b.slot)),
            face.blush.slot as f32,
            slot(face.liner),
            0.0,
        ],
        [
            face.blush.at.0,
            face.blush.at.1,
            face.blush.r.0,
            face.blush.r.1,
        ],
        [
            frame.squash,
            frame.breath,
            frame.lean.to_radians(),
            frame.lift / crate::raymarch::PPU,
        ],
    ];
    let palette = scene.palette();
    for i in 0..PALETTE {
        u.push(palette.get(i).map_or([0.0; 4], |c| v4(*c, 1.0)));
    }
    u
}
