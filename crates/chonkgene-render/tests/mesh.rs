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

#[test]
fn sitting_brings_the_bottom_down_and_keeps_the_front_paws_planted() {
    for id in QUAD_SPECIES {
        let (_, sk) = quad(id).unwrap();
        let (leg, hips) = (sk.bone("leg_fl").unwrap(), sk.bone("hips").unwrap());
        let paw = v3(0.3, -1.05, 0.5);
        let bottom = v3(0.0, -0.78, -0.48);
        let mut walk = Walk::new(&sk, 1.0).unwrap();
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
fn mesh_frames_wait_for_the_gpu_budget_instead_of_overspending_it() {
    use chonkgene_core::anim::{Expression, PoseParams};
    // A share of nothing: only what the budget starts with (a burst of
    // 50 ms of GPU time) can pay for frames, and frames are guessed to
    // cost at least 4 ms each, so a dozen at most, however many are asked.
    let Some(mut r) = lone_renderer("shiba", 0.0) else {
        eprintln!("skipped: no GPU");
        return;
    };
    r.finish();
    assert!(r.ready());
    let mut drawn = 0;
    for f in 1..=60 {
        let params = PoseParams {
            clock: f as f32 / 30.0,
            ..Default::default()
        };
        r.render(&params, Expression::Neutral);
        drawn += usize::from(r.changed());
        // Time for a frame on its way to land.
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    assert!(drawn <= 13, "{drawn} frames came out of a 50 ms burst");
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
