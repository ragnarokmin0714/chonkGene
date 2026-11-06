//! The program's own icon (PLAN §1.9, "Program icon"): the Shiba, the Fold
//! and the chick piled in a nest. Baked from `assets/icon/src` by
//! `tools/iconbake`; the exe also embeds `chonkgene.ico` (build.rs).

use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

macro_rules! baked {
    ($($px:literal),*) => {
        [$(($px, include_bytes!(concat!("../../../assets/icon/chonkgene-", $px, ".png")))),*]
    };
}

/// Every baked size, smallest first. 16–32 are drawn on their own pixel grid.
const PNGS: [(u32, &[u8]); 9] = baked!(16, 20, 24, 32, 40, 48, 64, 128, 256);

/// The largest baked size as PNG (the Linux app-menu entry's icon).
pub fn png_256() -> &'static [u8] {
    PNGS[PNGS.len() - 1].1
}

/// The icon at `px` pixels square, as straight-alpha RGBA: the baked size
/// itself when there is one, else the next larger one scaled down.
pub fn rgba(px: u32) -> Vec<u8> {
    let px = px.clamp(1, 256);
    let (size, png) = PNGS
        .iter()
        .find(|(size, _)| *size >= px)
        .unwrap_or(&PNGS[PNGS.len() - 1]);
    let Ok(src) = Pixmap::decode_png(png) else {
        return vec![0; (px * px * 4) as usize];
    };
    let pm = if *size == px {
        src
    } else {
        let Some(mut dst) = Pixmap::new(px, px) else {
            return vec![0; (px * px * 4) as usize];
        };
        let s = px as f32 / *size as f32;
        let paint = PixmapPaint {
            quality: FilterQuality::Bicubic,
            ..PixmapPaint::default()
        };
        dst.draw_pixmap(
            0,
            0,
            src.as_ref(),
            &paint,
            Transform::from_scale(s, s),
            None,
        );
        dst
    };
    pm.pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

/// Icons for the tray, as (RGBA, side) pairs. Windows shows one image at its
/// small-icon size (16 px at 100 %, scaled with the display); a
/// StatusNotifierItem host picks from several.
pub fn tray(scale: f32) -> Vec<(Vec<u8>, u32)> {
    if cfg!(windows) {
        let px = (16.0 * scale.max(1.0)).round() as u32;
        vec![(rgba(px), px)]
    } else {
        [16, 20, 24, 32, 48, 64].map(|px| (rgba(px), px)).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_baked_size_decodes_at_its_size() {
        for (size, png) in PNGS {
            let pm = Pixmap::decode_png(png).expect("a baked PNG decodes");
            assert_eq!((pm.width(), pm.height()), (size, size));
            assert!(
                pm.pixels().iter().any(|p| p.alpha() == 255),
                "{size} px is not empty"
            );
        }
    }

    #[test]
    fn in_between_sizes_are_scaled_from_the_next_larger() {
        for px in [1, 16, 18, 28, 100, 256, 300] {
            let side = px.clamp(1, 256);
            assert_eq!(rgba(px).len(), (side * side * 4) as usize, "{px} px");
        }
        // 28 px comes from the 32 px drawing, so its corners stay clear.
        let img = rgba(28);
        assert_eq!(img[3], 0);
        assert!(img.chunks(4).any(|p| p[3] == 255));
    }

    #[test]
    fn the_exe_icon_holds_every_size_windows_asks_for() {
        let ico = include_bytes!("../../../assets/icon/chonkgene.ico");
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
        let sides: Vec<u32> = (0..count)
            .map(|i| match ico[6 + 16 * i] {
                0 => 256,
                s => s as u32,
            })
            .collect();
        assert_eq!(sides, [16, 20, 24, 32, 40, 48, 64, 256]);
    }
}
