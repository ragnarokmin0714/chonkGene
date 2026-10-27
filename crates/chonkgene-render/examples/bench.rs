//! Cost of a pet: rendering its sprites once, then its frames (PLAN §5).
//! `cargo run --release -p chonkgene-render --example bench`

use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use chonkgene_core::anim::Animator;
use chonkgene_render::sprites::{Variant, render_one};
use chonkgene_render::{KEYS, Renderer};
use chonkgene_rig::builtin;

/// Simulated seconds per pet.
const SECS: f32 = 120.0;

fn main() {
    let model = Arc::new(builtin::load("shiba").expect("model"));
    for scale in [1.0, 1.5, 2.0] {
        let variant = Variant {
            coat: 0,
            chonk: 0.33,
            scale,
        };
        let start = Instant::now();
        for key in KEYS {
            black_box(render_one(&model, variant, key, 1));
        }
        let per_sprite = start.elapsed().as_secs_f64() * 1e3 / KEYS.len() as f64;

        let mut r = Renderer::new(Arc::clone(&model), scale, 0.33, 0).expect("renderer");
        r.finish();
        let idle = run(&mut r, false);
        let walking = run(&mut r, true);
        let (w, h) = Renderer::pixel_size(scale);
        // Default desktop (PLAN §5): 6 pets, one walking, five idle.
        let six = (walking.cpu + 5.0 * idle.cpu) * 100.0;
        println!(
            "scale {scale}: {w}×{h} px; sprites {per_sprite:.0} ms each on one thread ({} per pet)\n  \
             idle: {:.1} fps, {:.1} shown/s, {:.3}% core; walking: {:.1} fps, {:.1} shown/s, {:.3}% core\n  \
             6 pets ≈ {six:.2}% of one core drawing, plus {:.0} frames/s to show",
            KEYS.len(),
            idle.fps,
            idle.shown,
            idle.cpu * 100.0,
            walking.fps,
            walking.shown,
            walking.cpu * 100.0,
            walking.shown + 5.0 * idle.shown,
        );
    }
}

struct Cost {
    fps: f64,
    /// Frames that changed and would be uploaded to the window.
    shown: f64,
    /// Share of one core spent drawing.
    cpu: f64,
}

/// Run one pet at the frame rate it asks for, drawing every frame.
fn run(r: &mut Renderer, walking: bool) -> Cost {
    let mut anim = Animator::new(2, 0.33, 0.35);
    anim.walking = walking;
    let (mut t, mut frames, mut shown) = (0.0, 0u32, 0u32);
    let start = Instant::now();
    while t < SECS {
        let dt = 1.0 / anim.fps(false) as f32;
        anim.step(dt);
        t += dt;
        black_box(r.render(&anim.params(), anim.expression));
        frames += 1;
        if r.changed() {
            shown += 1;
        }
    }
    let secs = f64::from(SECS);
    Cost {
        fps: f64::from(frames) / secs,
        shown: f64::from(shown) / secs,
        cpu: start.elapsed().as_secs_f64() / secs,
    }
}
