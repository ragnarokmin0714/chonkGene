//! Skinned-mesh spike: a four-legged pet (`shiba`, `fox`) baked into a mesh
//! and walked.
//!
//! `cargo run --release -p chonkgene-render --example mesh -- peek <species> <out dir>`
//! ray-marches the model from a few angles, to check its shape.
//!
//! `... -- walk|sit|lie|look|<gesture> <species> <out dir> [scale] [coat]`
//! bakes the mesh, then writes `<mode>-<frame>.png` at 30 fps. `walk`:
//! walking right, turning, walking left, stopping, and a poke in the belly.
//! `sit`: standing, sitting down, sitting a while, getting up. `lie`: lying
//! down asleep and getting up; `rest`: lying down awake, tapping along;
//! `fly`, `swim` (birds). `look`: glances while sitting. `fidget`:
//! sitting a while, then standing, fidgeting all along (each frame's
//! fidget is printed). `life`: 20 s left alone as the app leaves it, its
//! animator choosing the species' gestures and blinking; `smile`: the same,
//! pleased (a pat), smiling at you; `scene`: 20 s in the species' own
//! scene (lying chewing a bone, lying in the sun, swimming, perched,
//! pecking at seeds, swishing its tails in the fox-fire), lively as the
//! app has it (`NOLIVE=1`: as before 2026-11-05, not lively). A gesture
//! (`eat`, `dig`, `scratch`, `groom`, `shake`, `preen`, `peck`, `flap`,
//! `stretch`, `yawn`, `wave`) is played through once. It prints the bake
//! time, the mesh size and what each frame cost.

use std::time::Instant;

use chonkgene_core::anim::{Act, gesture_secs};
use chonkgene_render::fidget::Tongue;
use chonkgene_render::gpu::Gpu;
use chonkgene_render::mesh::bake;
use chonkgene_render::mesh_gpu::MeshCanvas;
use chonkgene_render::raymarch;
use chonkgene_render::skin::{Gesture, Skeleton, mesh_model};
use chonkgene_render::waddle::Mover;
use chonkgene_rig::math::v3;
use chonkgene_rig::{Layer, Look, Model, Pose, PoseId, Scene};
use tiny_skia::{Color, IntSize, Pixmap, PixmapPaint, Transform};

const FPS: f32 = 30.0;
/// Walking speed, pixels a second at 100% (chonkgene-core motion).
const WALK_SPEED: f32 = 60.0;
/// Pixels a world unit at 100% (raymarch.rs).
const PPU: f32 = 58.0;

/// What the pet does at time `t`: whether it walks, which way it faces;
/// it may also poke or sit the walker.
type Script = Box<dyn Fn(f32, &mut Mover) -> (bool, f32)>;

fn walk() -> (f32, Script) {
    (
        7.4,
        Box::new(|t, w| {
            if (t - 5.6).abs() < 0.01 {
                w.poke(2.0);
            }
            (t < 4.0, if t < 1.9 { 1.0 } else { -1.0 })
        }),
    )
}

fn sit() -> (f32, Script) {
    (
        5.0,
        Box::new(|t, w| {
            w.sit((0.6..3.4).contains(&t));
            (false, 1.0)
        }),
    )
}

/// Lying down awake (趴下), tapping along as if you typed, then up.
fn rest() -> (f32, Script) {
    (
        6.0,
        Box::new(|t, w| {
            w.rest((0.3..5.0).contains(&t));
            let at = t - 2.5;
            w.gesture((0.0..1.8).contains(&at).then(|| Gesture {
                act: Act::Tap,
                e: (std::f32::consts::PI * at / 1.8).sin().sqrt(),
                t: at,
            }));
            (false, 1.0)
        }),
    )
}

/// A bird flying (wings beating, feet tucked), then swimming.
fn fly_swim(swim: bool) -> (f32, Script) {
    (
        4.0,
        Box::new(move |t, w| {
            let on = (0.3..3.4).contains(&t);
            if swim {
                w.swim(on);
            } else {
                w.fly(on);
            }
            (false, 1.0)
        }),
    )
}

fn lie() -> (f32, Script) {
    (
        5.0,
        Box::new(|t, w| {
            w.lie((0.3..4.0).contains(&t));
            (false, 1.0)
        }),
    )
}

/// Glances left and right while sitting (the head turns toward you too).
fn look() -> (f32, Script) {
    (
        4.0,
        Box::new(|t, w| {
            w.sit(true);
            w.look(if t < 1.3 {
                35.0
            } else if t < 2.6 {
                -35.0
            } else {
                0.0
            });
            (false, 1.0)
        }),
    )
}

thread_local! {
    /// The eyes the `life` script's animator wants this frame.
    static EYES: std::cell::Cell<Option<Look>> = const { std::cell::Cell::new(None) };
    /// How high the `life` script's animator has the pet hopping, world
    /// units.
    static HOP: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

/// Left alone 20 s, as the app leaves a pet: its animator picks the
/// species' own gestures and blinks; resting, it sits down, as
/// mesh_frames.rs does.
fn life(id: &str, smiling: bool) -> (f32, Script) {
    use chonkgene_core::anim::{Animator, Expression};
    use chonkgene_core::species::Species;
    use chonkgene_render::skin::{Feel, Mood};
    let species = Species::from_id(id).expect("species");
    let mut anim = Animator::new(5, 0.33, species.waddle())
        .with_habits(species.habits())
        .with_joys(species.joys())
        .with_slow_blinks(species.slow_blinks())
        .with_pops(false);
    let expression = if smiling {
        Expression::Happy
    } else {
        Expression::Neutral
    };
    anim.expression = expression;
    // The animator, the gesture under way and since when, since when it
    // has rested, and the script's clock last call.
    let state = std::cell::RefCell::new((anim, None::<(Act, f32)>, None::<f32>, 0.0f32));
    (
        20.0,
        Box::new(move |t, w| {
            let (anim, gesture, rest, last) = &mut *state.borrow_mut();
            anim.step((t - *last).max(1e-4));
            *last = t;
            let mut p = anim.params();
            // `NOJOY=1`: smiling as before 2026-11-05, nothing to do but sit.
            if smiling && std::env::var_os("NOJOY").is_some() {
                p.act = Act::Rest;
                p.bob = 0.0;
            }
            if gesture.map(|g| g.0) != Some(p.act) {
                *gesture = gesture_secs(p.act).map(|_| (p.act, p.clock));
            }
            let standing = gesture.is_some_and(|(act, _)| !act.seated());
            let seated = gesture.is_some_and(|(act, _)| act.seated());
            if standing {
                *rest = None;
            } else {
                rest.get_or_insert(p.clock);
            }
            let rested = rest.is_some_and(|since| p.clock - since > 0.8);
            // Pleased, it sits facing you between its little things, as
            // mesh_frames.rs has it.
            w.sit(!standing && (rested || seated || smiling));
            if smiling {
                w.feel(Feel {
                    mood: Mood::Content,
                    ..Feel::default()
                });
            }
            HOP.with(|h| h.set((-p.bob).max(0.0) / PPU));
            w.look(match p.act {
                Act::LookLeft => 35.0,
                Act::LookRight => -35.0,
                _ => 0.0,
            });
            w.gesture(gesture.and_then(|(act, since)| {
                let secs = gesture_secs(act)?;
                let at = (p.clock - since).max(0.0);
                Some(Gesture {
                    act,
                    e: (std::f32::consts::PI * (at / secs).min(1.0)).sin().sqrt(),
                    t: at,
                })
            }));
            w.breathe(p.breath);
            w.set_mood(p.lively && std::env::var_os("NOLIVE").is_none(), p.puffed);
            EYES.with(|e| e.set(Some(chonkgene_render::key_for(expression, &p).look)));
            (false, 1.0)
        }),
    )
}

/// 20 s in the species' own scene as the app plays it: lying chewing a
/// bone (Shiba), lying in the sun (cats), swimming (duckling), perched
/// looking about (owl), pecking at seeds (chick), spinning among its fire
/// (fox); its gesture again as soon as the last is done, lively.
fn scene(id: &str) -> (f32, Script) {
    use chonkgene_core::anim::{Animator, Posture};
    use chonkgene_core::species::Species;
    let species = Species::from_id(id).expect("species");
    let mut anim = Animator::new(9, 0.33, species.waddle())
        .with_habits(species.habits())
        .with_slow_blinks(species.slow_blinks())
        .with_pops(false);
    let (posture, act) = match species {
        Species::Shiba => (Posture::Lie, Some(Act::Chew)),
        Species::Munchkin | Species::Fold => (Posture::Lie, None),
        Species::Duck => (Posture::Swim, None),
        Species::Chick => (Posture::Free, Some(Act::Peck)),
        Species::Fox => (Posture::Free, Some(Act::Swish)),
        Species::Owl => (Posture::Free, None),
    };
    anim.posture = posture;
    anim.lively = std::env::var_os("NOLIVE").is_none();
    let state = std::cell::RefCell::new((anim, None::<(Act, f32)>, 0.0f32));
    (
        20.0,
        Box::new(move |t, w| {
            let (anim, gesture, last) = &mut *state.borrow_mut();
            if let Some(act) = act
                && anim.doing().is_none()
            {
                anim.gesture(act);
            }
            anim.step((t - *last).max(1e-4));
            *last = t;
            let p = anim.params();
            if gesture.map(|g| g.0) != Some(p.act) {
                *gesture = gesture_secs(p.act).map(|_| (p.act, p.clock));
            }
            w.rest(posture == Posture::Lie);
            w.swim(posture == Posture::Swim);
            w.sit(posture == Posture::Free && gesture.is_none_or(|(a, _)| a.seated()));
            HOP.with(|h| h.set((-p.bob).max(0.0) / PPU));
            w.look(match p.act {
                Act::LookLeft => 35.0,
                Act::LookRight => -35.0,
                _ => 0.0,
            });
            w.gesture(gesture.and_then(|(act, since)| {
                let secs = gesture_secs(act)?;
                let at = (p.clock - since).max(0.0);
                Some(Gesture {
                    act,
                    e: (std::f32::consts::PI * (at / secs).min(1.0)).sin().sqrt(),
                    t: at,
                })
            }));
            w.breathe(p.breath);
            w.set_mood(p.lively, p.puffed);
            EYES.with(|e| e.set(Some(chonkgene_render::key_for(anim.expression, &p).look)));
            (false, 1.0)
        }),
    )
}

/// Sitting six seconds, then standing six, fidgeting all the while.
fn fidget() -> (f32, Script) {
    (
        12.0,
        Box::new(|t, w| {
            w.sit(t < 6.0);
            (false, 1.0)
        }),
    )
}

/// A gesture from a second in, eased in and out over its length as the app
/// does; sitting before it, and through it if it is done sitting.
fn gesture(act: Act) -> (f32, Script) {
    let secs = gesture_secs(act).expect("a gesture with a length");
    (
        secs + 1.8,
        Box::new(move |t, w| {
            let at = t - 1.0;
            let on = (0.0..secs).contains(&at);
            w.sit(if on { act.seated() } else { true });
            w.gesture(on.then(|| Gesture {
                act,
                e: (std::f32::consts::PI * at / secs).sin().sqrt(),
                t: at,
            }));
            (false, 1.0)
        }),
    )
}

const GESTURES: [(&str, Act); 24] = [
    ("stretch", Act::Stretch),
    ("yawn", Act::Yawn),
    ("wave", Act::Wave),
    ("eat", Act::Eat),
    ("dig", Act::Dig),
    ("scratch", Act::Scratch),
    ("groom", Act::Groom),
    ("shake", Act::Shake),
    ("preen", Act::Preen),
    ("peck", Act::Peck),
    ("flap", Act::Flap),
    ("wiggle", Act::Wiggle),
    ("bounce", Act::Bounce),
    ("spin", Act::Spin),
    ("knead", Act::Knead),
    ("paw", Act::Paw),
    ("tap", Act::Tap),
    ("tilt", Act::Tilt),
    ("sniff", Act::Sniff),
    ("rub", Act::Rub),
    ("chew", Act::Chew),
    ("swish", Act::Swish),
    ("nudge", Act::Nudge),
    ("dance", Act::Dance),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(mode), Some(id), Some(out)) = (args.first(), args.get(1), args.get(2)) else {
        eprintln!(
            "usage: mesh peek|walk|sit|lie|look|fidget|life|smile|scene|<gesture> <species> <out dir> [scale] [coat]"
        );
        std::process::exit(2);
    };
    let scale: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let (model, sk) = mesh_model(id).unwrap_or_else(|e| panic!("{e}"));
    let coat = args
        .get(4)
        .map(|c| model.coat(c).unwrap_or_else(|| panic!("no coat {c:?}")))
        .unwrap_or(0);
    std::fs::create_dir_all(out).expect("out dir");
    let script = match mode.as_str() {
        "peek" => return peek(&model, out),
        "walk" => walk(),
        "sit" => sit(),
        "lie" => lie(),
        "rest" => rest(),
        "fly" => fly_swim(false),
        "swim" => fly_swim(true),
        "look" => look(),
        "fidget" => fidget(),
        "life" => life(id, false),
        "smile" => life(id, true),
        "scene" => scene(id),
        g => match GESTURES.iter().find(|(name, _)| *name == g) {
            Some(&(_, act)) => gesture(act),
            None => panic!("no mode {mode:?}"),
        },
    };
    run(&model, &sk, coat, out, scale, mode, script);
}

fn peek(model: &Model, out: &str) {
    for yaw in [0.0, 45.0, 90.0, 180.0] {
        let mut pose = model.pose(PoseId::Sit).clone();
        pose.root.rot = v3(0.0, yaw, 0.0);
        let scene = Scene::posed(model, 0, 0.33, &pose, Look::Open, Layer::All);
        let s = raymarch::render(&scene, 1.0, 8);
        let size = IntSize::from_wh(s.width, s.height).expect("size");
        let px = Pixmap::from_vec(s.rgba, size).expect("pixmap");
        px.save_png(format!("{out}/peek-{yaw:03}.png"))
            .expect("png");
    }
}

fn run(
    model: &Model,
    sk: &Skeleton,
    coat: usize,
    out: &str,
    scale: f32,
    name: &str,
    (secs, script): (f32, Script),
) {
    let t0 = Instant::now();
    // `CELL=0.03` overrides the model's own grid cell.
    let cell: f32 = std::env::var("CELL")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(model.bake_cell);
    let mesh = bake(model, sk, coat, 0.33, cell).expect("bake");
    println!(
        "baked {} vertices, {} triangles in {:.2} s",
        mesh.vertices.len(),
        mesh.indices.len() / 3,
        t0.elapsed().as_secs_f32()
    );
    let gpu = Gpu::new().expect("gpu");
    println!("GPU: {}", gpu.name());
    let (cw, ch) = model.canvas.unwrap_or((4.5, 3.3));
    let (w, h) = (
        (cw * PPU * scale).ceil() as u32,
        (ch * PPU * scale).ceil() as u32,
    );
    let mut canvas = MeshCanvas::new(&gpu, &mesh, w, h, scale);
    let look = if name == "lie" {
        Look::Asleep
    } else {
        Look::Open
    };
    let face = |look| Scene::posed(model, coat, 0.33, &Pose::default(), look, Layer::All);
    let mut frame = Pixmap::new(w, h).expect("frame");
    let mut walker = Mover::new(sk, 1.0, model).expect("walk");
    let mut x = 0.0f32;
    let mut wall = 0.0f64;
    let mut gpu_secs = Vec::new();
    let frames = (secs * FPS) as u32;
    for f in 0..frames {
        let t = f as f32 / FPS;
        let (walking, facing) = script(t, &mut walker);
        let mut joints = Vec::new();
        for _ in 0..2 {
            joints = walker.step(0.5 / FPS, walking, facing);
            x += WALK_SPEED * scale * walker.speed() * facing * 0.5 / FPS;
        }
        // `TURN=90` turns the whole pet further, for a look from the side.
        if let Some(turn) = std::env::var("TURN")
            .ok()
            .and_then(|t| t.parse::<f32>().ok())
        {
            joints[0].rot.y += turn;
        }
        joints[0].shift.y += HOP.with(|h| h.get());
        let bones = sk.pose(&joints);
        // The animator's eyes (`life`), and the tongue out, as the
        // renderer shows them.
        let look = EYES.with(|e| e.get()).unwrap_or(look);
        let fidget = walker.fidget();
        let look = match fidget.tongue() {
            Some(Tongue::Blep) if look == Look::Open => Look::Blep,
            Some(Tongue::Pant) if matches!(look, Look::Open | Look::Happy) => Look::Pant,
            _ => look,
        };
        if name == "fidget" {
            let ears = fidget.ears.map(|e| if e.is_some() { "^" } else { "." });
            println!(
                "{f:03} {:?} ears {}{}",
                fidget.run.map(|r| r.kind),
                ears[0],
                ears[1]
            );
        }
        let start = Instant::now();
        let secs = canvas
            .draw(&gpu, &face(look), &bones, walker.yaw(), &mut frame)
            .expect("draw");
        wall += start.elapsed().as_secs_f64();
        gpu_secs.extend(secs);
        let wide = w * 2;
        let mut page = Pixmap::new(wide, h).expect("page");
        page.fill(Color::TRANSPARENT);
        let left = (wide - w) as f32 / 2.0 + x - 60.0 * scale;
        page.draw_pixmap(
            left.round() as i32,
            0,
            frame.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        page.save_png(format!("{out}/{name}-{f:03}.png"))
            .expect("png");
    }
    let n = frames as f64;
    println!(
        "{w}x{h}: {:.2} ms a frame wall (draw + readback)",
        wall / n * 1e3
    );
    if !gpu_secs.is_empty() {
        let avg = gpu_secs.iter().map(|s| f64::from(*s)).sum::<f64>() / gpu_secs.len() as f64;
        println!("GPU time {:.3} ms a frame", avg * 1e3);
    }
}
