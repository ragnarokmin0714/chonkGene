//! The GPU ray-marcher draws what the CPU one does (PLAN §3.1). Skipped,
//! with a note, on machines without any GPU adapter (software ones count).

use chonkgene_render::KEYS;
use chonkgene_render::gpu::{Frame, Gpu};
use chonkgene_render::sprites::{Variant, render_layer};
use chonkgene_rig::{Layer, Scene, builtin};
use tiny_skia::{IntSize, Pixmap};

#[test]
fn gpu_matches_cpu_for_every_pose_and_face() {
    let gpu = match Gpu::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    };
    let model = builtin::load("shiba").unwrap();
    let mut canvas = gpu.canvas(1.0);
    let (w, h) = canvas.size();
    let mut pm = Pixmap::new(w, h).unwrap();
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let variant = Variant {
        coat: 0,
        chonk: 0.33,
        scale: 1.0,
    };
    for key in KEYS {
        let scene = Scene::new(&model, 0, 0.33, key.pose, key.look, Layer::All);
        gpu.render(&mut canvas, &scene, &Frame::default(), true, &mut pm)
            .unwrap();
        let s = render_layer(&model, variant, key, Layer::All, threads);
        let cpu = Pixmap::from_vec(s.rgba, IntSize::from_wh(s.width, s.height).unwrap()).unwrap();
        // The GPU samples every pixel five times, the CPU only edges, so
        // the plush noise differs a little inside; faces must not.
        let off = pm
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(cpu.data().as_chunks::<4>().0)
            .filter(|(p, q)| p.iter().zip(q.iter()).any(|(a, b)| a.abs_diff(*b) > 24))
            .count();
        let share = off as f64 / f64::from(w * h);
        assert!(share < 0.002, "{key:?}: {off} pixels differ");
    }
}
