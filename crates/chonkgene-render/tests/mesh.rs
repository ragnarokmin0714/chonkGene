//! The skinned-mesh spike: the baked mesh looks like the ray-marched pet,
//! its weights hold together, and the four-legged walk plants its feet.

use chonkgene_render::gpu::Gpu;
use chonkgene_render::mesh::bake;
use chonkgene_render::mesh_gpu::{MAX_BONES, MeshCanvas};
use chonkgene_render::raymarch;
use chonkgene_render::skin::{
    BIPED_SPECIES, QUAD_SPECIES, Skeleton, Walk, human, human_model, mesh_model, quad,
};
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
    let (model, sk) = quad("fox").unwrap();
    lands_in_the_same_place(name, &model, &sk, strip);
}

fn lands_in_the_same_place(
    name: &str,
    model: &Model,
    sk: &Skeleton,
    strip: fn(&mut chonkgene_rig::Model),
) {
    let gpu = match Gpu::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    };
    let mut bare = model.clone();
    strip(&mut bare);
    let (mesh_with, ray_with) = draw_both(&gpu, model, sk);
    let (mesh_bare, ray_bare) = draw_both(&gpu, &bare, sk);
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
fn the_fox_marks_land_in_the_same_place_on_the_gpu_and_the_cpu() {
    let (model, _) = quad("fox").unwrap();
    assert!(model.face.forehead.is_some());
    feature_lands_in_the_same_place("marks", |m| m.face.forehead = None);
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
    chubby_renderer(id, 0.33)
}

/// [`app_renderer`] for a body `chonk` chubby (0 … 1).
fn chubby_renderer(id: &str, chonk: f32) -> Option<chonkgene_render::MeshRenderer> {
    let (model, _) = mesh_model(id).unwrap();
    model_renderer(model, chonk)
}

/// [`app_renderer`] for any model (the fox's human form).
fn model_renderer(model: Model, chonk: f32) -> Option<chonkgene_render::MeshRenderer> {
    use std::sync::OnceLock;
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    let gpu = GPU.get_or_init(|| Gpu::new().ok()).as_ref()?;
    chonkgene_render::MeshRenderer::new(gpu, std::sync::Arc::new(model), 1.0, chonk, 0)
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

/// Screen x (pixels at 100%) of a two-legged pet's left foot over a
/// steady walk, as the window carries it along, and its height.
fn foot_track(model: &Model, sk: &Skeleton) -> Vec<(f32, f32)> {
    let leg = sk.bone("leg_l").unwrap();
    let hip = sk.pivots[leg];
    let sole = v3(hip.x, -1.05, hip.z + 0.1);
    let mut walk = Mover::new(sk, 1.0, model).unwrap();
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
    // The birds, and the fox's human form on her two feet.
    let pets = BIPED_SPECIES
        .iter()
        .map(|&id| (id, mesh_model(id).unwrap()))
        .chain([("fox-human", human("fox", "ponytail", "white").unwrap())]);
    for (id, (model, sk)) in pets {
        let track = foot_track(&model, &sk);
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

/// Roger's twenty-seventh pass, item 2: an action by zone, so every pet
/// has shoulders (環抱) and feet (舉高高) apart from its front paws (握手;
/// a bird's are its wings): a four-legged pet's front paws in front of its
/// feet and its shoulders over its front legs, a bird's shoulders the tops
/// of its wings, the human form's hands on her forearms, her feet below
/// them and her shoulders above them.
#[test]
fn every_pet_has_shoulders_and_feet_apart_from_its_front_paws() {
    use chonkgene_rig::ZONES;
    let zone = |name: &str| ZONES.iter().position(|z| *z == name).unwrap() as u8;
    let forms = QUAD_SPECIES
        .iter()
        .chain(&BIPED_SPECIES)
        .map(|id| (id.to_string(), mesh_model(id).unwrap()))
        .chain([(
            "fox-human".to_string(),
            human("fox", "ponytail", "white").unwrap(),
        )]);
    for (id, (model, sk)) in forms {
        let mesh = bake(&model, &sk, 0, 0.33, model.bake_cell).unwrap();
        let of = |z: &str| -> Vec<&chonkgene_render::mesh::Vertex> {
            mesh.vertices
                .iter()
                .zip(&mesh.zones)
                .filter(|(_, k)| **k == zone(z))
                .map(|(v, _)| v)
                .collect()
        };
        let mean = |z: &str, axis: usize| {
            let v = of(z);
            v.iter().map(|v| v.pos[axis]).sum::<f32>() / v.len().max(1) as f32
        };
        let (paw, foot, shoulder) = (of("paw").len(), of("foot").len(), of("shoulder").len());
        assert!(foot >= 100, "{id}: {foot} foot vertices");
        assert!(shoulder >= 100, "{id}: {shoulder} shoulder vertices");
        let main_bone = |v: &chonkgene_render::mesh::Vertex| {
            let k = (0..4)
                .max_by(|&a, &b| v.weights[a].total_cmp(&v.weights[b]))
                .unwrap();
            sk.names[v.bones[k] as usize].as_str()
        };
        if BIPED_SPECIES.contains(&id.as_str()) {
            assert_eq!(paw, 0, "{id}: a bird's wings are its hands");
            assert!(
                mean("shoulder", 1) > mean("wing", 1),
                "{id}: its shoulders are not the tops of its wings"
            );
        } else if id == "fox-human" {
            assert!(paw >= 100, "{id}: {paw} hand vertices");
            assert!(
                of("paw").iter().all(|v| main_bone(v).starts_with("fore_")),
                "{id}: a hand off her forearms"
            );
            assert!(
                mean("foot", 1) < mean("paw", 1),
                "{id}: her feet above her hands"
            );
            assert!(
                mean("shoulder", 1) > mean("paw", 1),
                "{id}: her shoulders below her hands"
            );
        } else {
            assert!(paw >= 100, "{id}: {paw} front paw vertices");
            assert!(
                mean("paw", 2) > mean("foot", 2),
                "{id}: its front paws behind its feet"
            );
            assert!(
                mean("shoulder", 2) > mean("back", 2) && mean("shoulder", 1) > mean("paw", 1),
                "{id}: its shoulders are not over its front legs"
            );
        }
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

/// Roger's twenty-seventh pass, item 6 (b), patted on the bottom: a cat,
/// sitting, gets up and pushes its rump up and its tail straight up
/// (電梯屁股); the Shiba kicks a hind leg and looks back over its shoulder.
#[test]
fn patted_on_the_bottom_a_cat_puts_its_rump_up_and_the_shiba_kicks() {
    use chonkgene_core::anim::Act;
    use chonkgene_render::skin::Gesture;
    const DT: f32 = 1.0 / 60.0;
    for id in ["munchkin", "fold"] {
        let (model, sk) = mesh_model(id).unwrap();
        let tip = *sk.tails()[0].last().unwrap();
        let butt = sk.bone("butt").unwrap();
        // Rump and tail tip heights after `secs`, sitting first.
        let heights = |act: Option<Act>, sit: bool| {
            let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
            walk.sit(sit);
            settle(&mut walk, 2.0);
            let mut j = Vec::new();
            for k in 0..(1.4 / DT) as usize {
                walk.gesture(act.map(|act| Gesture {
                    act,
                    e: 1.0,
                    t: k as f32 * DT,
                }));
                j = walk.step(DT, false, 1.0);
            }
            let bones = sk.pose(&j);
            let y = |b: usize| bones[b].apply(sk.pivots[b]).y;
            (y(butt), y(tip))
        };
        let (rump, tail) = heights(Some(Act::ButtUp), true);
        let (stand_rump, stand_tail) = heights(None, false);
        assert!(
            rump > stand_rump + 0.05,
            "{id}: rump at {rump:.2}, standing {stand_rump:.2}"
        );
        assert!(
            tail > stand_tail + 0.1,
            "{id}: tail tip at {tail:.2}, standing {stand_tail:.2}"
        );
    }
    let (model, sk) = mesh_model("shiba").unwrap();
    let (head, body) = (sk.bone("head").unwrap(), sk.bone("body").unwrap());
    let hind: Vec<usize> = ["leg_bl", "leg_br"]
        .iter()
        .map(|n| sk.bone(n).unwrap())
        .collect();
    // (how far round it looks, how much a hind leg moves) over a second.
    let take = |rump: f32| {
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        settle(&mut walk, 2.0);
        let (mut look, mut lo, mut hi) = (0.0f32, f32::MAX, f32::MIN);
        for _ in 0..60 {
            walk.feel(chonkgene_render::skin::Feel {
                rump,
                ..Default::default()
            });
            let j = walk.step(DT, false, 1.0);
            look = look.max((j[head].rot.y + j[body].rot.y).abs());
            for &l in &hind {
                lo = lo.min(j[l].rot.x);
                hi = hi.max(j[l].rot.x);
            }
        }
        (look, hi - lo)
    };
    let ((look, kick), (still_look, still_kick)) = (take(1.0), take(0.0));
    assert!(
        look > still_look + 15.0,
        "looked back {look:.0}° ({still_look:.0}°)"
    );
    assert!(
        kick > still_kick + 10.0,
        "a hind leg moved {kick:.0}° ({still_kick:.0}°)"
    );
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

/// Asleep on its back, the belly swells round with each breath (Roger's
/// twenty-first pass: pushed out along its bone it came to a point): its
/// soft spot's bone grows and shrinks with the breath, by a good part of
/// its size; curled up or lying chin down it does not grow at all.
#[test]
fn a_belly_up_sleeper_swells_round_with_each_breath() {
    use chonkgene_render::skin::SleepPose;
    for id in ["shiba", "munchkin"] {
        let (model, sk) = mesh_model(id).unwrap();
        let belly = sk.bone("belly").unwrap();
        for pose in [SleepPose::BellyUp, SleepPose::Curl] {
            let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
            walk.sleep_pose(pose);
            walk.lie(true);
            settle(&mut walk, 6.0);
            let (mut least, mut most) = (f32::MAX, f32::MIN);
            for k in 0..240 {
                walk.breathe((k as f32 / 240.0 * std::f32::consts::TAU).sin());
                let swell = walk.step(1.0 / 60.0, false, 1.0)[belly].swell;
                least = least.min(swell);
                most = most.max(swell);
            }
            if pose == SleepPose::BellyUp {
                assert!(
                    most > 0.2 && least < 0.05,
                    "{id}: swells {least:.2} … {most:.2}"
                );
            } else {
                assert!(most.abs() < 1e-6, "{id} {pose:?}: swells {most:.2}");
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
/// flat into a bulge and wobbles, 「ㄉㄨㄞ ㄉㄨㄞ」 (Roger's twenty-seventh
/// pass, item 2: two or three bulges), and settles flat again.
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
    let deep = held.last().copied().unwrap_or(0.0);
    a.poke(Zone::Head);
    let after = frames(&mut r, &mut a, 2.5);
    let out = after.iter().copied().fold(0.0f32, f32::min);
    assert!(out < -0.01, "let go, it sprang out only {out:.3}");
    // Each time out past a twelfth of how deep it was pressed.
    let (mut bulges, mut bulging) = (0, false);
    for &d in &after {
        if d < -deep / 12.0 && !bulging {
            bulges += 1;
            bulging = true;
        } else if d > 0.0 {
            bulging = false;
        }
    }
    assert!(bulges >= 2, "let go, it bulged out {bulges} times");
    assert_eq!(after.last(), Some(&0.0), "still pressed in at the end");
}

/// Its cheeks squeezed between your hands (PLAN §8, Roger's twenty-seventh
/// pass, item 6 (a)): its face is narrower while they press; let go, it
/// springs back out wider than it was (「ㄉㄨㄞ ㄉㄨㄞ」) and settles as it
/// was. Every pet, and the fox's human form, each against itself left
/// alone (its head moves on its own meanwhile: looking about, sitting).
#[test]
fn squeezed_cheeks_narrow_the_face_and_spring_back_out() {
    use chonkgene_core::anim::{Animator, Expression};
    let model = |id: &str| {
        if id == "fox-human" {
            human("fox", "ponytail", "white").unwrap().0
        } else {
            mesh_model(id).unwrap().0
        }
    };
    // Cheek to cheek, pixels, every frame: 1.5 s alone, 1 s squeezed (or
    // not), 3 s let go; and how deep the hands are in at the end.
    const ALONE: usize = 45;
    const HELD: usize = 30;
    let trace = |id: &str, squeezed: bool| -> Option<(Vec<f32>, f32)> {
        let mut r = model_renderer(model(id), 0.33)?;
        let mut a = Animator::new(1, 0.33, 0.35).with_pops(false);
        r.still();
        let mut widths = Vec::new();
        for f in 0..ALONE + HELD + 90 {
            let squeeze = if squeezed && (ALONE..ALONE + HELD).contains(&f) {
                1.0
            } else {
                0.0
            };
            let hands = r
                .cheeks_at()
                .and_then(|c| chonkgene_render::props::cheek_hands(c, 1.0, squeeze));
            r.hug_press(hands.map(|h| {
                let [p, q] = h.hands;
                ([(p.press, p.grip), (q.press, q.grip)], h.firm)
            }));
            a.step(1.0 / 30.0);
            r.render(&a.params(), Expression::Neutral);
            r.finish();
            let [left, right] = r.cheeks_at().expect("its cheeks");
            widths.push(right.0 - left.0);
        }
        Some((widths, r.hands_in()))
    };
    for id in QUAD_SPECIES
        .iter()
        .chain(&BIPED_SPECIES)
        .chain(&["fox-human"])
    {
        let Some((alone, _)) = trace(id, false) else {
            eprintln!("skipped: no GPU");
            return;
        };
        let (squeezed, dents) = trace(id, true).expect("the GPU went");
        let at = |f: usize| (squeezed[f], alone[f]);
        let (held, was) = at(ALONE + HELD - 1);
        assert!(
            held < 0.92 * was,
            "{id}: squeezed {held:.1} px wide, {was:.1} alone"
        );
        let out = (ALONE + HELD..ALONE + HELD + 18)
            .map(|f| squeezed[f] - alone[f])
            .fold(f32::MIN, f32::max);
        assert!(out > 1.5, "{id}: let go, only {out:.1} px wider than alone");
        assert_eq!(dents, 0.0, "{id}: let go, the dents settle");
        let (last, was) = at(squeezed.len() - 1);
        assert!(
            (last - was).abs() < 0.5,
            "{id}: settled {last:.1} px wide, {was:.1} alone"
        );
    }
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

/// Hugged, your hands press in on it (the backs of two hands on its
/// belly): its body is squeezed narrower; let go, it springs back as it
/// was.
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
        let back = chonkgene_render::props::Grip::Back;
        let (l, rt) = (
            left + 0.3 * (belly.0 - left),
            right - 0.3 * (right - belly.0),
        );
        r.hug_press(Some(([((l, belly.1), back), ((rt, belly.1), back)], 1.0)));
        run(&mut r, &mut a, 1.0 / 30.0);
    }
    assert!(r.hands_in() > 0.05, "pressed in {}", r.hands_in());
    let held = width(&r, belly);
    // Pressed into the screen, the dents show in its shading; the squeeze
    // narrows its outline.
    assert!(held + 2 <= before, "hugged {held} px wide, {before} before");
    r.hug_press(None);
    run(&mut r, &mut a, 2.0);
    assert_eq!(r.hands_in(), 0.0, "let go, the dents settle");
    let after = width(&r, belly);
    assert!(
        after.abs_diff(before) <= 1,
        "let go {after} px wide, {before} before"
    );
}

#[test]
fn the_fox_human_form_fits_the_shader_and_stands_on_her_feet() {
    let model = human_model("fox").unwrap();
    assert_eq!(model.styles(), ["ponytail", "twintails", "princess"]);
    let hairs: Vec<&str> = model.hairs.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(hairs, ["white", "red", "gold", "night"]);
    // The shaders hold sixteen colours (gpu.rs `PALETTE`).
    assert!(
        model.slots.len() <= 16,
        "{} colour slots",
        model.slots.len()
    );
    for style in model.styles() {
        let (model, sk) = human("fox", style, "red").unwrap();
        assert!(
            sk.names.len() <= MAX_BONES,
            "{style}: {} bones",
            sk.names.len()
        );
        assert_eq!(model.parts[model.face.on].name, "head", "{style}");
        assert_eq!(
            model.face.mouth_on.map(|m| model.parts[m].name.as_str()),
            Some("mouth")
        );
        let mesh = bake(&model, &sk, 0, 0.33, model.bake_cell).unwrap();
        // The shadow is under her feet and skirt; the tails, held up off
        // the floor, do not spread it.
        let (across, along) = mesh.shadow;
        assert!(
            (0.3..0.7).contains(&across),
            "{style}: shadow across {across}"
        );
        assert!((0.3..0.7).contains(&along), "{style}: shadow along {along}");
    }
    // The hair's colour is laid over every coat; an unknown hairdo is the
    // first (a save from a later version).
    let (red, _) = human("fox", "nonsense", "red").unwrap();
    let hair = red.slots.iter().position(|s| s == "hair").unwrap();
    assert!(
        red.coats
            .iter()
            .all(|c| c.colors[hair] == [0xD8, 0x32, 0x2A, 0xFF])
    );
    // Her figure is made in Blender: a hairdo's hair only in it, 紅髮版's
    // armbands only on her coat (白色), all of it embedded.
    let on = |coat: &str| -> Vec<&str> {
        let c = red.coat(coat).unwrap();
        red.meshes_on(c).map(|m| m.name.as_str()).collect()
    };
    assert_eq!(on("white"), ["figure", "armbands", "hair_ponytail"]);
    assert_eq!(on("snow"), ["figure", "hair_ponytail"]);
    assert!(red.meshes.iter().all(|m| !m.src.is_empty()));
}

/// Her human form walks on two legs (PLAN §8, her human form:
/// standing and walking): each arm swings with the other side's leg, her
/// nine tails stream out behind and flow, and her hair and bell swing.
#[test]
fn the_fox_human_form_swings_her_arms_and_her_tails_and_hair_flow() {
    let (model, sk) = human("fox", "ponytail", "white").unwrap();
    let bone = |n: &str| sk.bone(n).unwrap();
    let (leg_l, leg_r, arm_l, arm_r) = (bone("leg_l"), bone("leg_r"), bone("arm_l"), bone("arm_r"));
    let tails = sk.tails();
    assert_eq!(tails.len(), 9);
    let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
    walk.still();
    let (mut with, mut against, mut arm_span) = (0.0f32, 0.0f32, (f32::MAX, f32::MIN));
    let mut tips: Vec<f32> = Vec::new();
    let mut moved = [0.0f32; 3];
    let mut last: Option<Vec<chonkgene_render::skin::Joint>> = None;
    for f in 0..360 {
        let j = walk.step(1.0 / 60.0, true, 1.0);
        if f >= 120 {
            // Arms and legs as forward swings (both turn about x, forward
            // negative).
            with += j[arm_l].rot.x * j[leg_r].rot.x;
            against += j[arm_l].rot.x * j[leg_l].rot.x;
            arm_span = (
                arm_span.0.min(j[arm_r].rot.x),
                arm_span.1.max(j[arm_r].rot.x),
            );
            let posed = sk.pose(&j);
            for chain in &tails {
                let tip = *chain.last().unwrap();
                tips.push(posed[tip].apply(sk.pivots[tip]).x);
            }
            if let Some(prev) = &last {
                for (k, name) in ["hair_b", "bell", chain_name(&sk, &tails)]
                    .iter()
                    .enumerate()
                {
                    let b = bone(name);
                    moved[k] += (j[b].rot - prev[b].rot).length();
                }
            }
        }
        last = Some(j);
    }
    assert!(
        with > 0.0 && against < 0.0,
        "arms with the other leg: {with:.0} vs {against:.0}"
    );
    assert!(
        arm_span.1 - arm_span.0 > 20.0,
        "arms swing {:.1}..{:.1}",
        arm_span.0,
        arm_span.1
    );
    // Facing right (+x), the tails' tips are out behind her, to the left.
    let behind = tips.iter().filter(|&&x| x < 0.0).count() as f32 / tips.len() as f32;
    assert!(
        behind > 0.9,
        "only {:.0}% of tail tips behind her",
        behind * 100.0
    );
    for (k, what) in ["hair", "bell", "tails"].iter().enumerate() {
        assert!(moved[k] > 20.0, "her {what} hardly moved: {:.1}°", moved[k]);
    }
}

/// Her human form sits on her heels, her legs folded back either side of
/// her (女の子座り) and her skirt spread to the floor, all of it above the
/// floor line (legs out in front lay along it, under the picture's bottom
/// from the app's camera); asleep curled on her side, she lies along the
/// floor under her tails, which neither prop her up nor cover her face
/// (PLAN §8, her human form: sitting and sleeping).
#[test]
fn the_fox_human_form_sits_on_her_heels_and_curls_up_under_her_tails() {
    use chonkgene_render::skin::SleepPose;
    use chonkgene_rig::{FLOOR, ZONES};
    let (model, sk) = human("fox", "ponytail", "white").unwrap();
    let mesh = bake(&model, &sk, 0, 0.33, model.bake_cell).unwrap();
    let tail = ZONES.iter().position(|z| *z == "tail").unwrap() as u8;
    let head = ZONES.iter().position(|z| *z == "head").unwrap() as u8;
    // The skin as posed: (lowest of all, lowest not on a tail, top of the
    // head).
    let skin = |joints: &[chonkgene_render::skin::Joint]| {
        let bones = sk.pose(joints);
        let (mut low, mut body_low, mut crown) = (f32::MAX, f32::MAX, f32::MIN);
        for (v, &zone) in mesh.vertices.iter().zip(&mesh.zones).step_by(3) {
            let rest = v3(v.pos[0], v.pos[1], v.pos[2]);
            let mut p = V3::ZERO;
            for (b, w) in v.bones.iter().zip(v.weights) {
                p = p + bones[*b as usize].apply(rest) * w;
            }
            low = low.min(p.y);
            if zone != tail {
                body_low = body_low.min(p.y);
            }
            if zone == head {
                crown = crown.max(p.y);
            }
        }
        (low, body_low, crown)
    };
    let settle = |w: &mut Mover, secs: f32| {
        let mut j = Vec::new();
        for _ in 0..(secs * 60.0) as usize {
            j = w.step(1.0 / 60.0, false, 1.0);
        }
        j
    };
    let mut w = Mover::new(&sk, 1.0, &model).unwrap();
    w.still();
    let (_, standing_low, standing_crown) = skin(&settle(&mut w, 1.0));
    assert!(
        (standing_low - FLOOR).abs() < 0.05,
        "stands at {standing_low}"
    );
    // Sitting: lower, on the floor, her feet out beside her skirt.
    w.sit(true);
    let j = settle(&mut w, 3.0);
    let (_, low, crown) = skin(&j);
    assert!((low - FLOOR).abs() < 0.08, "sits with her lowest at {low}");
    assert!(
        standing_crown - crown > 0.25,
        "sitting, her head only {:.2} lower",
        standing_crown - crown
    );
    let posed = sk.pose(&j);
    for leg in ["leg_l", "leg_r"] {
        let b = sk.bone(leg).unwrap();
        let hip = sk.pivots[b];
        let foot = posed[b].apply(v3(hip.x, FLOOR + 0.05, hip.z));
        assert!(foot.x.abs() > 0.45, "{leg}: foot at x {:.2}", foot.x);
        assert!(foot.z < 0.0, "{leg}: foot in front ({:.2})", foot.z);
        assert!(foot.y > FLOOR - 0.06, "{leg}: foot under the floor");
    }
    // Curled up on her side asleep.
    w.sit(false);
    w.sleep_pose(SleepPose::Curl);
    w.lie(true);
    let j = settle(&mut w, 4.0);
    assert!(w.ground() > 0.9, "laid on the floor: {}", w.ground());
    let posed = sk.pose(&j);
    let at = |name: &str| {
        let b = sk.bone(name).unwrap();
        posed[b].apply(sk.pivots[b])
    };
    let (h, hips) = (at("head"), at("hips"));
    assert!(
        (h.x - hips.x).abs() > 0.5 && (h.y - hips.y).abs() < 0.3,
        "lying along the floor: head {h:?}, hips {hips:?}"
    );
    let (low, body_low, _) = skin(&j);
    assert!(
        low > body_low - 0.02,
        "her tails reach {low:.2}, under her body ({body_low:.2})"
    );
    for chain in sk.tails() {
        let tip = *chain.last().unwrap();
        let p = posed[tip].apply(sk.pivots[tip]);
        assert!(
            (p - h).length() > 0.45,
            "{} lies over her face",
            sk.names[tip]
        );
    }
}

/// A mesh made in Blender (`assets/species/fox/figure_blend.py`) joins the
/// baked one: its colours come from the coat (dyed, so the hair's colour
/// follows), its bones by name, its weights summing to one; the model
/// without some of its parts keeps its face on the head; a bad file says
/// what is wrong with it.
#[test]
fn a_mesh_made_elsewhere_joins_the_baked_one() {
    use chonkgene_render::mesh::{Mesh, import};
    use chonkgene_rig::ZONES;
    let (model, sk) = human("fox", "ponytail", "red").unwrap();
    let plain = model.only(|p| p.name != "cheek" && p.name != "nose");
    assert!(plain.parts.len() < model.parts.len());
    assert_eq!(plain.parts[plain.face.on].name, "head");
    let empty = || Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        eye_r: V3::ONE,
        mouth_r: V3::ONE,
        shadow: (0.5, 0.5),
        zones: Vec::new(),
        crown: 0,
    };
    let src = "v 0 1.5 0  0 1 0  hair hairtip 0.5  0.45 0.05 0.9  head 3 hair_b 1\n\
               v 0.1 1.4 0  0 1 0  hair hair 0  0.45 0.05 1  head 1\n\
               v 0 1.4 0.1  0 1 0  ribbon ribbon 0  0 0 1  hair_b 1\n\
               f 0 1 2\n";
    let mut mesh = empty();
    import(&mut mesh, &model, &sk, 0, "head", src).unwrap();
    assert_eq!((mesh.vertices.len(), mesh.indices.len()), (3, 3));
    let v = &mesh.vertices[0];
    assert_eq!(
        v.bones[..2],
        [
            sk.bone("head").unwrap() as u32,
            sk.bone("hair_b").unwrap() as u32
        ]
    );
    assert!((v.weights[0] - 0.75).abs() < 1e-6 && (v.weights[1] - 0.25).abs() < 1e-6);
    assert_eq!(v.pos[3], 0.9);
    // Red hair (#D8322A) halfway to its gold tips (#F9B84E).
    let half = |a: u8, b: u8| (a as f32 + b as f32) / 2.0 / 255.0;
    assert!((v.color[0] - half(0xD8, 0xF9)).abs() < 1e-4);
    assert!((v.color[2] - half(0x2A, 0x4E)).abs() < 1e-4);
    assert_eq!(mesh.crown, 0, "the top of the hair is the crown");
    assert_eq!(v.mouth[3], 0.0, "hair is no tail");
    // Her tails, made elsewhere, burn when she casts as baked ones do: a
    // vertex all on a tail is marked past −1 by how far along it is (the
    // middle of the third of five segments), one partly on it only by how
    // much.
    let mut tails = empty();
    let src = "v 0 0 0 0 1 0 fur fur 0 0 0 1 tail3_c 1\n\
               v 0 0 0 0 1 0 fur fur 0 0 0 1 hips 1 tail3 1\n";
    import(&mut tails, &model, &sk, 0, "tail", src).expect("import");
    assert!((tails.vertices[0].mouth[3] + 1.5).abs() < 1e-6);
    assert!((tails.vertices[1].mouth[3] + 0.5).abs() < 1e-6);
    for (bad, says) in [
        ("v 0 0 0 0 1 0 hair hair 0 0 0 1 nobone 1\n", "no such bone"),
        (
            "v 0 0 0 0 1 0 nocolour hair 0 0 0 1 head 1\n",
            "no colour slot",
        ),
        (
            "v 0 0 0 0 1 0 hair hair 0 0 0 1 head 1\nf 0 0 1\n",
            "names vertex 1",
        ),
        ("x 1 2 3\n", "not a v, an f, a z or a w"),
        ("z elbow\n", "no zone"),
        ("w 0 0 1 hair\n", "before any vertex"),
    ] {
        let err = import(&mut empty(), &model, &sk, 0, "head", bad).unwrap_err();
        assert!(err.contains(says), "{bad:?}: {err}");
    }
    // A whole figure names each piece's touch zone as it goes.
    let mut figure = empty();
    let src =
        "v 0 0 0 0 1 0 hair hair 0 0 0 1 head 1\nz paw\nv 1 0 0 0 1 0 hair hair 0 0 0 1 head 1\n";
    import(&mut figure, &model, &sk, 0, "head", src).expect("import");
    let zone = |z: &str| ZONES.iter().position(|n| *n == z).unwrap() as u8;
    assert_eq!(figure.zones, [zone("head"), zone("paw")]);
    // A woven border's vertex carries where it is on its band and the
    // pattern's colour, for the shader to weave.
    let mut band = empty();
    let src = "v 0 0 0 0 1 0 hair hair 0 0 0 1 head 1\nw 2.5 -0.5 0.03 hair\n";
    import(&mut band, &model, &sk, 0, "head", src).expect("import");
    let v = &band.vertices[0];
    assert_eq!(v.eye, [2.5, -0.5, 0.03, -1.0]);
    assert_eq!(v.mouth[3], 0.0, "no mouth, no tail");
    assert!(
        (v.mouth[0] - v.color[0]).abs() < 1e-6,
        "the pattern in the hair's colour"
    );
}

/// The middle bone of the first tail.
fn chain_name<'a>(sk: &'a Skeleton, tails: &[Vec<usize>]) -> &'a str {
    &sk.names[tails[0][tails[0].len() / 2]]
}

#[test]
fn the_human_forms_seal_and_eyes_land_in_the_same_place_on_the_gpu_and_the_cpu() {
    let (mut model, sk) = human("fox", "ponytail", "white").unwrap();
    assert!(model.face.seal && model.face.chibi);
    // The ray-marcher draws only what is baked: compare her bare head.
    model.meshes.clear();
    lands_in_the_same_place("seal", &model, &sk, |m| m.face.seal = false);
    lands_in_the_same_place("chibi eyes", &model, &sk, |m| m.face.chibi = false);
}

/// Roger's twenty-sixth pass, item 2 (the bone was in the Shiba's mouth
/// the next moment, the mouth never down to it): picking a thing up, the
/// mouth comes down to the floor, low enough that what it takes there is
/// still on the floor when it closes on it (a bone in the mouth hangs
/// about 0.16 below it), then goes back up.
#[test]
fn picking_up_brings_the_mouth_down_to_the_floor() {
    use chonkgene_core::anim::{Act, PICK_GRAB};
    use chonkgene_render::skin::Gesture;
    let mut short = Vec::new();
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        let (model, sk) = mesh_model(id).unwrap();
        let nose = ["nose", "beak", "bill"]
            .iter()
            .find_map(|n| model.part(n))
            .map(|i| &model.parts[i])
            .unwrap();
        let mouth_up = |joints: &[chonkgene_render::skin::Joint]| {
            sk.pose(joints)[nose.bone]
                .apply(nose.at + v3(0.0, -0.12, 0.0))
                .y
                - chonkgene_rig::FLOOR
        };
        let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
        walk.still();
        let standing = mouth_up(&settle(&mut walk, 2.0));
        let mut joints = Vec::new();
        for _ in 0..60 {
            walk.gesture(Some(Gesture {
                act: Act::Pick,
                e: 1.0,
                t: PICK_GRAB,
            }));
            joints = walk.step(1.0 / 60.0, false, 1.0);
        }
        let down = mouth_up(&joints);
        println!("{id}: mouth {standing:.2} up standing, {down:.2} picking up");
        if down >= 0.2 {
            short.push(format!(
                "{id}: the mouth only came down to {down:.2} ({standing:.2} standing)"
            ));
        }
        walk.gesture(None);
        let up = mouth_up(&settle(&mut walk, 1.0));
        assert!(
            (up - standing).abs() < 0.05,
            "{id}: back up to {up:.2}, not {standing:.2}"
        );
    }
    assert!(short.is_empty(), "{}", short.join("\n"));
}

/// How many solid pixels `pm` has along its bottom row (the shadow's faint
/// fringe, a little alpha, does not count): a pet standing on it touches it
/// with its feet; one cut off there, with a long flat edge.
fn bottom_row(pm: &Pixmap) -> u32 {
    let (w, h) = (pm.width(), pm.height());
    let d = pm.data();
    (0..w)
        .filter(|&x| d[(((h - 1) * w + x) * 4 + 3) as usize] > 120)
        .count() as u32
}

/// What a pet touched along the bottom of its picture in one hug: standing
/// on its feet before; in the hug and the moments after, its pose coming
/// back; and settled again (sitting pleased, or lying asleep); and how near
/// it came to the left, top and right edges all the while.
struct HugEdges {
    standing: u32,
    during: u32,
    settled: u32,
    edges: [u32; 3],
}

impl HugEdges {
    /// Not cut off: along the bottom no more than on its feet or settled,
    /// and clear of the other edges.
    fn whole(&self) -> bool {
        self.during <= self.standing.max(self.settled) + 4 && self.edges.iter().all(|&m| m >= 3)
    }
}

/// One hug of `role`, `chonk` chubby, from standing until settled again,
/// rendered by `r` (asleep: curled up first, and asleep all through).
fn hug_edges(
    r: &mut chonkgene_render::MeshRenderer,
    chonk: f32,
    role: chonkgene_core::anim::Hug,
    asleep: bool,
) -> HugEdges {
    use chonkgene_core::anim::{Animator, Expression};
    use chonkgene_render::skin::SleepPose;
    let mut a = Animator::new(1, chonk, 0.0).with_pops(false);
    r.sleep_in(None);
    let mut standing = 0;
    for _ in 0..20 {
        a.step(0.1);
        r.render(&a.params(), Expression::Neutral);
        r.finish();
        standing = standing.max(bottom_row(r.pixmap()));
    }
    r.sleep_in(asleep.then_some(SleepPose::Curl));
    a.hug(role);
    let e = if asleep {
        Expression::Asleep
    } else {
        Expression::Happy
    };
    let secs = role.secs();
    let (mut during, mut settled, mut edges) = (0, 0, [u32::MAX; 3]);
    for i in 1..=((secs + 1.5) / 0.05) as usize {
        a.step(0.05);
        r.render(&a.params(), e);
        r.finish();
        let t = i as f32 * 0.05;
        let bottom = bottom_row(r.pixmap());
        if t <= secs + 0.6 {
            during = during.max(bottom);
        } else if t >= secs + 1.0 {
            settled = settled.max(bottom);
        }
        let m = margins(r.pixmap());
        edges = [edges[0].min(m[0]), edges[1].min(m[1]), edges[2].min(m[2])];
    }
    HugEdges {
        standing,
        during,
        settled,
        edges,
    }
}

/// Roger (2026-11-20): held in any of the three ways (環抱, 舉高高, 搖搖抱),
/// the fox's lower tails were cut off along the bottom of its window. Held
/// by you, awake or asleep, slim or chubby, no pet reaches the left, top or
/// right edge of its picture, and along the bottom it touches no more than
/// on its feet or settled again, in your hands or just put down.
#[test]
fn a_pet_held_in_your_arms_is_never_cut_off() {
    use chonkgene_core::anim::{Hug, HugHold};
    let mut short = Vec::new();
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        for chonk in [0.33, 1.0] {
            let Some(mut r) = chubby_renderer(id, chonk) else {
                eprintln!("skipped: no GPU");
                return;
            };
            for (hold, asleep) in [
                (HugHold::Wrap, false),
                (HugHold::Carry, false),
                (HugHold::Cradle, false),
                (HugHold::Cradle, true),
            ] {
                let h = hug_edges(&mut r, chonk, Hug::ByYou(hold), asleep);
                if !h.whole() {
                    short.push(format!(
                        "{id} {chonk} {hold:?}{}: {} px along the bottom (standing {}, \
                         settled {}), {:?} px from the left, top and right",
                        if asleep { " asleep" } else { "" },
                        h.during,
                        h.standing,
                        h.settled,
                        h.edges
                    ));
                }
            }
        }
    }
    assert!(short.is_empty(), "{}", short.join("\n"));
}

/// Roger's twenty-sixth pass, item 4: the pets froze in mid-fall for
/// seconds while the app's thread waited for a frame the GPU was slow over.
/// A wait now has a limit; when it runs out the frame is still on its way,
/// not lost, and is taken the next time.
#[test]
fn a_frame_waited_for_briefly_stays_on_its_way_and_is_taken_later() {
    use chonkgene_render::gpu::Wait;
    let Ok(gpu) = Gpu::new() else {
        eprintln!("skipped: no GPU");
        return;
    };
    let (model, sk) = shiba();
    let mesh = bake(&model, &sk, 0, 0.33, 0.025).unwrap();
    let scene = Scene::posed(&model, 0, 0.33, &Pose::default(), Look::Open, Layer::All);
    let (w, h) = (200, 160);
    let mut canvas = MeshCanvas::new(&gpu, &mesh, w, h, 1.0);
    let mut pm = Pixmap::new(w, h).unwrap();
    for _ in 0..5 {
        canvas.submit(&gpu, &scene, &sk.pose(&[]), 0.0).unwrap();
        let got = canvas
            .collect(&gpu, &mut pm, Wait::Upto(std::time::Duration::ZERO))
            .unwrap();
        if got {
            assert!(!canvas.pending(), "taken, so no longer on its way");
            continue;
        }
        assert!(canvas.pending(), "not ready in time: kept on its way");
        assert!(canvas.collect(&gpu, &mut pm, Wait::Done).unwrap());
        assert!(!canvas.pending());
    }
    assert!(opaque(&pm) > 1000, "the picture came through");
}

/// Roger (2026-11-20, again): the fox's lower tails cut off by the bottom
/// of her window in a hug; and again (2026-11-23) as it ended, a chubby fox
/// dropped for half a second before her tails came back. Hugging another
/// pet (face to face, either side; the fox's bear hug, up on her hind legs;
/// held in it), slim or chubby, no pet reaches the left, top or right edge
/// of its picture, and along the bottom it touches no more than on its feet
/// or settled again, in the hug or just after.
#[test]
fn a_pet_hugging_another_is_never_cut_off() {
    use chonkgene_core::anim::Hug;
    let mut short = Vec::new();
    for id in QUAD_SPECIES.iter().chain(&BIPED_SPECIES) {
        for chonk in [0.33, 1.0] {
            let Some(mut r) = chubby_renderer(id, chonk) else {
                eprintln!("skipped: no GPU");
                return;
            };
            let mut roles = vec![
                Hug::Cuddle { front: true },
                Hug::Cuddle { front: false },
                Hug::Squeezed,
            ];
            if *id == "fox" {
                roles.push(Hug::Bear);
            }
            for role in roles {
                let h = hug_edges(&mut r, chonk, role, false);
                if !h.whole() {
                    short.push(format!(
                        "{id} {chonk} {role:?}: {} px along the bottom (standing {}, \
                         settled {}), {:?} px from the left, top and right",
                        h.during, h.standing, h.settled, h.edges
                    ));
                }
            }
        }
    }
    assert!(short.is_empty(), "{}", short.join("\n"));
}

/// Roger's twenty-seventh pass, item 2: a chin scratch, each its own way,
/// against the same pet left alone: the Shiba's double chin pushed up, the
/// 曼赤肯's chin up and then its face rubbed side to side, the 折耳貓 rolled
/// over on its side, the owl's head going round both ways (its body
/// leaning with it), the chick puffed out, the duckling's head nibbling
/// and its tail waggling, the fox's tail brought round to the hand.
#[test]
fn a_chin_scratch_moves_each_pet_its_own_way() {
    use chonkgene_core::chin::ChinScratch;
    use chonkgene_core::species::Species;
    use chonkgene_render::skin::{Feel, Joint};
    const DT: f32 = 1.0 / 60.0;
    for species in Species::ALL {
        let id = species.id();
        let kind = species.chin_scratch();
        let (model, sk) = mesh_model(id).unwrap();
        // Every frame's joints and posed bones, scratched or not, and the
        // tail tip its walk says hooks the hand.
        let run = |scratched: bool| {
            let mut walk = Mover::new(&sk, 1.0, &model).unwrap();
            settle(&mut walk, 2.0);
            let mut frames: Vec<(Vec<Joint>, Option<usize>)> = Vec::new();
            for k in 0..(kind.secs() / DT) as usize {
                walk.feel(Feel {
                    chin: scratched.then_some((kind, k as f32 * DT)),
                    ..Default::default()
                });
                let j = walk.step(DT, false, 1.0);
                frames.push((j, walk.hook_tip()));
            }
            frames
        };
        let (on, off) = (run(true), run(false));
        let bone = |n: &str| sk.bone(n).unwrap_or_else(|| panic!("{id}: no {n}"));
        let span = |f: &[(Vec<Joint>, Option<usize>)], get: &dyn Fn(&[Joint]) -> f32| {
            let v: Vec<f32> = f.iter().map(|(j, _)| get(j)).collect();
            let lo = v.iter().copied().fold(f32::MAX, f32::min);
            let hi = v.iter().copied().fold(f32::MIN, f32::max);
            (lo, hi)
        };
        let more = |get: &dyn Fn(&[Joint]) -> f32, by: f32, what: &str| {
            let ((a, b), (c, d)) = (span(&on, get), span(&off, get));
            assert!(
                b - a > d - c + by,
                "{id}: {what} spans {:.2} scratched, {:.2} left alone",
                b - a,
                d - c
            );
        };
        match kind {
            ChinScratch::Jowls => {
                let chin = bone("chin");
                let (_, up) = span(&on, &|j| j[chin].shift.y);
                assert!(up > 0.04, "{id}: double chin pushed up {up:.3}");
            }
            ChinScratch::Knuckles => {
                let head = bone("head");
                let (up, _) = span(&on, &|j| j[head].rot.x);
                let (rest, _) = span(&off, &|j| j[head].rot.x);
                assert!(up < rest - 15.0, "{id}: chin up {up:.0}° ({rest:.0}°)");
                more(&|j| j[head].rot.z, 15.0, "the head's rub");
            }
            ChinScratch::Topple => {
                let (lo, hi) = span(&on, &|j| j[0].rot.z);
                assert!(hi.max(-lo) > 60.0, "{id}: rolled over {lo:.0}…{hi:.0}°");
            }
            ChinScratch::Round => {
                let (head, body) = (bone("head"), bone("body"));
                let (lo, hi) = span(&on, &|j| j[head].rot.z);
                assert!(
                    lo < -15.0 && hi > 25.0 && hi - lo > 55.0,
                    "{id}: head round {lo:.0}…{hi:.0}°"
                );
                more(&|j| j[body].rot.z, 15.0, "the body's lean");
                more(&|j| j[head].rot.x, 20.0, "the head's nod");
            }
            ChinScratch::Puff => {
                let body = bone("body");
                let (_, puff) = span(&on, &|j| j[body].swell);
                assert!(puff > 0.15, "{id}: puffed {puff:.2}");
            }
            ChinScratch::Nibble => {
                more(&|j| j[bone("head")].rot.x, 10.0, "the nibbling");
                more(&|j| j[bone("tail")].rot.z, 30.0, "the waggle");
            }
            ChinScratch::Hook => {
                let hooked: Vec<_> = on
                    .iter()
                    .filter_map(|(j, tip)| Some((j, (*tip)?)))
                    .collect();
                assert!(hooked.len() > 30, "{id}: a tail hooked the hand");
                assert!(
                    off.iter().all(|(_, tip)| tip.is_none()),
                    "{id}: hooked alone"
                );
                // The tip comes forward round her side, against the same
                // moment left alone.
                let (j, tip) = hooked[hooked.len() / 2];
                let at = |j: &[Joint]| {
                    let bones = sk.pose(j);
                    bones[tip].apply(sk.pivots[tip])
                };
                let k = on.iter().position(|(o, _)| std::ptr::eq(o, j)).unwrap();
                let (now, alone) = (at(j), at(&off[k].0));
                assert!(
                    now.z > alone.z + 0.4,
                    "{id}: tail tip at z {:.2}, left alone {:.2}",
                    now.z,
                    alone.z
                );
            }
        }
    }
}

/// Roger, 2026-11-27: tickled until it rolled onto its back, its front
/// paws were gone (folded toward its head, they hid behind it). Rolled
/// over laughing, a good share of each front paw's skin is what a click
/// there lands on.
#[test]
fn rolled_over_laughing_its_front_paws_show() {
    use chonkgene_core::anim::{Expression, PoseParams};
    for id in ["shiba", "munchkin", "fold"] {
        let Some(mut r) = app_renderer(id) else {
            eprintln!("no GPU: skipped");
            return;
        };
        for k in 0..90 {
            let p = PoseParams {
                clock: k as f32 / 30.0,
                ..Default::default()
            };
            r.render(&p, Expression::Giggle);
            r.finish();
        }
        let pts = r.zone_points("paw");
        assert!(pts.len() > 50, "{id}: {} paw vertices", pts.len());
        let step = pts.len() / 60;
        let tried: Vec<_> = pts.iter().step_by(step.max(1)).collect();
        let seen = tried
            .iter()
            .filter(|&&&(x, y)| r.hit(x, y) == Some("paw"))
            .count();
        assert!(
            seen * 5 >= tried.len(),
            "{id}: {seen} of {} front paw points show",
            tried.len()
        );
    }
}
