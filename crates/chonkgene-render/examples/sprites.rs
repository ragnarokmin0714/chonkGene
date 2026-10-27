//! Every sprite of a species, for looking at while modelling:
//! `cargo run --release -p chonkgene-render --example sprites -- <species> <out dir>`
//! writes each pose and face (tail included), each coat sitting, and the
//! 麻糬 body.

use chonkgene_render::KEYS;
use chonkgene_render::sprites::{Variant, render_layer};
use chonkgene_rig::{Layer, builtin};
use tiny_skia::{IntSize, Pixmap};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(id), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!("usage: sprites <species> <out dir>");
        std::process::exit(2);
    };
    let model = builtin::load(id).unwrap_or_else(|e| panic!("{e}"));
    std::fs::create_dir_all(out).expect("out dir");
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let save = |name: String, v: Variant, key| {
        let s = render_layer(&model, v, key, Layer::All, threads);
        let pm = Pixmap::from_vec(s.rgba, IntSize::from_wh(s.width, s.height).unwrap()).unwrap();
        pm.save_png(format!("{out}/{name}.png")).expect("png");
    };
    let base = Variant {
        coat: 0,
        chonk: 0.33,
        scale: 1.0,
    };
    for (i, key) in KEYS.iter().enumerate() {
        save(
            format!("{i:02}-{}-{:?}", key.pose.name(), key.look).to_lowercase(),
            base,
            *key,
        );
    }
    for (c, coat) in model.coats.iter().enumerate() {
        save(
            format!("coat-{}", coat.name),
            Variant { coat: c, ..base },
            KEYS[0],
        );
    }
    save("mochi".into(), Variant { chonk: 1.0, ..base }, KEYS[0]);
}
