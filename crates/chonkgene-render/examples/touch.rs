//! Frames of a mesh pet being touched, feeling things and asleep, drawn the
//! way the app draws them (animator → mesh renderer), for tuning without a
//! desktop:
//! `cargo run --release -p chonkgene-render --example touch -- <species> <out dir> [scene]`
//! writes `<scene>-<frame>.png` at 15 fps. Scenes: `poke` (a quick poke in
//! the belly), `rub` (a long belly rub), `chin`, `head`, `cheek`, `tail`,
//! `paw`, `bottom`, `back`, `wing` (each held, then let go), `trap` (the
//! 曼赤肯's belly rubbed into its trap), `scratch` (held on the head
//! and rubbed about), `laugh`, `fedup`, `sulk`, and
//! `sleep-chin`, `sleep-curl`, `sleep-side`, `sleep-up`.

use std::sync::Arc;

use chonkgene_core::Zone;
use chonkgene_core::anim::{Animator, Expression, HOLD_SECS};
use chonkgene_core::species::Species;
use chonkgene_render::MeshRenderer;
use chonkgene_render::gpu::Gpu;
use chonkgene_render::skin::{SleepPose, mesh_model};

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
}

fn scenes() -> Vec<Scene> {
    let touch = |name, zone, from, to, secs| Scene {
        name,
        secs,
        expression: Expression::Neutral,
        hold: Some(Hold { zone, from, to }),
        sleep: None,
        walk_first: matches!(zone, Zone::Tail | Zone::Bottom | Zone::Back),
    };
    let feel = |name, expression, secs| Scene {
        name,
        secs,
        expression,
        hold: None,
        sleep: None,
        walk_first: false,
    };
    let sleep = |name, pose| Scene {
        name,
        secs: 4.0,
        expression: Expression::Asleep,
        hold: None,
        sleep: Some(pose),
        walk_first: false,
    };
    vec![
        touch("poke", Zone::Belly, 0.5, 0.75, 2.5),
        touch("rub", Zone::Belly, 0.3, 3.3, 4.5),
        touch("chin", Zone::Chin, 0.3, 2.8, 3.8),
        touch("head", Zone::Head, 0.3, 2.8, 3.8),
        touch("cheek", Zone::Cheek, 0.3, 2.3, 3.2),
        touch("tail", Zone::Tail, 0.3, 1.8, 3.2),
        touch("paw", Zone::Paw, 0.3, 2.3, 3.2),
        touch("bottom", Zone::Bottom, 0.3, 2.3, 3.2),
        touch("back", Zone::Back, 0.3, 2.3, 3.2),
        touch("wing", Zone::Wing, 0.3, 2.3, 3.2),
        // The 曼赤肯's belly rubbed long enough to spring its trap.
        touch("trap", Zone::Belly, 0.3, 5.5, 6.5),
        touch("scratch", Zone::Head, 0.3, 3.0, 4.0),
        feel("laugh", Expression::Giggle, 3.0),
        feel("fedup", Expression::FedUp, 3.0),
        feel("sulk", Expression::Sulk, 4.5),
        sleep("sleep-chin", SleepPose::Chin),
        sleep("sleep-curl", SleepPose::Curl),
        sleep("sleep-side", SleepPose::Side),
        sleep("sleep-up", SleepPose::BellyUp),
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
    let species = Species::from_id(id).unwrap_or_else(|| panic!("no species {id:?}"));
    let gpu: &'static Gpu = Box::leak(Box::new(Gpu::new().expect("a GPU")));
    gpu.set_budget(1.0);
    let (model, _) = mesh_model(id).unwrap_or_else(|e| panic!("{e}"));
    let model = Arc::new(model);
    std::fs::create_dir_all(out).expect("out dir");
    for scene in scenes() {
        if only.is_some_and(|o| o != scene.name) {
            continue;
        }
        let mut r = MeshRenderer::new(gpu, Arc::clone(&model), 1.0, 0.33, 0).expect("renderer");
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
        let frames = (scene.secs * FPS).round() as u32;
        for f in 0..frames {
            let t = f as f32 / FPS;
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
                    a.poke(zone);
                    pressed = None;
                    if rub {
                        a.expression = Expression::Happy;
                    }
                }
                if a.expression == Expression::Happy {
                    expression = Expression::Happy;
                }
            }
            a.expression = expression;
            for _ in 0..4 {
                a.step(0.25 / FPS);
            }
            let params = a.params();
            if expression == Expression::Asleep {
                r.render_asleep(&params, t);
                r.finish();
                r.render_asleep(&params, t);
            } else {
                r.render(&params, expression);
                r.finish();
            }
            let path = format!("{out}/{}-{f:03}.png", scene.name);
            r.pixmap().save_png(&path).expect("write frame");
        }
        println!("{}: {frames} frames", scene.name);
    }
}
