//! Speech bubbles (PLAN §1.9): white rounded box, soft drop shadow, a tail
//! pointing down at the pet, Iansui text.

use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use crate::text::Face;

const FONT: f32 = 21.0;
const PAD_X: f32 = 18.0;
const PAD_Y: f32 = 9.0;
const RADIUS: f32 = 20.0;
const TAIL: f32 = 8.0;
const SHADOW_Y: f32 = 4.0;
const INK: [u8; 4] = [0x4B, 0x35, 0x28, 255];

fn paint(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

/// A bubble for `text` at `scale`; the tail tip is at the bottom centre.
pub fn bubble(face: &mut Face, text: &str, scale: f32) -> Option<Pixmap> {
    let size = FONT * scale;
    let (ascent, descent) = face.extent(size);
    let text_w = face.measure(text, size);
    let w = text_w + 2.0 * PAD_X * scale;
    let h = ascent + descent + 2.0 * PAD_Y * scale;
    let tail = TAIL * scale;
    let shadow = SHADOW_Y * scale;
    let margin = 2.0;
    let mut pm = Pixmap::new(
        (w + 2.0 * margin).ceil() as u32,
        (h + tail + shadow + 2.0 * margin).ceil() as u32,
    )?;
    let (x, y) = (margin, margin);
    let cx = x + w / 2.0;

    let mut tail_path = PathBuilder::new();
    tail_path.move_to(cx - tail, y + h - 1.0);
    tail_path.line_to(cx, y + h + tail);
    tail_path.line_to(cx + tail, y + h - 1.0);
    tail_path.close();
    let tail_path = tail_path.finish()?;
    let body = rounded(x, y, w, h, RADIUS * scale)?;

    let shadow_ts = Transform::from_translate(0.0, shadow);
    let shade = paint([0x4B, 0x35, 0x28, 26]);
    pm.fill_path(&body, &shade, FillRule::Winding, shadow_ts, None);
    pm.fill_path(&tail_path, &shade, FillRule::Winding, shadow_ts, None);
    let white = paint([255, 255, 255, 255]);
    pm.fill_path(
        &body,
        &white,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    pm.fill_path(
        &tail_path,
        &white,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    let baseline = y + PAD_Y * scale + ascent;
    face.draw(
        &mut pm,
        text,
        cx - text_w / 2.0,
        baseline,
        size,
        &paint(INK),
    );
    Some(pm)
}
