//! Every mesh pet coloured by the hit zone under each pixel, the way a
//! click finds it (`MeshRenderer::hit`), sitting facing you and walking
//! side-on, for checking the zones by eye (PLAN §8, Roger's sixteenth
//! pass, item 4):
//! `cargo run --release -p chonkgene-render --example zones -- <out dir> [species…]`
//! writes `<species>.png` (the plain pet beside its zones) and
//! `legend.png`.

use std::sync::Arc;

use chonkgene_core::anim::{Animator, Expression};
use chonkgene_core::species::Species;
use chonkgene_render::MeshRenderer;
use chonkgene_render::gpu::Gpu;
use chonkgene_render::skin::mesh_model;
use tiny_skia::{Pixmap, PixmapPaint, Transform};

const SPECIES: [&str; 7] = ["shiba", "munchkin", "fold", "fox", "owl", "chick", "duck"];

/// A colour per zone, easy to tell apart.
fn colour(zone: &str) -> [u8; 3] {
    match zone {
        "head" => [0x4C, 0x9B, 0xE8],
        "chin" => [0xF2, 0x6B, 0x3A],
        "cheek" => [0xF2, 0x9C, 0xC8],
        "belly" => [0xF5, 0xD0, 0x3B],
        "paw" => [0x6A, 0x4C, 0x9C],
        "wing" => [0x30, 0x30, 0x38],
        "back" => [0x3A, 0xA0, 0x48],
        "bottom" => [0xC8, 0x3A, 0x3A],
        "tail" => [0x8A, 0x5A, 0x2E],
        _ => [0x80, 0x80, 0x80],
    }
}

/// The picture with each pixel tinted by the zone a click there finds.
fn tinted(r: &MeshRenderer) -> Pixmap {
    let mut pm = r.pixmap().clone();
    let w = pm.width();
    let data = pm.data_mut();
    for y in 0..r.pixmap().height() {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if data[i + 3] < 40 {
                continue;
            }
            let Some(zone) = r.hit(x as f32 + 0.5, y as f32 + 0.5) else {
                continue;
            };
            let c = colour(zone);
            let a = f32::from(data[i + 3]) / 255.0;
            for k in 0..3 {
                let shade = f32::from(data[i + k]) / 255.0;
                let tint = f32::from(c[k]) * a * (0.55 + 0.45 * shade);
                data[i + k] = tint.min(255.0) as u8;
            }
        }
    }
    pm
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("usage: zones <out dir> [species…]");
        std::process::exit(2);
    };
    let only: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    let gpu: &'static Gpu = Box::leak(Box::new(Gpu::new().expect("a GPU")));
    gpu.set_budget(1.0);
    std::fs::create_dir_all(out).expect("out dir");
    for id in SPECIES {
        if !only.is_empty() && !only.contains(&id) {
            continue;
        }
        let species = Species::from_id(id).expect("species");
        let (model, _) = mesh_model(id).unwrap_or_else(|e| panic!("{e}"));
        let mut tiles = Vec::new();
        // Sitting facing you, then walking side-on (its back and tail in
        // view).
        for walking in [false, true] {
            let mut r =
                MeshRenderer::new(gpu, Arc::new(model.clone()), 1.0, 0.33, 0).expect("renderer");
            let mut a = Animator::new(3, 0.33, species.waddle())
                .with_pops(false)
                .with_habits(&[]);
            r.finish();
            for _ in 0..45 {
                a.walking = walking;
                a.step(1.0 / 30.0);
                r.render(&a.params(), Expression::Neutral);
                r.finish();
            }
            tiles.push(r.pixmap().clone());
            tiles.push(tinted(&r));
        }
        let (tw, th) = (tiles[0].width(), tiles[0].height());
        let mut sheet = Pixmap::new(tw * tiles.len() as u32, th).expect("sheet");
        sheet.fill(tiny_skia::Color::from_rgba8(250, 246, 240, 255));
        for (k, t) in tiles.iter().enumerate() {
            sheet.draw_pixmap(
                (k as u32 * tw) as i32,
                0,
                t.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        sheet
            .save_png(format!("{out}/{id}.png"))
            .expect("write sheet");
        println!("{id}");
    }
}
