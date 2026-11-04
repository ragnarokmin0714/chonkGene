//! Bakes the program icon: `cargo run --release` here (or with
//! `--manifest-path tools/iconbake/Cargo.toml`) after changing any SVG in
//! `assets/icon/src`, then commit the PNGs and `chonkgene.ico` it rewrites.
//!
//! Small sizes come from sources drawn on their own pixel grid; scaling the
//! 256 px drawing down that far blurs the faces into blobs.

use std::fs;
use std::path::{Path, PathBuf};

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};

/// Every size baked, with the source drawn for it.
const SIZES: [(u32, &str); 9] = [
    (16, "chonkgene-16.svg"),
    (20, "chonkgene-24.svg"),
    (24, "chonkgene-24.svg"),
    (32, "chonkgene-32.svg"),
    (40, "chonkgene.svg"),
    (48, "chonkgene.svg"),
    (64, "chonkgene.svg"),
    (128, "chonkgene.svg"),
    (256, "chonkgene.svg"),
];

/// The sizes Windows asks an .ico for (Explorer views, taskbar, title bar,
/// Alt-Tab, at 100–300 % scaling).
const ICO: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon");
    let mut pngs = Vec::new();
    for (px, src) in SIZES {
        let png = render(&dir.join("src").join(src), px);
        fs::write(out(&dir, px), &png).expect("the PNG is written");
        pngs.push((px, png));
    }
    let entries: Vec<_> = pngs.iter().filter(|(px, _)| ICO.contains(px)).collect();
    fs::write(dir.join("chonkgene.ico"), ico(&entries)).expect("the .ico is written");
    println!(
        "baked {} PNGs and chonkgene.ico into {}",
        pngs.len(),
        dir.display()
    );
}

fn out(dir: &Path, px: u32) -> PathBuf {
    dir.join(format!("chonkgene-{px}.png"))
}

fn render(svg: &Path, px: u32) -> Vec<u8> {
    let data = fs::read(svg).unwrap_or_else(|e| panic!("{}: {e}", svg.display()));
    let tree = Tree::from_data(&data, &Options::default()).expect("the SVG parses");
    let mut pm = Pixmap::new(px, px).expect("a non-empty size");
    let s = px as f32 / tree.size().width();
    resvg::render(&tree, Transform::from_scale(s, s), &mut pm.as_mut());
    pm.encode_png().expect("the PNG encodes")
}

/// An .ico holding each image as PNG, which Windows Vista and later read at
/// every size.
fn ico(images: &[&(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (px, png) in images {
        // 256 is written as 0: the field is one byte.
        let side = if *px >= 256 { 0 } else { *px as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in images {
        out.extend_from_slice(png);
    }
    out
}
