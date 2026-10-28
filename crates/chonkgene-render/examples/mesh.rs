//! Skinned-mesh spike: a four-legged pet (`shiba`, `fox`) baked into a mesh
//! and walked.
//!
//! `cargo run --release -p chonkgene-render --example mesh -- peek <species> <out dir>`
//! ray-marches the model from a few angles, to check its shape.
//!
//! `... -- walk <species> <out dir> [scale] [coat]` bakes the mesh, then writes
//! `walk-<frame>.png` at 30 fps: walking right, turning, walking left,
//! stopping, and a poke in the belly. It prints the bake time, the mesh
//! size and what each frame cost.

use std::time::Instant;

use chonkgene_render::gpu::Gpu;
use chonkgene_render::mesh::bake;
use chonkgene_render::mesh_gpu::MeshCanvas;
use chonkgene_render::raymarch;
use chonkgene_render::skin::{Skeleton, Walk, quad};
use chonkgene_rig::math::v3;
use chonkgene_rig::{Layer, Look, Model, Pose, PoseId, Scene};
use tiny_skia::{Color, IntSize, Pixmap, PixmapPaint, Transform};

const FPS: f32 = 30.0;
/// Walking speed, pixels a second at 100% (chonkgene-core motion).
const WALK_SPEED: f32 = 60.0;
/// Pixels a world unit at 100% (raymarch.rs).
const PPU: f32 = 58.0;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(mode), Some(id), Some(out)) = (args.first(), args.get(1), args.get(2)) else {
        eprintln!("usage: mesh peek|walk <species> <out dir> [scale] [coat]");
        std::process::exit(2);
    };
    let scale: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let (model, sk) = quad(id).unwrap_or_else(|e| panic!("{e}"));
    let coat = args
        .get(4)
        .map(|c| model.coat(c).unwrap_or_else(|| panic!("no coat {c:?}")))
        .unwrap_or(0);
    std::fs::create_dir_all(out).expect("out dir");
    match mode.as_str() {
        "peek" => peek(&model, out),
        "walk" => walk(&model, &sk, coat, out, scale),
        _ => panic!("no mode {mode:?}"),
    }
}

fn peek(model: &Model, out: &str) {
    for yaw in [0.0, 45.0, 90.0, 180.0] {
        let mut pose = model.pose(PoseId::Sit).clone();
        pose.root.rot = v3(0.0, yaw, 0.0);
        let scene = Scene::posed(model, 0, 0.33, &pose, Look::Open, Layer::All);
        let s = raymarch::render(&scene, 1.0, 8);
        let size = IntSize::from_wh(s.width, s.height).expect("size");
        let px = Pixmap::from_vec(s.rgba, size).expect("pixmap");
        px.save_png(format!("{out}/peek-{yaw:03}.png"))
            .expect("png");
    }
}

fn walk(model: &Model, sk: &Skeleton, coat: usize, out: &str, scale: f32) {
    let t0 = Instant::now();
    let mesh = bake(model, sk, coat, 0.33, 0.025).expect("bake");
    println!(
        "baked {} vertices, {} triangles in {:.2} s",
        mesh.vertices.len(),
        mesh.indices.len() / 3,
        t0.elapsed().as_secs_f32()
    );
    let gpu = Gpu::new().expect("gpu");
    println!("GPU: {}", gpu.name());
    let (w, h) = (
        (4.5 * PPU * scale).ceil() as u32,
        (3.3 * PPU * scale).ceil() as u32,
    );
    let mut canvas = MeshCanvas::new(&gpu, &mesh, w, h, scale);
    let face = Scene::posed(model, coat, 0.33, &Pose::default(), Look::Open, Layer::All);
    let mut frame = Pixmap::new(w, h).expect("frame");
    let mut walker = Walk::new(sk, 1.0).expect("walk");
    let mut x = 0.0f32;
    let mut wall = 0.0f64;
    let mut gpu_secs = Vec::new();
    let frames = (7.4 * FPS) as u32;
    for f in 0..frames {
        let t = f as f32 / FPS;
        let walking = t < 4.0;
        let facing = if t < 1.9 { 1.0 } else { -1.0 };
        if (t - 5.6).abs() < 0.01 {
            walker.poke(2.0);
        }
        let mut joints = Vec::new();
        for _ in 0..2 {
            joints = walker.step(0.5 / FPS, walking, facing);
            x += WALK_SPEED * scale * walker.speed * facing * 0.5 / FPS;
        }
        let bones = sk.pose(&joints);
        let start = Instant::now();
        let secs = canvas
            .draw(&gpu, &face, &bones, walker.yaw(), &mut frame)
            .expect("draw");
        wall += start.elapsed().as_secs_f64();
        gpu_secs.extend(secs);
        let wide = w * 2;
        let mut page = Pixmap::new(wide, h).expect("page");
        page.fill(Color::TRANSPARENT);
        let left = (wide - w) as f32 / 2.0 + x - 60.0 * scale;
        page.draw_pixmap(
            left.round() as i32,
            0,
            frame.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        page.save_png(format!("{out}/walk-{f:03}.png"))
            .expect("png");
    }
    let n = frames as f64;
    println!(
        "{w}x{h}: {:.2} ms a frame wall (draw + readback)",
        wall / n * 1e3
    );
    if !gpu_secs.is_empty() {
        let avg = gpu_secs.iter().map(|s| f64::from(*s)).sum::<f64>() / gpu_secs.len() as f64;
        println!("GPU time {:.3} ms a frame", avg * 1e3);
    }
}
