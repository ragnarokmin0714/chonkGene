//! The skinned-mesh spike: the baked mesh looks like the ray-marched pet,
//! its weights hold together, and the four-legged walk plants its feet.

use chonkgene_render::gpu::Gpu;
use chonkgene_render::mesh::bake;
use chonkgene_render::mesh_gpu::MeshCanvas;
use chonkgene_render::raymarch;
use chonkgene_render::skin::{SHIBA_SHADOW, Skeleton, Walk, shiba_quad};
use chonkgene_rig::math::{V3, v3};
use chonkgene_rig::{Layer, Look, Model, Pose, Scene};
use tiny_skia::Pixmap;

/// Pixels a world unit at 100%, and the walking pace (chonkgene-core motion).
const PPU: f32 = 58.0;
const WALK_SPEED: f32 = 60.0;

fn shiba() -> (Model, Skeleton) {
    shiba_quad().unwrap()
}

#[test]
fn every_vertex_hangs_on_bones_that_add_up() {
    let (model, sk) = shiba();
    let mesh = bake(&model, &sk, 0, 0.33, 0.025).unwrap();
    assert!(
        mesh.vertices.len() > 10_000,
        "{} vertices",
        mesh.vertices.len()
    );
    assert_eq!(mesh.indices.len() % 3, 0);
    assert!(
        mesh.indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len())
    );
    let leg = sk.bone("leg_fl").unwrap() as u32;
    let mut on_paw = 0;
    for v in &mesh.vertices {
        let sum: f32 = v.weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "weights add to {sum}");
        // A front paw moves with its own leg alone, or it would stretch
        // back to the body as the leg swings.
        let p = v3(v.pos[0], v.pos[1], v.pos[2]);
        if (p - v3(0.3, -0.95, 0.5)).length() < 0.2 && p.y < -0.98 {
            on_paw += 1;
            let w = v.bones.iter().zip(v.weights).find(|(b, _)| **b == leg);
            assert!(w.is_some_and(|(_, w)| w > 0.95), "paw vertex {p:?}: {v:?}");
        }
    }
    assert!(on_paw > 0);
}

/// Screen x (pixels at 100%) of the front-left paw's sole over a steady
/// walk, as the window carries the pet along, and its height.
fn paw_track(sk: &Skeleton) -> Vec<(f32, f32)> {
    let leg = sk.bone("leg_fl").unwrap();
    let sole = v3(0.3, -1.05, 0.5);
    let mut walk = Walk::new(sk, 1.0).unwrap();
    let dt = 1.0 / 60.0;
    let mut x = 0.0;
    let mut track = Vec::new();
    for f in 0..300 {
        let joints = walk.step(dt, true, 1.0);
        x += WALK_SPEED * walk.speed * dt;
        // Past the start, once the turn side-on has settled.
        if f >= 120 {
            let p: V3 = sk.pose(&joints)[leg].apply(sole);
            track.push((x + p.x * PPU, p.y));
        }
    }
    track
}

#[test]
fn a_planted_paw_stays_put_while_the_body_walks_over_it() {
    let (_, sk) = shiba();
    let track = paw_track(&sk);
    let low = track.iter().map(|t| t.1).fold(f32::MAX, f32::min);
    // Each run of frames with the paw down: how far it slid.
    let mut worst = 0.0f32;
    let mut run: Vec<f32> = Vec::new();
    for &(x, y) in track.iter().chain([(0.0, f32::MAX)].iter()) {
        if y < low + 0.03 {
            run.push(x);
        } else if run.len() > 3 {
            let (a, b) = run
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
            worst = worst.max(b - a);
            run.clear();
        } else {
            run.clear();
        }
    }
    // Without planting, the paw would ride along with the window: 60 px/s
    // over the ~0.3 s it is down, some 18 px.
    eprintln!("planted paw slid at most {worst:.2} px");
    assert!(worst < 3.0, "a planted paw slid {worst:.1} px");
    let travel = track.last().unwrap().0 - track[0].0;
    assert!(
        travel > 100.0,
        "the paw kept up with the walk: {travel:.0} px"
    );
}

#[test]
fn the_mesh_draws_the_same_pet_as_the_ray_marcher() {
    let gpu = match Gpu::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    };
    let (model, sk) = shiba();
    let mesh = bake(&model, &sk, 0, 0.33, 0.025).unwrap();
    let scene = Scene::posed(&model, 0, 0.33, &Pose::default(), Look::Open, Layer::All);
    let s = raymarch::render(&scene, 1.0, 4);
    let mut canvas = MeshCanvas::new(&gpu, &mesh, s.width, s.height, 1.0);
    let mut pm = Pixmap::new(s.width, s.height).unwrap();
    let rest = sk.pose(&[]);
    canvas
        .draw(&gpu, &scene, &rest, 0.0, SHIBA_SHADOW, &mut pm)
        .unwrap();
    // The pet's own pixels (not its shadow) in both: they cover the same
    // ground, and where both draw the pet, the colours agree.
    let solid = |a: u8| a > 200;
    let (mut both, mut either, mut diff) = (0u32, 0u32, 0u64);
    for (m, r) in pm
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(s.rgba.as_chunks::<4>().0)
    {
        let (a, b) = (solid(m[3]), solid(r[3]));
        if a || b {
            either += 1;
        }
        if a && b {
            both += 1;
            diff += (0..3).map(|i| m[i].abs_diff(r[i]) as u64).sum::<u64>();
        }
    }
    let overlap = both as f32 / either as f32;
    let mean = diff as f32 / (3 * both) as f32;
    eprintln!("overlap {overlap:.3}, colour difference {mean:.2}");
    assert!(overlap > 0.97, "silhouettes overlap {overlap:.3}");
    assert!(mean < 6.0, "colours differ by {mean:.1} on average");
}

#[test]
fn a_poke_wobbles_the_belly_and_it_settles() {
    let (_, sk) = shiba();
    let belly = sk.bone("belly").unwrap();
    let mut walk = Walk::new(&sk, 1.0).unwrap();
    let dt = 1.0 / 60.0;
    for _ in 0..60 {
        walk.step(dt, false, 1.0);
    }
    walk.poke(2.0);
    let wobble: Vec<f32> = (0..180)
        .map(|_| walk.step(dt, false, 1.0)[belly].shift.y)
        .collect();
    // Big enough to see (0.05 world units is 3 px at 100%), springing
    // back past rest rather than just sagging, and still within a second
    // or two: a jelly, not a pendulum.
    let peak = wobble.iter().copied().fold(0.0f32, f32::max);
    let dip = wobble.iter().copied().fold(0.0f32, f32::min);
    assert!(peak > 0.05, "the belly rose only {peak:.3}");
    assert!(dip < -0.02, "the belly never sprang back: {dip:.3}");
    let late = wobble[150..].iter().map(|y| y.abs()).fold(0.0f32, f32::max);
    assert!(late < 0.005, "still wobbling {late:.3} after 2.5 s");
}
