//! Cost of a pet: rendering its sprites once, then each frame's blit
//! (PLAN §5). `cargo run --release -p chonkgene-render --example bench`

use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use chonkgene_core::Zone;
use chonkgene_core::anim::Animator;
use chonkgene_render::sprites::{Variant, render_one};
use chonkgene_render::{KEYS, Renderer};
use chonkgene_rig::builtin;

const FRAMES: u32 = 4000;

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
        let mut anim = Animator::new(1, 0.33);
        let start = Instant::now();
        for i in 0..FRAMES {
            if i % 120 == 0 {
                anim.poke(Zone::Belly);
            }
            anim.walking = i % 600 < 100;
            anim.step(1.0 / 60.0);
            black_box(r.render(&anim.params(), anim.expression));
        }
        let us = start.elapsed().as_secs_f64() * 1e6 / f64::from(FRAMES);
        let (w, h) = Renderer::pixel_size(scale);
        // Default desktop (PLAN §5): 6 pets, one walking at 30 fps, five idle at 10 fps.
        let six = (30.0 + 5.0 * 10.0) * us / 1e4;
        println!(
            "scale {scale}: {w}×{h} px; sprites {per_sprite:.0} ms each on one thread ({} per pet); \
             frames {us:.0} µs → 6 pets ≈ {six:.2}% of one core",
            KEYS.len()
        );
    }
}
