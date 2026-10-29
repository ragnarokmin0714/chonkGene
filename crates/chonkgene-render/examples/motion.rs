//! Frames of a pet moving, for tuning motion without a desktop:
//! `cargo run --release -p chonkgene-render --example motion -- <species> <out dir>`
//! writes `<scene>-<frame>.png` at 30 fps for a walk with a turn, a held poke,
//! a pat, each idle gesture, giggling and sulking. The pet walks across a
//! wider canvas, as its window would across the desktop.

use std::sync::Arc;

use chonkgene_core::Zone;
use chonkgene_core::anim::{Act, Animator, Expression};
use chonkgene_core::species::Species;
use chonkgene_render::Renderer;
use chonkgene_rig::builtin;
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

const FPS: f32 = 30.0;
/// Walking speed, pixels a second (chonkgene-core motion).
const WALK_SPEED: f32 = 60.0;

/// What happens at a moment of a scene.
type Script = fn(&mut Animator, f32, &mut f32);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(id), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!("usage: motion <species> <out dir>");
        std::process::exit(2);
    };
    let species = Species::from_id(id).unwrap_or_else(|| panic!("no species {id:?}"));
    let model = Arc::new(builtin::load(id).unwrap_or_else(|e| panic!("{e}")));
    std::fs::create_dir_all(out).expect("out dir");
    let mut r = Renderer::new(model, None, 1.0, 0.33, 0).expect("renderer");
    r.finish();

    let scenes: [(&str, f32, Script); 10] = [
        ("walk", 4.0, |a, t, x| {
            a.walking = t < 3.4;
            a.facing = if t < 1.8 { 1.0 } else { -1.0 };
            if a.walking {
                *x += WALK_SPEED * a.facing / FPS;
            }
        }),
        ("poke", 1.6, |a, t, _| {
            if t == 0.0 {
                a.press();
            }
            if (t - 0.4).abs() < 0.01 {
                a.poke(Zone::Belly);
            }
        }),
        ("pat", 1.6, |a, t, _| {
            if t == 0.0 {
                a.expression = Expression::Happy;
                a.cheer();
            }
        }),
        ("look-left", 2.0, |a, t, _| gesture(a, t, Act::LookLeft)),
        ("look-right", 2.0, |a, t, _| gesture(a, t, Act::LookRight)),
        ("stretch", 2.4, |a, t, _| gesture(a, t, Act::Stretch)),
        ("yawn", 2.4, |a, t, _| gesture(a, t, Act::Yawn)),
        ("wave", 2.0, |a, t, _| gesture(a, t, Act::Wave)),
        ("giggle", 2.0, |a, _, _| a.expression = Expression::Giggle),
        ("sulk", 3.0, |a, _, _| a.expression = Expression::Sulk),
    ];
    let (w, h) = Renderer::pixel_size(1.0);
    for (name, secs, script) in scenes {
        let mut a = Animator::new(3, 0.33, species.waddle());
        let mut x = 0.0f32;
        let frames = (secs * FPS).round() as u32;
        let wide = if name == "walk" { w * 2 } else { w };
        for f in 0..frames {
            let t = f as f32 / FPS;
            script(&mut a, t, &mut x);
            // Two physics steps a frame, as at 60 fps.
            a.step(0.5 / FPS);
            a.step(0.5 / FPS);
            // GPU frames arrive on a later call; a preview waits for each.
            r.render(&a.params(), a.expression);
            r.finish();
            let frame = r.pixmap().clone();
            let mut canvas = Pixmap::new(wide, h).expect("canvas");
            canvas.fill(Color::TRANSPARENT);
            let left = (wide - w) as f32 / 2.0 + x - if name == "walk" { 50.0 } else { 0.0 };
            canvas.draw_pixmap(
                left.round() as i32,
                0,
                frame.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
            canvas
                .save_png(format!("{out}/{name}-{f:03}.png"))
                .expect("png");
        }
    }
}

/// Start `act` a moment in, so the frames show the cut into it too.
fn gesture(a: &mut Animator, t: f32, act: Act) {
    if (t - 0.2).abs() < 0.01 {
        a.gesture(act);
    }
}
