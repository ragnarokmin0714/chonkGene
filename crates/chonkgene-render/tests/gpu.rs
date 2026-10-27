//! The GPU ray-marcher draws what the CPU one does (PLAN §3.1). Skipped,
//! with a note, on machines without any GPU adapter (software ones count).

use chonkgene_core::anim::{Expression, PoseParams};
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

fn at(clock: f32) -> PoseParams {
    PoseParams {
        clock,
        ..Default::default()
    }
}

/// Render and wait for the picture (frames arrive on a later call).
fn draw(r: &mut chonkgene_render::Renderer, p: &PoseParams, e: Expression) -> Pixmap {
    r.render(p, e);
    r.finish();
    r.pixmap().clone()
}

fn differ(a: &Pixmap, b: &Pixmap) -> usize {
    a.data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.data().as_chunks::<4>().0)
        .filter(|(p, q)| p[3].abs_diff(q[3]) > 64)
        .count()
}

/// A frame is sent without waiting and shown when it has arrived, on a
/// later call; until then the last picture stays up.
#[test]
fn gpu_frames_arrive_later_without_blocking() {
    let Some(mut r) = renderer() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let sit = draw(&mut r, &at(0.0), Expression::Neutral);
    let squashed = PoseParams {
        squash: 0.3,
        ..at(0.1)
    };
    r.render(&squashed, Expression::Neutral);
    assert!(r.busy(), "a frame is on its way");
    assert_eq!(r.pixmap().data(), sit.data(), "the old picture stays up");
    r.finish();
    assert!(differ(r.pixmap(), &sit) > 100, "then the new one shows");
}

/// Lying down blends every bone over a moment instead of cutting.
#[test]
fn moving_between_poses_blends() {
    let Some(mut r) = renderer() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let sit = draw(&mut r, &at(0.0), Expression::Neutral);
    draw(&mut r, &at(0.05), Expression::Asleep);
    let mid = draw(&mut r, &at(0.2), Expression::Asleep);
    assert!(r.busy(), "still lying down at 0.2 s");
    let loaf = draw(&mut r, &at(1.0), Expression::Asleep);
    assert!(!r.busy());
    assert!(differ(&mid, &sit) > 200, "the middle is not the start");
    assert!(differ(&mid, &loaf) > 200, "the middle is not the end");
}

/// Pokes land on the zone under the cursor, through the frame's squash.
#[test]
fn gpu_frames_hit_test_their_zones() {
    let Some(mut r) = renderer() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let squashed = PoseParams {
        squash: 0.2,
        ..at(0.0)
    };
    let frame = draw(&mut r, &squashed, Expression::Neutral);
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

/// At rest, frames repeat: the same breath step is not drawn again, a
/// cached one shows at once (no GPU), and matches a fresh one. A wag at
/// rest comes from the cache too once seen (sulking: the tail faces you).
#[test]
fn idle_frames_come_from_the_cache() {
    let Some(mut r) = renderer() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let rest = |b: f32, tail: f32, clock: f32| PoseParams {
        breath: b,
        tail,
        ..at(clock)
    };
    let first = draw(&mut r, &rest(0.6, 0.0, 0.0), Expression::Sulk);
    r.render(&rest(0.61, 0.0, 0.1), Expression::Sulk);
    assert!(!r.changed(), "the same breath step");
    draw(&mut r, &rest(-1.0, 0.0, 0.2), Expression::Sulk);
    let wag = draw(&mut r, &rest(0.6, 9.0, 0.3), Expression::Sulk);
    assert!(differ(&wag, &first) > 20, "the tail moved");
    // Breathing at rest is drawn in 2D over the cached frame: no GPU.
    r.render(&rest(-1.0, 0.0, 0.35), Expression::Sulk);
    assert!(r.changed() && !r.busy(), "a breath step without the GPU");
    for (tail, want) in [(0.0, &first), (9.4, &wag)] {
        r.render(&rest(0.6, tail, 0.4 + tail), Expression::Sulk);
        assert!(r.changed() && !r.busy(), "shown from the cache at once");
        assert_eq!(r.pixmap().data(), want.data());
    }
}

/// A hop lifts the pet; its shadow stays on the floor.
#[test]
fn a_hop_leaves_the_shadow_on_the_floor() {
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
    let down = draw(&mut r, &at(0.0), Expression::Neutral);
    let up = draw(
        &mut r,
        &PoseParams {
            bob: -24.0,
            ..at(1.0)
        },
        Expression::Neutral,
    );
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
