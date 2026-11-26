//! Every mesh pet coloured by the hit zone under each pixel, the way a
//! click finds it (`MeshRenderer::hit`), sitting facing you and walking
//! side-on, at full size and at the smallest (50%), for checking the
//! zones by eye (PLAN §8, Roger's sixteenth pass, item 4; twenty-seventh
//! pass, item 2: shoulders, front and hind paws):
//! `cargo run --release -p chonkgene-render --example zones -- <out dir> [species…]`
//! writes `<species>.png` (the plain pet beside its zones; `fox-human` is
//! the fox's human form) and `legend.png`; `SCALES="2"` draws other sizes.

use std::sync::Arc;

use chonkgene_core::anim::{Animator, Expression};
use chonkgene_core::species::Species;
use chonkgene_render::MeshRenderer;
use chonkgene_render::gpu::Gpu;
use chonkgene_render::skin::{human, mesh_model};
use chonkgene_render::text::Face;
use tiny_skia::{Color, Paint, Pixmap, PixmapPaint, Rect, Transform};

const SPECIES: [&str; 8] = [
    "shiba",
    "munchkin",
    "fold",
    "fox",
    "fox-human",
    "owl",
    "chick",
    "duck",
];

/// The sizes drawn: full, and the smallest the settings allow.
const SCALES: [f32; 2] = [1.0, 0.5];

/// Each zone's colour, easy to tell apart, and its name in the settings'
/// words.
const ZONE_COLOURS: [(&str, [u8; 3], &str); 11] = [
    ("head", [0x4C, 0x9B, 0xE8], "頭"),
    ("chin", [0xF2, 0x6B, 0x3A], "下巴"),
    ("cheek", [0xF2, 0x9C, 0xC8], "臉頰"),
    ("shoulder", [0xD0, 0x2E, 0x8C], "肩"),
    ("paw", [0x6A, 0x4C, 0x9C], "手（前腳）"),
    ("wing", [0x30, 0x30, 0x38], "翅膀"),
    ("foot", [0x2C, 0xC0, 0xD8], "腳（後腳）"),
    ("back", [0x3A, 0xA0, 0x48], "背"),
    ("belly", [0xF5, 0xD0, 0x3B], "肚子"),
    ("bottom", [0xC8, 0x3A, 0x3A], "屁股"),
    ("tail", [0x8A, 0x5A, 0x2E], "尾巴"),
];

fn colour(zone: &str) -> [u8; 3] {
    ZONE_COLOURS
        .iter()
        .find(|(z, ..)| *z == zone)
        .map_or([0x80; 3], |(_, c, _)| *c)
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

fn paint(c: [u8; 3]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(c[0], c[1], c[2], 255));
    p.anti_alias = true;
    p
}

/// Every zone's swatch and name, in a row.
fn legend(out: &str) {
    let mut face = Face::ui();
    let (size, gap) = (22.0, 18.0);
    let width: f32 = ZONE_COLOURS
        .iter()
        .map(|(.., name)| 28.0 + face.measure(name, size) + gap)
        .sum();
    let mut pm = Pixmap::new(width.ceil() as u32 + 16, 44).expect("legend");
    pm.fill(Color::from_rgba8(250, 246, 240, 255));
    let mut x = 8.0;
    for (_, c, name) in ZONE_COLOURS {
        let r = Rect::from_xywh(x, 11.0, 22.0, 22.0).expect("swatch");
        pm.fill_rect(r, &paint(c), Transform::identity(), None);
        face.draw(
            &mut pm,
            name,
            x + 28.0,
            30.0,
            size,
            &paint([0x3B, 0x2A, 0x22]),
        );
        x += 28.0 + face.measure(name, size) + gap;
    }
    pm.save_png(format!("{out}/legend.png"))
        .expect("write legend");
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
    legend(out);
    for name in SPECIES {
        if !only.is_empty() && !only.contains(&name) {
            continue;
        }
        let human_form = name.strip_suffix("-human");
        let id = human_form.unwrap_or(name);
        let species = Species::from_id(id).expect("species");
        let (model, _) = match human_form {
            Some(id) => human(id, "", ""),
            None => mesh_model(id),
        }
        .unwrap_or_else(|e| panic!("{e}"));
        let model = Arc::new(model);
        // A row per size: sitting facing you, then walking side-on (its
        // back and tail in view), each plain and tinted.
        // `SCALES="2"`: other sizes (close-ups).
        let scales: Vec<f32> = std::env::var("SCALES")
            .ok()
            .map(|s| {
                s.split_whitespace()
                    .filter_map(|x| x.parse().ok())
                    .collect()
            })
            .unwrap_or_else(|| SCALES.to_vec());
        let mut rows = Vec::new();
        for scale in scales {
            let mut tiles = Vec::new();
            for walking in [false, true] {
                let mut r =
                    MeshRenderer::new(gpu, Arc::clone(&model), scale, 0.33, 0).expect("renderer");
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
            rows.push(tiles);
        }
        let width = rows
            .iter()
            .map(|t| t.iter().map(Pixmap::width).sum::<u32>())
            .max()
            .unwrap_or(1);
        let height = rows.iter().map(|t| t[0].height()).sum::<u32>();
        let mut sheet = Pixmap::new(width, height).expect("sheet");
        sheet.fill(Color::from_rgba8(250, 246, 240, 255));
        let mut y = 0;
        for tiles in &rows {
            let mut x = 0;
            for t in tiles {
                sheet.draw_pixmap(
                    x,
                    y,
                    t.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
                x += t.width() as i32;
            }
            y += tiles[0].height() as i32;
        }
        sheet
            .save_png(format!("{out}/{name}.png"))
            .expect("write sheet");
        println!("{name}");
    }
}
