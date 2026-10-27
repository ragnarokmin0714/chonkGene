//! Draws chonkGene pets (docs/PLAN.md §3.1).
//!
//! Pets are 3D models ray-marched on the CPU into a few sprites per pet, off
//! the UI thread (`sprites`). A frame picks the sprite for the moment's pose
//! and face, then squashes, sways and mirrors it with one 2D transform, and
//! wags the tail (its own small sprite) about where it joins the body, so a
//! frame costs a blit or two (PLAN §5).

pub mod bubble;
pub mod gpu;
pub mod gpu_check;
pub mod menu;
pub mod raymarch;
pub mod sprites;
pub mod text;

use std::sync::Arc;

use chonkgene_core::anim::{Act, BEATS, Expression, PoseParams};
use chonkgene_rig::{Look, Model, PoseId};
use tiny_skia::{
    Color, FillRule, FilterQuality, LineCap, LineJoin, Paint, Pixmap, PixmapPaint, Stroke,
    Transform,
};

use crate::raymarch::FLOOR_PX;
pub use crate::sprites::{KEYS, Key, Variant};
use crate::sprites::{Ready, Sprites};

/// Breathing in place is quantised to this many steps, so idle frames are
/// copies of a few cached images instead of a filtered blit each (PLAN §5).
const BREATH_STEPS: u8 = 6;
/// Cached frames kept at most: a step's beats on both feet with a blink.
const MAX_CACHED: usize = 4 * BEATS as usize;

/// A frame that comes round again: a breathing step in place, or a beat of
/// a steady walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Beat {
    Breath(u8),
    Walk(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CacheId {
    key: Key,
    mirrored: bool,
    beat: Beat,
    /// Drawn without the tail, which the frame wags on top.
    bare: bool,
}

/// A cached frame: only the rows from `top` that hold any pixels.
struct Cached {
    id: CacheId,
    top: usize,
    img: Pixmap,
}

/// What the last frame showed: the sprite and its transform, and the tail's.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shown {
    key: Key,
    ts: Transform,
    tail: Option<Transform>,
}

pub struct Renderer {
    sprites: Sprites,
    pixmap: Pixmap,
    /// The last frame, for hit-testing.
    last: Option<Shown>,
    /// Frames already drawn that come round again, oldest first.
    cache: Vec<Cached>,
    /// The pixmap no longer matches `last` (new sprites, overlays drawn).
    stale: bool,
    /// Whether the last `render` produced a different picture.
    changed: bool,
}

/// The sprite for an expression and what the body is doing. Strong
/// feelings take over the whole body; otherwise steps and gestures do.
pub fn key_for(expression: Expression, params: &PoseParams) -> Key {
    let eyes = if params.eyes_open > 0.66 {
        Look::Open
    } else if params.eyes_open > 0.25 {
        Look::Half
    } else {
        Look::Shut
    };
    let gesture = |eyes| match params.act {
        Act::Rest => (PoseId::Sit, eyes),
        Act::StepA => (PoseId::WalkA, eyes),
        Act::StepB => (PoseId::WalkB, eyes),
        Act::LookLeft => (PoseId::LookLeft, eyes),
        Act::LookRight => (PoseId::LookRight, eyes),
        Act::Stretch => (PoseId::Stretch, Look::Happy),
        Act::Yawn => (PoseId::Yawn, Look::Yawn),
        Act::Wave => (PoseId::Wave, Look::Happy),
    };
    let (pose, look) = match expression {
        Expression::Neutral => gesture(eyes),
        Expression::Happy => match params.act {
            Act::Rest => (PoseId::Sit, Look::Happy),
            _ => gesture(Look::Open),
        },
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
            cache: Vec::new(),
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
            self.cache.clear();
            self.stale = true;
        }
        any
    }

    /// Wait for every sprite (tests, benchmarks, screenshots).
    pub fn finish(&mut self) {
        self.sprites.finish();
        self.cache.clear();
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
        let want = key_for(expression, params);
        let Some((key, ready)) = self.sprites.best(want) else {
            self.changed = self.stale || self.last.is_some();
            self.pixmap.fill(Color::TRANSPARENT);
            (self.last, self.stale) = (None, false);
            return &self.pixmap;
        };
        let tail = self.sprites.tail(key.pose);
        let size = (self.pixmap.width(), self.pixmap.height());
        let scale = self.scale();
        let mirrored = params.facing < 0.0;
        let turned = params.facing.abs() == 1.0;
        // Under ~1 px of squash at 100% counts as none.
        let quiet = squash(params).abs() < 0.004 && params.sway.abs() < 0.05 && turned;
        // Breathing in place, quantised to a few steps.
        let step = ((params.breath.clamp(-1.0, 1.0) + 1.0) / 2.0 * f32::from(BREATH_STEPS - 1))
            .round() as u8;
        let breathing = PoseParams {
            breath: f32::from(step) / f32::from(BREATH_STEPS - 1) * 2.0 - 1.0,
            bob: 0.0,
            ..*params
        };

        // What this frame shows, how to draw it, and which cached image (if
        // any) holds it, moved down `dy` whole pixels.
        let (shown, cached, dy) = if let Some(n) = params.beat.filter(|_| turned) {
            // A beat of a steady walk: the same every step.
            let ts = frame_transform(params, scale, size);
            let shown = Shown {
                key,
                ts,
                tail: tail.and_then(|t| tail_transform(t, ts, params.tail)),
            };
            let id = CacheId {
                key,
                mirrored,
                beat: Beat::Walk(n),
                bare: false,
            };
            (shown, Some((id, shown)), 0)
        } else if quiet && (params.tail == 0.0 || params.bob == 0.0) {
            // Breathing in place, maybe hopping (whole pixels), maybe
            // wagging (the tail drawn over a cached tailless body).
            let ts = frame_transform(&breathing, scale, size);
            let at_rest = tail.and_then(|t| tail_transform(t, ts, 0.0));
            let dy = (params.bob * scale).round() as i32;
            let down = |ts: Transform| ts.post_translate(0.0, dy as f32);
            let bare = params.tail != 0.0;
            let shown = Shown {
                key,
                ts: down(ts),
                tail: tail.and_then(|t| tail_transform(t, ts, params.tail).map(down)),
            };
            let id = CacheId {
                key,
                mirrored,
                beat: Beat::Breath(step),
                bare,
            };
            let drawn = Shown {
                key,
                ts,
                tail: if bare { None } else { at_rest },
            };
            (shown, Some((id, drawn)), dy)
        } else {
            let ts = frame_transform(params, scale, size);
            let shown = Shown {
                key,
                ts,
                tail: tail.and_then(|t| tail_transform(t, ts, params.tail)),
            };
            (shown, None, 0)
        };
        if !self.stale && self.last == Some(shown) {
            self.changed = false;
            return &self.pixmap;
        }
        (self.last, self.stale, self.changed) = (Some(shown), false, true);
        self.pixmap.fill(Color::TRANSPARENT);
        let Some((id, drawn)) = cached else {
            draw(&mut self.pixmap, ready, tail, &shown);
            return &self.pixmap;
        };
        let at = match self.cache.iter().position(|c| c.id == id) {
            Some(at) => at,
            None => {
                // Keep only frames that may come round again: the same way
                // round, and the same breathing sprite or the same walk.
                self.cache.retain(|c| {
                    c.id.mirrored == mirrored
                        && match (c.id.beat, id.beat) {
                            (Beat::Breath(_), Beat::Breath(_)) => c.id.key == key,
                            (Beat::Walk(_), Beat::Walk(_)) => true,
                            _ => false,
                        }
                });
                if self.cache.len() >= MAX_CACHED {
                    self.cache.remove(0);
                }
                // A blank canvas: the pixmap still holds the previous frame, and
                // copying it would bake a ghost of the old pose into the cache.
                let Some(mut img) = Pixmap::new(size.0, size.1) else {
                    return &self.pixmap;
                };
                let body = if id.bare { None } else { tail };
                draw(&mut img, ready, body, &drawn);
                let Some(c) = trim(id, &img) else {
                    return &self.pixmap;
                };
                self.cache.push(c);
                self.cache.len() - 1
            }
        };
        let c = &self.cache[at];
        let wag = tail.zip(shown.tail).filter(|_| id.bare);
        match wag {
            Some((t, tts)) if t.sprite.tail.is_some_and(|(_, front)| !front) => {
                // The tail behind: draw it, then lay the body over it.
                blit(&mut self.pixmap, t, tts);
                over_rows(c, &mut self.pixmap, dy);
            }
            _ => {
                paste_rows(c, &mut self.pixmap, dy);
                if let Some((t, tts)) = wag {
                    blit(&mut self.pixmap, t, tts);
                }
            }
        }
        &self.pixmap
    }

    /// Render a sleeping pet: the frame plus a snot bubble that swells with
    /// each breath and a drifting 「z」. `phase` is seconds asleep.
    pub fn render_asleep(&mut self, params: &PoseParams, phase: f32) -> &Pixmap {
        // The bubble and the z move every frame, and they are drawn over the
        // sprite, so the sprite is always drawn afresh.
        self.stale = true;
        self.render(params, Expression::Asleep);
        let Some(Shown { key, ts, .. }) = self.last else {
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

    /// The hit zone under pixel `(x, y)` of the last frame, through the
    /// tail when it is drawn in front.
    pub fn hit(&self, x: f32, y: f32) -> Option<&'static str> {
        let shown = self.last?;
        let at = |sprite: &Ready, ts: Transform| {
            let mut p = [tiny_skia::Point::from_xy(x, y)];
            ts.invert()?.map_points(&mut p);
            sprite.sprite.zone_at(p[0].x, p[0].y)
        };
        let body = || at(self.sprites.get(shown.key)?, shown.ts);
        let tail = || at(self.sprites.tail(shown.key.pose)?, shown.tail?);
        let front = self
            .sprites
            .tail(shown.key.pose)
            .and_then(|t| t.sprite.tail)
            .is_some_and(|(_, front)| front);
        if front {
            tail().or_else(body)
        } else {
            body().or_else(tail)
        }
    }

    /// Where a speech bubble points, in pixmap pixels: the top of the head.
    pub fn anchor(&self) -> (f32, f32) {
        let fallback = (self.pixmap.width() as f32 / 2.0, 0.0);
        let Some(Shown { key, ts, .. }) = self.last else {
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

/// The tail's transform: turned `wag` degrees about where it joins the body,
/// then moved with the rest of the pet.
fn tail_transform(tail: &Ready, ts: Transform, wag: f32) -> Option<Transform> {
    let ((x, y), _) = tail.sprite.tail?;
    Some(ts.pre_concat(Transform::from_rotate_at(wag, x, y)))
}

fn blit(dst: &mut Pixmap, r: &Ready, ts: Transform) {
    let paint = PixmapPaint {
        quality: FilterQuality::Bilinear,
        ..PixmapPaint::default()
    };
    let crop = ts.pre_translate(r.offset.0, r.offset.1);
    dst.draw_pixmap(0, 0, r.pixmap.as_ref(), &paint, crop, None);
}

/// Draw the body and the tail (behind or in front of it) into `dst`.
fn draw(dst: &mut Pixmap, body: &Ready, tail: Option<&Ready>, shown: &Shown) {
    let tail = tail.zip(shown.tail);
    let front = tail.is_some_and(|(t, _)| t.sprite.tail.is_some_and(|(_, f)| f));
    if let Some((t, ts)) = tail.filter(|_| !front) {
        blit(dst, t, ts);
    }
    blit(dst, body, shown.ts);
    if let Some((t, ts)) = tail.filter(|_| front) {
        blit(dst, t, ts);
    }
}

/// Keep only the rows of `img` that hold pixels.
fn trim(id: CacheId, img: &Pixmap) -> Option<Cached> {
    let row = img.width() as usize * 4;
    let solid = |y: &usize| img.data()[y * row..(y + 1) * row].iter().any(|&b| b != 0);
    let rows = 0..img.height() as usize;
    let top = rows.clone().find(solid).unwrap_or(0);
    let bottom = rows.rev().find(solid).map_or(top, |y| y + 1);
    let h = (bottom - top).max(1) as u32;
    let mut out = Pixmap::new(img.width(), h)?;
    let len = (bottom - top) * row;
    out.data_mut()[..len].copy_from_slice(&img.data()[top * row..top * row + len]);
    Some(Cached { id, top, img: out })
}

/// The rows of `dst` that cached frame `c`, moved down `dy`, lands on, and
/// the matching byte ranges: (into `c`, into `dst`).
fn rows_of(
    c: &Cached,
    dst: &Pixmap,
    dy: i32,
) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let row = dst.width() as usize * 4;
    let from = c.top as i32 + dy;
    let to = (from + c.img.height() as i32).min(dst.height() as i32);
    let skip = (-from).max(0);
    let first = from.max(0);
    if to <= first {
        return None;
    }
    let n = (to - first) as usize * row;
    let src = skip as usize * row;
    let at = first as usize * row;
    Some((src..src + n, at..at + n))
}

/// Copy a cached frame into `dst` (already clear), moved down `dy` rows.
fn paste_rows(c: &Cached, dst: &mut Pixmap, dy: i32) {
    if let Some((src, at)) = rows_of(c, dst, dy) {
        dst.data_mut()[at].copy_from_slice(&c.img.data()[src]);
    }
}

/// Lay a cached frame over `dst` (premultiplied source-over), moved down
/// `dy` rows. Most pixels are fully clear or fully solid, so this is about
/// a copy's cost, far below a filtered draw.
fn over_rows(c: &Cached, dst: &mut Pixmap, dy: i32) {
    let Some((src, at)) = rows_of(c, dst, dy) else {
        return;
    };
    let src = &c.img.data()[src];
    let dst = &mut dst.data_mut()[at];
    for (s, d) in src
        .as_chunks::<4>()
        .0
        .iter()
        .zip(dst.as_chunks_mut::<4>().0)
    {
        match s[3] {
            0 => {}
            255 => d.copy_from_slice(s),
            a => {
                let keep = 255 - u16::from(a);
                for i in 0..4 {
                    d[i] = s[i] + ((u16::from(d[i]) * keep + 127) / 255) as u8;
                }
            }
        }
    }
}

fn squash(p: &PoseParams) -> f32 {
    0.35 * p.squash + 0.15 * p.belly
}

/// Squash and breathe about the point under the pet, then sway, bob and
/// mirror: the 2D motion every frame adds to a sprite. Mid-turn the pet is
/// squeezed thin and a little taller, then filled out again mirrored.
pub fn frame_transform(p: &PoseParams, scale: f32, size: (u32, u32)) -> Transform {
    let (cx, fy) = (size.0 as f32 / 2.0, size.1 as f32 - FLOOR_PX * scale);
    let squash = squash(p);
    let facing = p.facing.clamp(-1.0, 1.0);
    let thin = facing.abs().max(0.25).copysign(facing);
    let sx = (1.0 + squash) * thin;
    let sy = (1.0 - squash) * (1.0 + 0.015 * p.breath) * (1.0 + 0.15 * (1.0 - facing.abs()));
    Transform::from_translate(cx, fy + p.bob * scale)
        .pre_rotate(p.sway)
        .pre_scale(sx, sy)
        .pre_translate(-cx, -fy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Laying a cached frame over another matches tiny-skia's source-over.
    #[test]
    fn over_rows_blends_like_tiny_skia() {
        let (w, h) = (16, 12);
        let mut src = Pixmap::new(w, h).unwrap();
        let mut under = Pixmap::new(w, h).unwrap();
        let (srcs, unders) = (src.data_mut(), under.data_mut());
        for (i, (s, u)) in srcs
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(unders.as_chunks_mut::<4>().0)
            .enumerate()
        {
            let a = [0u8, 255, 128, 37, 200][i % 5];
            let c = (i * 37 % 256) as u8;
            s.copy_from_slice(&[c.min(a), (c / 2).min(a), a / 3, a]);
            let b = [255u8, 90, 0][i % 3];
            u.copy_from_slice(&[(i * 11 % 256) as u8 % (b / 2 + 1), b / 2, b / 4, b]);
        }
        let mut expected = under.clone();
        expected.draw_pixmap(
            0,
            0,
            src.as_ref(),
            &PixmapPaint::default(),
            Transform::from_translate(0.0, 2.0),
            None,
        );
        let id = CacheId {
            key: KEYS[0],
            mirrored: false,
            beat: Beat::Breath(0),
            bare: true,
        };
        let c = Cached {
            id,
            top: 0,
            img: src,
        };
        let mut got = under;
        over_rows(&c, &mut got, 2);
        for (a, b) in expected.data().iter().zip(got.data()) {
            assert!(a.abs_diff(*b) <= 1, "{a} vs {b}");
        }
    }
}
