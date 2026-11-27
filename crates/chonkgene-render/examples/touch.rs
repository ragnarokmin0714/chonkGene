//! Frames of a mesh pet being touched, feeling things and asleep, drawn the
//! way the app draws them (animator → mesh renderer), for tuning without a
//! desktop:
//! `cargo run --release -p chonkgene-render --example touch -- <species> <out dir> [scene]`
//! writes `<scene>-<frame>.png` at 15 fps (`SLOW=4` in the environment:
//! four times slower, to check a gait frame by frame; `GLOW=1`: the fox
//! casting all through, her flame tails). Scenes: `poke` (a quick poke in
//! the belly), `rub` (a long belly rub), `chin`, `head`, `cheek`, `tail`,
//! `paw`, `bottom`, `back`, `wing` (each held, then let go), `pat` (your
//! hand patting its head, 摸摸頭 from its menu), `squeeze` (held still
//! on a cheek: your hands squeeze both, let go they spring out rosy and
//! it rubs its face or its cheeks), `chinscratch` (two taps on its chin:
//! your hand scratches it from below, each its own way), `trap` (the
//! 曼赤肯's belly rubbed into its trap), `scratch` (held on the head
//! and rubbed about), `laugh`, `fedup`, `sulk`, `jog` and `run` (running
//! at pace 1.6, half way into a gallop, and 2.2, a full one),
//! `sleep-chin`, `sleep-curl`, `sleep-side`, `sleep-up`, `sit` (sitting
//! and staying, 坐下), and `bury` (the Shiba lying, grinning, the beach's
//! sand piled over it and burst out of). `fox-human` is the fox's human
//! form, in hairdo `STYLE` and hair colour `HAIR` from the environment;
//! `COAT=` picks a coat by name; `SCALE=` draws it bigger.

use std::sync::Arc;

use chonkgene_core::Zone;
use chonkgene_core::anim::{Act, Animator, Expression, HOLD_SECS, Hug, HugHold, Posture};
use chonkgene_core::species::Species;
use chonkgene_render::MeshRenderer;
use chonkgene_render::gpu::Gpu;
use chonkgene_render::skin::{SleepPose, human, mesh_model};

const FPS: f32 = 15.0;

/// A touch: on which zone, from when to when (seconds into the scene).
struct Hold {
    zone: Zone,
    from: f32,
    to: f32,
}

struct Scene {
    name: &'static str,
    secs: f32,
    expression: Expression,
    hold: Option<Hold>,
    sleep: Option<SleepPose>,
    /// Walk a little first, so it stands side-on (a tail shows).
    walk_first: bool,
    /// A hug in this part, from 0.3 s in.
    hug: Option<Hug>,
    /// Held all through (sliding, gliding, flying).
    posture: Option<Posture>,
    /// Walking all through at this pace (running).
    pace: Option<f32>,
}

fn scenes() -> Vec<Scene> {
    let touch = |name, zone, from, to, secs| Scene {
        name,
        secs,
        expression: Expression::Neutral,
        hold: Some(Hold { zone, from, to }),
        sleep: None,
        // Sitting facing you, a four-legged pet's hind feet are under it.
        walk_first: matches!(zone, Zone::Tail | Zone::Bottom | Zone::Back | Zone::Foot),
        hug: None,
        posture: None,
        pace: None,
    };
    let feel = |name, expression, secs| Scene {
        name,
        secs,
        expression,
        hold: None,
        sleep: None,
        walk_first: false,
        hug: None,
        posture: None,
        pace: None,
    };
    let sleep = |name, pose| Scene {
        name,
        secs: 4.0,
        expression: Expression::Asleep,
        hold: None,
        sleep: Some(pose),
        walk_first: false,
        hug: None,
        posture: None,
        pace: None,
    };
    let hug = |name, role: Hug| Scene {
        name,
        secs: role.secs() + 0.6,
        expression: Expression::Happy,
        hold: None,
        sleep: None,
        walk_first: true,
        hug: Some(role),
        posture: None,
        pace: None,
    };
    let travel = |name, posture| Scene {
        name,
        secs: 2.0,
        expression: Expression::Happy,
        hold: None,
        sleep: None,
        walk_first: true,
        hug: None,
        posture: Some(posture),
        pace: None,
    };
    let run = |name, pace| Scene {
        name,
        secs: 2.0,
        expression: Expression::Happy,
        hold: None,
        sleep: None,
        walk_first: true,
        hug: None,
        posture: None,
        pace: Some(pace),
    };
    // Lying, grinning, then buried to the neck (the app draws the sand
    // mound over its picture the same way).
    let bury = Scene {
        name: "bury",
        secs: 4.0,
        expression: Expression::Neutral,
        hold: None,
        sleep: None,
        walk_first: true,
        hug: None,
        posture: Some(Posture::Lie),
        pace: None,
    };
    vec![
        bury,
        run("jog", 1.6),
        run("run", 2.2),
        hug("hug-front", Hug::Cuddle { front: true }),
        hug("hug-back", Hug::Cuddle { front: false }),
        hug("hug-bear", Hug::Bear),
        hug("hug-squeezed", Hug::Squeezed),
        hug("hug-you", Hug::ByYou(HugHold::Wrap)),
        hug("hug-carry", Hug::ByYou(HugHold::Carry)),
        hug("hug-cradle", Hug::ByYou(HugHold::Cradle)),
        // Picked up asleep: rocked without waking (Roger's log, 2026-11-16).
        Scene {
            name: "hug-asleep",
            expression: Expression::Asleep,
            sleep: Some(SleepPose::Curl),
            walk_first: false,
            ..hug("", Hug::ByYou(HugHold::Cradle))
        },
        travel("slide", Posture::Slide),
        travel("glide", Posture::Glide),
        travel("fly", Posture::Fly),
        touch("poke", Zone::Belly, 0.5, 0.75, 2.5),
        touch("rub", Zone::Belly, 0.3, 3.3, 4.5),
        touch("chin", Zone::Chin, 0.3, 2.8, 3.8),
        touch("head", Zone::Head, 0.3, 2.8, 3.8),
        touch("cheek", Zone::Cheek, 0.3, 2.3, 3.2),
        touch("squeeze", Zone::Cheek, 0.3, 2.3, 6.0),
        touch("tail", Zone::Tail, 0.3, 1.8, 3.2),
        touch("paw", Zone::Paw, 0.3, 2.3, 3.2),
        touch("foot", Zone::Foot, 0.3, 2.3, 3.2),
        touch("shoulder", Zone::Shoulder, 0.3, 2.3, 3.2),
        touch("bottom", Zone::Bottom, 0.3, 2.3, 3.2),
        touch("back", Zone::Back, 0.3, 2.3, 3.2),
        touch("wing", Zone::Wing, 0.3, 2.3, 3.2),
        // The 曼赤肯's belly rubbed long enough to spring its trap.
        touch("trap", Zone::Belly, 0.3, 5.5, 6.5),
        touch("scratch", Zone::Head, 0.3, 3.0, 4.0),
        feel("laugh", Expression::Giggle, 3.0),
        // 摸摸頭 from its menu: your hand pats its head, from 0.3 s.
        feel("pat", Expression::Neutral, 4.4),
        feel("fedup", Expression::FedUp, 3.0),
        // Patted on the bottom (item 6 (b)), from 0.3 s: a cat's 電梯屁股,
        // the Shiba's kick and look back, the fox's tails swung at you.
        feel("buttup", Expression::Happy, 3.0),
        feel("rump", Expression::Happy, 2.6),
        feel("swish", Expression::Neutral, 2.0),
        // Two taps on its chin: your hand scratches it from below, from
        // 0.3 s, each its own way.
        feel("chinscratch", Expression::Neutral, 7.2),
        feel("sulk", Expression::Sulk, 4.5),
        sleep("sleep-chin", SleepPose::Chin),
        sleep("sleep-curl", SleepPose::Curl),
        sleep("sleep-side", SleepPose::Side),
        sleep("sleep-up", SleepPose::BellyUp),
        Scene {
            secs: 4.0,
            walk_first: false,
            ..travel("sit", Posture::Sit)
        },
    ]
}

/// The middle of where `zone` shows in the picture: the pixel among those
/// that hit it nearest their average.
fn spot(r: &MeshRenderer, zone: Zone) -> Option<(f32, f32)> {
    let (w, h) = (r.pixmap().width(), r.pixmap().height());
    let mut hits = Vec::new();
    for y in (0..h).step_by(3) {
        for x in (0..w).step_by(3) {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            if r.hit(fx, fy).and_then(Zone::from_name) == Some(zone) {
                hits.push((fx, fy));
            }
        }
    }
    let n = hits.len() as f32;
    let mid = hits
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
    hits.into_iter().min_by(|a, b| {
        let d = |p: &(f32, f32)| (p.0 - mid.0).powi(2) + (p.1 - mid.1).powi(2);
        d(a).total_cmp(&d(b))
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(id), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!("usage: touch <species> <out dir> [scene]");
        std::process::exit(2);
    };
    let only = args.get(2);
    let slow: f32 = std::env::var("SLOW")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0f32)
        .max(1.0);
    let human_form = id.strip_suffix("-human");
    let id = human_form.unwrap_or(id);
    let species = Species::from_id(id).unwrap_or_else(|| panic!("no species {id:?}"));
    let gpu: &'static Gpu = Box::leak(Box::new(Gpu::new().expect("a GPU")));
    gpu.set_budget(1.0);
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let (model, _) = match human_form {
        Some(id) => human(id, &env("STYLE"), &env("HAIR")),
        None => mesh_model(id),
    }
    .unwrap_or_else(|e| panic!("{e}"));
    let coat = model.coat(&env("COAT")).unwrap_or(0);
    let model = Arc::new(model);
    std::fs::create_dir_all(out).expect("out dir");
    for scene in scenes() {
        if only.is_some_and(|o| o != scene.name) {
            continue;
        }
        // `SCALE=2`: drawn twice the size (close-ups of the face).
        let scale: f32 = std::env::var("SCALE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);
        let mut r =
            MeshRenderer::new(gpu, Arc::clone(&model), scale, 0.33, coat).expect("renderer");
        r.sleep_in(scene.sleep);
        let mut a = Animator::new(3, 0.33, species.waddle())
            .with_pops(false)
            .with_habits(&[]);
        // Settle into the rest pose first, so the touch starts from it.
        r.finish();
        for k in 0..45 {
            a.walking = scene.walk_first && k < 40;
            a.step(1.0 / 30.0);
            r.render(&a.params(), Expression::Neutral);
            r.finish();
        }
        let mut pressed = None;
        let mut hugged = false;
        let mut gestured = false;
        let (mut pat_down, mut pat_was) = (false, false);
        let (mut chin_down, mut chin_was) = (false, false);
        let mut mound = None;
        let frames = (scene.secs * FPS * slow).round() as u32;
        for f in 0..frames {
            let t = f as f32 / FPS / slow;
            let mut expression = scene.expression;
            if let Some(h) = &scene.hold {
                if pressed.is_none() && t >= h.from && t < h.to {
                    let Some(at) = spot(&r, h.zone) else {
                        println!("{}: no {:?} in sight", scene.name, h.zone);
                        break;
                    };
                    a.press(h.zone, at);
                    a.fond = Some(species.fondness(h.zone));
                    pressed = Some((h.zone, at));
                }
                // A scratch: the finger rubbing to and fro.
                if scene.name == "scratch"
                    && let Some((_, at)) = pressed
                {
                    let wiggle = 10.0 * (std::f32::consts::TAU * 3.0 * t).sin();
                    a.knead((at.0 + wiggle, at.1));
                }
                // The app springs the trap once the belly has been rubbed
                // long enough.
                if let Some((Zone::Belly, _)) = pressed
                    && let Some(after) = species.belly_trap()
                    && a.held_secs() >= after
                    && a.params().touch.trap == 0.0
                    && a.held_secs() < after + 0.1
                {
                    a.spring_trap(1.8);
                }
                if let Some((zone, _)) = pressed
                    && t >= h.to
                {
                    // A long rub anywhere it does not mind pleases it (the
                    // app's pat).
                    let rub = a.held_secs() >= HOLD_SECS
                        && species.fondness(zone) != chonkgene_core::species::Fondness::Dislikes;
                    let squeezed = a.params().touch.squeeze > 0.0;
                    a.poke(zone);
                    pressed = None;
                    if rub {
                        a.expression = Expression::Happy;
                    }
                    // Its cheeks let go (the app's `cheeks_let_go`): a
                    // loved cheek, its face rubbed on your hand, pleased;
                    // otherwise pouting, rubbing them.
                    if squeezed {
                        use chonkgene_core::species::Fondness;
                        if species.fondness(zone) == Fondness::Loves {
                            a.gesture(Act::Nuzzle);
                        } else {
                            a.expression = Expression::Neutral;
                            a.gesture(Act::CheekRub);
                        }
                    }
                }
                if a.expression == Expression::Happy {
                    expression = Expression::Happy;
                }
            }
            if let Some(role) = scene.hug
                && !hugged
                && t >= 0.3
            {
                a.hug = role;
                a.gesture(Act::Hug);
                hugged = true;
            }
            if !gestured && t >= 0.3 {
                gestured = true;
                match scene.name {
                    "buttup" => a.gesture(Act::ButtUp),
                    "swish" => a.gesture(Act::Swish),
                    "rump" => a.pat_rump(),
                    _ => {}
                }
            }
            if let Some(p) = scene.posture {
                a.posture = p;
            }
            a.grin = scene.name == "bury" && t >= 0.6;
            if let Some(pace) = scene.pace {
                a.pace = pace;
                a.walking = true;
            }
            // 摸摸頭, as the app drives it (`Pet::pat_step`): pressed in at
            // each pat and let go between, rubbed along while stroked;
            // happy once the hand has gone.
            use chonkgene_render::props::{PatKind, draw_pat_hand, pat_hand, pat_pose};
            let pat_t = t - 0.3;
            let pat = (scene.name == "pat")
                .then(|| pat_pose(PatKind::Pat, pat_t))
                .flatten();
            if scene.name == "pat" {
                let crown = r.anchor();
                let at = |p: &chonkgene_render::props::PatPose| {
                    pat_hand(crown, a.facing, scale, p).press
                };
                match pat {
                    Some(p) if p.down && !pat_down => {
                        a.press(Zone::Head, at(&p));
                        a.fond = Some(species.fondness(Zone::Head));
                    }
                    Some(p) if p.down && p.stroking => a.knead(at(&p)),
                    Some(p) if !p.down && pat_down => a.poke(Zone::Head),
                    None if pat_down => a.poke(Zone::Head),
                    _ => {}
                }
                if pat_was && pat.is_none() {
                    a.expression = Expression::Happy;
                    a.cheer();
                }
                pat_down = pat.is_some_and(|p| p.down);
                pat_was = pat.is_some();
                a.patted = pat.is_some();
                if a.expression == Expression::Happy {
                    expression = Expression::Happy;
                }
            }
            // A chin scratch, as the app drives it (`Pet::chin_step`): the
            // hand on its chin pressing in, rubbing along as it strokes, off
            // it (no poke) between the Shiba's pats; happy once it is over.
            use chonkgene_render::props::{chin_hand, draw_chin_hand};
            let chin_t = t - 0.3;
            let chin_kind = species.chin_scratch();
            let build = chonkgene_render::props::Build::of(r.species());
            let chin = (scene.name == "chinscratch")
                .then(|| chin_kind.beats(chin_t))
                .flatten();
            if scene.name == "chinscratch" {
                if chin.is_some() && !chin_was {
                    a.scratch_chin(Some(chin_kind));
                }
                let hand = chin
                    .zip(r.chin_at())
                    .map(|(cb, at)| chin_hand(at, a.facing, scale, &cb, r.hook_at(), build));
                match (chin, hand) {
                    (Some(cb), Some(h)) if cb.on && !chin_down => {
                        a.press(Zone::Chin, h.press);
                        a.fond = Some(species.fondness(Zone::Chin));
                    }
                    (Some(cb), Some(h)) if cb.on => a.knead(h.press),
                    _ if chin_down => a.let_go(),
                    _ => {}
                }
                chin_down = chin.is_some_and(|cb| cb.on);
                if chin_was && chin.is_none() {
                    a.scratch_chin(None);
                    a.expression = Expression::Happy;
                }
                chin_was = chin.is_some();
                if a.expression == Expression::Happy {
                    expression = Expression::Happy;
                }
            }
            a.expression = expression;
            for _ in 0..4 {
                a.step(0.25 / FPS / slow);
            }
            // Your hands in a hug, found on its picture (the last one, as
            // the app does) and pressing in before it is drawn.
            use chonkgene_render::props::{
                BodyShape, Build, HandLayer, draw_hug_hands, hug_hands, hug_lift,
            };
            let (by_you, hold) = match scene.hug {
                Some(Hug::ByYou(hold)) if t >= 0.3 => (true, hold),
                _ => (false, HugHold::Wrap),
            };
            let secs = hold.secs();
            // Or squeezing its cheeks (a finger held still on one).
            let squeeze = a.params().touch.squeeze;
            let hands = r.body_at().filter(|_| by_you).and_then(|(belly, chest)| {
                let body = BodyShape {
                    belly,
                    chest,
                    armpits: r.armpits(),
                    edge: &|y, side| r.body_edge(y, side),
                    bottom: &|x| r.body_bottom(x),
                };
                let build = Build::of(r.species());
                hug_hands(&body, build, hold, a.facing, scale, t - 0.3, secs)
            });
            let hands = hands.or_else(|| {
                let cheeks = r.cheeks_at().filter(|_| squeeze > 0.0)?;
                chonkgene_render::props::cheek_hands(cheeks, scale, squeeze)
            });
            r.hug_press(hands.map(|h| {
                let [p, q] = h.hands;
                ([(p.press, p.grip), (q.press, q.grip)], h.firm)
            }));
            let mut params = a.params();
            // `GLOW=1`: the fox casting all through (her face, her tails).
            if std::env::var_os("GLOW").is_some() {
                params.glow = 1.0;
            }
            if expression == Expression::Asleep {
                r.render_asleep(&params, t);
                r.finish();
                r.render_asleep(&params, t);
            } else {
                r.render(&params, expression);
                r.finish();
            }
            let path = format!("{out}/{}-{f:03}.png", scene.name);
            if let (Some(cb), Some(at)) = (chin, r.chin_at()) {
                let mut pm = r.pixmap().clone();
                let hand = chin_hand(at, a.facing, scale, &cb, r.hook_at(), build);
                draw_chin_hand(&mut pm, &hand, chin_t);
                pm.save_png(&path).expect("write frame");
                continue;
            }
            if let Some(p) = pat {
                let mut pm = r.pixmap().clone();
                let hand = pat_hand(r.anchor(), a.facing, scale, &p);
                draw_pat_hand(&mut pm, &hand, pat_t);
                pm.save_png(&path).expect("write frame");
                continue;
            }
            if by_you || hands.is_some() {
                // The hands over it; held up in front, it is lifted too (the
                // app moves its window).
                let mut src =
                    tiny_skia::Pixmap::new(r.pixmap().width(), r.pixmap().height()).expect("frame");
                if let Some(h) = &hands {
                    draw_hug_hands(&mut src, h, HandLayer::Behind, scale, t - 0.3);
                }
                src.draw_pixmap(
                    0,
                    0,
                    r.pixmap().as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    tiny_skia::Transform::identity(),
                    None,
                );
                if let Some(h) = &hands {
                    draw_hug_hands(&mut src, h, HandLayer::Front, scale, t - 0.3);
                }
                // Room above it for the lift (the app moves its window up
                // instead), so its head is not cut off.
                const ROOM: i32 = 30;
                let lift = hug_lift(hold, scale, t - 0.3, secs).round() as i32;
                let mut pm =
                    tiny_skia::Pixmap::new(src.width(), src.height() + ROOM as u32).unwrap();
                pm.draw_pixmap(
                    0,
                    ROOM - lift,
                    src.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    tiny_skia::Transform::identity(),
                    None,
                );
                pm.save_png(&path).expect("write frame");
                continue;
            }
            if scene.name == "bury" && t >= 0.8 {
                // The sand piled on (from 0.8 s), then burst out of (3.2 s).
                use chonkgene_render::props::{head_round, mound_for, sand_mound};
                let mut pm = r.pixmap().clone();
                let floor = pm.height() as f32 - chonkgene_render::raymarch::FLOOR_PX;
                let head = head_round(r.anchor(), r.mouth().expect("a mouth"));
                let m = *mound.get_or_insert_with(|| mound_for(&pm, floor, head).expect("a pet"));
                let rise = ((t - 0.8) / 0.9).min(1.0);
                let burst = ((t - 3.2) / 0.6).clamp(0.0, 1.0);
                sand_mound(&mut pm, &m, floor, head, rise, burst, 1.0);
                pm.save_png(&path).expect("write frame");
                continue;
            }
            r.pixmap().save_png(&path).expect("write frame");
        }
        println!("{}: {frames} frames", scene.name);
    }
}
