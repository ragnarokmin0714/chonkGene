//! Drawing cost of one pet frame (PLAN §5 target: ≤ 70 µs).
//! `cargo run --release -p chonkgene-render --example bench`

use std::hint::black_box;
use std::time::Instant;

use chonkgene_core::Zone;
use chonkgene_core::anim::Animator;
use chonkgene_render::{BoneMap, Renderer};
use chonkgene_rig::{Rig, builtin};

const FRAMES: u32 = 4000;

fn main() {
    let rig = Rig::load(builtin::SHIBA).expect("rig");
    let map = BoneMap::new(&rig);
    for scale in [1.0, 1.5] {
        let mut r = Renderer::new(&rig, scale, 0.33, 0).expect("renderer");
        let mut anim = Animator::new(1, 0.33);
        let mut pose = rig.rest_pose();
        let start = Instant::now();
        for i in 0..FRAMES {
            if i % 120 == 0 {
                anim.poke(Zone::Belly);
            }
            anim.walking = i % 600 < 100;
            anim.step(1.0 / 60.0);
            map.apply(&anim.params(), &mut pose);
            black_box(r.render(&rig, &pose));
        }
        let us = start.elapsed().as_secs_f64() * 1e6 / f64::from(FRAMES);
        let (w, h) = Renderer::pixel_size(&rig, scale);
        // Default desktop (PLAN §5): 6 pets, one walking at 30 fps, five idle at 10 fps.
        let six = (30.0 + 5.0 * 10.0) * us / 1e4;
        println!(
            "scale {scale}: {w}×{h} px, {us:.0} µs/frame → 6 pets ≈ {six:.2}% of one core, each extra idle pet ≈ {:.2}%",
            10.0 * us / 1e4
        );
    }
}
