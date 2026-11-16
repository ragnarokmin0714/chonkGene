//! The pond's beach on a desktop-blue board, to look at its dunes (PLAN
//! §8, Roger's twentieth pass, item 5):
//! `cargo run --release -p chonkgene-render --example beach -- <out dir> [scale]`
//! writes `beach.png`, the beach with its pond, and the line a pet's feet
//! follow over the dunes marked with dots.

use chonkgene_core::play::{Thing, dune_height};
use chonkgene_render::props::{self, Look};
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PixmapPaint, Transform};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("usage: beach <out dir> [scale]");
        std::process::exit(2);
    };
    let s: f32 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(1.0);
    std::fs::create_dir_all(out).expect("out dir");
    let ((bw, bh), bfoot) = props::canvas(Thing::Beach, s);
    let ((pw, ph), pfoot) = props::canvas(Thing::Pond, s);
    let room = (60.0 * s) as u32;
    let mut board = Pixmap::new(bw + 2 * room, bh.max(ph) + 2 * room).expect("board");
    board.fill(Color::from_rgba8(0x7C, 0xA8, 0xCE, 255));
    // The floor: where both feet stand.
    let floor = (room + bh.max(ph)) as f32 - (bh as f32 - bfoot.1);
    let mid = board.width() as f32 / 2.0;
    let look = Look {
        t: 5.0,
        grounded: true,
        ..Look::default()
    };
    let mut beach = Pixmap::new(bw, bh).expect("beach");
    props::draw(&mut beach, Thing::Beach, bfoot, s, look);
    let mut pond = Pixmap::new(pw, ph).expect("pond");
    props::draw(&mut pond, Thing::Pond, pfoot, s, look);
    for (pm, foot) in [(&beach, bfoot), (&pond, pfoot)] {
        board.draw_pixmap(
            (mid - foot.0).round() as i32,
            (floor - foot.1).round() as i32,
            pm.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    // Where a pet's feet go over the sand.
    let mut paint = Paint::default();
    paint.set_color_rgba8(0xE0, 0x40, 0x60, 220);
    let half = 0.5 * Thing::Beach.size().0;
    let mut x = -half;
    while x <= half {
        let y = floor - dune_height(x) * s;
        if let Some(c) = PathBuilder::from_circle(mid + x * s, y, 1.6 * s) {
            board.fill_path(
                &c,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        x += 20.0;
    }
    board.save_png(format!("{out}/beach.png")).expect("write");
}
