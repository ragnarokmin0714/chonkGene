//! Draws chonkGene pets (docs/PLAN.md §3.1).
//!
//! Pets are 3D models ray-marched on the CPU into a few sprites per pet, off
//! the UI thread (`sprites`). A frame picks the sprite for the moment's pose
//! and face, then squashes, sways and mirrors it with one 2D transform, so a
//! frame costs a blit (PLAN §5).

pub mod bubble;
pub mod menu;
pub mod raymarch;
pub mod sprites;
pub mod text;

use std::sync::Arc;

use chonkgene_core::anim::{Expression, PoseParams};
use chonkgene_rig::{Look, Model, PoseId};
use tiny_skia::{
    Color, FillRule, FilterQuality, LineCap, LineJoin, Paint, Pixmap, PixmapPaint, Stroke,
    Transform,
};

use crate::raymarch::FLOOR_PX;
use crate::sprites::Sprites;
pub use crate::sprites::{KEYS, Key, Variant};

/// Breathing in place is quantised to this many steps, so idle frames are
/// copies of a few cached images instead of a filtered blit each (PLAN §5).
const BREATH_STEPS: u8 = 6;

pub struct Renderer {
    sprites: Sprites,
    pixmap: Pixmap,
    /// The sprite and transform of the last frame, for hit-testing.
    last: Option<(Key, Transform)>,
    /// Idle frames already drawn: (sprite, mirrored, breath step) → image.
    still: Vec<((Key, bool, u8), Pixmap)>,
    /// The pixmap no longer matches `last` (new sprites, overlays drawn).
    stale: bool,
    /// Whether the last `render` produced a different picture.
    changed: bool,
}

/// The sprite that shows an expression, given how open the eyes are.
pub fn key_for(expression: Expression, eyes_open: f32) -> Key {
    let eyes = if eyes_open > 0.66 {
        Look::Open
    } else if eyes_open > 0.25 {
        Look::Half
    } else {
        Look::Shut
    };
    let (pose, look) = match expression {
        Expression::Neutral => (PoseId::Sit, eyes),
        Expression::Happy => (PoseId::Sit, Look::Happy),
        Expression::Giggle => (PoseId::BellyUp, Look::Laugh),
        Expression::FedUp => (
            PoseId::Melt,
            if eyes == Look::Shut {
                Look::Shut
            } else {
                Look::FedUp
            },
        ),
        Expression::Sulk => (PoseId::Sulk, Look::Pout),
        Expression::Asleep => (PoseId::Loaf, Look::Asleep),
    };
    Key { pose, look }
}

impl Renderer {
    /// Pixel size of a pet drawn at `scale` (1.0 = 100%).
    pub fn pixel_size(scale: f32) -> (u32, u32) {
        raymarch::pixel_size(scale)
    }

    /// Starts rendering the pet's sprites in the background; frames are
    /// empty until the first one lands.
    pub fn new(model: Arc<Model>, scale: f32, chonk: f32, coat: usize) -> Option<Self> {
        let (w, h) = Self::pixel_size(scale);
        Some(Self {
            sprites: Sprites::new(model, Variant { coat, chonk, scale }),
            pixmap: Pixmap::new(w, h)?,
            last: None,
            still: Vec::new(),
            stale: true,
            changed: true,
        })
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    fn scale(&self) -> f32 {
        self.sprites.variant().scale
    }

    pub fn set_chonk(&mut self, chonk: f32) {
        let v = self.sprites.variant();
        if (chonk - v.chonk).abs() > 1e-3 {
            self.sprites.set_variant(Variant { chonk, ..v });
        }
    }

    pub fn set_coat(&mut self, coat: usize) {
        let v = self.sprites.variant();
        self.sprites.set_variant(Variant { coat, ..v });
    }

    /// Take in sprites finished in the background. True if any arrived, so
    /// the caller can redraw.
    pub fn poll(&mut self) -> bool {
        let any = self.sprites.poll();
        if any {
            self.still.clear();
            self.stale = true;
        }
        any
    }

    /// Wait for every sprite (tests, benchmarks, screenshots).
    pub fn finish(&mut self) {
        self.sprites.finish();
        self.still.clear();
        self.stale = true;
    }

    /// Is the first sprite in, so the pet can be seen?
    pub fn ready(&self) -> bool {
        self.sprites.best(KEYS[0]).is_some()
    }

    /// Did the last `render` change the picture? When it did not, there is
    /// nothing to show again (PLAN §5: no upload when the pose is unchanged).
    pub fn changed(&self) -> bool {
        self.changed
    }

    pub fn render(&mut self, params: &PoseParams, expression: Expression) -> &Pixmap {
        let want = key_for(expression, params.eyes_open);
        let Some((key, ready)) = self.sprites.best(want) else {
            self.changed = self.stale || self.last.is_some();
            self.pixmap.fill(Color::TRANSPARENT);
            (self.last, self.stale) = (None, false);
            return &self.pixmap;
        };
        let size = (self.pixmap.width(), self.pixmap.height());
        let paint = PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..PixmapPaint::default()
        };
        // Under ~1 px of squash at 100% counts as none.
        let still = squash(params).abs() < 0.004 && params.sway.abs() < 0.05;
        if !still {
            let ts = frame_transform(params, self.scale(), size);
            if !self.stale && self.last == Some((key, ts)) {
                self.changed = false;
                return &self.pixmap;
            }
            self.pixmap.fill(Color::TRANSPARENT);
            let crop = ts.pre_translate(ready.offset.0, ready.offset.1);
            self.pixmap
                .draw_pixmap(0, 0, ready.pixmap.as_ref(), &paint, crop, None);
            (self.last, self.stale, self.changed) = (Some((key, ts)), false, true);
            return &self.pixmap;
        }
        // Only breathing (and maybe a walking bob): copy a cached image,
        // shifted by whole pixels.
        let step = ((params.breath.clamp(-1.0, 1.0) + 1.0) / 2.0 * f32::from(BREATH_STEPS - 1))
            .round() as u8;
        let mirrored = params.facing < 0.0;
        let p = PoseParams {
            breath: f32::from(step) / f32::from(BREATH_STEPS - 1) * 2.0 - 1.0,
            bob: 0.0,
            ..*params
        };
        let ts = frame_transform(&p, self.scale(), size);
        let dy = (params.bob * self.scale()).round() as i32;
        let shown = Some((key, ts.post_translate(0.0, dy as f32)));
        if !self.stale && self.last == shown {
            self.changed = false;
            return &self.pixmap;
        }
        let id = (key, mirrored, step);
        if !self.still.iter().any(|(k, _)| *k == id) {
            // A different sprite makes the cached images useless.
            self.still
                .retain(|((k, m, _), _)| *k == key && *m == mirrored);
            let mut img = self.pixmap.clone();
            let crop = ts.pre_translate(ready.offset.0, ready.offset.1);
            img.draw_pixmap(0, 0, ready.pixmap.as_ref(), &paint, crop, None);
            self.still.push((id, img));
        }
        self.pixmap.fill(Color::TRANSPARENT);
        if let Some((_, img)) = self.still.iter().find(|(k, _)| *k == id) {
            shift_copy(img, &mut self.pixmap, dy);
        }
        (self.last, self.stale, self.changed) = (shown, false, true);
        &self.pixmap
    }

    /// Render a sleeping pet: the frame plus a snot bubble that swells with
    /// each breath and a drifting 「z」. `phase` is seconds asleep.
    pub fn render_asleep(&mut self, params: &PoseParams, phase: f32) -> &Pixmap {
        // The bubble and the z move every frame, and they are drawn over the
        // sprite, so the sprite is always drawn afresh.
        self.stale = true;
        self.render(params, Expression::Asleep);
        let Some((key, ts)) = self.last else {
            return &self.pixmap;
        };
        let Some(ready) = self.sprites.get(key) else {
            return &self.pixmap;
        };
        let s = self.scale();
        let (nx, ny) = ready.sprite.nose;
        let tau = std::f32::consts::TAU;

        // Snot bubble beside the nose, swelling with the slow sleeping breath.
        let swell = 0.5 + 0.5 * (phase * tau / 5.1).sin();
        if let Some(bubble) =
            tiny_skia::PathBuilder::from_circle(nx + 3.0 * s * swell, ny, (2.5 + 5.5 * swell) * s)
        {
            let mut fill = Paint::default();
            fill.set_color_rgba8(0xD6, 0xEE, 0xFA, 220);
            fill.anti_alias = true;
            self.pixmap
                .fill_path(&bubble, &fill, FillRule::Winding, ts, None);
            let mut edge = fill.clone();
            edge.set_color_rgba8(0xA9, 0xD6, 0xEE, 255);
            let stroke = Stroke {
                width: 1.2 * s,
                ..Stroke::default()
            };
            self.pixmap.stroke_path(&bubble, &edge, &stroke, ts, None);
        }

        // A 「z」 drifting up and fading every 2.4 s.
        let cycle = (phase / 2.4).fract();
        // Placed in sprite space but drawn upright, so a mirrored pet's z
        // still reads as a z.
        let mut at = [tiny_skia::Point::from_xy(
            nx + (24.0 + 6.0 * cycle) * s,
            ny - (48.0 + 14.0 * cycle) * s,
        )];
        ts.map_points(&mut at);
        let z = 8.0 * s;
        let (zx, zy) = (at[0].x - z / 2.0, at[0].y);
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(zx, zy);
        pb.line_to(zx + z, zy);
        pb.line_to(zx, zy + z);
        pb.line_to(zx + z, zy + z);
        if let Some(path) = pb.finish() {
            let mut ink = Paint::default();
            let alpha = (255.0 * (1.0 - cycle) * (cycle * 4.0).min(1.0)) as u8;
            ink.set_color_rgba8(0x8E, 0x7C, 0x6E, alpha);
            ink.anti_alias = true;
            let stroke = Stroke {
                width: 2.4 * s,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Stroke::default()
            };
            self.pixmap
                .stroke_path(&path, &ink, &stroke, Transform::identity(), None);
        }
        &self.pixmap
    }

    /// The hit zone under pixel `(x, y)` of the last frame.
    pub fn hit(&self, x: f32, y: f32) -> Option<&'static str> {
        let (key, ts) = self.last?;
        let inv = ts.invert()?;
        let mut p = [tiny_skia::Point::from_xy(x, y)];
        inv.map_points(&mut p);
        self.sprites.get(key)?.sprite.zone_at(p[0].x, p[0].y)
    }

    /// Where a speech bubble points, in pixmap pixels: the top of the head.
    pub fn anchor(&self) -> (f32, f32) {
        let fallback = (self.pixmap.width() as f32 / 2.0, 0.0);
        let Some((key, ts)) = self.last else {
            return fallback;
        };
        let Some(ready) = self.sprites.get(key) else {
            return fallback;
        };
        let mut p = [tiny_skia::Point::from_xy(
            ready.sprite.top.0,
            ready.sprite.top.1,
        )];
        ts.map_points(&mut p);
        (p[0].x, p[0].y)
    }
}

/// Copy `src` into `dst` (same size, already clear) moved down `dy` rows.
fn shift_copy(src: &Pixmap, dst: &mut Pixmap, dy: i32) {
    let row = src.width() as usize * 4;
    let h = src.height() as i32;
    let (from, to, rows) = if dy >= 0 {
        (0, dy, h - dy)
    } else {
        (-dy, 0, h + dy)
    };
    if rows <= 0 {
        return;
    }
    let (from, to, len) = (from as usize * row, to as usize * row, rows as usize * row);
    dst.data_mut()[to..to + len].copy_from_slice(&src.data()[from..from + len]);
}

fn squash(p: &PoseParams) -> f32 {
    0.35 * p.squash + 0.15 * p.belly
}

/// Squash and breathe about the point under the pet, then sway, bob and
/// mirror: the 2D motion every frame adds to a sprite.
pub fn frame_transform(p: &PoseParams, scale: f32, size: (u32, u32)) -> Transform {
    let (cx, fy) = (size.0 as f32 / 2.0, size.1 as f32 - FLOOR_PX * scale);
    let squash = squash(p);
    let sx = (1.0 + squash) * p.facing.signum();
    let sy = (1.0 - squash) * (1.0 + 0.015 * p.breath);
    Transform::from_translate(cx, fy + p.bob * scale)
        .pre_rotate(p.sway)
        .pre_scale(sx, sy)
        .pre_translate(-cx, -fy)
}
