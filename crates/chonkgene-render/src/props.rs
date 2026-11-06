//! Things on the desktop and little effects (PLAN §8, Roger's fourth
//! pass, items 2 and 4; "Catnip"): the ball, the frisbee, catnip, the
//! scenes' bone, seeds, branch, pond, sunbeam and butterfly, each drawn in
//! 2D into a window of its own (or into a pet's picture, carried in its
//! mouth); and the effects drawn over a pet's picture: hearts, sparkles,
//! dust, splashes, ripples, the water it swims in and the fox's fire.
//!
//! All in the soft, round style of the pets: plain fills, a lighter
//! highlight, a soft darker rim, and a contact shadow under anything on
//! the floor.

use std::f32::consts::{PI, TAU};

use chonkgene_core::play::Thing;
use tiny_skia::{
    BlendMode, Color, FillRule, GradientStop, LineCap, Mask, Paint, Path, PathBuilder, Pixmap,
    PixmapPaint, Point, RadialGradient, Rect, SpreadMode, Stroke, Transform,
};

/// Room around a thing in its own window, pixels at 100%: for its shadow,
/// its rim and what rises off it (catnip's smell).
const MARGIN: f32 = 6.0;
/// Above catnip, for the wavy smell rising from it, pixels at 100%.
const SMELL_ROOM: f32 = 46.0;

fn paint(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

fn with_alpha(c: [u8; 4], a: f32) -> [u8; 4] {
    [
        c[0],
        c[1],
        c[2],
        (f32::from(c[3]) * a.clamp(0.0, 1.0)) as u8,
    ]
}

fn circle(pm: &mut Pixmap, x: f32, y: f32, r: f32, c: [u8; 4]) {
    if let Some(p) = PathBuilder::from_circle(x, y, r.max(0.1)) {
        pm.fill_path(
            &p,
            &paint(c),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn oval(pm: &mut Pixmap, x: f32, y: f32, rx: f32, ry: f32, c: [u8; 4], ts: Transform) {
    if let Some(r) = Rect::from_xywh(x - rx, y - ry, 2.0 * rx.max(0.1), 2.0 * ry.max(0.1))
        && let Some(p) = PathBuilder::from_oval(r)
    {
        pm.fill_path(&p, &paint(c), FillRule::Winding, ts, None);
    }
}

fn stroke_path(pm: &mut Pixmap, pb: PathBuilder, c: [u8; 4], width: f32, ts: Transform) {
    if let Some(p) = pb.finish() {
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            ..Stroke::default()
        };
        pm.stroke_path(&p, &paint(c), &stroke, ts, None);
    }
}

/// A soft glow: `c` in the middle fading out to nothing at `r`.
fn glow(pm: &mut Pixmap, x: f32, y: f32, r: f32, c: [u8; 4]) {
    let stops = vec![
        GradientStop::new(0.0, Color::from_rgba8(c[0], c[1], c[2], c[3])),
        GradientStop::new(
            0.45,
            Color::from_rgba8(c[0], c[1], c[2], (f32::from(c[3]) * 0.45) as u8),
        ),
        GradientStop::new(1.0, Color::from_rgba8(c[0], c[1], c[2], 0)),
    ];
    let Some(shader) = RadialGradient::new(
        Point::from_xy(x, y),
        0.0,
        Point::from_xy(x, y),
        r.max(0.5),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    ) else {
        return;
    };
    let p = Paint {
        shader,
        anti_alias: true,
        ..Paint::default()
    };
    if let Some(path) = PathBuilder::from_circle(x, y, r.max(0.5)) {
        pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
    }
}

/// The shadow under something on the floor at `x`, `y`, `w` wide.
fn shadow(pm: &mut Pixmap, x: f32, y: f32, w: f32, s: f32) {
    oval(
        pm,
        x,
        y,
        0.5 * w,
        2.6 * s,
        [0x40, 0x34, 0x30, 46],
        Transform::identity(),
    );
}

/// The size of the window a thing is drawn in at `scale`, pixels, and
/// where in it the middle of the thing's bottom goes.
pub fn canvas(thing: Thing, scale: f32) -> ((u32, u32), (f32, f32)) {
    let (w, h) = thing.size();
    let top = if thing == Thing::Catnip {
        SMELL_ROOM
    } else {
        MARGIN
    };
    let size = (
        ((w + 2.0 * MARGIN) * scale).ceil() as u32,
        ((h + top + MARGIN) * scale).ceil() as u32,
    );
    let foot = (size.0 as f32 / 2.0, size.1 as f32 - MARGIN * scale);
    (size, foot)
}

/// How a thing looks this moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    /// Radians turned (a ball rolling, a frisbee spinning).
    pub spin: f32,
    /// Seconds, for anything that moves on its own (the smell rising, the
    /// water rippling, wings beating).
    pub t: f32,
    /// How much of it is used up (catnip, seeds), 0 … 1.
    pub used: f32,
    /// Lying on the floor (it gets a shadow) rather than in the air or in
    /// a mouth.
    pub grounded: bool,
    /// Facing left (the branch grows out from the right).
    pub flip: bool,
    /// Held in a mouth, turned with the head this far, radians (+ is
    /// clockwise on the screen).
    pub turn: f32,
    /// Held by its rim in a mouth: a frisbee shows more of its face.
    pub held: bool,
}

/// A frisbee in a mouth is drawn this much smaller than on the floor.
const CARRIED: f32 = 0.85;

/// Draw `thing` held in a mouth at `mouth` (pixels of `pm`), the pet facing
/// `facing` (−1 left, 1 right): a bone across the jaws; a frisbee or a
/// ball gripped at its edge between the front teeth, sticking out in front
/// of the muzzle on the side it faces, so the face stays in sight.
pub fn carried(pm: &mut Pixmap, thing: Thing, mouth: (f32, f32), facing: f32, s: f32, look: Look) {
    let (w, h) = thing.size();
    let side = if facing < 0.0 { -1.0 } else { 1.0 };
    let look = Look { held: true, ..look };
    match thing {
        Thing::Frisbee => {
            // The rim in the teeth, the disc hanging down in front of the
            // chin, tipped out the way it faces (a little smaller than on
            // the floor, so it stays inside the pet's picture).
            let s = CARRIED * s;
            let rx = 0.5 * w * s;
            let mut centre = (mouth.0 + side * 0.35 * rx, mouth.1 + 0.62 * rx);
            // Walking, the head reaches far forward: never past the edge of
            // the pet's picture, where it would be cut off.
            let room = pm.width() as f32 - rx - 1.0;
            if room > rx + 1.0 {
                centre.0 = centre.0.clamp(rx + 1.0, room);
            }
            let look = Look {
                turn: look.turn + side * 0.5,
                ..look
            };
            draw(pm, thing, (centre.0, centre.1 + 0.55 * h * s), s, look);
        }
        Thing::Ball => {
            let r = 0.5 * h * s;
            let centre = (mouth.0 + side * 0.6 * r, mouth.1 + 0.35 * r);
            draw(pm, thing, (centre.0, centre.1 + r), s, look);
        }
        _ => draw(pm, thing, (mouth.0, mouth.1 + 0.4 * h * s), 0.8 * s, look),
    }
}

/// Draw `thing` with the middle of its bottom at `at` (pixels of `pm`).
pub fn draw(pm: &mut Pixmap, thing: Thing, at: (f32, f32), s: f32, look: Look) {
    let (w, h) = thing.size();
    let (x, y) = at;
    match thing {
        Thing::Ball => {
            let r = 0.5 * h * s;
            if look.grounded {
                shadow(pm, x, y, 1.6 * r, s);
            }
            let (cx, cy) = (x, y - r);
            circle(pm, cx, cy, r, [0xC9, 0x5B, 0x62, 255]);
            circle(pm, cx, cy, 0.92 * r, [0xF0, 0x7E, 0x86, 255]);
            // A cream band round it, turning as it rolls.
            let ts = Transform::from_rotate_at(look.spin.to_degrees(), cx, cy);
            let mut band = PathBuilder::new();
            band.move_to(cx - 0.9 * r, cy - 0.2 * r);
            band.quad_to(cx, cy + 0.35 * r, cx + 0.9 * r, cy - 0.2 * r);
            stroke_path(pm, band, [0xFF, 0xF1, 0xD6, 255], 0.28 * r, ts);
            circle(
                pm,
                cx - 0.35 * r,
                cy - 0.4 * r,
                0.22 * r,
                [255, 255, 255, 150],
            );
        }
        Thing::Frisbee => {
            let rx = 0.5 * w * s;
            let ry = if look.held { 0.55 * rx } else { 0.32 * h * s };
            if look.grounded {
                shadow(pm, x, y, 1.9 * rx, s);
            }
            // Tipped a little toward you, wobbling as it spins.
            let tilt = 6.0 * (look.spin * 0.5).sin();
            let (cx, cy) = (x, y - 0.55 * h * s);
            let ts = Transform::from_rotate_at(tilt + look.turn.to_degrees(), cx, cy);
            oval(pm, cx, cy + 0.25 * ry, rx, ry, [0x2E, 0x8C, 0x8A, 255], ts);
            oval(pm, cx, cy, rx, ry, [0x5C, 0xC8, 0xC2, 255], ts);
            oval(
                pm,
                cx,
                cy,
                0.62 * rx,
                0.58 * ry,
                [0x84, 0xDB, 0xD4, 255],
                ts,
            );
            // A mark on it that goes round as it spins.
            let (sx, sy) = (look.spin.cos(), look.spin.sin());
            oval(
                pm,
                cx + 0.45 * rx * sx,
                cy + 0.45 * ry * sy,
                0.16 * rx,
                0.2 * ry,
                [0xFF, 0xF4, 0xC2, 230],
                ts,
            );
        }
        Thing::Catnip => {
            let left = 1.0 - look.used.clamp(0.0, 1.0);
            shadow(pm, x, y, 0.8 * w * s, s);
            // A sprig: a bent stem with pairs of soft leaves.
            let green = [0x6E, 0xA8, 0x5A, 255];
            let light = [0x9C, 0xD0, 0x7E, 255];
            let mut stem = PathBuilder::new();
            stem.move_to(x - 0.42 * w * s, y - 2.0 * s);
            stem.quad_to(x, y - 0.6 * h * s, x + 0.42 * w * s, y - 0.25 * h * s);
            stroke_path(
                pm,
                stem,
                [0x5A, 0x86, 0x48, 255],
                2.0 * s,
                Transform::identity(),
            );
            for (k, (u, v)) in [(-0.25, 0.32), (0.05, 0.5), (0.3, 0.42), (-0.05, 0.62)]
                .iter()
                .enumerate()
            {
                let (lx, ly) = (x + u * w * s, y - v * h * s);
                let a = if k % 2 == 0 { -30.0 } else { 35.0 };
                let ts = Transform::from_rotate_at(a, lx, ly);
                oval(pm, lx, ly, 6.5 * s, 3.8 * s, green, ts);
                oval(pm, lx - 1.0 * s, ly - 1.0 * s, 4.0 * s, 2.0 * s, light, ts);
            }
            // Tiny lilac flowers on top.
            for (u, v) in [(0.36, 0.72), (0.42, 0.62)] {
                circle(
                    pm,
                    x + u * w * s,
                    y - v * h * s,
                    2.4 * s,
                    [0xC3, 0xA6, 0xE4, 255],
                );
            }
            // The smell: wavy lines rising and fading, fewer as it is used.
            let lines = (3.0 * left).ceil() as usize;
            for k in 0..lines {
                let phase = (look.t * 0.45 + k as f32 / 3.0).fract();
                let x0 = x + (k as f32 - 1.0) * 9.0 * s;
                let y0 = y - h * s - phase * SMELL_ROOM * 0.7 * s;
                let a = (1.0 - phase) * (phase * 4.0).min(1.0) * left;
                let mut wave = PathBuilder::new();
                wave.move_to(x0, y0);
                for i in 1..=6 {
                    let yy = y0 - i as f32 * 2.6 * s;
                    let xx = x0 + 2.6 * s * (i as f32 * 1.3 + look.t * 3.0).sin();
                    wave.line_to(xx, yy);
                }
                stroke_path(
                    pm,
                    wave,
                    with_alpha([0x8F, 0xC9, 0x8A, 210], a),
                    1.6 * s,
                    Transform::identity(),
                );
            }
        }
        Thing::Bone => {
            if look.grounded {
                shadow(pm, x, y, 0.9 * w * s, s);
            }
            // Tipped a little this way or that (gnawed at, carried).
            let (cx, cy) = (x, y - 0.5 * h * s);
            let ts =
                Transform::from_rotate_at(8.0 * look.spin.sin() + look.turn.to_degrees(), cx, cy);
            let rim = [0xD8, 0xC6, 0xA4, 255];
            let fill = [0xFB, 0xF2, 0xDF, 255];
            let half = 0.36 * w * s;
            let knob = 0.2 * h * s;
            for (c, grow) in [(rim, 1.2 * s), (fill, 0.0)] {
                if let Some(r) = Rect::from_xywh(
                    cx - half,
                    cy - 0.22 * h * s - grow,
                    2.0 * half,
                    0.44 * h * s + 2.0 * grow,
                ) {
                    let p = PathBuilder::from_rect(r);
                    pm.fill_path(&p, &paint(c), FillRule::Winding, ts, None);
                }
                for sx in [-1.0, 1.0] {
                    for sy in [-1.0, 1.0] {
                        let mut k = Point::from_xy(cx + sx * half, cy + sy * 0.55 * knob);
                        ts.map_point(&mut k);
                        circle(pm, k.x, k.y, knob + grow, c);
                    }
                }
            }
        }
        Thing::Seeds => {
            let left = 1.0 - look.used.clamp(0.0, 1.0);
            let n = (16.0 * left).ceil() as usize;
            for k in 0..n {
                // Scattered by a fixed pattern, so they stay put.
                let u = ((k as f32 * 0.618_034).fract() - 0.5) * w * s;
                let v = ((k as f32 * 0.381_966 * 3.0).fract()) * 0.6 * h * s;
                let ts = Transform::from_rotate_at(k as f32 * 47.0, x + u, y - v - 2.0 * s);
                let c = if k % 3 == 0 {
                    [0xE9, 0xC4, 0x6A, 255]
                } else {
                    [0xC4, 0x93, 0x5A, 255]
                };
                oval(pm, x + u, y - v - 2.0 * s, 2.6 * s, 1.6 * s, c, ts);
            }
        }
        Thing::Branch => {
            // A branch growing out from the side (the right, flipped: the
            // left), its top the perch at `at`; leaves along it.
            let dir = if look.flip { -1.0 } else { 1.0 };
            let (bx, by) = (x + dir * 0.5 * w * s, y + 0.14 * h * s);
            let tip = (x - dir * 0.48 * w * s, y + 0.02 * h * s);
            let bark = [0x8A, 0x5E, 0x42, 255];
            let mut limb = PathBuilder::new();
            limb.move_to(bx, by);
            limb.quad_to(x, y + 0.2 * h * s, tip.0, tip.1);
            stroke_path(pm, limb, bark, 9.0 * s, Transform::identity());
            let mut top = PathBuilder::new();
            top.move_to(bx, by - 2.5 * s);
            top.quad_to(x, y + 0.12 * h * s, tip.0, tip.1 - 1.0 * s);
            stroke_path(
                pm,
                top,
                [0xB0, 0x82, 0x60, 255],
                3.0 * s,
                Transform::identity(),
            );
            for (k, u) in [0.15f32, 0.4, 0.68, 0.9].iter().enumerate() {
                let lx = bx + (tip.0 - bx) * u;
                let ly = by + (tip.1 - by) * u - 6.0 * s;
                let sway = 8.0 * (look.t * 1.3 + k as f32).sin();
                let a = if k % 2 == 0 { -40.0 } else { 220.0 } + sway;
                let ts = Transform::from_rotate_at(a, lx, ly);
                oval(
                    pm,
                    lx + 7.0 * s,
                    ly,
                    8.0 * s,
                    4.0 * s,
                    [0x6C, 0xA8, 0x5C, 255],
                    ts,
                );
                oval(
                    pm,
                    lx + 6.0 * s,
                    ly - 1.0 * s,
                    5.0 * s,
                    2.0 * s,
                    [0x9D, 0xCF, 0x84, 255],
                    ts,
                );
            }
        }
        Thing::Pond => {
            let (cx, cy, rx, ry) = pond_oval(at, s);
            let id = Transform::identity();
            // The bank round it, a little wider at the front, grass at the
            // ends.
            oval(
                pm,
                cx,
                cy + 2.5 * s,
                rx + 5.0 * s,
                ry + 4.5 * s,
                [0xA8, 0x94, 0x78, 230],
                id,
            );
            for (k, end) in [-1.0f32, 1.0].iter().enumerate() {
                for blade in 0..3 {
                    let bx = cx + end * (rx + (1.0 - blade as f32) * 4.0 * s);
                    let by = cy + (0.1 + 0.25 * blade as f32) * ry;
                    let sway = 1.5 * s * (TAU * 0.4 * look.t + (k * 3 + blade) as f32).sin();
                    let mut pb = PathBuilder::new();
                    pb.move_to(bx, by);
                    pb.quad_to(
                        bx + end * 2.0 * s,
                        by - 6.0 * s,
                        bx + end * 3.0 * s + sway,
                        by - 11.0 * s,
                    );
                    stroke_path(pm, pb, [0x6E, 0xA8, 0x5C, 255], 2.0 * s, id);
                }
            }
            // The water, deep at the back under the far bank, shallower
            // toward you.
            if let Some(water) = oval_path(cx, cy, rx, ry) {
                pm.fill_path(&water, &paint(POND_BANK), FillRule::Winding, id, None);
                if let (Some(shallow), Some(mut inside)) = (
                    oval_path(cx, cy + 0.3 * ry, 0.98 * rx, 0.8 * ry),
                    Mask::new(pm.width(), pm.height()),
                ) {
                    inside.fill_path(&water, FillRule::Winding, true, id);
                    pm.fill_path(
                        &shallow,
                        &water_paint(cy, ry),
                        FillRule::Winding,
                        id,
                        Some(&inside),
                    );
                }
            }
            let (hx, hy, hrx, hry) = pond_light(cx, cy, rx, ry);
            oval(pm, hx, hy, hrx, hry, POND_LIGHT, id);
            // A light rim along the near edge.
            let mut rim = PathBuilder::new();
            rim.move_to(cx - 0.92 * rx, cy + 0.35 * ry);
            rim.quad_to(cx, cy + 1.32 * ry, cx + 0.92 * rx, cy + 0.35 * ry);
            stroke_path(pm, rim, [0xE4, 0xF4, 0xFA, 200], 2.0 * s, id);
            // Rings spreading on the water.
            for k in 0..3 {
                let p = (look.t * 0.35 + k as f32 / 3.0).fract();
                let (ox, oy) = (cx + (k as f32 - 1.0) * 0.4 * rx, cy + 0.15 * ry);
                ripple(pm, (ox, oy), (0.32 * rx, 0.45 * ry), p, s);
            }
            // A lily pad with a notch, and a flower.
            let (lx, ly) = (cx + 0.6 * rx, cy + 0.2 * ry);
            oval(
                pm,
                lx,
                ly,
                13.0 * s,
                5.0 * s,
                [0x5E, 0x9E, 0x55, 255],
                Transform::identity(),
            );
            circle(
                pm,
                lx + 2.0 * s,
                ly - 3.0 * s,
                3.2 * s,
                [0xF6, 0xB8, 0xC8, 255],
            );
            circle(
                pm,
                lx + 2.0 * s,
                ly - 3.0 * s,
                1.4 * s,
                [0xFF, 0xE8, 0x9A, 255],
            );
        }
        Thing::Sunbeam => {
            // A warm slanting shaft of light down to the floor, motes in it.
            let (bw, bh) = (w * s, h * s);
            let fade = (look.t / 1.5).min(1.0) * (1.0 - look.used).clamp(0.0, 1.0);
            let stops = vec![
                GradientStop::new(0.0, Color::from_rgba8(0xFF, 0xF1, 0xB8, 0)),
                GradientStop::new(
                    0.55,
                    Color::from_rgba8(0xFF, 0xEA, 0xA0, (70.0 * fade) as u8),
                ),
                GradientStop::new(
                    1.0,
                    Color::from_rgba8(0xFF, 0xE2, 0x8A, (110.0 * fade) as u8),
                ),
            ];
            if let Some(shader) = tiny_skia::LinearGradient::new(
                Point::from_xy(x, y - bh),
                Point::from_xy(x, y),
                stops,
                SpreadMode::Pad,
                Transform::identity(),
            ) {
                let p = Paint {
                    shader,
                    anti_alias: true,
                    ..Paint::default()
                };
                let mut beam = PathBuilder::new();
                beam.move_to(x - 0.05 * bw, y - bh);
                beam.line_to(x + 0.25 * bw, y - bh);
                beam.line_to(x + 0.5 * bw, y);
                beam.line_to(x - 0.5 * bw, y);
                beam.close();
                if let Some(path) = beam.finish() {
                    pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
                }
            }
            oval(
                pm,
                x,
                y - 1.5 * s,
                0.5 * bw,
                4.0 * s,
                [0xFF, 0xE4, 0x90, (90.0 * fade) as u8],
                Transform::identity(),
            );
            for k in 0..9 {
                let p = (look.t * 0.08 + k as f32 * 0.13).fract();
                let mx = x + ((k as f32 * 0.7).sin() * 0.3 + 0.1 * (look.t + k as f32).sin()) * bw;
                let my = y - p * bh;
                circle(
                    pm,
                    mx,
                    my,
                    1.3 * s,
                    [255, 252, 230, (160.0 * fade * (1.0 - p)) as u8],
                );
            }
        }
        Thing::Butterfly => {
            // Wings beating: squeezed sideways as they close.
            let beat = (TAU * 3.0 * look.t).sin();
            let open = 0.25 + 0.75 * beat.abs();
            let (cx, cy) = (x, y - 0.5 * h * s);
            for side in [-1.0f32, 1.0] {
                let ts = Transform::from_translate(cx, cy)
                    .pre_scale(open, 1.0)
                    .pre_translate(-cx, -cy);
                oval(
                    pm,
                    cx + side * 6.0 * s,
                    cy - 3.0 * s,
                    6.5 * s,
                    5.5 * s,
                    [0xF4, 0xA6, 0x5E, 245],
                    ts,
                );
                oval(
                    pm,
                    cx + side * 4.5 * s,
                    cy + 4.5 * s,
                    4.5 * s,
                    3.8 * s,
                    [0xF7, 0xC7, 0x6E, 245],
                    ts,
                );
                circle(
                    pm,
                    cx + side * 6.0 * open * s,
                    cy - 3.0 * s,
                    1.6 * s,
                    [0x5A, 0x40, 0x38, 200],
                );
            }
            oval(
                pm,
                cx,
                cy,
                1.4 * s,
                6.0 * s,
                [0x4A, 0x36, 0x30, 255],
                Transform::identity(),
            );
            let mut feelers = PathBuilder::new();
            feelers.move_to(cx, cy - 5.0 * s);
            feelers.line_to(cx - 3.0 * s, cy - 10.0 * s);
            feelers.move_to(cx, cy - 5.0 * s);
            feelers.line_to(cx + 3.0 * s, cy - 10.0 * s);
            stroke_path(
                pm,
                feelers,
                [0x4A, 0x36, 0x30, 255],
                0.9 * s,
                Transform::identity(),
            );
        }
    }
}

/// An effect over a pet's picture, started at some moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fx {
    /// Hearts floating up from over the head (pleased).
    Hearts,
    /// Sparkles twinkling round a point (a catch, a level up).
    Sparkles,
    /// Puffs of dust at the feet (a hard landing, skidding to a stop).
    Dust,
    /// Drops flying from the water (into the pond, a shake).
    Splash,
    /// A bubble of light round a pet held up by the fox's spell (定身術),
    /// shimmering; `at` is the middle of the pet.
    Bubble,
    /// The fox casting: a glowing ring on the floor round its feet and
    /// fox-fire spiralling up; `at` is its feet.
    Spell,
    /// Two pets tussling: a cloud of dust with paws, a tail and stars
    /// poking out of it, rolling about; `at` is the middle of the pet.
    Brawl,
    /// A fright (put down by the fox's spell): lines of shock over the
    /// head and a drop of sweat sliding down; `at` is the top of the head.
    Fright,
}

impl Fx {
    pub const ALL: [Fx; 8] = [
        Fx::Hearts,
        Fx::Sparkles,
        Fx::Dust,
        Fx::Splash,
        Fx::Bubble,
        Fx::Spell,
        Fx::Brawl,
        Fx::Fright,
    ];

    /// How long it shows, seconds.
    pub fn secs(self) -> f32 {
        match self {
            Fx::Hearts => 1.6,
            Fx::Sparkles => 0.9,
            Fx::Dust => 0.7,
            Fx::Splash => 0.8,
            Fx::Bubble => 3.6,
            Fx::Spell => 1.4,
            Fx::Brawl => 2.6,
            Fx::Fright => 1.8,
        }
    }
}

/// Draw `fx`, `t` seconds in, at `at` (pixels of `pm`: the top of the head
/// for hearts, the feet for dust, the waterline for a splash).
pub fn effect(pm: &mut Pixmap, fx: Fx, at: (f32, f32), s: f32, t: f32) {
    let u = (t / fx.secs()).clamp(0.0, 1.0);
    let fade = 1.0 - u * u;
    let (x, y) = at;
    match fx {
        Fx::Hearts => {
            for (k, dx) in [-14.0f32, 4.0, 18.0].iter().enumerate() {
                let start = k as f32 * 0.18;
                let v = ((t - start) / (fx.secs() - start)).clamp(0.0, 1.0);
                if t < start {
                    continue;
                }
                let hx = x + dx * s + 4.0 * s * (TAU * 1.2 * t + k as f32).sin();
                let hy = y - 6.0 * s - v * 36.0 * s;
                let size = (5.0 + 2.0 * (k % 2) as f32) * s * (0.6 + 0.4 * (v * 4.0).min(1.0));
                heart(
                    pm,
                    hx,
                    hy,
                    size,
                    with_alpha([0xF2, 0x7B, 0x9A, 240], 1.0 - v * v),
                );
            }
        }
        Fx::Sparkles => {
            for k in 0..5 {
                let a = TAU * k as f32 / 5.0 + 0.6;
                let d = (10.0 + 14.0 * u) * s;
                let tw = (PI * (u * 2.0 + k as f32 * 0.3)).sin().abs();
                star(
                    pm,
                    x + a.cos() * d,
                    y + a.sin() * d * 0.7,
                    (2.5 + 3.5 * tw) * s,
                    with_alpha([0xFF, 0xE0, 0x7A, 255], fade),
                );
            }
        }
        Fx::Dust => {
            for side in [-1.0f32, 1.0] {
                for k in 0..3 {
                    let d = (6.0 + 10.0 * k as f32) * s * (0.4 + u);
                    let r = (3.0 + 2.0 * k as f32) * s * (0.6 + 0.8 * u);
                    circle(
                        pm,
                        x + side * d,
                        y - r * 0.6 - 3.0 * s * u,
                        r,
                        with_alpha([0xD9, 0xCE, 0xC0, 170], fade),
                    );
                }
            }
        }
        Fx::Bubble => bubble(pm, at, s, t, (1.0 - ((u - 0.92) / 0.08).max(0.0)).min(1.0)),
        Fx::Spell => spell(pm, at, s, t, u, fade),
        Fx::Brawl => brawl(pm, at, s, t, 1.0 - ((u - 0.9) / 0.1).max(0.0)),
        Fx::Fright => fright(pm, at, s, t, fade),
        Fx::Splash => {
            for k in 0..7 {
                let a = PI * (0.15 + 0.7 * k as f32 / 6.0);
                let speed = (60.0 + 14.0 * (k % 3) as f32) * s;
                let dx = -a.cos() * speed * t;
                let dy = -a.sin() * speed * t + 160.0 * s * t * t;
                circle(
                    pm,
                    x + dx,
                    y + dy,
                    (2.6 - 0.8 * u) * s,
                    with_alpha([0x8C, 0xCB, 0xF0, 230], fade),
                );
            }
        }
    }
}

/// The spell's bubble round a pet (see [`Fx::Bubble`]): see-through, a
/// bright rim, a highlight and a twinkle going round it.
fn bubble(pm: &mut Pixmap, at: (f32, f32), s: f32, t: f32, fade: f32) {
    let (x, y) = at;
    // Swelling up as it forms, wobbling a little after.
    let grow = (t / 0.35).min(1.0);
    let wob = 1.0 + 0.03 * (TAU * 1.3 * t).sin();
    let (rx, ry) = (74.0 * s * grow * wob, 70.0 * s * grow / wob);
    let id = Transform::identity();
    oval(
        pm,
        x,
        y,
        rx,
        ry,
        with_alpha([0xBF, 0xE6, 0xFF, 60], fade),
        id,
    );
    let mut rim = PathBuilder::new();
    if let Some(r) = Rect::from_xywh(x - rx, y - ry, 2.0 * rx, 2.0 * ry) {
        rim.push_oval(r);
    }
    stroke_path(
        pm,
        rim,
        with_alpha([0xF4, 0xFB, 0xFF, 200], fade),
        2.4 * s,
        id,
    );
    let mut shine = PathBuilder::new();
    shine.move_to(x - 0.75 * rx, y - 0.2 * ry);
    shine.quad_to(x - 0.7 * rx, y - 0.75 * ry, x - 0.15 * rx, y - 0.85 * ry);
    stroke_path(
        pm,
        shine,
        with_alpha([255, 255, 255, 220], fade),
        3.2 * s,
        id,
    );
    let a = 0.9 * TAU * t;
    star(
        pm,
        x + a.cos() * 0.92 * rx,
        y + a.sin() * 0.92 * ry,
        (3.0 + 1.5 * (TAU * 2.0 * t).sin().abs()) * s,
        with_alpha([0xFF, 0xF2, 0xB0, 255], fade),
    );
}

/// The fox's casting ring (see [`Fx::Spell`]).
fn spell(pm: &mut Pixmap, at: (f32, f32), s: f32, t: f32, u: f32, fade: f32) {
    let (x, y) = at;
    let id = Transform::identity();
    let grow = (u / 0.25).min(1.0);
    let (rx, ry) = (70.0 * s * grow, 16.0 * s * grow);
    oval(
        pm,
        x,
        y,
        rx,
        ry,
        with_alpha([0xA8, 0xD8, 0xFF, 70], fade),
        id,
    );
    for (k, c) in [[0x9E, 0xD4, 0xFF, 230], [0xFF, 0xB3, 0xD1, 200]]
        .iter()
        .enumerate()
    {
        let f = 1.0 - 0.18 * k as f32;
        let mut ring = PathBuilder::new();
        if let Some(r) = Rect::from_xywh(x - f * rx, y - f * ry, 2.0 * f * rx, 2.0 * f * ry) {
            ring.push_oval(r);
        }
        stroke_path(pm, ring, with_alpha(*c, fade), 2.0 * s, id);
    }
    // Fox-fire rising round it, each flame in its own place, swaying as it
    // goes up (Roger: not spinning round).
    for k in 0..6 {
        let p = (u * 1.4 + k as f32 / 6.0).fract();
        let a = TAU * (k as f32 / 6.0 + 1.0 / 12.0);
        let sway = 4.0 * s * (TAU * 1.6 * t + k as f32 * 1.9).sin();
        let (ox, oy) = (
            x + a.cos() * rx * (1.0 - 0.4 * p) + sway * p,
            y - p * 110.0 * s + a.sin() * ry,
        );
        let c = if k % 2 == 0 {
            [0x8F, 0xCF, 0xFF, 255]
        } else {
            [0xFF, 0xA8, 0xCC, 255]
        };
        glow(
            pm,
            ox,
            oy,
            (5.0 - 2.0 * p) * s,
            with_alpha(c, fade * (1.0 - p)),
        );
    }
}

/// A fright (see [`Fx::Fright`]): three lines of shock popping out over
/// the head, and a drop of sweat by it sliding down.
fn fright(pm: &mut Pixmap, at: (f32, f32), s: f32, t: f32, fade: f32) {
    let (x, y) = at;
    let id = Transform::identity();
    let pop = (t / 0.15).min(1.0);
    let ink = with_alpha([0x4A, 0x36, 0x30, 230], fade);
    for k in [-1.0f32, 0.0, 1.0] {
        let a = -std::f32::consts::FRAC_PI_2 + 0.55 * k;
        let (r0, r1) = (10.0 * s, (10.0 + 10.0 * pop) * s);
        let mut line = PathBuilder::new();
        line.move_to(x + a.cos() * r0, y - 4.0 * s + a.sin() * r0);
        line.line_to(x + a.cos() * r1, y - 4.0 * s + a.sin() * r1);
        stroke_path(pm, line, ink, 2.2 * s, id);
    }
    let (dx, dy) = (x + 24.0 * s, y + (8.0 + 10.0 * (t / 1.8).min(1.0)) * s);
    let r = 4.5 * s * pop;
    let mut drop = PathBuilder::new();
    drop.move_to(dx, dy - 2.4 * r);
    drop.quad_to(dx + r, dy - 0.6 * r, dx + r, dy);
    drop.quad_to(dx + r, dy + r, dx, dy + r);
    drop.quad_to(dx - r, dy + r, dx - r, dy);
    drop.quad_to(dx - r, dy - 0.6 * r, dx, dy - 2.4 * r);
    drop.close();
    if let Some(path) = drop.finish() {
        pm.fill_path(
            &path,
            &paint(with_alpha([0x9C, 0xD4, 0xF5, 235], fade)),
            FillRule::Winding,
            id,
            None,
        );
    }
    circle(
        pm,
        dx - 0.35 * r,
        dy - 0.1 * r,
        0.3 * r,
        with_alpha([255, 255, 255, 230], fade),
    );
}

/// A tussle's dust cloud (see [`Fx::Brawl`]): puffs rolling about, big
/// enough to hide the pet behind it, paws, a tail and stars poking out.
fn brawl(pm: &mut Pixmap, at: (f32, f32), s: f32, t: f32, fade: f32) {
    let (x, y) = at;
    let id = Transform::identity();
    let grow = (t / 0.2).min(1.0);
    let spin = TAU * 0.8 * t;
    // Paws and a tail first, so the cloud's puffs overlap their roots.
    for k in 0..4 {
        let a = spin * if k % 2 == 0 { 1.0 } else { -1.3 } + k as f32 * 1.7;
        let (px, py) = (x + a.cos() * 110.0 * s, y + a.sin() * 74.0 * s);
        let fur = if k % 2 == 0 {
            [0xE8, 0x9A, 0x58, 255]
        } else {
            [0xC9, 0xC6, 0xD6, 255]
        };
        if k == 3 {
            let mut tail = PathBuilder::new();
            tail.move_to(x + a.cos() * 60.0 * s, y + a.sin() * 40.0 * s);
            tail.quad_to(px + 14.0 * s, py - 18.0 * s, px, py - 4.0 * s);
            stroke_path(pm, tail, with_alpha(fur, fade), 9.0 * s, id);
        } else {
            circle(pm, px, py, 9.0 * s * grow, with_alpha(fur, fade));
            for toe in [-1.0f32, 0.0, 1.0] {
                circle(
                    pm,
                    px + toe * 4.5 * s + a.cos() * 6.0 * s,
                    py + a.sin() * 5.0 * s,
                    2.6 * s,
                    with_alpha([0xFF, 0xD6, 0xD6, 255], fade),
                );
            }
        }
    }
    // The cloud: a ring of puffs round a full middle, all rolling.
    oval(
        pm,
        x,
        y,
        78.0 * s * grow,
        56.0 * s * grow,
        with_alpha([0xEE, 0xE7, 0xDC, 255], fade),
        id,
    );
    for k in 0..11 {
        let a = TAU * k as f32 / 11.0 + 0.6 * spin;
        let r = (24.0 + 6.0 * (TAU * 2.0 * t + k as f32).sin()) * s * grow;
        let (cx, cy) = (x + a.cos() * 74.0 * s * grow, y + a.sin() * 50.0 * s * grow);
        circle(
            pm,
            cx,
            cy + 3.0 * s,
            r,
            with_alpha([0xD6, 0xCC, 0xBE, 255], fade),
        );
        circle(pm, cx, cy, r, with_alpha([0xF4, 0xEF, 0xE7, 255], fade));
    }
    // Stars and a swirl of action over it.
    for k in 0..3 {
        let a = -spin * 1.4 + k as f32 * TAU / 3.0;
        star(
            pm,
            x + a.cos() * 50.0 * s,
            y - 30.0 * s + a.sin() * 18.0 * s,
            (5.0 + 2.0 * (TAU * 3.0 * t + k as f32).sin().abs()) * s,
            with_alpha([0xFF, 0xD5, 0x5C, 255], fade),
        );
    }
    let mut swish = PathBuilder::new();
    swish.move_to(x - 30.0 * s, y + 6.0 * s);
    swish.quad_to(
        x,
        y - 22.0 * s + 8.0 * s * spin.sin(),
        x + 32.0 * s,
        y + 4.0 * s,
    );
    stroke_path(
        pm,
        swish,
        with_alpha([0xB8, 0xAE, 0xA2, 255], fade),
        2.5 * s,
        id,
    );
}

fn heart(pm: &mut Pixmap, x: f32, y: f32, r: f32, c: [u8; 4]) {
    let mut pb = PathBuilder::new();
    pb.move_to(x, y + r);
    pb.cubic_to(
        x - 1.6 * r,
        y - 0.1 * r,
        x - 0.9 * r,
        y - 1.3 * r,
        x,
        y - 0.45 * r,
    );
    pb.cubic_to(x + 0.9 * r, y - 1.3 * r, x + 1.6 * r, y - 0.1 * r, x, y + r);
    pb.close();
    if let Some(p) = pb.finish() {
        pm.fill_path(
            &p,
            &paint(c),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn star(pm: &mut Pixmap, x: f32, y: f32, r: f32, c: [u8; 4]) {
    let mut pb = PathBuilder::new();
    for k in 0..8 {
        let a = TAU * k as f32 / 8.0 - PI / 2.0;
        let d = if k % 2 == 0 { r } else { 0.32 * r };
        let (px, py) = (x + a.cos() * d, y + a.sin() * d);
        if k == 0 {
            pb.move_to(px, py);
        } else {
            pb.line_to(px, py);
        }
    }
    pb.close();
    if let Some(p) = pb.finish() {
        pm.fill_path(
            &p,
            &paint(c),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// The pond's water and its lighter middle.
const POND_WATER: [u8; 4] = [0x6F, 0xB8, 0xE0, 235];
const POND_DEEP: [u8; 4] = [0x3C, 0x84, 0xB8, 240];
const POND_BANK: [u8; 4] = [0x2E, 0x62, 0x8A, 245];
const POND_LIGHT: [u8; 4] = [0x9C, 0xD6, 0xF2, 150];

/// The pond's water: deep blue at the back (`cy - ry`) to light at the
/// front.
fn water_paint(cy: f32, ry: f32) -> Paint<'static> {
    let c = |c: [u8; 4]| Color::from_rgba8(c[0], c[1], c[2], c[3]);
    let stops = vec![
        GradientStop::new(0.0, c(POND_DEEP)),
        GradientStop::new(1.0, c(POND_WATER)),
    ];
    match tiny_skia::LinearGradient::new(
        Point::from_xy(0.0, cy - ry),
        Point::from_xy(0.0, cy + ry),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        Some(shader) => Paint {
            shader,
            anti_alias: true,
            ..Paint::default()
        },
        None => paint(POND_WATER),
    }
}

/// A ring spreading on water from `at`, `p` (0 … 1) of the way out to
/// `reach` (radii), fading as it goes.
fn ripple(pm: &mut Pixmap, at: (f32, f32), reach: (f32, f32), p: f32, s: f32) {
    let mut ring = PathBuilder::new();
    if let Some(r) = Rect::from_xywh(
        at.0 - p * reach.0,
        at.1 - p * reach.1,
        2.0 * p * reach.0 + 0.1,
        2.0 * p * reach.1 + 0.1,
    ) {
        ring.push_oval(r);
    }
    let fade = (1.0 - p) * (p / 0.15).min(1.0);
    stroke_path(
        pm,
        ring,
        with_alpha([255, 255, 255, 235], fade),
        (2.2 - 0.8 * p) * s,
        Transform::identity(),
    );
}

/// Rings spreading on the pond from each spot in `xs` (screen x of
/// something in the water: a pet wading or swimming), drawn over the
/// pond's own picture, `pond` the middle of its bottom on `pm`.
pub fn pond_stirs(pm: &mut Pixmap, pond: (f32, f32), xs: &[f32], s: f32, t: f32) {
    let (cx, cy, rx, ry) = pond_oval(pond, s);
    let Some(water) = oval_path(cx, cy, rx, ry) else {
        return;
    };
    let Some(mut inside) = Mask::new(pm.width(), pm.height()) else {
        return;
    };
    inside.fill_path(&water, FillRule::Winding, true, Transform::identity());
    let mut rings = Pixmap::new(pm.width(), pm.height());
    for (i, &x) in xs.iter().enumerate() {
        let Some(rings) = rings.as_mut() else {
            return;
        };
        for k in 0..2 {
            let p = (t * 0.8 + 0.5 * k as f32 + 0.37 * i as f32).fract();
            ripple(rings, (x, cy + 0.25 * ry), (0.45 * rx, 0.7 * ry), p, s);
        }
    }
    if let Some(rings) = rings {
        pm.draw_pixmap(
            0,
            0,
            rings.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            Some(&inside),
        );
    }
}

/// The pond's water, an oval: middle and radii, for the pond with the
/// middle of its bottom at `at`.
pub fn pond_oval(at: (f32, f32), s: f32) -> (f32, f32, f32, f32) {
    let (w, h) = Thing::Pond.size();
    (at.0, at.1 - 0.45 * h * s, 0.47 * w * s, 0.38 * h * s)
}

/// The lighter middle of the pond's water (middle and radii).
fn pond_light(cx: f32, cy: f32, rx: f32, ry: f32) -> (f32, f32, f32, f32) {
    (cx - 0.1 * rx, cy - 0.15 * ry, 0.7 * rx, 0.55 * ry)
}

fn oval_path(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<Path> {
    let r = Rect::from_xywh(cx - rx, cy - ry, 2.0 * rx, 2.0 * ry)?;
    let mut pb = PathBuilder::new();
    pb.push_oval(r);
    pb.finish()
}

/// The pond's water over a duckling swimming in it (PLAN §8 "Roger's
/// tenth pass", item 2): from the waterline `line` down, the pond's own
/// blues clipped to the pond's oval, so the body under the water shows
/// faintly through the same colours as the pond behind it; whatever of
/// the picture hangs below the pond's near edge is wiped out (it would
/// show under the pond). A pet wading has `line` low, over its legs. `pond` is where the middle of the pond's bottom
/// lies on `pm` (its own window is behind), `x` the middle of the body,
/// for the rings spreading from it.
pub fn pond_front(pm: &mut Pixmap, pond: (f32, f32), x: f32, line: f32, s: f32, t: f32) {
    let (w, h) = (pm.width(), pm.height());
    let (cx, cy, rx, ry) = pond_oval(pond, s);
    let (Some(water), Some(mut inside)) = (oval_path(cx, cy, rx, ry), Mask::new(w, h)) else {
        return;
    };
    inside.fill_path(&water, FillRule::Winding, true, Transform::identity());
    // Under a waterline that laps a little.
    let mut pb = PathBuilder::new();
    let n = 24;
    for k in 0..=n {
        let u = k as f32 / n as f32;
        let wx = u * w as f32;
        let wy = line + 1.1 * s * (TAU * (u * w as f32 / (60.0 * s) + 0.6 * t)).sin();
        if k == 0 {
            pb.move_to(wx, wy);
        } else {
            pb.line_to(wx, wy);
        }
    }
    pb.line_to(w as f32, h as f32);
    pb.line_to(0.0, h as f32);
    pb.close();
    let Some(under) = pb.finish() else {
        return;
    };
    let id = Transform::identity();
    // Below the pond's near edge, nothing of the body shows (on the bank
    // either side of the pond, the legs do).
    let mut outside = inside.clone();
    outside.invert();
    if let Some(strip) = Rect::from_xywh(cx - rx, 0.0, 2.0 * rx, h as f32) {
        outside.intersect_path(&PathBuilder::from_rect(strip), FillRule::Winding, false, id);
    }
    let clear = Paint {
        blend_mode: BlendMode::Clear,
        ..Paint::default()
    };
    pm.fill_path(&under, &clear, FillRule::Winding, id, Some(&outside));
    // In the water: the pond's blues over the body, a touch thinner.
    let mut wet = inside.clone();
    wet.intersect_path(&under, FillRule::Winding, true, id);
    pm.fill_path(
        &under,
        &water_paint(cy, ry),
        FillRule::Winding,
        id,
        Some(&wet),
    );
    let (hx, hy, hrx, hry) = pond_light(cx, cy, rx, ry);
    if let Some(light) = oval_path(hx, hy, hrx, hry) {
        pm.fill_path(
            &light,
            &paint(with_alpha(POND_LIGHT, 0.92)),
            FillRule::Winding,
            id,
            Some(&wet),
        );
    }
    // A bright line along the waterline and rings spreading from the
    // body, on the water only.
    let stroke = |pm: &mut Pixmap, pb: PathBuilder, c: [u8; 4], width: f32| {
        let Some(path) = pb.finish() else {
            return;
        };
        let st = Stroke {
            width,
            line_cap: LineCap::Round,
            ..Stroke::default()
        };
        pm.stroke_path(&path, &paint(c), &st, id, Some(&inside));
    };
    let mut top = PathBuilder::new();
    top.move_to(x - 0.3 * rx, line + 1.0 * s);
    top.line_to(x + 0.3 * rx, line + 1.0 * s);
    stroke(pm, top, [0xC8, 0xE8, 0xF8, 210], 1.5 * s);
    for k in 0..2 {
        let p = (t * 0.5 + k as f32 * 0.5).fract();
        let (ex, ey) = ((0.2 + 0.25 * p) * rx, (2.0 + 4.0 * p) * s);
        let mut ring = PathBuilder::new();
        if let Some(r) = Rect::from_xywh(x - ex, line + 2.0 * s - ey, 2.0 * ex, 2.0 * ey) {
            ring.push_oval(r);
        }
        stroke(pm, ring, with_alpha([255, 255, 255, 170], 1.0 - p), 1.1 * s);
    }
}

/// The fox's fire: little orbs of blue and pink fox-fire hanging round the
/// pet's middle `at`, each in its own place, bobbing and drifting a little
/// (Roger: not going round and round), `t` seconds in.
pub fn foxfire(pm: &mut Pixmap, at: (f32, f32), reach: (f32, f32), s: f32, t: f32) {
    let colours = [
        [0x8F, 0xC8, 0xFF, 230],
        [0xFF, 0x9E, 0xC6, 230],
        [0xB6, 0xA8, 0xFF, 230],
        [0x8F, 0xE8, 0xF0, 230],
    ];
    for (k, c) in colours.iter().enumerate() {
        let kf = k as f32;
        let a = TAU * ((kf + 0.5) / colours.len() as f32 + 0.02 * (TAU * 0.13 * t + kf).sin());
        let (ox, oy) = (
            at.0 + a.cos() * reach.0,
            at.1 + a.sin() * reach.1 + 4.0 * s * (TAU * 0.8 * t + kf).sin(),
        );
        glow(pm, ox, oy, 15.0 * s, with_alpha(*c, 0.7));
        circle(pm, ox, oy, 4.2 * s, [255, 255, 255, 230]);
        // A flame's tip flickering up off it.
        let mut tip = PathBuilder::new();
        tip.move_to(ox - 3.4 * s, oy - 1.0 * s);
        tip.quad_to(
            ox + 2.0 * s * (TAU * 2.0 * t + kf).sin(),
            oy - 13.0 * s,
            ox + 3.4 * s,
            oy - 1.0 * s,
        );
        tip.close();
        if let Some(p) = tip.finish() {
            pm.fill_path(
                &p,
                &paint(*c),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Thing; 9] = [
        Thing::Ball,
        Thing::Frisbee,
        Thing::Catnip,
        Thing::Bone,
        Thing::Seeds,
        Thing::Branch,
        Thing::Pond,
        Thing::Sunbeam,
        Thing::Butterfly,
    ];

    fn edges_clear(pm: &Pixmap) -> bool {
        let (w, h) = (pm.width(), pm.height());
        let d = pm.data();
        let a = |x: u32, y: u32| d[((y * w + x) * 4 + 3) as usize];
        (0..h).all(|y| a(0, y) == 0 && a(w - 1, y) == 0) && (0..w).all(|x| a(x, 0) == 0)
    }

    #[test]
    fn every_thing_fits_its_window_however_it_turns() {
        for thing in ALL {
            for scale in [1.0, 1.5, 2.0] {
                let ((w, h), foot) = canvas(thing, scale);
                for spin in [0.0, 0.8, 2.0, 3.5] {
                    for t in [0.0, 0.4, 1.1, 2.5] {
                        let mut pm = Pixmap::new(w, h).unwrap();
                        let look = Look {
                            spin,
                            t,
                            grounded: true,
                            ..Look::default()
                        };
                        draw(&mut pm, thing, foot, scale, look);
                        let drawn = pm
                            .data()
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .filter(|p| p[3] > 0)
                            .count();
                        // (A sunbeam fades in from nothing.)
                        let fading_in = thing == Thing::Sunbeam && t == 0.0;
                        assert!(drawn > 40 || fading_in, "{thing:?} drew {drawn} pixels");
                        assert!(
                            edges_clear(&pm),
                            "{thing:?} at {scale} spin {spin} t {t} touches an edge"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn catnip_and_seeds_show_less_as_they_are_used() {
        for thing in [Thing::Catnip, Thing::Seeds] {
            let ((w, h), foot) = canvas(thing, 1.0);
            let count = |used: f32| {
                let mut pm = Pixmap::new(w, h).unwrap();
                draw(
                    &mut pm,
                    thing,
                    foot,
                    1.0,
                    Look {
                        t: 0.5,
                        used,
                        grounded: true,
                        ..Look::default()
                    },
                );
                pm.data()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[3] > 0)
                    .count()
            };
            assert!(count(0.9) < count(0.0), "{thing:?} looks no less used");
        }
    }

    #[test]
    fn effects_fade_out_by_their_end() {
        for fx in Fx::ALL {
            let mut pm = Pixmap::new(160, 160).unwrap();
            effect(&mut pm, fx, (80.0, 90.0), 1.0, 0.3 * fx.secs());
            let early = pm
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 8)
                .count();
            assert!(early > 20, "{fx:?} shows nothing early on");
            let mut pm = Pixmap::new(160, 160).unwrap();
            effect(&mut pm, fx, (80.0, 90.0), 1.0, fx.secs());
            let end = pm
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 8)
                .count();
            assert_eq!(end, 0, "{fx:?} still shows at its end");
        }
    }

    /// A tussle's cloud hides the two pets behind it: solid over the whole
    /// middle of a pet's picture.
    #[test]
    fn a_tussle_cloud_hides_the_pet_behind_it() {
        let mut pm = Pixmap::new(260, 190).unwrap();
        effect(&mut pm, Fx::Brawl, (130.0, 110.0), 1.0, 1.0);
        for (x, y) in [(130, 110), (70, 110), (190, 110), (130, 70), (130, 150)] {
            let a = pm.pixel(x, y).map_or(0, |p| p.alpha());
            assert_eq!(a, 255, "the pet shows through at ({x}, {y})");
        }
    }
}
