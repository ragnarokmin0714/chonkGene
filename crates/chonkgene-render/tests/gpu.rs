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
        // Both trace pixel centres and add rays at edges; only rounding
        // (and the GPU keeping the centre sample in 8 bits) differs.
        let off = pm
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(cpu.data().as_chunks::<4>().0)
            .filter(|(p, q)| p.iter().zip(q.iter()).any(|(a, b)| a.abs_diff(*b) > 8))
            .count();
        let share = off as f64 / f64::from(w * h);
        assert!(share < 0.001, "{key:?}: {off} pixels differ");
    }
}

/// A GPU renderer for the tests, or none (skipped) without any adapter.
fn renderer() -> Option<chonkgene_render::Renderer> {
    use std::sync::{Arc, OnceLock};
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    let gpu = GPU.get_or_init(|| Gpu::new().ok()).as_ref()?;
    let model = Arc::new(builtin::load("shiba").unwrap());
    chonkgene_render::Renderer::gpu(gpu, model, 1.0, 0.33, 0)
}

fn at(clock: f32) -> chonkgene_core::anim::PoseParams {
    chonkgene_core::anim::PoseParams {
        clock,
        ..Default::default()
    }
}

/// Lying down blends every bone over a moment instead of cutting.
#[test]
fn moving_between_poses_blends() {
    use chonkgene_core::anim::Expression;
    let Some(mut r) = renderer() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let sit = r.render(&at(0.0), Expression::Neutral).clone();
    r.render(&at(0.05), Expression::Asleep);
    let mid = r.render(&at(0.2), Expression::Asleep).clone();
    assert!(r.blending(), "still lying down at 0.2 s");
    let loaf = r.render(&at(1.0), Expression::Asleep).clone();
    assert!(!r.blending());
    let differ = |a: &Pixmap, b: &Pixmap| {
        a.data()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.data().as_chunks::<4>().0)
            .filter(|(p, q)| p[3].abs_diff(q[3]) > 64)
            .count()
    };
    assert!(differ(&mid, &sit) > 200, "the middle is not the start");
    assert!(differ(&mid, &loaf) > 200, "the middle is not the end");
}

/// Pokes land on the zone under the cursor, through the frame's squash.
#[test]
fn gpu_frames_hit_test_their_zones() {
    use chonkgene_core::anim::Expression;
    let Some(mut r) = renderer() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let squashed = chonkgene_core::anim::PoseParams {
        squash: 0.2,
        ..at(0.0)
    };
    let frame = r.render(&squashed, Expression::Neutral).clone();
    let (ax, ay) = r.anchor();
    assert_eq!(r.hit(ax, ay + 6.0), Some("head"));
    assert_eq!(r.hit(1.0, 1.0), None);
    for zone in ["belly", "cheek", "paw", "bottom"] {
        assert!(
            (0..frame.height()).step_by(2).any(|y| (0..frame.width())
                .step_by(2)
                .any(|x| r.hit(x as f32, y as f32) == Some(zone))),
            "no {zone} zone"
        );
    }
}

/// Breathing in place repeats a few frames: the same breath step is not
/// drawn again, and a frame from the cache matches a fresh one.
#[test]
fn idle_frames_come_from_the_cache() {
    use chonkgene_core::anim::{Expression, PoseParams};
    let Some(mut r) = renderer() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let breath = |b: f32, clock: f32| PoseParams {
        breath: b,
        ..at(clock)
    };
    let first = r.render(&breath(0.6, 0.0), Expression::Neutral).clone();
    r.render(&breath(0.61, 0.1), Expression::Neutral);
    assert!(!r.changed(), "the same breath step");
    r.render(&breath(-1.0, 0.2), Expression::Neutral);
    assert!(r.changed());
    let again = r.render(&breath(0.6, 0.3), Expression::Neutral).clone();
    assert!(r.changed());
    assert_eq!(first.data(), again.data());
}

/// A hop lifts the pet; its shadow stays on the floor.
#[test]
fn a_hop_leaves_the_shadow_on_the_floor() {
    use chonkgene_core::anim::{Expression, PoseParams};
    let Some(mut r) = renderer() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let top = |pm: &Pixmap| {
        (0..pm.height())
            .find(|&y| (0..pm.width()).any(|x| pm.pixel(x, y).is_some_and(|p| p.alpha() > 200)))
            .unwrap_or(0)
    };
    let floor_shade = |pm: &Pixmap| {
        // Rows just under the feet: only the contact shadow lives there.
        let y = pm.height() - 4;
        (0..pm.width())
            .filter_map(|x| pm.pixel(x, y))
            .map(|p| u32::from(p.alpha()))
            .sum::<u32>()
    };
    let down = r.render(&at(0.0), Expression::Neutral).clone();
    let up = r
        .render(
            &PoseParams {
                bob: -24.0,
                ..at(1.0)
            },
            Expression::Neutral,
        )
        .clone();
    assert!(
        top(&down) >= top(&up) + 20,
        "{} vs {}",
        top(&down),
        top(&up)
    );
    let (a, b) = (floor_shade(&down), floor_shade(&up));
    assert!(b > 0, "the shadow is still on the floor");
    assert!(b < a, "and fainter: {b} vs {a}");
}
