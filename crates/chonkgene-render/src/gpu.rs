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

/// The device and the pipeline, shared by every pet.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    name: String,
}

/// One pet's render target and buffers.
pub struct Canvas {
    scale: f32,
    w: u32,
    h: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    row: u32,
    uniforms: wgpu::Buffer,
    parts: wgpu::Buffer,
    marks: wgpu::Buffer,
    bind: wgpu::BindGroup,
    part_cap: usize,
    mark_cap: usize,
}

type Vec4 = [f32; 4];

fn v4(v: V3, w: f32) -> Vec4 {
    [v.x, v.y, v.z, w]
}

impl Gpu {
    /// Find a GPU (a software one counts) and build the pipeline.
    pub fn new() -> Result<Self, String> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|e| format!("no GPU: {e}"))?;
        let name = format!(
            "{} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|e| format!("GPU device: {e}"))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pet"),
            source: wgpu::ShaderSource::Wgsl(include_str!("pet.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pet"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            name,
        })
    }

    /// The adapter in use, for logs.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// A canvas for a pet drawn at `scale` (1.0 = 100%).
    pub fn canvas(&self, scale: f32) -> Canvas {
        let cam = Camera::new(scale);
        let (w, h) = (cam.w, cam.h);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pet"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.buffer(
            u64::from(row * h),
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
        let bind = self.bind(&uniforms, &parts, &marks);
        Canvas {
            scale,
            w,
            h,
            texture,
            view,
            readback,
            row,
            uniforms,
            parts,
            marks,
            bind,
            part_cap: PARTS,
            mark_cap: MARKS,
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

    fn bind(
        &self,
        uniforms: &wgpu::Buffer,
        parts: &wgpu::Buffer,
        marks: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
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
            ],
        })
    }

    /// Draw `scene` into `out` (the canvas's size, premultiplied RGBA).
    /// `shadow`: draw the contact shadow (not for a part drawn on its own).
    pub fn render(
        &self,
        canvas: &mut Canvas,
        scene: &Scene,
        frame: &Frame,
        shadow: bool,
        out: &mut Pixmap,
    ) -> Result<(), String> {
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
            canvas.bind = self.bind(&canvas.uniforms, &canvas.parts, &canvas.marks);
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
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &canvas.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &canvas.bind, &[]);
            pass.draw(0..3, 0..1);
        }
        enc.copy_texture_to_buffer(
            canvas.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &canvas.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(canvas.row),
                    rows_per_image: None,
                },
            },
            canvas.texture.size(),
        );
        self.queue.submit([enc.finish()]);
        let slice = canvas.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("GPU: {e}"))?;
        {
            let data = slice
                .get_mapped_range()
                .map_err(|e| format!("GPU readback: {e}"))?;
            let (w, h, row) = (
                canvas.w as usize * 4,
                canvas.h as usize,
                canvas.row as usize,
            );
            if out.width() != canvas.w || out.height() != canvas.h {
                return Err("canvas and pixmap sizes differ".into());
            }
            for (y, dst) in out.data_mut().chunks_exact_mut(w).take(h).enumerate() {
                dst.copy_from_slice(&data[y * row..y * row + w]);
            }
        }
        canvas.readback.unmap();
        Ok(())
    }
}

impl Canvas {
    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
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

fn uniforms(scene: &Scene, cam: &Camera, frame: &Frame, shadow: bool, n: usize) -> Vec<Vec4> {
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
        v4(scene.center, scene.radius),
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
            0.0,
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
