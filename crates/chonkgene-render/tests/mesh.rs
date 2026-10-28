//! The skinned-mesh spike: the baked mesh looks like the ray-marched pet,
//! its weights hold together, and the four-legged walk plants its feet.

use chonkgene_render::gpu::Gpu;
use chonkgene_render::mesh::bake;
use chonkgene_render::mesh_gpu::{MAX_BONES, MeshCanvas};
use chonkgene_render::raymarch;
use chonkgene_render::skin::{QUAD_SPECIES, Skeleton, Walk, quad};
use chonkgene_rig::math::{V3, v3};
use chonkgene_rig::{Layer, Look, Model, Pose, Scene};
use tiny_skia::Pixmap;

/// Pixels a world unit at 100%, and the walking pace (chonkgene-core motion).
const PPU: f32 = 58.0;
const WALK_SPEED: f32 = 60.0;

fn shiba() -> (Model, Skeleton) {
    quad("shiba").unwrap()
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
    for id in QUAD_SPECIES {
        let (model, sk) = quad(id).unwrap();
        let (mesh, ray) = draw_both(&gpu, &model, &sk);
        // The pet's own pixels (not its shadow) in both: they cover the
        // same ground, and where both draw the pet, the colours agree.
        let solid = |a: u8| a > 200;
        let (mut both, mut either, mut diff) = (0u32, 0u32, 0u64);
        for (m, r) in mesh.iter().zip(&ray) {
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
        eprintln!("{id}: overlap {overlap:.3}, colour difference {mean:.2}");
        assert!(overlap > 0.97, "{id}: silhouettes overlap {overlap:.3}");
        assert!(mean < 6.0, "{id}: colours differ by {mean:.1} on average");
    }
}

/// The pet at rest facing you, drawn as a mesh and ray-marched: RGBA pixels
/// of each, the same size.
fn draw_both(gpu: &Gpu, model: &Model, sk: &Skeleton) -> (Vec<[u8; 4]>, Vec<[u8; 4]>) {
    let mesh = bake(model, sk, 0, 0.33, 0.025).unwrap();
    let scene = Scene::posed(model, 0, 0.33, &Pose::default(), Look::Open, Layer::All);
    let s = raymarch::render(&scene, 1.0, 4);
    let mut canvas = MeshCanvas::new(gpu, &mesh, s.width, s.height, 1.0);
    let mut pm = Pixmap::new(s.width, s.height).unwrap();
    canvas
        .draw(gpu, &scene, &sk.pose(&[]), 0.0, &mut pm)
        .unwrap();
    (
        pm.data().as_chunks::<4>().0.to_vec(),
        s.rgba.as_chunks::<4>().0.to_vec(),
    )
}

#[test]
fn the_fox_eyeliner_lands_in_the_same_place_on_the_gpu_and_the_cpu() {
    let gpu = match Gpu::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    };
    let (model, sk) = quad("fox").unwrap();
    assert!(model.face.liner.is_some());
    let mut bare = model.clone();
    bare.face.liner = None;
    let (mesh_liner, ray_liner) = draw_both(&gpu, &model, &sk);
    let (mesh_bare, ray_bare) = draw_both(&gpu, &bare, &sk);
    // The pixels the liner changes, in each renderer (face.rs on the CPU,
    // paint.wgsl on the GPU): there are some, and they are the same ones.
    let changed = |a: &[[u8; 4]], b: &[[u8; 4]]| -> Vec<bool> {
        a.iter()
            .zip(b)
            .map(|(p, q)| (0..3).any(|i| p[i].abs_diff(q[i]) > 40))
            .collect()
    };
    let on_gpu = changed(&mesh_liner, &mesh_bare);
    let on_cpu = changed(&ray_liner, &ray_bare);
    let both = on_gpu
        .iter()
        .zip(&on_cpu)
        .filter(|(a, b)| **a && **b)
        .count();
    let either = on_gpu
        .iter()
        .zip(&on_cpu)
        .filter(|(a, b)| **a || **b)
        .count();
    assert!(both > 10, "the liner shows on {both} pixels");
    assert!(
        both as f32 / either as f32 > 0.7,
        "liner pixels agree on {both} of {either}"
    );
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

#[test]
fn every_four_legged_pet_fits_the_shader_and_stands_on_its_paws() {
    for id in QUAD_SPECIES {
        let (model, sk) = quad(id).unwrap();
        assert!(
            sk.names.len() <= MAX_BONES,
            "{id}: {} bones",
            sk.names.len()
        );
        let mesh = bake(&model, &sk, 0, 0.33, 0.025).unwrap();
        // The shadow covers the four paws (x ±0.3, z 0.5 and −0.6 from
        // the middle) but stays under the pet.
        let (across, along) = mesh.shadow;
        assert!((0.4..1.0).contains(&across), "{id}: shadow across {across}");
        assert!((0.7..1.5).contains(&along), "{id}: shadow along {along}");
        Walk::new(&sk, 1.0).unwrap();
    }
}

/// Where each tail's base and tip are over `secs` of standing or walking,
/// sampled every frame.
fn tail_paths(sk: &Skeleton, walking: bool, secs: f32) -> Vec<(Vec<V3>, Vec<V3>)> {
    let tails = sk.tails();
    let mut walk = Walk::new(sk, 1.0).unwrap();
    let mut paths = vec![(Vec::new(), Vec::new()); tails.len()];
    let dt = 1.0 / 60.0;
    for _ in 0..(secs / dt) as usize {
        let pose = sk.pose(&walk.step(dt, walking, 1.0));
        for (t, chain) in tails.iter().enumerate() {
            // A point 0.25 out along the first bone, and one 0.25 on past
            // the last bone's root, the way the tail runs: its tip.
            let (base, tip) = (chain[0], chain[chain.len() - 1]);
            let along =
                |from: usize, to: usize| (sk.pivots[to] - sk.pivots[from]).normalize() * 0.25;
            let root = sk.pivots[base] + along(base, chain[1]);
            let end = sk.pivots[tip] + along(chain[chain.len() - 2], tip);
            paths[t].0.push(pose[base].apply(root));
            paths[t].1.push(pose[tip].apply(end));
        }
    }
    paths
}

fn sweep(path: &[V3]) -> f32 {
    let mut worst = 0.0f32;
    for a in path {
        for b in path {
            worst = worst.max((*a - *b).length());
        }
    }
    worst
}

#[test]
fn tails_swish_like_a_whip_standing_or_walking() {
    for id in QUAD_SPECIES {
        let (_, sk) = quad(id).unwrap();
        assert!(!sk.tails().is_empty(), "{id} has no tail");
        for walking in [false, true] {
            // Past the first second, once the walk has got going.
            for (base, tip) in tail_paths(&sk, walking, 4.0) {
                let (base, tip) = (sweep(&base[60..]), sweep(&tip[60..]));
                // The tip swings visibly: 0.1 world units is 6 px at 100%.
                assert!(
                    tip > 0.1,
                    "{id}: tail tip swings {tip:.3} (walking {walking})"
                );
                assert!(tip > base, "{id}: tip {tip:.3} vs base {base:.3}");
            }
            // And the tail bends as it goes, not swinging stiff from the
            // root: the last segment turns against the one before it.
            let mut walk = Walk::new(&sk, 1.0).unwrap();
            let tips: Vec<usize> = sk.tails().iter().map(|c| c[c.len() - 1]).collect();
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for f in 0..240 {
                let joints = walk.step(1.0 / 60.0, walking, 1.0);
                if f >= 60 {
                    lo = lo.min(joints[tips[0]].rot.z);
                    hi = hi.max(joints[tips[0]].rot.z);
                }
            }
            assert!(hi - lo > 4.0, "{id}: the tip bends only {:.1}°", hi - lo);
        }
    }
}

#[test]
fn nine_tails_ripple_rather_than_sway_as_one() {
    let (_, sk) = quad("fox").unwrap();
    assert_eq!(sk.tails().len(), 9);
    let paths = tail_paths(&sk, false, 3.0);
    // Each tip's sideways offset from its own average, at one moment: in
    // step they would all share a sign.
    let at = paths[0].1.len() - 1;
    let sides: Vec<f32> = paths
        .iter()
        .map(|(_, tip)| {
            let mean = tip.iter().map(|p| p.x).sum::<f32>() / tip.len() as f32;
            tip[at].x - mean
        })
        .collect();
    let left = sides.iter().filter(|s| **s < 0.0).count();
    assert!((2..=7).contains(&left), "tails swing together: {sides:?}");
}

#[test]
fn a_tail_sways_like_seaweed_with_the_wave_running_root_to_tip() {
    for id in QUAD_SPECIES {
        let (_, sk) = quad(id).unwrap();
        let chain = sk.tails()[0].clone();
        assert!(chain.len() >= 3, "{id}: a tail of {} segments", chain.len());
        let mut walk = Walk::new(&sk, 1.0).unwrap();
        let (mut root, mut mid) = (Vec::new(), Vec::new());
        for f in 0..600 {
            let joints = walk.step(1.0 / 60.0, false, 1.0);
            if f >= 120 {
                root.push(joints[chain[0]].rot.z);
                mid.push(joints[chain[2]].rot.z);
            }
        }
        // How many frames later the middle segment repeats the root's
        // bend: the lag that lines the two up best. A wave running down
        // the tail puts it a little behind, never ahead or in step.
        let score = |lag: usize| -> f32 {
            root.iter()
                .zip(&mid[lag..])
                .map(|(a, b)| a * b)
                .sum::<f32>()
        };
        let best = (0..60)
            .max_by(|a, b| score(*a).total_cmp(&score(*b)))
            .unwrap();
        assert!(
            (10..45).contains(&best),
            "{id}: the middle of the tail follows the root {best} frames late"
        );
    }
}

#[test]
fn each_tail_segment_turns_about_its_own_root() {
    // A segment whose pivot is missing turns about the middle of the pet
    // instead, flinging the whole tail around (fox/gen_tails.py output
    // pasted without its PIVOTS once did exactly that).
    for id in QUAD_SPECIES {
        let (model, sk) = quad(id).unwrap();
        for part in &model.parts {
            if !sk.names[part.bone].starts_with("tail") {
                continue;
            }
            let off = (sk.pivots[part.bone] - part.at).length();
            assert!(
                off < 0.01,
                "{id}: {} turns {off:.2} from its root",
                part.name
            );
        }
    }
}
