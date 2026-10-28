//! Skinned-mesh spike: draws a baked mesh (mesh.rs) posed by a skeleton
//! (skin.rs) in one raster pass with 4× multisampling, its shadow first,
//! and reads the picture back into a pixmap as gpu.rs does.

use chonkgene_rig::Scene;
use chonkgene_rig::math::V3;
use tiny_skia::Pixmap;

use crate::gpu::{Frame, Gpu, Vec4, uniforms, v4};
use crate::mesh::{Mesh, Vertex};
use crate::raymarch::Camera;
use crate::skin::{Affine, ROOT_PIVOT};

const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;
/// Bones the shader holds (mesh.wgsl).
pub const MAX_BONES: usize = 16;

/// One pet's mesh on the GPU, and where it draws.
pub struct MeshCanvas {
    cam: Camera,
    pet: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    output: wgpu::Texture,
    output_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    row: u32,
    timer: Option<(wgpu::QuerySet, wgpu::Buffer)>,
    eye_r: V3,
    mouth_r: V3,
    /// GPU seconds measured so far, and frames drawn.
    pub measured: f64,
    pub frames: u32,
}

impl MeshCanvas {
    /// A `w` × `h` canvas at `scale` (1.0 = 100%) for `mesh`.
    pub fn new(gpu: &Gpu, mesh: &Mesh, w: u32, h: u32, scale: f32) -> Self {
        let device = gpu.device();
        let mut cam = Camera::new(scale);
        (cam.w, cam.h) = (w, h);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("mesh.wgsl"), include_str!("paint.wgsl")).into(),
            ),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
            4 => Float32x4, 5 => Uint32x4, 6 => Float32x4,
        ];
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        })];
        let pipeline = |vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        strip: bool,
                        blend: Option<wgpu::BlendState>,
                        depth_write: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: if strip {
                        wgpu::PrimitiveTopology::TriangleStrip
                    } else {
                        wgpu::PrimitiveTopology::TriangleList
                    },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(if depth_write {
                        wgpu::CompareFunction::Less
                    } else {
                        wgpu::CompareFunction::Always
                    }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let pet = pipeline("vs_pet", "fs_pet", &buffers, false, None, true);
        let shadow = pipeline(
            "vs_shadow",
            "fs_shadow",
            &[],
            true,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            false,
        );
        let buffer = |size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let vertices = buffer(
            (mesh.vertices.len() * size_of::<Vertex>()) as u64,
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        gpu.queue()
            .write_buffer(&vertices, 0, bytemuck::cast_slice(&mesh.vertices));
        let indices = buffer(
            (mesh.indices.len() * 4) as u64,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );
        gpu.queue()
            .write_buffer(&indices, 0, bytemuck::cast_slice(&mesh.indices));
        let uniforms = buffer(
            (UNIFORM_VECS * 16) as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let texture = |format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let target = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let msaa = texture(COLOR, SAMPLES, target).create_view(&Default::default());
        let depth = texture(DEPTH, SAMPLES, target).create_view(&Default::default());
        let output = texture(COLOR, 1, target | wgpu::TextureUsages::COPY_SRC);
        let output_view = output.create_view(&Default::default());
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let timer = gpu.tick_ns().map(|_| {
            let set = device.create_query_set(&wgpu::QuerySetDescriptor {
                label: None,
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            });
            let resolve = buffer(
                16,
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            );
            (set, resolve)
        });
        let readback = buffer(
            u64::from(row * h) + if timer.is_some() { 16 } else { 0 },
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        );
        Self {
            cam,
            pet,
            shadow,
            vertices,
            indices,
            count: mesh.indices.len() as u32,
            uniforms,
            bind,
            msaa,
            depth,
            output,
            output_view,
            readback,
            row,
            timer,
            eye_r: mesh.eye_r,
            mouth_r: mesh.mouth_r,
            measured: 0.0,
            frames: 0,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.cam.w, self.cam.h)
    }

    /// Draw the mesh posed by `bones` (one per skeleton bone) into `out`
    /// and wait for it. `scene` gives the face (look, palette); `yaw`, in
    /// degrees, turns the shadow under the root's pivot, radii `shadow_r`
    /// (across, along the body). The GPU seconds it took, if measured.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        bones: &[Affine],
        yaw: f32,
        shadow_r: (f32, f32),
        out: &mut Pixmap,
    ) -> Result<Option<f32>, String> {
        if bones.len() > MAX_BONES {
            return Err(format!(
                "{} bones, the shader holds {MAX_BONES}",
                bones.len()
            ));
        }
        if out.width() != self.cam.w || out.height() != self.cam.h {
            return Err("canvas and pixmap sizes differ".into());
        }
        let mut u = uniforms(scene, &self.cam, &Frame::default(), true, 0);
        u.push(v4(self.eye_r, yaw.to_radians()));
        u.push(v4(self.mouth_r, 0.0));
        u.push([ROOT_PIVOT.x, ROOT_PIVOT.z, shadow_r.0, shadow_r.1]);
        for i in 0..MAX_BONES {
            let rows = bones.get(i).unwrap_or(&Affine::IDENTITY).rows();
            u.extend(rows);
        }
        debug_assert_eq!(u.len(), UNIFORM_VECS);
        let queue = gpu.queue();
        queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice::<Vec4, u8>(&u));
        let mut enc = gpu.device().create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa,
                    depth_slice: None,
                    resolve_target: Some(&self.output_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.timer.as_ref().map(|(set, _)| {
                    wgpu::RenderPassTimestampWrites {
                        query_set: set,
                        beginning_of_pass_write_index: Some(0),
                        end_of_pass_write_index: Some(1),
                    }
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_pipeline(&self.shadow);
            pass.draw(0..4, 0..1);
            pass.set_pipeline(&self.pet);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.count, 0, 0..1);
        }
        enc.copy_texture_to_buffer(
            self.output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.row),
                    rows_per_image: None,
                },
            },
            self.output.size(),
        );
        if let Some((set, resolve)) = &self.timer {
            enc.resolve_query_set(set, 0..2, resolve, 0);
            enc.copy_buffer_to_buffer(
                resolve,
                0,
                &self.readback,
                u64::from(self.row * self.cam.h),
                16,
            );
        }
        queue.submit([enc.finish()]);
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |_| {});
        gpu.device()
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("GPU: {e}"))?;
        let mut secs = None;
        {
            let data = self
                .readback
                .slice(..)
                .get_mapped_range()
                .map_err(|e| format!("GPU readback: {e}"))?;
            let (w, h, row) = (
                self.cam.w as usize * 4,
                self.cam.h as usize,
                self.row as usize,
            );
            for (y, dst) in out.data_mut().chunks_exact_mut(w).take(h).enumerate() {
                dst.copy_from_slice(&data[y * row..y * row + w]);
            }
            if let Some(ns) = gpu.tick_ns() {
                let at = row * h;
                let stamp = |i: usize| {
                    let b: [u8; 8] = data[at + i * 8..at + i * 8 + 8]
                        .try_into()
                        .unwrap_or([0; 8]);
                    u64::from_le_bytes(b)
                };
                let t = (stamp(1).saturating_sub(stamp(0)) as f64 * f64::from(ns) * 1e-9) as f32;
                if t > 0.0 && t < 1.0 {
                    self.measured += f64::from(t);
                    secs = Some(t);
                }
            }
        }
        self.readback.unmap();
        self.frames += 1;
        Ok(secs)
    }
}

/// pet.wgsl's 13 + palette, then eye and mouth radii, the shadow, and three
/// rows a bone.
const UNIFORM_VECS: usize = 13 + 16 + 3 + 3 * MAX_BONES;
