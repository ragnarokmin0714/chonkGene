//! The skinned-mesh spike: the baked mesh looks like the ray-marched pet,
//! its weights hold together, and the four-legged walk plants its feet.

use chonkgene_render::gpu::Gpu;
use chonkgene_render::mesh::bake;
use chonkgene_render::mesh_gpu::{MAX_BONES, MeshCanvas};
use chonkgene_render::raymarch;
use chonkgene_render::skin::{BIPED_SPECIES, QUAD_SPECIES, Skeleton, Walk, mesh_model, quad};
use chonkgene_render::waddle::Mover;
use chonkgene_rig::math::{M3, V3, v3};
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
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
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

/// Paints the fox with and without a face feature (`strip` takes it off a
/// copy of the model) and checks the pixels it changes are much the same
/// ones in the ray-marcher (face.rs) and the mesh (paint.wgsl): there are
/// some, and they agree.
fn feature_lands_in_the_same_place(name: &str, strip: fn(&mut chonkgene_rig::Model)) {
    let gpu = match Gpu::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    };
    let (model, sk) = quad("fox").unwrap();
    let mut bare = model.clone();
    strip(&mut bare);
    let (mesh_with, ray_with) = draw_both(&gpu, &model, &sk);
    let (mesh_bare, ray_bare) = draw_both(&gpu, &bare, &sk);
    let changed = |a: &[[u8; 4]], b: &[[u8; 4]]| -> Vec<bool> {
        a.iter()
            .zip(b)
            .map(|(p, q)| (0..3).any(|i| p[i].abs_diff(q[i]) > 40))
            .collect()
    };
    let on_gpu = changed(&mesh_with, &mesh_bare);
    let on_cpu = changed(&ray_with, &ray_bare);
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
    assert!(both > 10, "the {name} shows on {both} pixels");
    assert!(
        both as f32 / either as f32 > 0.7,
        "{name} pixels agree on {both} of {either}"
    );
}

#[test]
fn the_fox_eyeliner_lands_in_the_same_place_on_the_gpu_and_the_cpu() {
    let (model, _) = quad("fox").unwrap();
    assert!(model.face.liner.is_some());
    feature_lands_in_the_same_place("liner", |m| m.face.liner = None);
}

#[test]
fn the_fox_forehead_flame_lands_in_the_same_place_on_the_gpu_and_the_cpu() {
    let (model, _) = quad("fox").unwrap();
    assert!(model.face.forehead.is_some());
    feature_lands_in_the_same_place("forehead flame", |m| m.face.forehead = None);
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
fn every_mesh_pet_fits_the_shader_and_has_a_walk_and_a_shadow() {
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
        assert!(
            sk.names.len() <= MAX_BONES,
            "{id}: {} bones",
            sk.names.len()
        );
        let mesh = bake(&model, &sk, 0, 0.33, model.bake_cell).unwrap();
        let (across, along) = mesh.shadow;
        if QUAD_SPECIES.contains(id) {
            // The shadow covers the four paws (x ±0.3, z 0.5 and −0.6 from
            // the middle) but stays under the pet.
            assert!((0.4..1.0).contains(&across), "{id}: shadow across {across}");
            assert!((0.7..1.5).contains(&along), "{id}: shadow along {along}");
        } else {
            // A bird sits on its round belly: a shadow about as wide as it.
            assert!((0.7..1.3).contains(&across), "{id}: shadow across {across}");
            assert!((0.7..1.3).contains(&along), "{id}: shadow along {along}");
        }
        Mover::new(&sk, 1.0, &model).unwrap();
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
            // A point 0.25 out along the first bone, and the part of the
            // tail farthest from its root: the tip of a straight tail,
            // the top of a ring (whose tip curls back near the root).
            let base = chain[0];
            let along =
                |from: usize, to: usize| (sk.pivots[to] - sk.pivots[from]).normalize() * 0.25;
            let root = sk.pivots[base] + along(base, chain[1]);
            let reach = |b: &usize| (sk.pivots[*b] - sk.pivots[base]).length();
            let far = *chain
                .iter()
                .max_by(|a, b| reach(a).total_cmp(&reach(b)))
                .unwrap();
            paths[t].0.push(pose[base].apply(root));
            paths[t].1.push(pose[far].apply(sk.pivots[far]));
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
                    // Sideways: about z for a tail held up, about the
                    // vertical for one held out behind, or some of each.
                    let r = joints[tips[0]].rot;
                    lo = lo.min(r.y + r.z);
                    hi = hi.max(r.y + r.z);
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
            // The first part on each tail bone, named after it, starts at
            // its root (the fox hangs several cones on a bone, one after
            // another along it).
            if !sk.names[part.bone].starts_with("tail") || part.name != sk.names[part.bone] {
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

#[test]
fn sitting_brings_the_bottom_down_and_keeps_the_front_paws_planted() {
    for id in QUAD_SPECIES {
        let (_, sk) = quad(id).unwrap();
        let (leg, hips) = (sk.bone("leg_fl").unwrap(), sk.bone("hips").unwrap());
        let paw = v3(0.3, -1.05, 0.5);
        let bottom = v3(0.0, -0.78, -0.48);
        let mut walk = Walk::new(&sk, 1.0).unwrap();
        // Shifting its weight would move the bottom: compared without.
        walk.still();
        let dt = 1.0 / 60.0;
        let settle = |walk: &mut Walk, secs: f32| {
            let mut j = Vec::new();
            for _ in 0..(secs / dt) as usize {
                j = walk.step(dt, false, 1.0);
            }
            sk.pose(&j)
        };
        let stand = settle(&mut walk, 2.0);
        walk.sit(true);
        let sat = settle(&mut walk, 2.0);
        assert!(
            walk.sat() > 0.95,
            "{id}: only {:.2} of the way down",
            walk.sat()
        );
        // The paw stays on the floor (the body tips back about it), and
        // the bottom comes down most of the way to the floor.
        let paw_moved = (sat[leg].apply(paw).y - stand[leg].apply(paw).y).abs();
        assert!(
            paw_moved < 0.03,
            "{id}: the front paw moved {paw_moved:.3} up or down"
        );
        let drop = stand[hips].apply(bottom).y - sat[hips].apply(bottom).y;
        assert!(drop > 0.25, "{id}: the bottom came down only {drop:.2}");
        walk.sit(false);
        let up = settle(&mut walk, 2.0);
        let back = (up[hips].apply(bottom) - stand[hips].apply(bottom)).length();
        assert!(
            back < 0.01,
            "{id}: {back:.3} from standing after getting up"
        );
    }
}

#[test]
fn a_sitting_fox_fans_all_nine_tails_out_where_they_show() {
    let (_, sk) = quad("fox").unwrap();
    let mut walk = Walk::new(&sk, 1.0).unwrap();
    walk.sit(true);
    let mut joints = Vec::new();
    for _ in 0..240 {
        joints = walk.step(1.0 / 60.0, false, 1.0);
    }
    let pose = sk.pose(&joints);
    // Each tail's root, in the pet's own frame (the turn toward you taken
    // back out): off to one side or the other, clear of the head that
    // would hide a tail pointing straight up behind it, and both sides
    // used.
    let unturn = M3::rot_y(-walk.yaw().to_radians());
    let (mut left, mut right) = (0, 0);
    for chain in sk.tails() {
        let (a, b) = (chain[0], chain[1]);
        let dir = unturn
            .apply(pose[b].apply(sk.pivots[b]) - pose[a].apply(sk.pivots[a]))
            .normalize();
        assert!(
            dir.x.abs() > 0.3,
            "{}: points {dir:?}, behind the head",
            sk.names[a]
        );
        if dir.x < 0.0 {
            left += 1;
        } else {
            right += 1;
        }
    }
    assert!(left >= 4 && right >= 4, "{left} tails left, {right} right");
}

#[test]
fn the_bottom_plops_into_a_sit_and_waddles_on_the_walk() {
    for id in QUAD_SPECIES {
        let (_, sk) = quad(id).unwrap();
        let butt = sk.bone("butt").unwrap();
        let dt = 1.0 / 60.0;
        let mut walk = Walk::new(&sk, 1.0).unwrap();
        for _ in 0..60 {
            walk.step(dt, false, 1.0);
        }
        walk.sit(true);
        let ys: Vec<f32> = (0..180)
            .map(|_| walk.step(dt, false, 1.0)[butt].shift.y)
            .collect();
        // Squashed down as it lands, then springing back up past rest.
        let low = ys.iter().copied().fold(f32::MAX, f32::min);
        let low_at = ys.iter().position(|y| *y == low).unwrap();
        let high = ys[low_at..].iter().copied().fold(f32::MIN, f32::max);
        assert!(low < -0.04, "{id}: the bottom squashed only {low:.3}");
        assert!(high > 0.01, "{id}: no bounce back ({high:.3})");
        let mut walk = Walk::new(&sk, 1.0).unwrap();
        let xs: Vec<f32> = (0..240)
            .map(|_| walk.step(dt, true, 1.0)[butt].shift.x)
            .collect();
        let sway = xs[120..].iter().copied().fold(f32::MIN, f32::max)
            - xs[120..].iter().copied().fold(f32::MAX, f32::min);
        assert!(sway > 0.05, "{id}: the bottom waddles only {sway:.3}");
    }
}

#[test]
fn the_shiba_tail_tip_stays_clear_of_the_body() {
    // Roger: a doughnut like the first version's, but its end must not
    // join the body. Only the root may touch the rump; the tip keeps a gap
    // from the body and from the root, wider than the blend that would
    // melt them together.
    let (model, _) = quad("shiba").unwrap();
    let scene = Scene::posed(&model, 0, 0.33, &Pose::default(), Look::Open, Layer::All);
    let tip = &model.parts[model.part("tail_tip").unwrap()];
    let chonkgene_rig::sdf::Shape::Capsule { to, r } = tip.shape else {
        panic!("the tip is a capsule");
    };
    for p in [tip.at + to * 0.5, tip.at + to] {
        for pv in scene.parts() {
            let name = &pv.part.name;
            if name.starts_with("tail") && name != "tail" {
                continue;
            }
            let gap = pv.shape.distance(pv.a.apply(p) + pv.b) * pv.k - r;
            assert!(gap > 0.04, "the tail tip is {gap:.3} from {name}");
        }
    }
}

/// The app's mesh renderer for a species with a mesh model, on any GPU (a
/// software one counts here); none without a GPU.
fn app_renderer(id: &str) -> Option<chonkgene_render::MeshRenderer> {
    use std::sync::OnceLock;
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    let gpu = GPU.get_or_init(|| Gpu::new().ok()).as_ref()?;
    let (model, _) = mesh_model(id).unwrap();
    chonkgene_render::MeshRenderer::new(gpu, std::sync::Arc::new(model), 1.0, 0.33, 0)
}

fn opaque(pm: &Pixmap) -> usize {
    pm.data()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 200)
        .count()
}

#[test]
fn the_app_draws_every_mesh_pet_walking_and_answers_clicks() {
    use chonkgene_core::anim::{Act, Expression, PoseParams};
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let Some(mut r) = app_renderer(id) else {
            eprintln!("skipped: no GPU");
            return;
        };
        // Frames come back one call late and are paid for out of the GPU
        // budget: wait for each one so every render here shows its pose.
        let shot = |r: &mut chonkgene_render::MeshRenderer, p: &PoseParams, e| {
            r.render(p, e);
            r.finish();
            r.pixmap().clone()
        };
        let at = |clock: f32, act: Act| PoseParams {
            clock,
            act,
            ..Default::default()
        };
        let still = shot(&mut r, &at(0.0, Act::Rest), Expression::Neutral);
        assert!(
            opaque(&still) > 5_000,
            "{id}: drew {} pixels",
            opaque(&still)
        );
        // Walking, the legs keep moving: frames a beat apart differ.
        let mut frames = Vec::new();
        for f in 1..=30 {
            let act = if f % 2 == 0 { Act::StepA } else { Act::StepB };
            frames.push(shot(&mut r, &at(f as f32 / 30.0, act), Expression::Neutral));
        }
        let moved = frames[10]
            .data()
            .iter()
            .zip(frames[20].data())
            .filter(|(a, b)| a.abs_diff(**b) > 40)
            .count();
        assert!(
            moved > 500,
            "{id}: walking frames differ by only {moved} bytes"
        );
        // A click just under the top of the head lands on the head; one on
        // the empty corner lands on nothing.
        shot(&mut r, &at(2.0, Act::Rest), Expression::Neutral);
        let (x, y) = r.anchor();
        let head = (0..40)
            .map(|d| r.hit(x, y + d as f32))
            .find(Option::is_some)
            .flatten();
        assert_eq!(head, Some("head"), "{id}: under the crown at ({x}, {y})");
        assert_eq!(r.hit(1.0, 1.0), None);
        // Asleep: sitting, eyes shut, and the snot bubble drawn over it.
        r.render_asleep(&at(3.0, Act::Rest), 1.0);
        r.finish();
        let asleep = r.render_asleep(&at(3.0, Act::Rest), 1.0).clone();
        assert!(opaque(&asleep) > 5_000);
    }
}

/// Casting, the fox's forehead and eyes glow and seal lines grow over its
/// face (Roger's thirteenth pass); on a face with no forehead mark the
/// glow changes nothing.
#[test]
fn the_fox_face_glows_while_it_casts_and_no_one_elses_does() {
    use chonkgene_core::anim::{Expression, PoseParams};
    for (id, glows) in [("fox", true), ("shiba", false)] {
        let Some(mut r) = app_renderer(id) else {
            eprintln!("skipped: no GPU");
            return;
        };
        // Settled first: the first frames still move.
        let mut shot = |glow: f32| {
            let p = PoseParams {
                glow,
                ..Default::default()
            };
            for _ in 0..3 {
                r.render(&p, Expression::Neutral);
                r.finish();
            }
            r.pixmap().clone()
        };
        shot(0.0);
        let differ = |a: &Pixmap, b: &Pixmap| {
            a.data()
                .as_chunks::<4>()
                .0
                .iter()
                .zip(b.data().as_chunks::<4>().0)
                .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 30))
                .count()
        };
        let (plain, again, lit) = (shot(0.0), shot(0.0), shot(1.0));
        // What two frames without it differ by anyway (it still breathes).
        let noise = differ(&plain, &again);
        let changed = differ(&again, &lit);
        if glows {
            assert!(
                changed > noise + 400,
                "{id}: only {changed} pixels lit up ({noise} anyway)"
            );
        } else {
            assert!(
                changed <= noise + 20,
                "{id}: {changed} pixels changed ({noise} anyway)"
            );
        }
    }
}

/// A mesh renderer of its own on a GPU of its own (so a budget set here
/// touches no other test); none without a GPU.
fn lone_renderer(id: &str, budget: f32) -> Option<chonkgene_render::MeshRenderer> {
    let gpu: &'static Gpu = Box::leak(Box::new(Gpu::new().ok()?));
    gpu.set_budget(budget);
    let (model, _) = quad(id).unwrap();
    chonkgene_render::MeshRenderer::new(gpu, std::sync::Arc::new(model), 1.0, 0.33, 0)
}

#[test]
fn the_mesh_bakes_on_its_own_thread_and_shows_when_it_lands() {
    use chonkgene_core::anim::{Expression, PoseParams};
    let Some(mut r) = lone_renderer("fox", 1.0) else {
        eprintln!("skipped: no GPU");
        return;
    };
    // The fox takes most of a second to bake: `new` returned without it.
    assert!(!r.ready());
    assert!(!r.changed());
    assert_eq!(r.hit(100.0, 100.0), None);
    let start = std::time::Instant::now();
    while !r.poll() {
        assert!(start.elapsed().as_secs() < 60, "the bake never landed");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(r.ready());
    r.render(&PoseParams::default(), Expression::Neutral);
    r.finish();
    assert!(opaque(r.pixmap()) > 5_000);
}

#[test]
fn mesh_frames_wait_for_the_gpu_budget_but_never_starve() {
    use chonkgene_core::anim::{Expression, PoseParams};
    // A share of nothing: only what the budget starts with (a burst of
    // 50 ms of GPU time) can pay for frames, and frames are guessed to
    // cost at least 4 ms each, so a dozen at most from the budget; after
    // that a frame held back a quarter second goes anyway (Roger: pets
    // stood frozen for seconds), so new pictures never stop for long.
    let Some(mut r) = lone_renderer("shiba", 0.0) else {
        eprintln!("skipped: no GPU");
        return;
    };
    r.finish();
    assert!(r.ready());
    let mut arrived = Vec::new();
    let start = std::time::Instant::now();
    for f in 1..=60 {
        let params = PoseParams {
            clock: f as f32 / 30.0,
            ..Default::default()
        };
        r.render(&params, Expression::Neutral);
        if r.changed() {
            arrived.push(start.elapsed().as_secs_f32());
        }
        // Time for a frame on its way to land.
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    let secs = start.elapsed().as_secs_f32();
    let most = 13 + (secs / 0.25).ceil() as usize;
    assert!(
        arrived.len() <= most,
        "{} frames in {secs:.1} s out of a 50 ms burst",
        arrived.len()
    );
    let gaps = arrived.windows(2).map(|w| w[1] - w[0]);
    let longest = gaps.fold(arrived.first().copied().unwrap_or(secs), f32::max);
    assert!(longest < 0.6, "no new picture for {longest:.2} s");
}

/// The settings preview (Roger's sixteenth pass, item 2: its motion
/// lagged): an eager renderer shows every frame in the call that asks for
/// it, a budget of nothing notwithstanding, so what shows is never a call
/// behind.
#[test]
fn an_eager_mesh_renderer_shows_each_frame_in_the_call_that_asks() {
    use chonkgene_core::anim::{Expression, PoseParams};
    let Some(mut r) = lone_renderer("shiba", 0.0) else {
        eprintln!("skipped: no GPU");
        return;
    };
    r.finish();
    r.set_eager(true);
    for f in 1..=40 {
        let params = PoseParams {
            clock: f as f32 / 30.0,
            ..Default::default()
        };
        r.render(&params, Expression::Neutral);
        assert!(r.changed(), "frame {f} did not show in its own call");
    }
}

/// Screen x (pixels at 100%) of a bird's left foot over a steady waddle,
/// as the window carries it along, and its height.
fn foot_track(id: &str) -> Vec<(f32, f32)> {
    let (model, sk) = mesh_model(id).unwrap();
    let leg = sk.bone("leg_l").unwrap();
    let hip = sk.pivots[leg];
    let sole = v3(hip.x, -1.05, hip.z + 0.1);
    let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
    let dt = 1.0 / 60.0;
    let mut x = 0.0;
    let mut track = Vec::new();
    for f in 0..300 {
        let joints = walk.step(dt, true, 1.0);
        x += WALK_SPEED * walk.speed() * dt;
        if f >= 120 {
            let p: V3 = sk.pose(&joints)[leg].apply(sole);
            track.push((x + p.x * PPU, p.y));
        }
    }
    track
}

#[test]
fn a_planted_bird_foot_stays_put_while_the_body_waddles_over_it() {
    for id in BIPED_SPECIES {
        let track = foot_track(id);
        let low = track.iter().map(|t| t.1).fold(f32::MAX, f32::min);
        let mut worst = 0.0f32;
        let mut run: Vec<f32> = Vec::new();
        for &(x, y) in track.iter().chain([(0.0, f32::MAX)].iter()) {
            if y < low + 0.02 {
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
        eprintln!("{id}: planted foot slid at most {worst:.2} px");
        assert!(worst < 4.0, "{id}: a planted foot slid {worst:.1} px");
        let travel = track.last().unwrap().0 - track[0].0;
        assert!(travel > 100.0, "{id}: the foot kept up: {travel:.0} px");
    }
}

#[test]
fn a_bird_waddles_rolling_onto_each_planted_leg() {
    for id in BIPED_SPECIES {
        let (model, sk) = mesh_model(id).unwrap();
        let body = sk.bone("body").unwrap();
        let head = sk.bone("head").unwrap();
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        let (mut lo, mut hi, mut head_lo, mut head_hi) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for f in 0..360 {
            let joints = walk.step(1.0 / 60.0, true, 1.0);
            if f >= 120 {
                let roll = joints[body].rot.z;
                lo = lo.min(roll);
                hi = hi.max(roll);
                // The head's own roll cancels most of the body's.
                let h = roll + joints[head].rot.z;
                head_lo = head_lo.min(h);
                head_hi = head_hi.max(h);
            }
        }
        assert!(
            hi > 2.0 && lo < -2.0,
            "{id}: body rolls only {lo:.1}..{hi:.1}"
        );
        assert!(
            head_hi - head_lo < 0.6 * (hi - lo),
            "{id}: the head rolls as much as the body"
        );
    }
}

#[test]
fn idle_gestures_move_every_mesh_pet() {
    use chonkgene_core::anim::Act;
    use chonkgene_render::skin::Gesture;
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
        // At rest standing, and sitting (where the gestures done sitting
        // are measured from).
        let rest_joints = |sitting: bool| {
            let mut rest = Mover::new(&sk, 1.0, &model).unwrap();
            // Fidgets would move it about on their own: compared without.
            rest.still();
            rest.sit(sitting);
            let mut joints = Vec::new();
            for _ in 0..60 {
                joints = rest.step(1.0 / 60.0, false, 1.0);
            }
            joints
        };
        // Every gesture its species does at rest, and eating.
        let species = chonkgene_core::species::Species::from_id(id).unwrap();
        let acts = species
            .habits()
            .iter()
            .chain(species.joys())
            .map(|&(act, _)| act)
            .filter(|&act| chonkgene_core::anim::gesture_secs(act).is_some())
            .chain([Act::Eat]);
        for act in acts {
            let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
            walk.still();
            // Gestures done sitting are seen sitting.
            walk.sit(act.seated());
            let rest_joints = rest_joints(act.seated());
            let mut joints = Vec::new();
            for _ in 0..60 {
                // 0.18 s in: a bite's head right down.
                walk.gesture(Some(Gesture {
                    act,
                    e: 1.0,
                    t: 0.18,
                }));
                joints = walk.step(1.0 / 60.0, false, 1.0);
            }
            // (Tails and wings swish on their own clock, and the fox's
            // bell swings on, so they differ.)
            let off = |a: &[chonkgene_render::skin::Joint], b: &[chonkgene_render::skin::Joint]| {
                (0..a.len())
                    .filter(|&i| !sk.names[i].starts_with("tail") && sk.names[i] != "bell")
                    .map(|i| (a[i].rot - b[i].rot).length())
                    .fold(0.0f32, f32::max)
            };
            let most = off(&joints, &rest_joints);
            assert!(
                most > 10.0,
                "{id}: {act:?} turned no bone by more than {most:.1}°"
            );
            // Over, the pet goes back to rest.
            walk.gesture(None);
            for _ in 0..60 {
                joints = walk.step(1.0 / 60.0, false, 1.0);
            }
            let left = off(&joints, &rest_joints);
            assert!(left < 1.0, "{id}: {act:?} left a bone {left:.1}° off");
        }
    }
}

#[test]
fn asleep_every_mesh_pet_lies_down_and_gets_up_again() {
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
        let head = sk.bone("head").unwrap();
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        // Compared without fidgets: they move the head about.
        walk.still();
        let settle = |walk: &mut Mover, secs: f32| {
            let mut j = Vec::new();
            for _ in 0..(secs * 60.0) as usize {
                j = walk.step(1.0 / 60.0, false, 1.0);
            }
            sk.pose(&j)[head].apply(sk.pivots[head])
        };
        let up = settle(&mut walk, 2.0);
        walk.lie(true);
        let down = settle(&mut walk, 4.0);
        // The head comes down: onto the paws, or tucked into the chest.
        assert!(
            up.y - down.y > 0.08,
            "{id}: the head came down only {:.3}",
            up.y - down.y
        );
        walk.lie(false);
        let again = settle(&mut walk, 4.0);
        assert!((again - up).length() < 0.02, "{id}: did not get up again");
    }
}

#[test]
fn a_mesh_pet_sits_down_after_standing_about() {
    use chonkgene_core::anim::{Expression, PoseParams};
    let Some(mut r) = app_renderer("shiba") else {
        eprintln!("skipped: no GPU");
        return;
    };
    // Measured without fidgets: the head's drift changes the height too.
    r.still();
    // How tall the picture stands: sitting up, the pet is taller.
    let tall = |pm: &Pixmap| {
        let w = pm.width() as usize;
        let d = pm.data();
        let rows: Vec<usize> = (0..pm.height() as usize)
            .filter(|y| (0..w).any(|x| d[(y * w + x) * 4 + 3] > 200))
            .collect();
        rows.last().unwrap() - rows[0]
    };
    let mut at = |clock: f32| {
        let p = PoseParams {
            clock,
            ..Default::default()
        };
        r.render(&p, Expression::Neutral);
        r.finish();
        tall(r.pixmap())
    };
    let mut standing = 0;
    for f in 0..90 {
        let h = at(f as f32 / 30.0);
        if f == 15 {
            standing = h;
        }
    }
    let sitting = at(3.0);
    assert!(
        sitting > standing + 3,
        "standing {standing} px tall, after 3 s still {sitting} px"
    );
}

/// The closest any drawn pixel comes to each edge of the picture (left,
/// top, right, bottom), pixels; full size if nothing is drawn.
fn margins(pm: &Pixmap) -> [u32; 4] {
    let (w, h) = (pm.width(), pm.height());
    let d = pm.data();
    let mut m = [w, h, w, h];
    for y in 0..h {
        for x in 0..w {
            if d[((y * w + x) * 4 + 3) as usize] > 8 {
                m[0] = m[0].min(x);
                m[1] = m[1].min(y);
                m[2] = m[2].min(w - 1 - x);
                m[3] = m[3].min(h - 1 - y);
            }
        }
    }
    m
}

/// Roger saw the fox's tails cut off at the edge of its window: no pet
/// reaches the left, top or right edge of its picture, whatever it does.
/// (Along the bottom the faint edge of the shadow may touch.)
#[test]
fn no_mesh_pet_is_cut_off_by_the_edge_of_its_picture() {
    use chonkgene_core::Zone;
    use chonkgene_core::anim::{Animator, Expression};
    use chonkgene_render::skin::SleepPose;
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let Some(mut r) = app_renderer(id) else {
            eprintln!("skipped: no GPU");
            return;
        };
        let mut a = Animator::new(1, 0.33, 0.0).with_pops(false);
        let mut worst = [u32::MAX; 3];
        let mut what = [""; 3];
        let mut run = |r: &mut chonkgene_render::MeshRenderer,
                       a: &mut Animator,
                       secs: f32,
                       doing: &'static str,
                       e: Expression| {
            for _ in 0..(secs * 10.0) as usize {
                a.step(0.1);
                r.render(&a.params(), e);
                r.finish();
                let m = margins(r.pixmap());
                for i in 0..3 {
                    if m[i] < worst[i] {
                        (worst[i], what[i]) = (m[i], doing);
                    }
                }
            }
        };
        run(&mut r, &mut a, 1.0, "standing", Expression::Neutral);
        a.walking = true;
        run(&mut r, &mut a, 1.5, "walking right", Expression::Neutral);
        a.facing = -1.0;
        run(
            &mut r,
            &mut a,
            1.5,
            "turning and walking left",
            Expression::Neutral,
        );
        a.pace = 4.0;
        a.facing = 1.0;
        run(&mut r, &mut a, 1.0, "zoomies", Expression::Neutral);
        (a.walking, a.pace) = (false, 1.0);
        run(&mut r, &mut a, 3.0, "sitting down", Expression::Neutral);
        a.cheer();
        run(&mut r, &mut a, 1.5, "hopping", Expression::Happy);
        let species = chonkgene_core::species::Species::from_id(id).unwrap();
        for &(g, _) in species.habits() {
            a.gesture(g);
            run(&mut r, &mut a, 3.2, "a gesture", Expression::Neutral);
        }
        for &(g, _) in species.joys() {
            a.gesture(g);
            run(&mut r, &mut a, 3.2, "a joy", Expression::Happy);
        }
        a.eat();
        run(&mut r, &mut a, 3.8, "eating", Expression::Neutral);
        a.facing = -1.0;
        run(&mut r, &mut a, 2.0, "turning sitting", Expression::Neutral);
        run(&mut r, &mut a, 3.0, "lying down", Expression::Asleep);
        // Feelings shown with the whole body, touches, every way of
        // sleeping (Roger, 2026-11-03).
        a.facing = 1.0;
        run(&mut r, &mut a, 3.0, "laughing", Expression::Giggle);
        run(&mut r, &mut a, 3.0, "fed up", Expression::FedUp);
        run(&mut r, &mut a, 4.0, "sulking", Expression::Sulk);
        let (w, h) = (r.pixmap().width() as f32, r.pixmap().height() as f32);
        for zone in [Zone::Belly, Zone::Chin, Zone::Tail, Zone::Paw] {
            a.press(zone, (0.5 * w, 0.6 * h));
            run(&mut r, &mut a, 2.5, "held", Expression::Neutral);
            a.poke(zone);
            run(&mut r, &mut a, 1.0, "let go", Expression::Neutral);
        }
        for pose in SleepPose::ALL {
            r.sleep_in(Some(pose));
            run(&mut r, &mut a, 4.0, "asleep", Expression::Asleep);
            run(&mut r, &mut a, 2.0, "waking", Expression::Neutral);
        }
        for (i, side) in ["left", "top", "right"].iter().enumerate() {
            assert!(
                worst[i] >= 3,
                "{id}: {} px from the {side} edge, {}",
                worst[i],
                what[i]
            );
        }
    }
}

/// Roger saw pets stop dead mid-walk. The window now slows to a stop
/// (chonkgene-core `Body`) as the legs do: walking on and coming to a stop,
/// a paw on the floor stays put.
#[test]
fn a_paw_stays_planted_while_the_pet_slows_to_a_stop() {
    use chonkgene_core::motion::{Area, Body, Motion};
    use chonkgene_core::terrain::Terrain;
    let area = Area {
        x: 0.0,
        y: 0.0,
        w: 4000.0,
        h: 1000.0,
    };
    let terrain = Terrain::flat(area);
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
        let (leg, sole) = match sk.bone("leg_l") {
            Some(leg) => {
                let hip = sk.pivots[leg];
                (leg, v3(hip.x, -1.05, hip.z + 0.1))
            }
            None => (sk.bone("leg_fl").unwrap(), v3(0.3, -1.05, 0.5)),
        };
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        let mut body = Body::on_ground(&terrain, 100.0, (261.0, 192.0), 8.0);
        let dt = 1.0 / 60.0;
        let mut track = Vec::new();
        for f in 0..240 {
            // Off at once, told to stop after two seconds.
            body.motion = if f < 120 { Motion::Walk } else { Motion::Idle };
            body.step(dt, &terrain);
            let joints = walk.step(dt, body.motion == Motion::Walk, 1.0);
            // Till both the window and the legs stand still (turning to
            // face you after, the paws pivot on the spot).
            if f >= 60 && (body.moving_on() || walk.speed() > 0.0) {
                let p: V3 = sk.pose(&joints)[leg].apply(sole);
                track.push((body.pos.0 + p.x * PPU, p.y));
            }
        }
        let low = track.iter().map(|t| t.1).fold(f32::MAX, f32::min);
        let mut worst = 0.0f32;
        let mut run: Vec<f32> = Vec::new();
        for &(x, y) in track.iter().chain([(0.0, f32::MAX)].iter()) {
            if y < low + 0.02 {
                run.push(x);
            } else {
                if run.len() > 3 {
                    let (a, b) = run
                        .iter()
                        .fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
                    worst = worst.max(b - a);
                }
                run.clear();
            }
        }
        eprintln!("{id}: a planted paw slid at most {worst:.2} px");
        assert!(worst < 3.0, "{id}: a planted paw slid {worst:.1} px");
    }
}

/// Roger hardly saw pets sit: every gesture stood them up. A sitting pet
/// yawns sitting; a stretch (a play bow) still stands it up.
#[test]
fn a_sitting_pet_yawns_sitting_but_stands_to_stretch() {
    use chonkgene_core::anim::{Act, Expression, PoseParams, gesture_secs};
    let Some(mut r) = app_renderer("shiba") else {
        eprintln!("skipped: no GPU");
        return;
    };
    let tall = |pm: &Pixmap| {
        let w = pm.width() as usize;
        let d = pm.data();
        let rows: Vec<usize> = (0..pm.height() as usize)
            .filter(|y| (0..w).any(|x| d[(y * w + x) * 4 + 3] > 200))
            .collect();
        rows.last().unwrap() - rows[0]
    };
    let mut clock = 0.0;
    let mut hold = |r: &mut chonkgene_render::MeshRenderer, act: Act, secs: f32| {
        let mut least = usize::MAX;
        for _ in 0..(secs * 30.0) as usize {
            clock += 1.0 / 30.0;
            let p = PoseParams {
                clock,
                act,
                ..Default::default()
            };
            r.render(&p, Expression::Neutral);
            r.finish();
            least = least.min(tall(r.pixmap()));
        }
        least
    };
    hold(&mut r, Act::Rest, 3.0);
    let sitting = hold(&mut r, Act::Rest, 0.5);
    let yawning = hold(&mut r, Act::Yawn, gesture_secs(Act::Yawn).unwrap());
    hold(&mut r, Act::Rest, 2.0);
    let stretching = hold(&mut r, Act::Stretch, gesture_secs(Act::Stretch).unwrap());
    assert!(
        yawning + 1 >= sitting,
        "yawning, {yawning} px tall against {sitting} sitting"
    );
    assert!(
        stretching + 6 < sitting,
        "stretching, still {stretching} px tall against {sitting} sitting"
    );
}

fn settle(walk: &mut Mover, secs: f32) -> Vec<chonkgene_render::skin::Joint> {
    let mut joints = Vec::new();
    for _ in 0..(secs * 60.0) as usize {
        joints = walk.step(1.0 / 60.0, false, 1.0);
    }
    joints
}

/// Roger: the fox slept like it stood. Asleep, its nine tails fold forward
/// over its back like a blanket: their tips come forward to the hips, where
/// standing they reach out far behind.
#[test]
fn a_sleeping_fox_folds_its_tails_over_itself() {
    let (model, sk) = quad("fox").unwrap();
    let hips = sk.bone("hips").unwrap();
    let tips = |lying: bool| {
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        walk.lie(lying);
        let joints = settle(&mut walk, 5.0);
        let pose = sk.pose(&joints);
        let unturn = M3::rot_y(-walk.yaw().to_radians());
        let chains = sk.tails();
        let z: f32 = chains
            .iter()
            .map(|c| {
                let tip = *c.last().unwrap();
                unturn.apply(pose[tip].apply(sk.pivots[tip])).z
            })
            .sum::<f32>()
            / chains.len() as f32;
        z - unturn.apply(pose[hips].apply(sk.pivots[hips])).z
    };
    let (standing, lying) = (tips(false), tips(true));
    assert!(
        lying - standing > 0.6,
        "tips {standing:.2} from the hips standing, {lying:.2} asleep"
    );
    assert!(
        lying > -0.3,
        "asleep, tips still {lying:.2} behind the hips"
    );
}

/// Roger: the chick and duckling slept like they stood. Asleep they sink
/// their heads into their feathers and lay them over on a shoulder; the
/// owl, which already read as asleep, stays upright.
#[test]
fn sleeping_chicks_and_ducklings_tuck_their_heads_in() {
    for id in BIPED_SPECIES {
        let (model, sk) = mesh_model(id).unwrap();
        let head = sk.bone("head").unwrap();
        let held = |lying: bool| {
            let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
            if lying {
                walk.lie(true);
            } else {
                walk.sit(true);
            }
            let joints = settle(&mut walk, 5.0);
            let y = sk.pose(&joints)[head].apply(sk.pivots[head]).y;
            (y, joints[head].rot.z.abs())
        };
        let ((sitting, _), (lying, lean)) = (held(false), held(true));
        if model.gait.tuck > 0.0 {
            assert!(
                sitting - lying > 0.12,
                "{id}: head only {:.2} lower asleep",
                sitting - lying
            );
            assert!(lean > 15.0, "{id}: head leans only {lean:.0}° asleep");
        } else {
            assert!(lean < 5.0, "{id}: head leans {lean:.0}° asleep");
        }
    }
}

/// Roger: the owl's face went misshapen when it turned its head to him.
/// Its head is melted into its body, so a head turning alone slides the
/// face across the body's outline: the body takes most of the turn.
#[test]
fn the_owl_turns_its_body_with_its_head() {
    let (model, sk) = mesh_model("owl").unwrap();
    let (head, body) = (sk.bone("head").unwrap(), sk.bone("body").unwrap());
    let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
    walk.sit(true);
    walk.look(35.0);
    let joints = settle(&mut walk, 3.0);
    let (h, b) = (joints[head].rot.y, joints[body].rot.y);
    assert!(h + b > 25.0, "turned only {:.0}° in all", h + b);
    assert!(b > 1.5 * h, "head {h:.0}°, body only {b:.0}°");
}

/// Roger, 2026-11-03: chin scratches and tail pulls. Every mesh pet has
/// skin that answers to its chin and to its tail.
#[test]
fn every_mesh_pet_has_a_chin_and_a_tail_to_touch() {
    let zone = |name: &str| {
        chonkgene_rig::ZONES
            .iter()
            .position(|z| *z == name)
            .unwrap() as u8
    };
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
        let mesh = bake(&model, &sk, 0, 0.33, model.bake_cell).unwrap();
        let count = |z: u8| mesh.zones.iter().filter(|&&v| v == z).count();
        let (chin, tail) = (count(zone("chin")), count(zone("tail")));
        // The owl's flat face has hardly any chin to speak of.
        let least = if *id == "owl" { 1 } else { 100 };
        assert!(chin >= least, "{id}: {chin} chin vertices");
        assert!(tail >= 100, "{id}: {tail} tail vertices");
    }
}

/// Roger's sixteenth pass, item 4: each zone gets its own reaction, so
/// every mesh pet has a back apart from its bottom, a belly, and the birds
/// wings apart from their feet; the bottom is the rump, not the body.
#[test]
fn every_mesh_pet_has_a_back_a_rump_and_birds_have_wings() {
    let zone = |name: &str| {
        chonkgene_rig::ZONES
            .iter()
            .position(|z| *z == name)
            .unwrap() as u8
    };
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
        let mesh = bake(&model, &sk, 0, 0.33, model.bake_cell).unwrap();
        let count = |z: &str| mesh.zones.iter().filter(|&&v| v == zone(z)).count();
        let (back, bottom, belly, wing) = (
            count("back"),
            count("bottom"),
            count("belly"),
            count("wing"),
        );
        assert!(back >= 200, "{id}: {back} back vertices");
        assert!(bottom >= 50, "{id}: {bottom} bottom vertices");
        assert!(belly >= 200, "{id}: {belly} belly vertices");
        assert!(
            bottom < back,
            "{id}: the bottom ({bottom}) is bigger than the back ({back})"
        );
        let bird = BIPED_SPECIES.contains(id);
        assert_eq!(wing > 100, bird, "{id}: {wing} wing vertices");
        // The back faces up, the belly down, on average.
        let mean_up = |z: &str| {
            let v: Vec<f32> = mesh
                .vertices
                .iter()
                .zip(&mesh.zones)
                .filter(|(_, k)| **k == zone(z))
                .map(|(v, _)| v.normal[1])
                .collect();
            v.iter().sum::<f32>() / v.len().max(1) as f32
        };
        assert!(
            mean_up("back") > mean_up("belly"),
            "{id}: the back faces lower than the belly"
        );
    }
}

fn feel(mood: chonkgene_render::skin::Mood) -> chonkgene_render::skin::Feel {
    chonkgene_render::skin::Feel {
        mood,
        ..Default::default()
    }
}

/// PLAN §1.2: a touch turns the pet to face you and look at the finger.
#[test]
fn a_touched_pet_turns_to_you_and_looks_at_the_finger() {
    use chonkgene_core::Zone;
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        settle(&mut walk, 3.0);
        let before = walk.yaw();
        walk.feel(chonkgene_render::skin::Feel {
            gaze: Some((25.0, 0.0)),
            held: Some((Zone::Head, None, 0.1)),
            ..Default::default()
        });
        let joints = settle(&mut walk, 0.4);
        assert!(
            walk.yaw().abs() < 12.0,
            "{id}: still turned {:.0}° (was {before:.0}°) after 0.4 s",
            walk.yaw()
        );
        let head = sk.bone("head").unwrap();
        let body = sk.bone("body").unwrap();
        let turn = joints[head].rot.y + joints[body].rot.y;
        // The birds look round less, and more slowly (Roger's fourteenth
        // pass: their heads seemed about to snap off).
        let least = if BIPED_SPECIES.contains(id) {
            6.0
        } else {
            12.0
        };
        assert!(
            turn > least,
            "{id}: head turned only {turn:.0}° to the finger"
        );
    }
}

/// Roger: cats, dogs and the fox slept with their heads up. Lying chin
/// down or curled up, the nose comes down below 60% of its height standing
/// (lying down before, it stayed at two thirds); on its side or back, the
/// body is rolled over by most of a quarter turn or more.
#[test]
fn four_legged_pets_sleep_with_their_heads_down() {
    use chonkgene_render::skin::SleepPose;
    for id in QUAD_SPECIES {
        let (model, sk) = mesh_model(id).unwrap();
        let nose = &model.parts[model.part("nose").unwrap()];
        let body = sk.bone("body").unwrap();
        let floor = chonkgene_rig::FLOOR;
        let nose_up = |joints: &[chonkgene_render::skin::Joint]| {
            sk.pose(joints)[nose.bone].apply(nose.at).y - floor
        };
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        let standing = nose_up(&settle(&mut walk, 2.0));
        let weights = chonkgene_render::skin::SleepPose::weights(id);
        for (k, pose) in SleepPose::ALL.into_iter().enumerate() {
            if weights[k] == 0 {
                continue;
            }
            let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
            walk.sleep_pose(pose);
            walk.lie(true);
            let joints = settle(&mut walk, 6.0);
            match pose {
                SleepPose::Chin | SleepPose::Curl => {
                    let up = nose_up(&joints);
                    assert!(
                        up < 0.6 * standing,
                        "{id} {pose:?}: nose {up:.2} up, {standing:.2} standing"
                    );
                }
                SleepPose::Side | SleepPose::BellyUp => {
                    let back = sk.pose(&joints)[body].m.apply(v3(0.0, 1.0, 0.0));
                    let tipped = back.y.clamp(-1.0, 1.0).acos().to_degrees();
                    let least = if pose == SleepPose::Side { 70.0 } else { 140.0 };
                    assert!(tipped > least, "{id} {pose:?}: rolled only {tipped:.0}°");
                }
            }
        }
    }
}

/// Laughing, a four-legged pet rolls onto its back, paws in the air; the
/// fox, whose tails would prop it up, laughs sitting.
#[test]
fn laughing_rolls_a_pet_onto_its_back() {
    use chonkgene_render::skin::Mood;
    for id in QUAD_SPECIES {
        let (model, sk) = mesh_model(id).unwrap();
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        walk.feel(feel(Mood::Laugh));
        let joints = settle(&mut walk, 3.0);
        let pose = sk.pose(&joints);
        let at = |name: &str| {
            let b = sk.bone(name).unwrap();
            pose[b].apply(sk.pivots[b] + v3(0.0, -0.4, 0.0)).y
        };
        let body = sk.bone("body").unwrap();
        let middle = pose[body].apply(sk.pivots[body]).y;
        let paws = (at("leg_fl") + at("leg_fr") + at("leg_bl") + at("leg_br")) / 4.0;
        if id == "fox" {
            assert!(
                paws < middle,
                "fox: paws {paws:.2} over the body {middle:.2}"
            );
        } else {
            assert!(
                paws > middle + 0.2,
                "{id}: paws {paws:.2}, the body {middle:.2}"
            );
        }
    }
}

/// A finger pressing in dents the flesh under it (mesh.wgsl), and only
/// there.
#[test]
fn a_finger_dents_the_flesh_under_it() {
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
    let (w, h) = chonkgene_render::mesh_pixel_size(&model, 1.0);
    let mut canvas = MeshCanvas::new(&gpu, &mesh, w, h, 1.0);
    let bones = sk.pose(&[]);
    let draw = |canvas: &mut MeshCanvas| {
        let mut pm = Pixmap::new(w, h).unwrap();
        canvas.draw(&gpu, &scene, &bones, 0.0, &mut pm).unwrap();
        pm
    };
    let flat = draw(&mut canvas);
    // The front of the belly.
    let belly = chonkgene_rig::ZONES
        .iter()
        .position(|z| *z == "belly")
        .unwrap() as u8;
    let front = (0..mesh.vertices.len())
        .filter(|&i| mesh.zones[i] == belly)
        .max_by(|&a, &b| mesh.vertices[a].pos[2].total_cmp(&mesh.vertices[b].pos[2]))
        .unwrap();
    let p = mesh.vertices[front].pos;
    let at = v3(p[0], p[1], p[2]);
    canvas.press = [
        Some(chonkgene_render::mesh_gpu::Press {
            at,
            dir: canvas.forward(),
            depth: 0.13,
            radius: 0.32,
        }),
        None,
    ];
    let dented = draw(&mut canvas);
    let (px, py) = canvas.project(at);
    let changed = |near: bool| {
        let mut n = 0;
        for y in 0..h {
            for x in 0..w {
                let d = (x as f32 - px).hypot(y as f32 - py);
                if (d < 15.0) != near {
                    continue;
                }
                let i = ((y * w + x) * 4) as usize;
                if (0..3).any(|k| flat.data()[i + k].abs_diff(dented.data()[i + k]) > 10) {
                    n += 1;
                }
            }
        }
        n
    };
    let (near, far) = (changed(true), changed(false));
    assert!(near > 60, "the dent changed {near} pixels round the finger");
    // A 0.32-unit dent is under 19 px across at 100%: nothing beyond 30 px.
    let far_off = {
        let mut n = 0;
        for y in 0..h {
            for x in 0..w {
                if (x as f32 - px).hypot(y as f32 - py) < 30.0 {
                    continue;
                }
                let i = ((y * w + x) * 4) as usize;
                if (0..4).any(|k| flat.data()[i + k] != dented.data()[i + k]) {
                    n += 1;
                }
            }
        }
        n
    };
    assert_eq!(
        far_off, 0,
        "{far} pixels changed, {far_off} far from the finger"
    );
}

/// Held, the flesh stays pressed in; let go, it springs back out past
/// flat, wobbles and settles flat again.
#[test]
fn pressed_flesh_springs_back_when_let_go() {
    use chonkgene_core::Zone;
    use chonkgene_core::anim::{Animator, Expression};
    let Some(mut r) = app_renderer("shiba") else {
        eprintln!("skipped: no GPU");
        return;
    };
    let mut a = Animator::new(1, 0.33, 0.35).with_pops(false);
    let frames = |r: &mut chonkgene_render::MeshRenderer, a: &mut Animator, secs: f32| {
        let mut depths = Vec::new();
        for _ in 0..(secs * 30.0) as usize {
            a.step(1.0 / 30.0);
            r.render(&a.params(), Expression::Neutral);
            r.finish();
            depths.push(r.pressed_in());
        }
        depths
    };
    frames(&mut r, &mut a, 1.0);
    let (x, y) = r.anchor();
    let at = (0..80)
        .map(|d| (x, y + d as f32))
        .find(|&(x, y)| r.hit(x, y).is_some())
        .expect("the pet under its crown");
    a.press(Zone::Head, at);
    let held = frames(&mut r, &mut a, 0.5);
    assert!(
        held.last().is_some_and(|&d| d > 0.04),
        "held, pressed in {:?}",
        held.last()
    );
    a.poke(Zone::Head);
    let after = frames(&mut r, &mut a, 2.5);
    let out = after.iter().copied().fold(0.0f32, f32::min);
    assert!(out < -0.01, "let go, it sprang out only {out:.3}");
    assert_eq!(after.last(), Some(&0.0), "still pressed in at the end");
}

/// Roger, 2026-11-04: whatever a pet is doing (sitting, idling, smiling,
/// looking back at you), it must not stop dead. Over a minute in each held
/// pose, something besides the tail (which sways anyway) moves at least
/// every 1.5 s; and without fidgets it would not.
#[test]
fn no_mesh_pet_ever_stops_dead() {
    use chonkgene_core::Zone;
    use chonkgene_render::skin::{Feel, Mood};
    const DT: f32 = 1.0 / 30.0;
    const WINDOW: usize = (1.5 / DT) as usize;
    let poses: [(&str, bool, Feel); 6] = [
        ("standing", false, Feel::default()),
        ("sitting", true, Feel::default()),
        (
            "pleased",
            true,
            Feel {
                mood: Mood::Content,
                ..Feel::default()
            },
        ),
        (
            "fed up",
            false,
            Feel {
                mood: Mood::FedUp,
                ..Feel::default()
            },
        ),
        (
            "sulking",
            false,
            Feel {
                mood: Mood::Sulk,
                ..Feel::default()
            },
        ),
        (
            "looking back",
            false,
            Feel {
                gaze: Some((0.0, 0.0)),
                held: Some((Zone::Tail, None, 2.0)),
                ..Feel::default()
            },
        ),
    ];
    // The most any bone but the root and the tails turned (degrees) or
    // shifted (a hundredth of a world unit counting as a degree) over
    // each 1.5 s, at the least.
    let stillest = |id: &str, sitting: bool, feel: Feel, fidgets: bool| -> f32 {
        let (model, sk) = mesh_model(id).unwrap();
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        walk.set_fidgets(id, 0.5, 0x5EED);
        if !fidgets {
            walk.still();
        }
        walk.sit(sitting);
        walk.feel(feel);
        let moving: Vec<usize> = (1..sk.names.len())
            .filter(|&b| !sk.names[b].starts_with("tail"))
            .collect();
        let mut frames = Vec::new();
        for k in 0..(62.0 / DT) as usize {
            let j = walk.step(DT, false, 1.0);
            // Settled into the pose first.
            if k as f32 * DT > 2.0 {
                frames.push(j);
            }
        }
        frames
            .windows(WINDOW)
            .map(|w| {
                moving
                    .iter()
                    .map(|&b| {
                        let span = |f: &dyn Fn(&chonkgene_render::skin::Joint) -> f32| {
                            let (lo, hi) = w
                                .iter()
                                .map(|j| f(&j[b]))
                                .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
                            hi - lo
                        };
                        [
                            span(&|j| j.rot.x),
                            span(&|j| j.rot.y),
                            span(&|j| j.rot.z),
                            100.0 * span(&|j| j.shift.x),
                            100.0 * span(&|j| j.shift.y),
                            100.0 * span(&|j| j.shift.z),
                        ]
                        .into_iter()
                        .fold(0.0f32, f32::max)
                    })
                    .fold(0.0f32, f32::max)
            })
            .fold(f32::MAX, f32::min)
    };
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        for (name, sitting, feel) in poses {
            let least = stillest(id, sitting, feel, true);
            assert!(least > 3.0, "{id} {name}: only {least:.2}° in 1.5 s");
        }
        let without = stillest(id, true, Feel::default(), false);
        assert!(without < 3.0, "{id} moves {without:.2}° without fidgets");
    }
}

/// Each pet fidgets its own way: the ears of those that have them twitch,
/// the fox's bell swings, a bird's wings shift, and the tongue comes out
/// for the Shiba (panting) and the Munchkin (a blep).
#[test]
fn every_species_fidgets_its_own_way() {
    use chonkgene_render::fidget::Tongue;
    const DT: f32 = 1.0 / 30.0;
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        walk.set_fidgets(id, 0.5, 0xF1D6);
        walk.sit(true);
        let watch: Vec<usize> = ["ear_l", "ear_r", "bell", "wing_l", "wing_r"]
            .iter()
            .filter_map(|n| sk.bone(n))
            .collect();
        let mut most = vec![0.0f32; watch.len()];
        let mut tongues = Vec::new();
        let mut rest = None;
        for k in 0..(120.0 / DT) as usize {
            let j = walk.step(DT, false, 1.0);
            if let Some(t) = walk.fidget().tongue()
                && !tongues.contains(&t)
            {
                tongues.push(t);
            }
            if k == (2.0 / DT) as usize {
                rest = Some(j.clone());
            }
            if let Some(r) = &rest {
                for (m, &b) in most.iter_mut().zip(&watch) {
                    *m = m.max((j[b].rot - r[b].rot).length());
                }
            }
        }
        for (m, &b) in most.iter().zip(&watch) {
            assert!(*m > 8.0, "{id}: {} only moved {m:.1}°", sk.names[b]);
        }
        let want: &[Tongue] = match *id {
            "shiba" => &[Tongue::Pant],
            "munchkin" | "fold" | "fox" => &[Tongue::Blep],
            _ => &[],
        };
        assert_eq!(tongues, want, "{id}");
        if QUAD_SPECIES.contains(id) {
            assert!(
                sk.bone("ear_l").is_some() && sk.bone("ear_r").is_some(),
                "{id} ears"
            );
        }
    }
    let (_, fox) = mesh_model("fox").unwrap();
    assert!(fox.bone("bell").is_some());
}

/// Roger, 2026-11-05: a smiling pet, and one in its scene, still looked
/// still, and the Shiba chewing its bone held stiller than anything: during
/// a gesture only the tail fidgeted, and a held scene pose had only the
/// ordinary fidgets. Lively (pleased or at play), the tail, ears and wings
/// keep going all the while, through a gesture as much as between them:
/// in every second of a long one they swing well over what they did.
#[test]
fn a_lively_pet_keeps_tail_ears_and_wings_going_through_gestures() {
    use chonkgene_core::anim::Act;
    use chonkgene_render::skin::Gesture;
    const DT: f32 = 1.0 / 30.0;
    const WINDOW: usize = (1.0 / DT) as usize;
    // The least, over every second, of the most any watched bone turned.
    let stillest = |id: &str, lively: bool| -> f32 {
        let (model, sk) = mesh_model(id).unwrap();
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        walk.set_fidgets(id, 0.5, 0x11FE);
        let quad = QUAD_SPECIES.contains(&id);
        // Lying gnawing at a bone, or a bird pecking about.
        let act = if quad { Act::Chew } else { Act::Peck };
        walk.rest(quad);
        let watch: Vec<usize> = (0..sk.names.len())
            .filter(|&b| {
                let n = &sk.names[b];
                n.starts_with("tail") || n.starts_with("ear_") || n.starts_with("wing_")
            })
            .collect();
        let mut frames = Vec::new();
        for k in 0..(30.0 / DT) as usize {
            let t = k as f32 * DT;
            walk.set_mood(lively, false);
            walk.gesture(Some(Gesture { act, e: 1.0, t }));
            let j = walk.step(DT, false, 1.0);
            if t > 2.0 {
                frames.push(j);
            }
        }
        frames
            .windows(WINDOW)
            .map(|w| {
                watch
                    .iter()
                    .map(|&b| {
                        let r0 = w[0][b].rot;
                        w.iter()
                            .map(|j| (j[b].rot - r0).length())
                            .fold(0.0, f32::max)
                    })
                    .fold(0.0f32, f32::max)
            })
            .fold(f32::MAX, f32::min)
    };
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (before, after) = (stillest(id, false), stillest(id, true));
        println!("{id}: {before:.1}° → {after:.1}°");
        assert!(after > 12.0, "{id}: only {after:.1}° in a second, lively");
        assert!(after > 2.0 * before, "{id}: {before:.1}° → {after:.1}°");
    }
}

/// Roger: the dog should stick its tongue out too. Out of breath (after a
/// run) or lively, the Shiba pants, tongue out, all the while, walking
/// too; the cats and the fox do not. And a snack done, a pet licks its
/// nose.
#[test]
fn a_dog_pants_after_a_run_and_a_pet_licks_its_nose_after_a_snack() {
    use chonkgene_core::anim::{Act, EAT_SECS};
    use chonkgene_render::fidget::{Kind, Tongue};
    use chonkgene_render::skin::Gesture;
    const DT: f32 = 1.0 / 30.0;
    // Frames it takes to start panting.
    const SETTLE: usize = 30;
    for id in QUAD_SPECIES {
        let (model, sk) = mesh_model(id).unwrap();
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        walk.set_fidgets(id, 0.5, 0x7A57);
        let mut panting: usize = 0;
        let n = (10.0 / DT) as usize;
        for k in 0..n {
            walk.set_mood(false, true);
            walk.step(DT, k < n / 2, 1.0);
            if k >= SETTLE && walk.fidget().tongue() == Some(Tongue::Pant) {
                panting += 1;
            }
        }
        let want = if id == "shiba" { n - SETTLE } else { 0 };
        assert_eq!(panting, want, "{id}: panting frames");
        // A snack, then a lick.
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        walk.set_fidgets(id, 0.5, 0x7A57);
        let mut t = 0.0;
        while t < EAT_SECS {
            walk.gesture(Some(Gesture {
                act: Act::Eat,
                e: 1.0,
                t,
            }));
            walk.step(DT, false, 1.0);
            t += DT;
        }
        walk.gesture(None);
        walk.step(DT, false, 1.0);
        assert_eq!(
            walk.fidget().run.map(|r| r.kind),
            Some(Kind::Lick),
            "{id} after its snack"
        );
        assert_eq!(walk.fidget().tongue(), Some(Tongue::Blep), "{id}");
    }
}

/// Roger's fourteenth pass: the chick and the duckling, turning to look
/// at a finger scratching them and back when it left, looked as if their
/// heads might snap off. Scratched by a finger rubbing to one side, then
/// let go, the head never swings round on its own (the body turns with
/// it), and the whole look never jumps from one frame to the next.
#[test]
fn a_scratched_bird_turns_with_its_body_and_settles_softly() {
    use chonkgene_core::Zone;
    for id in ["chick", "duck"] {
        let (model, sk) = mesh_model(id).unwrap();
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        walk.still();
        settle(&mut walk, 2.0);
        let head = sk.bone("head").unwrap();
        let body = sk.bone("body").unwrap();
        // The most the head turns on its own, and the look as a whole, from
        // one frame to the next (degrees).
        let (mut alone, mut jump, mut last) = (0.0_f32, 0.0_f32, None::<(f32, f32)>);
        for k in 0..180 {
            // A finger off to one side rubs back and forth for a second
            // and a half, then lets go.
            let touched = k < 90;
            let rub = 12.0 * (std::f32::consts::TAU * 3.0 * k as f32 / 60.0).sin();
            walk.feel(chonkgene_render::skin::Feel {
                gaze: touched.then_some((25.0 + rub, 10.0)),
                held: touched.then_some((Zone::Head, None, k as f32 / 60.0)),
                scratch: if touched { 1.0 } else { 0.0 },
                ..Default::default()
            });
            let j = walk.step(1.0 / 60.0, false, 1.0);
            let now = (j[head].rot.y, j[head].rot.y + j[body].rot.y + walk.yaw());
            if let Some(was) = last {
                alone = alone.max((now.0 - was.0).abs());
                jump = jump.max((now.1 - was.1).abs());
            }
            last = Some(now);
        }
        // Before: 3.05° and 5.08° in the frame after the finger left.
        assert!(
            alone < 1.2,
            "{id}: the head turned {alone:.2}° alone in a frame"
        );
        assert!(jump < 1.2, "{id}: the look turned {jump:.2}° in a frame");
    }
}

/// Roger's fourteenth pass: some pets' motion suddenly went sluggish. Every
/// species through every action, asking for frames as the app does (the
/// animator's rate, or the renderer's own if higher): no part of the body
/// moves so far between two frames that it jerks. The pose alone is worked
/// out here (the mesh never lands, so nothing is drawn on the GPU); what
/// the GPU budget does to the rate is in the app's `frames:` log lines.
#[test]
fn every_action_asks_for_frames_enough_to_move_smoothly() {
    use chonkgene_core::anim::{Act, Animator, Expression};
    const ACTS: [Act; 21] = [
        Act::Stretch,
        Act::Yawn,
        Act::Wave,
        Act::Eat,
        Act::Dig,
        Act::Scratch,
        Act::Groom,
        Act::Shake,
        Act::Preen,
        Act::Peck,
        Act::Flap,
        Act::Wiggle,
        Act::Bounce,
        Act::Spin,
        Act::Knead,
        Act::Paw,
        Act::Tap,
        Act::Tilt,
        Act::Sniff,
        Act::Rub,
        Act::Bliss,
    ];
    #[derive(Clone, Copy, Debug)]
    enum Doing {
        Rest,
        Walk,
        Gesture(Act),
        Face(Expression),
        Lively,
        Cheer,
        Pounce,
    }
    let mut all: Vec<Doing> = vec![
        Doing::Rest,
        Doing::Walk,
        Doing::Lively,
        Doing::Cheer,
        Doing::Pounce,
    ];
    all.extend(ACTS.map(Doing::Gesture));
    all.extend(
        [
            Expression::Happy,
            Expression::Giggle,
            Expression::FedUp,
            Expression::Sulk,
            Expression::Asleep,
        ]
        .map(Doing::Face),
    );
    // Frames asked for too slowly for how far the body moved: more than
    // `JERK` pixels at well under 60 a second, while it was already moving
    // the frame before (the first frame of a sudden move, a peck from
    // standing still, was booked before anything moved: nothing to go by).
    const JERK: f32 = 10.0;
    let mut slow = Vec::new();
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        // One renderer for the species (each new one bakes its mesh on a
        // thread of its own, which would load the machine for the timed
        // tests running alongside), at rest a while between actions.
        let Some(mut r) = app_renderer(id) else {
            eprintln!("skipped: no GPU");
            return;
        };
        r.still();
        let mut clock = 0.0_f32;
        for doing in &all {
            let mut anim = Animator::new(7, 0.33, 0.0);
            let mut t = -1.5_f32;
            let mut fps = 30;
            let mut jerks = Vec::new();
            let mut was = 0.0_f32;
            while t < 4.0 {
                if (0.0..0.05).contains(&t) {
                    match *doing {
                        Doing::Rest => {}
                        Doing::Walk => anim.walking = true,
                        Doing::Gesture(act) => anim.gesture(act),
                        Doing::Face(e) => anim.expression = e,
                        Doing::Lively => anim.lively = true,
                        Doing::Cheer => anim.cheer(),
                        Doing::Pounce => anim.pounce(),
                    }
                }
                let dt = 1.0 / fps as f32;
                anim.step(dt);
                t += dt;
                clock += dt;
                // The renderer's clock runs on across the actions.
                let params = chonkgene_core::anim::PoseParams {
                    clock,
                    ..anim.params()
                };
                if anim.expression == Expression::Asleep {
                    r.render_asleep(&params, t);
                } else {
                    r.render(&params, anim.expression);
                }
                let step = r.pose_step();
                if t > 0.0 && step > JERK && was > 0.5 * JERK && fps < 50 {
                    jerks.push(format!("{step:.1} px at {fps} fps, {t:.2} s"));
                }
                was = step;
                fps = anim.fps(false).max(r.min_fps());
            }
            if !jerks.is_empty() {
                slow.push(format!("{id} {doing:?}: {}", jerks.join("; ")));
            }
        }
    }
    assert!(slow.is_empty(), "jerky:\n{}", slow.join("\n"));
}

/// Hugged, your hands press in on its sides: its body is narrower between
/// them (dented and squeezed); let go, it springs back as it was.
#[test]
fn hug_hands_press_in_on_its_sides_and_let_go() {
    use chonkgene_core::anim::{Animator, Expression};
    let Some(mut r) = app_renderer("shiba") else {
        eprintln!("skipped: no GPU");
        return;
    };
    let mut a = Animator::new(1, 0.33, 0.35).with_pops(false);
    let run = |r: &mut chonkgene_render::MeshRenderer, a: &mut Animator, secs: f32| {
        for _ in 0..(secs * 30.0) as usize {
            a.step(1.0 / 30.0);
            r.render(&a.params(), Expression::Neutral);
            r.finish();
        }
    };
    // Its body's width on the belly's row: opaque pixels from the belly
    // out to each side.
    let width = |r: &chonkgene_render::MeshRenderer, (x, y): (f32, f32)| {
        let pm = r.pixmap();
        let solid = |x: i32| {
            x >= 0
                && (x as u32) < pm.width()
                && pm
                    .pixel(x as u32, y as u32)
                    .is_some_and(|p| p.alpha() >= 128)
        };
        let (mut l, mut rt) = (x as i32, x as i32);
        while solid(l - 1) {
            l -= 1;
        }
        while solid(rt + 1) {
            rt += 1;
        }
        rt - l
    };
    r.still();
    run(&mut r, &mut a, 1.5);
    let (belly, _) = r.body_at().expect("its body in the picture");
    let before = width(&r, belly);
    let left = r.body_edge(belly.1, -1.0).expect("its left side");
    let right = r.body_edge(belly.1, 1.0).expect("its right side");
    for _ in 0..30 {
        r.hug_press(Some(([(left + 3.0, belly.1), (right - 3.0, belly.1)], 1.0)));
        run(&mut r, &mut a, 1.0 / 30.0);
    }
    assert!(r.hands_in() > 0.05, "pressed in {}", r.hands_in());
    let held = width(&r, belly);
    assert!(held + 4 <= before, "hugged {held} px wide, {before} before");
    r.hug_press(None);
    run(&mut r, &mut a, 2.0);
    assert_eq!(r.hands_in(), 0.0, "let go, the dents settle");
    let after = width(&r, belly);
    assert!(
        after.abs_diff(before) <= 1,
        "let go {after} px wide, {before} before"
    );
}
