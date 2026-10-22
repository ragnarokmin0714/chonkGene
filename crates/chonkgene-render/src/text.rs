//! Text for bubbles and menus: glyph outlines from skrifa, filled by
//! tiny-skia like the pets, so text and art share one anti-aliasing.
//! Chinese needs no shaping; kerning is skipped.

use std::collections::HashMap;

use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::{FontRef, LocationRef, MetadataProvider, Size};
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Transform};

/// jf open 粉圓體 2.1, unmodified (its Reserved Font Names forbid renaming
/// only for modified versions; PLAN §1.9).
pub const HUNINN: &[u8] = include_bytes!("../../../assets/fonts/jf-openhuninn-2.1.ttf");
/// 芫荽 Iansui 1.020.
pub const IANSUI: &[u8] = include_bytes!("../../../assets/fonts/Iansui-Regular.ttf");

struct Glyph {
    /// In font units, y pointing down.
    path: Option<Path>,
    advance: f32,
}

pub struct Font {
    font: FontRef<'static>,
    upem: f32,
    ascent: f32,
    descent: f32,
    glyphs: HashMap<char, Option<Glyph>>,
}

impl Font {
    pub fn new(data: &'static [u8]) -> Result<Self, String> {
        let font = FontRef::new(data).map_err(|e| format!("cannot read font: {e}"))?;
        let m = font.metrics(Size::unscaled(), LocationRef::default());
        Ok(Self {
            font,
            upem: f32::from(m.units_per_em),
            ascent: m.ascent,
            descent: -m.descent,
            glyphs: HashMap::new(),
        })
    }

    fn glyph(&mut self, ch: char) -> Option<&Glyph> {
        let font = &self.font;
        self.glyphs
            .entry(ch)
            .or_insert_with(|| {
                let id = font.charmap().map(ch)?;
                let advance = font
                    .glyph_metrics(Size::unscaled(), LocationRef::default())
                    .advance_width(id)?;
                let outline = font.outline_glyphs().get(id)?;
                let mut pen = Pen(PathBuilder::new());
                let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
                outline.draw(settings, &mut pen).ok()?;
                Some(Glyph {
                    path: pen.0.finish(),
                    advance,
                })
            })
            .as_ref()
    }
}

struct Pen(PathBuilder);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, -y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, -y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(cx0, -cy0, x, -y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.cubic_to(cx0, -cy0, cx1, -cy1, x, -y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// Fonts in fallback order: a character missing from the first is taken
/// from the next.
pub struct Face {
    fonts: Vec<Font>,
}

impl Face {
    /// Speech bubbles: Iansui, falling back to huninn for names users type.
    pub fn bubble() -> Self {
        Self {
            fonts: [IANSUI, HUNINN]
                .into_iter()
                .filter_map(|d| Font::new(d).ok())
                .collect(),
        }
    }

    /// Menus and labels.
    pub fn ui() -> Self {
        Self {
            fonts: Font::new(HUNINN).into_iter().collect(),
        }
    }

    /// (font index, advance in font units) for `ch`.
    fn pick(&mut self, ch: char) -> Option<(usize, f32)> {
        (0..self.fonts.len()).find_map(|i| self.fonts[i].glyph(ch).map(|g| (i, g.advance)))
    }

    fn scale(&self, font: usize, size: f32) -> f32 {
        size / self.fonts[font].upem
    }

    /// Width of `text` at `size` pixels.
    pub fn measure(&mut self, text: &str, size: f32) -> f32 {
        text.chars()
            .filter_map(|ch| self.pick(ch).map(|(f, adv)| adv * self.scale(f, size)))
            .sum()
    }

    /// Ascent and descent of the first font at `size`, both positive.
    pub fn extent(&self, size: f32) -> (f32, f32) {
        self.fonts
            .first()
            .map(|f| (f.ascent * size / f.upem, f.descent * size / f.upem))
            .unwrap_or((size, 0.0))
    }

    /// Draw `text` with its baseline starting at `(x, y)`.
    pub fn draw(&mut self, pm: &mut Pixmap, text: &str, x: f32, y: f32, size: f32, paint: &Paint) {
        let mut pen_x = x;
        for ch in text.chars() {
            let Some((f, advance)) = self.pick(ch) else {
                continue;
            };
            let s = self.scale(f, size);
            if let Some(path) = self.fonts[f].glyph(ch).and_then(|g| g.path.as_ref()) {
                let ts = Transform::from_translate(pen_x, y).pre_scale(s, s);
                pm.fill_path(path, paint, FillRule::Winding, ts, None);
            }
            pen_x += advance * s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_fonts_cover_the_pet_lines_and_ui() {
        let mut bubble = Face::bubble();
        let mut ui = Face::ui();
        for text in [
            "哈哈哈哈！不行了啦～",
            "……人家在睡覺耶。",
            "全部收起來",
            "Ctrl+Alt+P",
        ] {
            assert!(bubble.measure(text, 20.0) > 0.0);
            assert!(ui.measure(text, 20.0) > 0.0);
        }
    }

    #[test]
    fn wider_text_measures_wider() {
        let mut ui = Face::ui();
        assert!(ui.measure("摸摸頭", 16.0) > ui.measure("摸", 16.0) * 2.5);
    }
}
