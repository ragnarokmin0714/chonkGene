//! Every thing on the desktop and every effect, on one board, to look at
//! (PLAN §8, Roger's fourth pass).
//!
//! `cargo run --release -p chonkgene-render --example props -- <out dir> [scale]`
//! writes `props.png`: each thing at three moments (its spin and clock
//! moving on), then each effect part way through, the water and the
//! fox's fire.

use chonkgene_core::play::Thing;
use chonkgene_render::props::{self, Fx, Look};
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

const THINGS: [Thing; 9] = [
    Thing::Ball,
    Thing::Frisbee,
    Thing::Catnip,
    Thing::Bone,
    Thing::Seeds,
    Thing::Branch,
    Thing::Pond,
    Thing::Sunbeam,
    Thing::Butterfly,
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("usage: props <out dir> [scale]");
        std::process::exit(2);
    };
    let scale: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1.5);
    std::fs::create_dir_all(out).expect("out dir");
    let cell = (360.0 * scale) as u32;
    let row = (340.0 * scale) as u32;
    let mut board = Pixmap::new(cell * 3, row * (THINGS.len() as u32 / 3 + 2)).expect("board");
    board.fill(Color::from_rgba8(0xF4, 0xF1, 0xEC, 255));
    for (i, thing) in THINGS.iter().enumerate() {
        let ((w, h), foot) = props::canvas(*thing, scale);
        let mut pm = Pixmap::new(w, h).expect("pixmap");
        props::draw(
            &mut pm,
            *thing,
            foot,
            scale,
            Look {
                spin: 0.7,
                t: 1.7,
                used: 0.0,
                grounded: true,
                flip: false,
            },
        );
        let (cx, cy) = ((i as u32 % 3) * cell, (i as u32 / 3) * row);
        board.draw_pixmap(
            (cx + (cell - w) / 2) as i32,
            (cy + row - h - 10) as i32,
            pm.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    // Effects, on a cell each.
    let y = (THINGS.len() as u32 / 3) * row;
    let mut fxpm = Pixmap::new(cell * 3, row).expect("fx");
    for (k, fx) in [Fx::Hearts, Fx::Sparkles, Fx::Dust, Fx::Splash]
        .iter()
        .enumerate()
    {
        let x = (k as f32 + 0.5) * cell as f32 * 0.75;
        props::effect(
            &mut fxpm,
            *fx,
            (x, 0.6 * row as f32),
            scale,
            0.35 * fx.secs(),
        );
    }
    board.draw_pixmap(
        0,
        y as i32,
        fxpm.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    let mut last = Pixmap::new(cell * 3, row).expect("last");
    props::water(
        &mut last,
        0.5 * cell as f32,
        0.6 * row as f32,
        90.0 * scale,
        scale,
        1.3,
    );
    props::foxfire(
        &mut last,
        (1.8 * cell as f32, 0.5 * row as f32),
        (90.0 * scale, 40.0 * scale),
        scale,
        1.0,
    );
    board.draw_pixmap(
        0,
        (y + row) as i32,
        last.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    board.save_png(format!("{out}/props.png")).expect("png");
}
