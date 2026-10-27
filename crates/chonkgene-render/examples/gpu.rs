//! The GPU renderer against the CPU one, and what it costs:
//! `cargo run --release -p chonkgene-render --example gpu -- <out dir>`
//! prints the adapter and the time per frame, writes the GPU and CPU
//! renders of every species sitting side by side, and frames of a real 3D
//! turn, a hop (the shadow stays on the floor) and lying down.

use std::time::Instant;

use chonkgene_render::KEYS;
use chonkgene_render::gpu::{Frame, Gpu};
use chonkgene_render::sprites::{Variant, render_layer};
use chonkgene_rig::{Layer, Look, PoseId, Scene, builtin};
use tiny_skia::{IntSize, Pixmap};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "gpu-out".into());
    std::fs::create_dir_all(&out).expect("out dir");
    let gpu = match Gpu::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("adapter: {}", gpu.name());
    let mut canvas = gpu.canvas(1.0);
    let (w, h) = canvas.size();
    let mut pm = Pixmap::new(w, h).expect("pixmap");
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());

    // GPU and CPU, side by side, and how far apart they are.
    for id in ["shiba", "munchkin", "fold", "owl", "chick", "duck"] {
        let model = builtin::load(id).expect("model");
        let key = KEYS[0];
        let scene = Scene::new(&model, 0, 0.33, key.pose, key.look, Layer::All);
        gpu.render(&mut canvas, &scene, &Frame::default(), true, &mut pm)
            .expect("render");
        let s = render_layer(&model, variant(), key, Layer::All, threads);
        let cpu = Pixmap::from_vec(s.rgba, IntSize::from_wh(s.width, s.height).unwrap()).unwrap();
        let (mean, off) = compare(&pm, &cpu);
        println!("{id}: mean channel difference {mean:.2}, {off:.2}% of pixels off by more than 8");
        side_by_side(&pm, &cpu)
            .save_png(format!("{out}/{id}-gpu-cpu.png"))
            .expect("png");
    }

    let model = builtin::load("shiba").expect("model");
    let sit = model.pose(PoseId::Sit).clone();
    let mirrored = sit.mirrored();
    let loaf = model.pose(PoseId::Loaf).clone();
    // A real turn: the pose blends into its mirror image, through facing you.
    let mut times = Vec::new();
    for i in 0..=12 {
        let t = i as f32 / 12.0;
        let pose = sit.blend(&mirrored, t);
        let scene = Scene::posed(&model, 0, 0.33, &pose, Look::Open, Layer::All);
        let started = Instant::now();
        gpu.render(&mut canvas, &scene, &Frame::default(), true, &mut pm)
            .expect("render");
        times.push(started.elapsed());
        pm.save_png(format!("{out}/turn-{i:02}.png")).expect("png");
    }
    // A hop: the pet rises, its shadow stays on the floor.
    for (i, lift) in [0.0, 10.0, 20.0, 28.0].into_iter().enumerate() {
        let scene = Scene::new(&model, 0, 0.33, PoseId::Sit, Look::Happy, Layer::All);
        let frame = Frame {
            lift,
            squash: if lift == 0.0 { 0.06 } else { -0.04 },
            ..Frame::default()
        };
        gpu.render(&mut canvas, &scene, &frame, true, &mut pm)
            .expect("render");
        pm.save_png(format!("{out}/hop-{i}.png")).expect("png");
    }
    // Lying down: sitting blends into the loaf.
    for i in 0..=6 {
        let pose = sit.blend(&loaf, i as f32 / 6.0);
        let scene = Scene::posed(&model, 0, 0.33, &pose, Look::Shut, Layer::All);
        gpu.render(&mut canvas, &scene, &Frame::default(), true, &mut pm)
            .expect("render");
        pm.save_png(format!("{out}/lie-{i}.png")).expect("png");
    }
    // A lean: about the feet, in 3D.
    let scene = Scene::new(&model, 0, 0.33, PoseId::Sit, Look::Open, Layer::All);
    for (name, lean) in [("lean-left", 8.0), ("lean-right", -8.0)] {
        let frame = Frame {
            lean,
            ..Frame::default()
        };
        gpu.render(&mut canvas, &scene, &frame, true, &mut pm)
            .expect("render");
        pm.save_png(format!("{out}/{name}.png")).expect("png");
    }

    times.sort();
    println!(
        "frame (render + readback) at {w}×{h}: median {:?}, fastest {:?}",
        times[times.len() / 2],
        times[0]
    );
}

fn variant() -> Variant {
    Variant {
        coat: 0,
        chonk: 0.33,
        scale: 1.0,
    }
}

/// Mean absolute channel difference, and the share of pixels off by more
/// than 8 in any channel.
fn compare(a: &Pixmap, b: &Pixmap) -> (f64, f64) {
    let (mut sum, mut off) = (0u64, 0usize);
    for (p, q) in a
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.data().as_chunks::<4>().0)
    {
        let d: Vec<u8> = p.iter().zip(q).map(|(x, y)| x.abs_diff(*y)).collect();
        sum += d.iter().map(|&x| u64::from(x)).sum::<u64>();
        if d.iter().any(|&x| x > 8) {
            off += 1;
        }
    }
    let px = a.data().len() / 4;
    (sum as f64 / (px * 4) as f64, off as f64 * 100.0 / px as f64)
}

fn side_by_side(a: &Pixmap, b: &Pixmap) -> Pixmap {
    let mut out = Pixmap::new(a.width() * 2, a.height()).unwrap();
    let paint = tiny_skia::PixmapPaint::default();
    out.draw_pixmap(
        0,
        0,
        a.as_ref(),
        &paint,
        tiny_skia::Transform::identity(),
        None,
    );
    out.draw_pixmap(
        a.width() as i32,
        0,
        b.as_ref(),
        &paint,
        tiny_skia::Transform::identity(),
        None,
    );
    out
}
