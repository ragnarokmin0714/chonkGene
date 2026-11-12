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
/// Room over a pond for water thrown in a water fight, pixels at 100%.
const SPRAY_ROOM: f32 = 130.0;

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
    let top = match thing {
        Thing::Catnip => SMELL_ROOM,
        Thing::Pond => SPRAY_ROOM,
        _ => MARGIN,
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
    /// A pond: water thrown in a water fight, from and to (x from the
    /// middle of the pond's bottom, pixels) and how far it has flown (0 …
    /// 1).
    pub sprays: [Option<(f32, f32, f32)>; 4],
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
            // No bank: water fading out into the desktop at its edges,
            // light at the rim and deep in the middle (Roger: borderless,
            // deeper, bigger, room for more of them).
            let (cx, cy, rx, ry) = pond_oval(at, s);
            water(pm, (cx, cy, rx, ry), look.t, None);
            // Rings spreading on the water.
            for k in 0..4 {
                let p = (look.t * 0.3 + k as f32 / 4.0).fract();
                let (ox, oy) = (cx + (k as f32 - 1.5) * 0.4 * rx, cy + 0.1 * ry);
                ripple(pm, (ox, oy), (0.22 * rx, 0.4 * ry), p, s);
            }
            // Two lily pads with a notch, one with a flower.
            for (k, (u, v, r)) in [(0.62, 0.25, 1.0), (-0.7, -0.15, 0.8)]
                .into_iter()
                .enumerate()
            {
                let (lx, ly) = (cx + u * rx, cy + v * ry);
                let bob = 0.8 * s * (TAU * 0.3 * look.t + k as f32).sin();
                oval(
                    pm,
                    lx,
                    ly + bob,
                    15.0 * r * s,
                    5.5 * r * s,
                    [0x5E, 0x9E, 0x55, 240],
                    Transform::identity(),
                );
                if k == 0 {
                    circle(
                        pm,
                        lx + 2.0 * s,
                        ly + bob - 3.5 * s,
                        3.4 * s,
                        [0xF6, 0xB8, 0xC8, 255],
                    );
                    circle(
                        pm,
                        lx + 2.0 * s,
                        ly + bob - 3.5 * s,
                        1.5 * s,
                        [0xFF, 0xE8, 0x9A, 255],
                    );
                }
            }
            // Water being thrown about (a water fight).
            for spray in look.sprays.iter().flatten() {
                splash_arc(pm, (at.0 + spray.0, cy), (at.0 + spray.1, cy), spray.2, s);
            }
        }
        Thing::Sunbeam => {
            // A warm slanting shaft of light down to the floor, a bright
            // pool where it lands, motes glowing in it (Roger's fifteenth
            // pass, item 8: the flat one looked weak and fake).
            let fade = (look.t / 1.5).min(1.0) * (1.0 - look.used).clamp(0.0, 1.0);
            sunbeam(pm, (x, y), (w * s, h * s), look.t, fade);
        }
        Thing::Spotlight => {
            // Snapping on in a quarter of a second, out over the last one.
            let on = (look.t / 0.25).min(1.0) * (1.0 - look.used).clamp(0.0, 1.0);
            spotlight(pm, (x, y), (w * s, h * s), look.t, on);
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
    /// The spell's bubble round a pet asleep: lilac, a crescent moon and
    /// stars drifting round its rim; `at` is the middle of the pet.
    Dream,
    /// Music notes floating up, swaying (dancing); `at` is the top of the
    /// head.
    Notes,
}

impl Fx {
    pub const ALL: [Fx; 10] = [
        Fx::Hearts,
        Fx::Sparkles,
        Fx::Dust,
        Fx::Splash,
        Fx::Bubble,
        Fx::Spell,
        Fx::Brawl,
        Fx::Fright,
        Fx::Dream,
        Fx::Notes,
    ];

    /// How long it shows, seconds.
    pub fn secs(self) -> f32 {
        match self {
            Fx::Hearts => 1.6,
            Fx::Sparkles => 0.9,
            Fx::Dust => 0.7,
            Fx::Splash => 0.8,
            Fx::Bubble | Fx::Dream => 3.6,
            Fx::Spell => 1.8,
            Fx::Brawl => 2.6,
            Fx::Fright => 1.8,
            Fx::Notes => 2.2,
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
        Fx::Bubble | Fx::Dream => {
            let fade = (1.0 - ((u - 0.92) / 0.08).max(0.0)).min(1.0);
            bubble(pm, at, (74.0 * s, 70.0 * s), s, t, fade, fx == Fx::Dream);
        }
        Fx::Spell => spell(pm, at, s, t, u, fade),
        Fx::Brawl => brawl(pm, at, s, t, 1.0 - ((u - 0.9) / 0.1).max(0.0)),
        Fx::Fright => fright(pm, at, s, t, fade),
        Fx::Notes => notes(pm, at, s, t, fade),
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

/// The spell's bubble (or a dream bubble) round a pet, `t` seconds in:
/// as [`effect`] for [`Fx::Bubble`] and [`Fx::Dream`], but in the middle
/// `at` and of the radii `size` that [`bubble_fit`] found for it.
pub fn bubble_round(
    pm: &mut Pixmap,
    dream: bool,
    at: (f32, f32),
    size: (f32, f32),
    s: f32,
    t: f32,
) {
    let u = (t / Fx::Bubble.secs()).clamp(0.0, 1.0);
    let fade = (1.0 - ((u - 0.92) / 0.08).max(0.0)).min(1.0);
    bubble(pm, at, size, s, t, fade, dream);
}

/// Where the opaque part of `pm` is (alpha over half): left, top, right,
/// bottom, pixels; none if it is all see-through.
pub fn opaque_bounds(pm: &Pixmap) -> Option<(f32, f32, f32, f32)> {
    let w = pm.width() as usize;
    let mut b: Option<(usize, usize, usize, usize)> = None;
    for (i, px) in pm.pixels().iter().enumerate() {
        if px.alpha() > 128 {
            let (x, y) = (i % w, i / w);
            b = Some(b.map_or((x, y, x, y), |(x0, y0, x1, y1)| {
                (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
            }));
        }
    }
    b.map(|(x0, y0, x1, y1)| (x0 as f32, y0 as f32, x1 as f32 + 1.0, y1 as f32 + 1.0))
}

/// The middle and radii of a bubble round a pet whose picture's opaque
/// part is `bounds`, on a canvas `canvas` big (Roger's fifteenth pass,
/// item 7: tall heads, ears and tufts stuck out of the one size for all):
/// all of it inside with room to spare, never smaller than the old bubble
/// (74 × 70 at 100%) unless the canvas is, and moved inside the canvas.
pub fn bubble_fit(
    bounds: (f32, f32, f32, f32),
    canvas: (f32, f32),
    s: f32,
) -> ((f32, f32), (f32, f32)) {
    let (x0, y0, x1, y1) = bounds;
    let (cx, cy) = (0.5 * (x0 + x1), 0.5 * (y0 + y1));
    // A pet is round, its box's corners empty: a fifth more than its half
    // sides, and a little, covers the ears and tufts at the top.
    let pad = 6.0 * s;
    let rx = (0.5 * (x1 - x0) * 1.2 + pad).max(74.0 * s);
    let ry = (0.5 * (y1 - y0) * 1.2 + pad).max(70.0 * s);
    // Inside the canvas: moved in from its edges, smaller only if it is
    // bigger than the canvas.
    let rx = rx.min(0.5 * canvas.0 - 1.0).max(1.0);
    let ry = ry.min(0.5 * canvas.1 - 1.0).max(1.0);
    let cx = cx.clamp(rx + 1.0, canvas.0 - rx - 1.0);
    let cy = cy.clamp(ry + 1.0, canvas.1 - ry - 1.0);
    ((cx, cy), (rx, ry))
}

/// The spell's bubble round a pet (see [`Fx::Bubble`]): see-through, a
/// bright rim, a highlight and a twinkle going round it; `dream`: lilac,
/// with a crescent moon and stars (see [`Fx::Dream`]).
fn bubble(
    pm: &mut Pixmap,
    at: (f32, f32),
    size: (f32, f32),
    s: f32,
    t: f32,
    fade: f32,
    dream: bool,
) {
    let (x, y) = at;
    // Swelling up as it forms, wobbling a little after.
    let grow = (t / 0.35).min(1.0);
    let wob = 1.0 + 0.03 * (TAU * 1.3 * t).sin();
    let (rx, ry) = (size.0 * grow * wob, size.1 * grow / wob);
    let id = Transform::identity();
    oval(
        pm,
        x,
        y,
        rx,
        ry,
        with_alpha(
            if dream {
                [0xD8, 0xC8, 0xFF, 70]
            } else {
                [0xBF, 0xE6, 0xFF, 60]
            },
            fade,
        ),
        id,
    );
    if dream {
        // A crescent moon up by the rim, and stars drifting round.
        let (mx, my, mr) = (x + 0.45 * rx, y - 0.55 * ry, 9.0 * s * grow);
        circle(pm, mx, my, mr, with_alpha([0xFF, 0xE9, 0x9A, 240], fade));
        circle(
            pm,
            mx + 0.45 * mr,
            my - 0.3 * mr,
            0.85 * mr,
            with_alpha([0xD8, 0xC8, 0xFF, 255], fade),
        );
        for k in 0..3 {
            let a = TAU * (0.12 * t + k as f32 / 3.0) + 2.0;
            let tw = (TAU * (0.8 * t + 0.3 * k as f32)).sin().abs();
            star(
                pm,
                x + a.cos() * 0.72 * rx,
                y + a.sin() * 0.72 * ry,
                (2.0 + 2.0 * tw) * s,
                with_alpha([0xFF, 0xF6, 0xD0, 255], fade),
            );
        }
    }
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
    // Wide, and the fire tall and big, so it reads as a spell from across
    // the desk (Roger's fourteenth and fifteenth passes: bigger, bigger
    // still). As wide as the fox's window allows, its glow and all.
    let grow = (u / 0.15).min(1.0);
    let (rx, ry) = (120.0 * s * grow, 17.0 * s * grow);
    // Raised a little off the feet, so the window's bottom edge does not
    // cut its near side off.
    let y = y - 8.0 * s;
    // A column of light going up from the ring.
    let rise = 175.0 * s;
    for k in 0..4 {
        let kf = k as f32 / 4.0;
        let a = (1.0 - kf) * fade * (0.5 + 0.5 * grow);
        oval(
            pm,
            x,
            y - kf * 0.75 * rise,
            rx * (1.0 - 0.45 * kf),
            ry + kf * 0.3 * rise,
            with_alpha([0xA8, 0xD4, 0xFF, 22], a),
            id,
        );
    }
    oval(
        pm,
        x,
        y,
        rx,
        ry,
        with_alpha([0xA8, 0xD8, 0xFF, 90], fade),
        id,
    );
    for (k, c) in [[0x9E, 0xD4, 0xFF, 240], [0xFF, 0xB3, 0xD1, 220]]
        .iter()
        .enumerate()
    {
        let f = 1.0 - 0.18 * k as f32;
        let mut ring = PathBuilder::new();
        if let Some(r) = Rect::from_xywh(x - f * rx, y - f * ry, 2.0 * f * rx, 2.0 * f * ry) {
            ring.push_oval(r);
        }
        stroke_path(pm, ring, with_alpha(*c, fade), 3.4 * s, id);
    }
    // A flash going out as it is cast.
    if u < 0.3 {
        let v = u / 0.3;
        let mut flash = PathBuilder::new();
        let (fx, fy) = (rx * (0.4 + 0.8 * v), ry * (0.4 + 0.8 * v));
        if let Some(r) = Rect::from_xywh(x - fx, y - fy, 2.0 * fx, 2.0 * fy) {
            flash.push_oval(r);
        }
        stroke_path(
            pm,
            flash,
            with_alpha([255, 255, 255, 240], 1.0 - v),
            (6.0 - 4.0 * v) * s,
            id,
        );
    }
    // Fox-fire rising round it, each flame in its own place, swaying as it
    // goes up (Roger: not spinning round), a bright heart in each; big at
    // the ring, smaller as they rise.
    const FLAMES: usize = 14;
    for k in 0..FLAMES {
        let kf = k as f32 / FLAMES as f32;
        let p = (u * 1.4 + kf).fract();
        let a = TAU * (kf + 0.5 / FLAMES as f32);
        let sway = 7.0 * s * (TAU * 1.6 * t + k as f32 * 1.9).sin();
        let (ox, oy) = (
            x + a.cos() * rx * (1.0 - 0.35 * p) + sway * p,
            y - p * rise + a.sin() * ry,
        );
        let c = if k % 2 == 0 {
            [0x8F, 0xCF, 0xFF, 255]
        } else {
            [0xFF, 0xA8, 0xCC, 255]
        };
        let alpha = fade * (1.0 - p * p) * grow;
        flame(pm, (ox, oy), (2.3 - 1.1 * p) * s, c, t + k as f32, alpha);
    }
}

/// The sunbeam's shaft, its corners (top left, top right, bottom right,
/// bottom left) for a thing `size` big with its foot at `at`: slanting in
/// from the upper right, narrow at the top and wide on the floor.
fn sunbeam_corners(at: (f32, f32), size: (f32, f32)) -> [(f32, f32); 4] {
    let ((x, y), (w, h)) = (at, size);
    [
        (x - 0.04 * w, y - h),
        (x + 0.2 * w, y - h),
        (x + 0.39 * w, y),
        (x - 0.39 * w, y),
    ]
}

fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let k = ((v - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// The sunbeam (see [`Thing::Sunbeam`]): its shaft worked out pixel by
/// pixel (feathered sides, whiter high up and golden low, streaks drifting
/// in it), the pool on the floor, the motes; `fade` 0 … 1. A layered
/// window can only lay colour over the desktop, so the pool is what makes
/// it read as light over a dark one.
fn sunbeam(pm: &mut Pixmap, at: (f32, f32), size: (f32, f32), t: f32, fade: f32) {
    if fade <= 0.0 {
        return;
    }
    let (x, y) = at;
    let (w, h) = size;
    let c = sunbeam_corners(at, size);
    let top = y - h;
    let (pw, ph) = (pm.width() as usize, pm.height() as usize);
    let data = pm.data_mut();
    for py in (top.max(0.0) as usize)..(y.max(0.0) as usize).min(ph) {
        let v = (py as f32 + 0.5 - top) / h;
        let left = c[0].0 + (c[3].0 - c[0].0) * v;
        let right = c[1].0 + (c[2].0 - c[1].0) * v;
        // In from nothing at the top, strongest low; white high up, golden
        // by the floor.
        let down = smooth(0.0, 0.25, v) * (0.62 + 0.38 * v);
        let colour = [255.0, 253.0 - 17.0 * v, 242.0 - 108.0 * v];
        let x0 = left.floor().max(0.0) as usize;
        let x1 = (right.ceil().max(0.0) as usize).min(pw);
        for px in x0..x1 {
            let f = (px as f32 + 0.5 - left) / (right - left);
            if !(0.0..=1.0).contains(&f) {
                continue;
            }
            let soft = smooth(0.0, 0.22, f) * smooth(1.0, 0.78, f);
            let mut streak = 0.0;
            for k in 0..3 {
                let kf = k as f32;
                let mid = 0.3 + 0.2 * kf + 0.05 * (0.35 * t + 2.0 * kf).sin();
                let d = (f - mid).abs() / (0.025 + 0.012 * kf);
                streak += (1.0 - d * d).max(0.0) * 0.35 * smooth(0.1, 0.5, v) * (1.0 - v);
            }
            let a = (soft * (0.55 * down + 0.5 * streak)).min(0.8) * fade;
            if a <= 0.0 {
                continue;
            }
            // Over what is there (premultiplied).
            let i = 4 * (py * pw + px);
            let keep = 1.0 - a;
            for (k, cv) in colour.iter().enumerate() {
                data[i + k] = (cv * a + f32::from(data[i + k]) * keep).min(255.0) as u8;
            }
            data[i + 3] = (255.0 * a + f32::from(data[i + 3]) * keep).min(255.0) as u8;
        }
    }
    // The pool where it lands: bright in the middle, soft round the edge.
    let (rx, ry) = (0.48 * w, 0.088 * w);
    let cy = y - 2.0 * (w / 260.0);
    let alpha = |a: f32| (a * fade) as u8;
    if let Some(shader) = tiny_skia::RadialGradient::new(
        Point::from_xy(0.0, 0.0),
        0.0,
        Point::from_xy(0.0, 0.0),
        rx,
        vec![
            GradientStop::new(0.0, Color::from_rgba8(0xFF, 0xF8, 0xE2, alpha(210.0))),
            GradientStop::new(0.55, Color::from_rgba8(0xFF, 0xEC, 0xB4, alpha(150.0))),
            GradientStop::new(1.0, Color::from_rgba8(0xFF, 0xE0, 0x90, 0)),
        ],
        SpreadMode::Pad,
        Transform::from_scale(1.0, ry / rx).post_translate(x, cy),
    ) && let Some(r) = Rect::from_xywh(x - rx, cy - ry, 2.0 * rx, 2.0 * ry)
    {
        let mut pool = PathBuilder::new();
        pool.push_oval(r);
        if let Some(path) = pool.finish() {
            let p = Paint {
                shader,
                anti_alias: true,
                ..Paint::default()
            };
            pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
        }
    }
    // Motes drifting up through it, glowing and twinkling.
    let s = w / 260.0;
    for k in 0..11 {
        let kf = k as f32;
        let p = (t * 0.06 + kf * 0.093).fract();
        let u = 0.5 + 0.38 * (kf * 1.7).sin() + 0.06 * (0.7 * t + kf).sin();
        let my = y - (1.0 - p) * h * 0.92;
        let v = (my - top) / h;
        let left = c[0].0 + (c[3].0 - c[0].0) * v;
        let right = c[1].0 + (c[2].0 - c[1].0) * v;
        let mx = left + (right - left) * u;
        let twinkle = 0.55 + 0.45 * (5.0 * t + 2.3 * kf).sin();
        let a = (1.0 - (2.0 * p - 1.0).powi(2)) * twinkle * fade;
        circle(pm, mx, my, 5.0 * s, [255, 246, 210, (55.0 * a) as u8]);
        circle(pm, mx, my, 1.9 * s, [255, 255, 245, (235.0 * a) as u8]);
    }
}

/// A pet lying in the sunbeam, lit by it (Roger's pick, fifteenth pass,
/// item 8): warmer and brighter on the side toward the light (the upper
/// right), and a warm rim along its edges facing it; `k` 0 … 1. Only its
/// own pixels change: `pm` must hold nothing but the pet.
pub fn sunlit(pm: &mut Pixmap, k: f32) {
    if k <= 0.0 {
        return;
    }
    let h = pm.height() as f32;
    // Across the pet itself, not its whole window.
    let Some((x0, y0, x1, y1)) = opaque_bounds(pm) else {
        return;
    };
    let (bw, bh) = (x1 - x0, y1 - y0);
    let w = pm.width() as f32;
    let a = |v: f32| (v * k) as u8;
    if let Some(shader) = tiny_skia::LinearGradient::new(
        Point::from_xy(x1, y0),
        Point::from_xy(x0 + 0.15 * bw, y0 + 0.85 * bh),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(0xFF, 0xE6, 0xA8, a(120.0))),
            GradientStop::new(0.6, Color::from_rgba8(0xFF, 0xE0, 0x9C, a(45.0))),
            GradientStop::new(1.0, Color::from_rgba8(0xFF, 0xD8, 0x90, 0)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    ) && let Some(r) = Rect::from_xywh(0.0, 0.0, w, h)
    {
        let warm = Paint {
            shader,
            blend_mode: BlendMode::SourceAtop,
            anti_alias: true,
            ..Paint::default()
        };
        pm.fill_rect(r, &warm, Transform::identity(), None);
    }
    // The rim: its pixels with nothing just up and to the right of them.
    let (iw, ih) = (pm.width() as usize, pm.height() as usize);
    let d = ((3.0 * h / 191.0).round() as usize).max(1);
    let alpha: Vec<u8> = pm.pixels().iter().map(|p| p.alpha()).collect();
    let data = pm.data_mut();
    for y in d..ih {
        for x in 0..iw.saturating_sub(d) {
            let i = y * iw + x;
            let here = alpha[i];
            if here > 160 && alpha[(y - d) * iw + x + d] < 40 {
                let m = 0.6 * k * f32::from(here) / 255.0;
                let cover = f32::from(here) / 255.0;
                for (c, warm) in data[4 * i..4 * i + 3].iter_mut().zip([255.0, 244.0, 205.0]) {
                    *c = (f32::from(*c) * (1.0 - m) + warm * m * cover) as u8;
                }
            }
        }
    }
}

/// A stage spotlight (see [`Thing::Spotlight`]): a cone of white light
/// straight down from above with crisp sides, a bright pool on the floor
/// with a sharp rim, dust sparkling in it; `on` 0 … 1.
fn spotlight(pm: &mut Pixmap, at: (f32, f32), size: (f32, f32), t: f32, on: f32) {
    if on <= 0.0 {
        return;
    }
    let ((x, y), (w, h)) = (at, size);
    let s = w / 240.0;
    let top = y - h;
    let (half_top, half_floor) = (0.07 * w, 0.42 * w);
    let (pw, ph) = (pm.width() as usize, pm.height() as usize);
    let data = pm.data_mut();
    for py in (top.max(0.0) as usize)..(y.max(0.0) as usize).min(ph) {
        let v = (py as f32 + 0.5 - top) / h;
        let half = half_top + (half_floor - half_top) * v;
        // A beam of light seen side on: brighter in the middle, the edges
        // crisp (a stage light, not the sun), stronger near the lamp.
        let down = 0.42 - 0.12 * v;
        let x0 = (x - half).floor().max(0.0) as usize;
        let x1 = ((x + half).ceil().max(0.0) as usize).min(pw);
        for px in x0..x1 {
            let f = ((px as f32 + 0.5 - x) / half).abs();
            if f > 1.0 {
                continue;
            }
            let edge = smooth(1.0, 0.9, f);
            let core = 0.75 + 0.25 * (1.0 - f * f);
            let a = (down * core * edge * on).min(0.85);
            if a <= 0.0 {
                continue;
            }
            let i = 4 * (py * pw + px);
            let keep = 1.0 - a;
            for (k, cv) in [255.0, 253.0, 244.0].iter().enumerate() {
                data[i + k] = (cv * a + f32::from(data[i + k]) * keep).min(255.0) as u8;
            }
            data[i + 3] = (255.0 * a + f32::from(data[i + 3]) * keep).min(255.0) as u8;
        }
    }
    // The pool: bright right out to a sharp rim.
    let (rx, ry) = (half_floor * 1.02, 0.1 * w);
    let cy = y - 2.0 * s;
    let alpha = |a: f32| (a * on) as u8;
    if let Some(shader) = RadialGradient::new(
        Point::from_xy(0.0, 0.0),
        0.0,
        Point::from_xy(0.0, 0.0),
        rx,
        vec![
            GradientStop::new(0.0, Color::from_rgba8(0xFF, 0xFF, 0xFA, alpha(230.0))),
            GradientStop::new(0.8, Color::from_rgba8(0xFF, 0xFB, 0xEC, alpha(200.0))),
            GradientStop::new(0.93, Color::from_rgba8(0xFF, 0xF4, 0xD8, alpha(150.0))),
            GradientStop::new(1.0, Color::from_rgba8(0xFF, 0xF0, 0xD0, 0)),
        ],
        SpreadMode::Pad,
        Transform::from_scale(1.0, ry / rx).post_translate(x, cy),
    ) && let Some(path) = oval_path(x, cy, rx, ry)
    {
        let p = Paint {
            shader,
            anti_alias: true,
            ..Paint::default()
        };
        pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
    }
    // The lamp's glare at the top.
    glow(
        pm,
        x,
        top + 6.0 * s,
        26.0 * s,
        [255, 255, 245, (200.0 * on) as u8],
    );
    // Dust sparkling in the light, drifting down slowly.
    for k in 0..14 {
        let kf = k as f32;
        let p = (kf * 0.137 + 0.03 * t).fract();
        let v = 0.12 + 0.82 * p;
        let half = half_top + (half_floor - half_top) * v;
        let u = (kf * 2.39).sin() * 0.8 + 0.08 * (0.6 * t + kf).sin();
        let twinkle = (0.5 + 0.5 * (6.0 * t + 1.7 * kf).sin()).powi(3);
        let a = twinkle * on * smooth(0.0, 0.1, p) * smooth(1.0, 0.9, p);
        let (sx, sy) = (x + u * half, top + v * h);
        star(
            pm,
            sx,
            sy,
            (1.5 + 2.5 * twinkle) * s,
            [255, 255, 250, (240.0 * a) as u8],
        );
    }
}

/// Music notes floating up from `at` and swaying, in pastels (♪ and ♫).
fn notes(pm: &mut Pixmap, at: (f32, f32), s: f32, t: f32, fade: f32) {
    const COLOURS: [[u8; 4]; 3] = [
        [0xF4, 0x8F, 0xB1, 255],
        [0x7F, 0xB8, 0xEC, 255],
        [0xF2, 0xB8, 0x4B, 255],
    ];
    let (x, y) = at;
    for k in 0..3 {
        let kf = k as f32;
        let u = (t - 0.35 * kf).max(0.0);
        if u <= 0.0 {
            continue;
        }
        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
        let nx = x + side * (14.0 + 10.0 * kf) * s + 7.0 * s * (3.0 * u + kf).sin();
        let ny = y - 8.0 * s - 34.0 * s * u;
        let a = fade * (1.0 - (u / 1.8).min(1.0)).max(0.0) * (u / 0.2).min(1.0);
        let c = with_alpha(COLOURS[k % 3], a);
        let r = 4.2 * s;
        let tilt = Transform::from_rotate_at(-20.0, nx, ny);
        oval(pm, nx, ny, r, 0.75 * r, c, tilt);
        let stem_x = nx + 0.9 * r;
        let mut stem = PathBuilder::new();
        stem.move_to(stem_x, ny - 0.2 * r);
        stem.line_to(stem_x, ny - 3.2 * r);
        if k == 1 {
            // ♫: two heads joined by a beam.
            let (x2, y2) = (nx + 2.6 * r, ny - 0.5 * r);
            oval(
                pm,
                x2,
                y2,
                r,
                0.75 * r,
                c,
                Transform::from_rotate_at(-20.0, x2, y2),
            );
            stem.move_to(x2 + 0.9 * r, y2 - 0.2 * r);
            stem.line_to(x2 + 0.9 * r, y2 - 3.2 * r);
            stem_path_beam(
                pm,
                (stem_x, ny - 3.2 * r),
                (x2 + 0.9 * r, y2 - 3.2 * r),
                c,
                1.4 * s,
            );
        } else {
            // ♪: a flag.
            stem.move_to(stem_x, ny - 3.2 * r);
            stem.quad_to(
                stem_x + 1.6 * r,
                ny - 2.4 * r,
                stem_x + 1.1 * r,
                ny - 1.2 * r,
            );
        }
        stroke_path(pm, stem, c, 1.3 * s, Transform::identity());
    }
}

/// The beam joining two notes' stems.
fn stem_path_beam(pm: &mut Pixmap, a: (f32, f32), b: (f32, f32), c: [u8; 4], width: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(a.0, a.1);
    pb.line_to(b.0, b.1);
    stroke_path(pm, pb, c, 2.0 * width, Transform::identity());
}

/// One flame of fox-fire at `at`, `size` 1 for the fox-fire scene's: a
/// glow, a white heart and a tip flickering up off it, `t` its own clock.
fn flame(pm: &mut Pixmap, at: (f32, f32), size: f32, c: [u8; 4], t: f32, alpha: f32) {
    let (ox, oy) = at;
    glow(pm, ox, oy, 20.0 * size, with_alpha(c, 0.85 * alpha));
    circle(
        pm,
        ox,
        oy,
        5.6 * size,
        with_alpha([255, 255, 255, 240], alpha),
    );
    let mut tip = PathBuilder::new();
    tip.move_to(ox - 4.5 * size, oy - 1.3 * size);
    tip.quad_to(
        ox + 2.6 * size * (TAU * 2.0 * t).sin(),
        oy - 18.0 * size,
        ox + 4.5 * size,
        oy - 1.3 * size,
    );
    tip.close();
    if let Some(p) = tip.finish() {
        pm.fill_path(
            &p,
            &paint(with_alpha(c, alpha)),
            FillRule::Winding,
            Transform::identity(),
            None,
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

/// The pond's water in the oval `(cx, cy, rx, ry)`, worked out pixel by
/// pixel: a light turquoise at the rim going deep blue toward the middle
/// and the back, fading out into whatever is under it over the outer
/// quarter (no bank), a soft band of sky reflected near the back. With `over` (a waterline, and how much of what is
/// under it still shows), it is drawn over a pet in it instead: from the
/// line down only, the pet's own pixels mixed into the water (deep: hardly
/// any of it shows), so the body goes down into the same water as the pond
/// behind it.
fn water(pm: &mut Pixmap, oval: (f32, f32, f32, f32), t: f32, over: Option<(f32, f32)>) {
    let (cx, cy, rx, ry) = oval;
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let x0 = (cx - rx).floor().max(0.0) as usize;
    let x1 = ((cx + rx).ceil().max(0.0) as usize).min(w);
    let y0 = (cy - ry).floor().max(0.0) as usize;
    let y1 = ((cy + ry).ceil().max(0.0) as usize).min(h);
    let shallow: [u8; 3] = [0x7C, 0xC8, 0xE6];
    let deep: [u8; 3] = [0x14, 0x48, 0x80];
    let data = pm.data_mut();
    for py in y0..y1 {
        let fy = (py as f32 + 0.5 - cy) / ry;
        for px in x0..x1 {
            let fx = (px as f32 + 0.5 - cx) / rx;
            let d = (fx * fx + fy * fy).sqrt();
            if d >= 1.0 {
                continue;
            }
            if let Some((line, _)) = over
                && (py as f32) < line
            {
                continue;
            }
            // Deeper toward the middle and the back.
            let depth =
                (1.0 - d).powf(0.6) * (0.75 + 0.25 * (-fy).max(0.0)) + 0.15 * (-fy).max(0.0);
            let depth = depth.clamp(0.0, 1.0);
            let mut c = [0.0f32; 3];
            for k in 0..3 {
                c[k] = f32::from(shallow[k]) + (f32::from(deep[k]) - f32::from(shallow[k])) * depth;
            }
            // The sky in it, a soft band near the back, moving a little.
            let band = (-((fy + 0.45) / 0.18).powi(2)).exp()
                * (0.75 + 0.25 * (TAU * (0.6 * fx + 0.15 * t)).sin());
            let light = 0.35 * band * (1.0 - d);
            for v in &mut c {
                *v += (255.0 - *v) * light;
            }
            let edge = smooth(1.0, 0.72, d);
            let a = 0.94 * edge;
            let i = 4 * (py * w + px);
            match over {
                None => {
                    let keep = 1.0 - a;
                    for k in 0..3 {
                        data[i + k] = (c[k] * a + f32::from(data[i + k]) * keep).min(255.0) as u8;
                    }
                    data[i + 3] = (255.0 * a + f32::from(data[i + 3]) * keep).min(255.0) as u8;
                }
                Some((_, through)) => {
                    // Only over the pet itself (the pond behind shows
                    // through the rest of its window: a second layer of
                    // water there boxed the pet in): the body turned to
                    // water, a little of it showing through, less so as
                    // the water thins out at the edge.
                    let cover = f32::from(data[i + 3]) / 255.0;
                    let m = a * (1.0 - through);
                    for k in 0..3 {
                        data[i + k] = (f32::from(data[i + k]) * (1.0 - m) + c[k] * cover * m)
                            .min(255.0) as u8;
                    }
                    // Where the water thins out to nothing, so does what
                    // is under it (it poked out past the pond's edge).
                    for v in &mut data[i..i + 4] {
                        *v = (f32::from(*v) * edge) as u8;
                    }
                }
            }
        }
    }
}

/// Water thrown from `from` to `to` (both on the water, pixels), `p` of
/// the way (0 … 1): a sheet of drops flying up in an arc and coming down
/// on the other one.
fn splash_arc(pm: &mut Pixmap, from: (f32, f32), to: (f32, f32), p: f32, s: f32) {
    if !(0.0..1.0).contains(&p) {
        return;
    }
    let high = 55.0 * s + 0.15 * (to.0 - from.0).abs();
    let fade = smooth(1.0, 0.8, p) * smooth(0.0, 0.08, p);
    let at = |u: f32, lift: f32| {
        let x = from.0 + (to.0 - from.0) * u;
        // Landing on the other one's body, not its feet.
        let y = from.1 + (to.1 - 40.0 * s - from.1) * u - 4.0 * high * lift * u * (1.0 - u);
        (x, y)
    };
    // The water itself: a stream with a tail, thick at its head.
    let tail = 0.3;
    let n = 12;
    for k in 0..n {
        let (u0, u1) = (
            (p - tail * (k + 1) as f32 / n as f32).max(0.0),
            (p - tail * k as f32 / n as f32).max(0.0),
        );
        if u1 <= u0 {
            continue;
        }
        let (a, b) = (at(u0, 1.0), at(u1, 1.0));
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        let thin = 1.0 - k as f32 / n as f32;
        stroke_path(
            pm,
            pb,
            with_alpha([0xC8, 0xEC, 0xFF, 150], fade * thin),
            (2.0 + 5.0 * thin) * s,
            Transform::identity(),
        );
    }
    // Drops flying off it, round the head and trailing behind.
    for k in 0..22 {
        let kf = k as f32;
        let u = (p - 0.32 * ((kf * 0.618).fract())).clamp(0.0, 1.0);
        let lift = 1.0 + 0.22 * (kf * 1.7).sin();
        let (x, y) = at(u, lift);
        let (x, y) = (
            x + 5.0 * s * (kf * 2.3).sin(),
            y + 4.0 * s * (kf * 3.1).cos(),
        );
        let r = (3.4 - 0.1 * kf).max(1.2) * s;
        circle(pm, x, y, r, with_alpha([0xD8, 0xF2, 0xFF, 235], fade));
        circle(
            pm,
            x - 0.35 * r,
            y - 0.35 * r,
            0.4 * r,
            with_alpha([255, 255, 255, 255], fade),
        );
    }
    // Where it lands: a burst of drops.
    if p > 0.75 {
        let q = (p - 0.75) / 0.25;
        for k in 0..6 {
            let a = PI * (0.15 + 0.7 * k as f32 / 5.0);
            let (dx, dy) = (
                -a.cos() * 26.0 * s * q,
                -a.sin() * 20.0 * s * q + 30.0 * s * q * q,
            );
            circle(
                pm,
                to.0 + dx,
                to.1 - 40.0 * s + dy,
                1.8 * s,
                with_alpha([0xD8, 0xF2, 0xFF, 230], 1.0 - q),
            );
        }
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
            ripple(rings, (x, cy + 0.25 * ry), (0.2 * rx, 0.7 * ry), p, s);
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

fn oval_path(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<Path> {
    let r = Rect::from_xywh(cx - rx, cy - ry, 2.0 * rx, 2.0 * ry)?;
    let mut pb = PathBuilder::new();
    pb.push_oval(r);
    pb.finish()
}

/// Below the pond's near edge nothing of a pet in it shows: legs there
/// would poke out under the water. Wiped from the waterline `line` down,
/// only under the near half of `oval` (beside the pond's ends, and behind
/// it, the body is out of the water and stays), and less toward the ends,
/// gone at them, as the pond itself fades out there. It used to wipe the
/// whole strip between the pond's ends outside the water: a pet stepping
/// out past an end had a square bitten out of it (Roger's sixteenth
/// pass, item 3: 「方塊殘影」).
pub fn clear_under_rim(pm: &mut Pixmap, oval: (f32, f32, f32, f32), line: f32) {
    use chonkgene_rig::math::smoothstep;
    let (cx, cy, rx, ry) = oval;
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let x0 = (cx - rx).floor().max(0.0) as usize;
    let x1 = ((cx + rx).ceil().max(0.0) as usize).min(w);
    let y0 = line.max(cy).ceil().max(0.0) as usize;
    let data = pm.data_mut();
    for px in x0..x1 {
        let u = ((px as f32 + 0.5 - cx) / rx).abs();
        if u >= 1.0 {
            continue;
        }
        let rim = cy + ry * (1.0 - u * u).sqrt();
        let wipe = 1.0 - smoothstep(0.7, 1.0, u);
        for py in y0..h {
            // A pixel's worth of soft edge along the rim itself.
            let under = (py as f32 + 0.5 - rim).clamp(0.0, 1.0);
            let keep = 1.0 - wipe * under;
            if keep >= 1.0 {
                continue;
            }
            let i = (py * w + px) * 4;
            for c in &mut data[i..i + 4] {
                *c = (f32::from(*c) * keep).round() as u8;
            }
        }
    }
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
    let (cx, cy, rx, ry) = pond_oval(pond, s);
    clear_under_rim(pm, (cx, cy, rx, ry), line);
    // The water over the body from the waterline down: deep, a little of
    // the body showing through.
    water(pm, (cx, cy, rx, ry), t, Some((line, 0.22)));
    // A bright line along the waterline and rings spreading from the body.
    let id = Transform::identity();
    let (Some(oval), Some(mut inside)) = (
        oval_path(cx, cy, rx, ry),
        Mask::new(pm.width(), pm.height()),
    ) else {
        return;
    };
    inside.fill_path(&oval, FillRule::Winding, true, id);
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
    let span = 0.12 * rx;
    let mut top = PathBuilder::new();
    let n = 12;
    for k in 0..=n {
        let u = k as f32 / n as f32;
        let wx = x - span + 2.0 * span * u;
        let wy = line + 1.0 * s + 1.1 * s * (TAU * (wx / (40.0 * s) + 0.6 * t)).sin();
        if k == 0 {
            top.move_to(wx, wy);
        } else {
            top.line_to(wx, wy);
        }
    }
    stroke(pm, top, [0xD8, 0xF0, 0xFA, 220], 1.6 * s);
    for k in 0..2 {
        let p = (t * 0.5 + k as f32 * 0.5).fract();
        let (ex, ey) = ((0.1 + 0.12 * p) * rx, (2.0 + 4.0 * p) * s);
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
        // Larger and brighter than at first (Roger's fourteenth pass).
        flame(pm, (ox, oy), 1.3 * s, *c, t + kf / TAU, 1.0);
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

    /// Roger's sixteenth pass, item 3: a pet stepping out past the pond's
    /// end had a square bitten out of it. Only what hangs under the near
    /// rim goes, and less toward the ends, with no hard edge anywhere.
    #[test]
    fn only_what_hangs_under_the_near_rim_is_wiped_and_softly() {
        let mut pm = Pixmap::new(300, 120).unwrap();
        pm.fill(Color::from_rgba8(200, 120, 60, 255));
        let oval = (150.0, 60.0, 100.0, 20.0);
        clear_under_rim(&mut pm, oval, 40.0);
        let a = |x: u32, y: u32| pm.data()[((y * 300 + x) * 4 + 3) as usize];
        // Under the rim in the middle: gone.
        assert_eq!(a(150, 100), 0);
        // In the water (over the rim), behind it, and beside the ends:
        // kept.
        assert_eq!(a(150, 70), 255);
        assert_eq!(a(150, 45), 255);
        assert_eq!(a(40, 100), 255);
        assert_eq!(a(260, 100), 255);
        // Beside an end, below the waterline, outside the water: kept (the
        // strip there was wiped).
        assert_eq!(a(55, 50), 255);
        // Toward the ends it fades out: no column jumps far from the next.
        for y in [80, 100, 119] {
            for x in 40..260 {
                let jump = i32::from(a(x + 1, y)) - i32::from(a(x, y));
                assert!(jump.abs() < 40, "a hard edge at ({x}, {y}): {jump}");
            }
        }
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

    /// The spell's bubble holds all of a tall pet (Roger: ears and heads
    /// stuck out), is never smaller than before, and stays in the canvas.
    #[test]
    fn the_bubble_fits_round_the_whole_pet() {
        let canvas = (261.0, 191.0);
        // A small chick: the old size.
        let (_, size) = bubble_fit((110.0, 120.0, 150.0, 180.0), canvas, 1.0);
        assert_eq!(size, (74.0, 70.0));
        // A fox with its tails up, 200 × 150: all of its box's middle
        // edges inside, with room.
        let b = (30.0, 30.0, 230.0, 180.0);
        let ((cx, cy), (rx, ry)) = bubble_fit(b, canvas, 1.0);
        assert!(cy - ry <= b.1 && cy + ry >= b.3, "{cy} ± {ry}");
        assert!(cx - rx <= b.0 && cx + rx >= b.2, "{cx} ± {rx}");
        // Never past the canvas.
        assert!(cy - ry >= 0.0 && cy + ry <= canvas.1);
        assert!(cx - rx >= 0.0 && cx + rx <= canvas.0);
        // Opaque bounds found from a picture.
        let mut pm = Pixmap::new(40, 30).unwrap();
        pm.fill_rect(
            Rect::from_xywh(10.0, 5.0, 12.0, 20.0).unwrap(),
            &paint([255, 0, 0, 255]),
            Transform::identity(),
            None,
        );
        assert_eq!(opaque_bounds(&pm), Some((10.0, 5.0, 22.0, 25.0)));
    }

    /// Lit by the sunbeam, only the pet's own pixels change, warmer on
    /// the side toward the light (the right) than the other.
    #[test]
    fn the_sun_lights_only_the_pet_warmer_toward_it() {
        let mut pm = Pixmap::new(120, 80).unwrap();
        let grey = [150, 150, 170, 255];
        pm.fill_rect(
            Rect::from_xywh(20.0, 20.0, 80.0, 50.0).unwrap(),
            &paint(grey),
            Transform::identity(),
            None,
        );
        let before = pm.clone();
        sunlit(&mut pm, 1.0);
        let px = |pm: &Pixmap, x: u32, y: u32| pm.pixel(x, y).unwrap();
        // Outside the pet: still nothing.
        assert_eq!(px(&pm, 5, 5).alpha(), 0);
        assert_eq!(px(&pm, 110, 75).alpha(), 0);
        // Inside: warmer (more red than blue) toward the right.
        let warmth = |x| {
            let p = px(&pm, x, 50);
            i32::from(p.red()) - i32::from(p.blue())
        };
        assert!(warmth(90) > warmth(30), "{} vs {}", warmth(90), warmth(30));
        assert!(warmth(90) > i32::from(grey[0]) - i32::from(grey[2]));
        assert_ne!(pm.data(), before.data());
        // Not lit at all: unchanged.
        let mut off = before.clone();
        sunlit(&mut off, 0.0);
        assert_eq!(off.data(), before.data());
    }

    /// Water thrown right across the pond, from one end to the other and
    /// at every moment of its flight, stays inside the pond's window.
    #[test]
    fn water_thrown_across_the_pond_stays_in_its_window() {
        for scale in [1.0, 2.0] {
            let ((w, h), foot) = canvas(Thing::Pond, scale);
            let reach = 0.45 * Thing::Pond.size().0 * scale;
            for p in [0.1, 0.3, 0.5, 0.7, 0.9] {
                let mut pm = Pixmap::new(w, h).unwrap();
                let look = Look {
                    t: 1.0,
                    sprays: [
                        Some((-reach, reach, p)),
                        Some((reach, -reach, p)),
                        None,
                        None,
                    ],
                    ..Look::default()
                };
                draw(&mut pm, Thing::Pond, foot, scale, look);
                assert!(edges_clear(&pm), "at {scale}, {p} of the way");
            }
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
