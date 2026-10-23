//! Renders the 3D look spike headlessly: one Shiba in a few poses, coats and
//! sizes, written as transparent PNGs. `cargo run --release -- <out dir>`.

use std::{fs::File, io::BufWriter, path::Path, time::Instant};

/// Final image size; the pet is rendered SS times larger and box-filtered down.
const SIZE: u32 = 320;
const SS: u32 = 3;

const RED: [[f32; 3]; 4] = [
    [0.93, 0.60, 0.33], // coat
    [1.00, 0.95, 0.86], // cream
    [0.20, 0.14, 0.13], // nose, eyes
    [1.00, 0.80, 0.78], // inner ear
];
const BLACK: [[f32; 3]; 4] = [
    [0.27, 0.22, 0.21],
    [1.00, 0.93, 0.83],
    [0.10, 0.07, 0.07],
    [0.96, 0.78, 0.74],
];

struct Shot {
    name: &'static str,
    coat: [[f32; 3]; 4],
    fat: f32,
    squash: f32,
    expr: f32, // 0 open, 1 laugh, 2 asleep, 3 fed up
    hat: bool,
    yaw: f32,
    legs: f32, // 0 sit, 1 paws up, 2 tucked
    pitch: f32,
    roll: f32,
    lift: f32,
    squish: f32,
    head: [f32; 3],
    shift: f32,
}

const SIT: Shot = Shot {
    name: "",
    coat: RED,
    fat: 1.0,
    squash: 1.0,
    expr: 0.0,
    hat: false,
    yaw: 0.35,
    legs: 0.0,
    pitch: 0.0,
    roll: 0.0,
    lift: 0.0,
    squish: 0.0,
    head: [0.0; 3],
    shift: 0.0,
};

fn shots() -> Vec<Shot> {
    vec![
        Shot { name: "sit-red", ..SIT },
        Shot { name: "sit-red-chonk", fat: 1.4, ..SIT },
        Shot { name: "sit-black", coat: BLACK, ..SIT },
        Shot { name: "party-hat", hat: true, yaw: -0.3, ..SIT },
        Shot {
            name: "belly-up-laugh",
            expr: 1.0,
            legs: 1.0,
            pitch: -0.35,
            roll: 1.40,
            lift: -0.20,
            shift: 0.35,
            yaw: 0.0,
            ..SIT
        },
        Shot {
            name: "asleep-loaf",
            expr: 2.0,
            legs: 2.0,
            squash: 0.82,
            head: [0.0, -0.30, 0.14],
            yaw: 0.5,
            ..SIT
        },
        Shot {
            name: "fed-up-melt",
            expr: 3.0,
            legs: 2.0,
            fat: 1.3,
            squash: 0.62,
            head: [0.0, -0.50, 0.18],
            yaw: 0.0,
            ..SIT
        },
    ]
}

/// Frame sequences: (folder, shots).
fn anims() -> Vec<(&'static str, Vec<Shot>)> {
    let turn = (0..36)
        .map(|i| {
            let a = i as f32 / 36.0 * std::f32::consts::TAU;
            Shot { name: "", yaw: 0.9 * a.sin(), ..SIT }
        })
        .collect();
    // Poke: squash in, then a damped spring wobble back; eyes shut on impact.
    let poke = (0..30)
        .map(|i| {
            let t = i as f32 / 30.0 * 1.5;
            let squish = if t < 0.12 { 0.28 * t / 0.12 } else { 0.28 * (-(t - 0.12) * 4.0).exp() * ((t - 0.12) * 16.0).cos() };
            Shot { name: "", squish, expr: if (0.04..0.4).contains(&t) { 1.0 } else { 0.0 }, ..SIT }
        })
        .collect();
    vec![("anim-turn", turn), ("anim-poke", poke)]
}

fn uniforms(s: &Shot, w: u32, h: u32) -> Vec<u8> {
    let c = |i: usize| [s.coat[i][0], s.coat[i][1], s.coat[i][2], 1.0];
    let vals: [[f32; 4]; 8] = [
        [w as f32, h as f32, s.yaw, s.legs],
        [s.fat, s.squash, s.expr, if s.hat { 1.0 } else { 0.0 }],
        [s.pitch, s.roll, s.lift, s.squish],
        [s.head[0], s.head[1], s.head[2], s.shift],
        c(0),
        c(1),
        c(2),
        c(3),
    ];
    vals.iter().flatten().flat_map(|f| f.to_le_bytes()).collect()
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "out".into());
    std::fs::create_dir_all(&out).expect("create out dir");

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("no GPU adapter (software ones count)");
    eprintln!("adapter: {:?}", adapter.get_info().name);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("pet"),
        source: wgpu::ShaderSource::Wgsl(include_str!("pet.wgsl").into()),
    });
    let format = wgpu::TextureFormat::Rgba8Unorm;
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
            targets: &[Some(format.into())],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    let (w, h) = (SIZE * SS, SIZE * SS);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 128,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }],
    });

    let mut jobs: Vec<(String, Shot)> = shots().into_iter().map(|s| (format!("{}.png", s.name), s)).collect();
    for (dir, frames) in anims() {
        std::fs::create_dir_all(Path::new(&out).join(dir)).expect("create anim dir");
        jobs.extend(frames.into_iter().enumerate().map(|(i, s)| (format!("{dir}/{i:03}.png"), s)));
    }
    for (file, shot) in jobs {
        let started = Instant::now();
        queue.write_buffer(&ubuf, 0, &uniforms(&shot, w, h));
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        enc.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None },
            },
            texture.size(),
        );
        queue.submit([enc.finish()]);
        readback.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map"));
        device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        let big = readback.slice(..).get_mapped_range().expect("mapped").to_vec();
        readback.unmap();

        let img = downsample(&big, row as usize, SIZE as usize);
        let path = Path::new(&out).join(file);
        write_png(&path, &img, SIZE);
        eprintln!("{} in {:?}", path.display(), started.elapsed());
    }
}

/// Box-filters premultiplied RGBA by SS, then un-premultiplies for PNG.
fn downsample(big: &[u8], row: usize, size: usize) -> Vec<u8> {
    let ss = SS as usize;
    let mut out = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let i = (y * ss + sy) * row + (x * ss + sx) * 4;
                    for c in 0..4 {
                        acc[c] += big[i + c] as f32;
                    }
                }
            }
            let n = (ss * ss) as f32;
            let a = acc[3] / n;
            let o = (y * size + x) * 4;
            for c in 0..3 {
                out[o + c] = if a > 0.0 { (acc[c] / n * 255.0 / a).round().min(255.0) as u8 } else { 0 };
            }
            out[o + 3] = a.round() as u8;
        }
    }
    out
}

fn write_png(path: &Path, rgba: &[u8], size: u32) {
    let file = BufWriter::new(File::create(path).expect("create png"));
    let mut enc = png::Encoder::new(file, size, size);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().expect("png header").write_image_data(rgba).expect("png data");
}
