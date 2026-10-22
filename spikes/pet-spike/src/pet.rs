//! A stand-in pet: a chubby Shiba-like figure with breathing, a belly spring
//! and a walking bob. Real art comes from the rig pipeline (PLAN §3.3); this
//! only has to cost roughly the same to draw.

use tiny_skia::{FillRule, LineCap, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};

/// Pixmap edge length in physical pixels.
pub const SIZE: u32 = 256;

type Rgba = [u8; 4];
const MAIN: Rgba = [232, 149, 90, 255];
const LIGHT: Rgba = [255, 243, 226, 255];
const INNER: Rgba = [255, 210, 181, 255];
const INK: Rgba = [59, 42, 34, 255];
const BLUSH: Rgba = [247, 169, 160, 220];
const WHITE: Rgba = [255, 255, 255, 255];
const SHADOW: Rgba = [59, 42, 34, 24];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Walk,
    Held,
    Fall,
}

pub struct Pet {
    pub mode: Mode,
    /// 1.0 faces right, -1.0 faces left.
    pub facing: f32,
    /// Sideways swing while held, in radians.
    pub sway: f32,
    t: f32,
    squash: f32,
    squash_v: f32,
}

impl Pet {
    pub fn new(seed: u32) -> Self {
        Self {
            mode: Mode::Idle,
            facing: 1.0,
            sway: 0.0,
            t: seed as f32 * 0.37,
            squash: 0.0,
            squash_v: 0.0,
        }
    }

    pub fn poke(&mut self) {
        self.squash_v += 9.0;
    }

    fn settling(&self) -> bool {
        self.squash.abs() > 0.003 || self.squash_v.abs() > 0.03
    }

    /// Frames per second this pet wants right now (PLAN §5).
    pub fn fps(&self) -> u32 {
        match self.mode {
            Mode::Held | Mode::Fall => 60,
            _ if self.settling() => 60,
            Mode::Walk => 30,
            Mode::Idle => 15,
        }
    }

    pub fn step(&mut self, dt: f32) {
        let dt = dt.min(0.1);
        self.t += dt;
        // Semi-implicit Euler spring-damper; stays stable down to 15 fps with these constants.
        let accel = -180.0 * self.squash - 9.0 * self.squash_v;
        self.squash_v += accel * dt;
        self.squash += self.squash_v * dt;
        if !self.settling() {
            self.squash = 0.0;
            self.squash_v = 0.0;
        }
        self.sway *= (1.0 - 6.0 * dt).max(0.0);
    }

    pub fn draw(&self, pm: &mut Pixmap) {
        let size = SIZE as f32;
        let scale = size / 200.0 * 0.96;
        let bob = if self.mode == Mode::Walk {
            -(self.t * 9.0).sin().abs() * 6.0
        } else {
            0.0
        };
        // Design space is the mockup's 200 × 180 box with the feet at y = 176.
        let base = Transform::from_translate(size / 2.0, size - 4.0 + bob)
            .pre_scale(scale * self.facing, scale)
            .pre_rotate(self.sway.to_degrees())
            .pre_translate(-100.0, -176.0);

        let breath = 1.0 + 0.015 * (self.t * std::f32::consts::TAU / 3.2).sin();
        let s = self.squash.clamp(-0.4, 0.4);
        let body = base
            .pre_translate(100.0, 174.0)
            .pre_scale(1.0 + 0.45 * s, (1.0 - 0.45 * s) * breath)
            .pre_translate(-100.0, -174.0);
        let belly = body
            .pre_translate(100.0, 138.0)
            .pre_scale(1.0 + 0.35 * s, 1.0 - 0.35 * s)
            .pre_translate(-100.0, -138.0);
        let head = base.pre_translate(0.0, 10.0 * s);

        fill(pm, oval(100.0, 175.0, 70.0, 5.0), SHADOW, base);
        stroke(
            pm,
            curve(&[(148.0, 132.0), (178.0, 128.0), (184.0, 94.0), (164.0, 86.0)]),
            MAIN,
            14.0,
            body,
        );
        fill(pm, oval(100.0, 128.0, 60.0, 46.0), MAIN, body);
        fill(pm, oval(100.0, 138.0, 38.0, 31.0), LIGHT, belly);
        for (cx, cy, rx, ry) in [
            (62.0, 168.0, 17.0, 8.0),
            (138.0, 168.0, 17.0, 8.0),
            (85.0, 165.0, 11.0, 9.0),
            (115.0, 165.0, 11.0, 9.0),
        ] {
            fill(pm, oval(cx, cy, rx, ry), LIGHT, body);
        }

        fill(
            pm,
            poly(&[(56.0, 52.0), (63.0, 15.0), (72.0, 13.0), (95.0, 34.0)]),
            MAIN,
            head,
        );
        fill(
            pm,
            poly(&[(144.0, 52.0), (137.0, 15.0), (128.0, 13.0), (105.0, 34.0)]),
            MAIN,
            head,
        );
        fill(
            pm,
            poly(&[(64.0, 42.0), (68.0, 22.0), (84.0, 35.0)]),
            INNER,
            head,
        );
        fill(
            pm,
            poly(&[(136.0, 42.0), (132.0, 22.0), (116.0, 35.0)]),
            INNER,
            head,
        );
        fill(pm, oval(100.0, 66.0, 52.0, 42.0), MAIN, head);
        for (cx, cy, rx, ry) in [
            (72.0, 84.0, 20.0, 15.0),
            (128.0, 84.0, 20.0, 15.0),
            (100.0, 90.0, 30.0, 17.0),
            (80.0, 54.0, 6.0, 4.0),
            (120.0, 54.0, 6.0, 4.0),
        ] {
            fill(pm, oval(cx, cy, rx, ry), LIGHT, head);
        }

        // Blink for ~0.12 s every 4.2 s.
        let blink = if (self.t % 4.2) > 4.08 { 0.15 } else { 1.0 };
        for cx in [80.0, 120.0] {
            fill(pm, oval(cx, 68.0, 5.0, 6.0 * blink), INK, head);
            if blink > 0.5 {
                fill(pm, oval(cx + 2.0, 65.5, 1.9, 1.9), WHITE, head);
            }
        }
        fill(pm, oval(100.0, 80.0, 6.0, 4.2), INK, head);
        stroke(pm, mouth(), INK, 2.4, head);
        fill(pm, oval(66.0, 84.0, 8.0, 4.5), BLUSH, head);
        fill(pm, oval(134.0, 84.0, 8.0, 4.5), BLUSH, head);
    }
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

fn fill(pm: &mut Pixmap, path: Option<Path>, c: Rgba, ts: Transform) {
    if let Some(path) = path {
        pm.fill_path(&path, &paint(c), FillRule::Winding, ts, None);
    }
}

fn stroke(pm: &mut Pixmap, path: Option<Path>, c: Rgba, width: f32, ts: Transform) {
    if let Some(path) = path {
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            ..Stroke::default()
        };
        pm.stroke_path(&path, &paint(c), &stroke, ts, None);
    }
}

fn oval(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<Path> {
    PathBuilder::from_oval(Rect::from_xywh(cx - rx, cy - ry, 2.0 * rx, 2.0 * ry)?)
}

fn poly(points: &[(f32, f32)]) -> Option<Path> {
    let (first, rest) = points.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.0, first.1);
    for p in rest {
        pb.line_to(p.0, p.1);
    }
    pb.close();
    pb.finish()
}

fn curve(p: &[(f32, f32); 4]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(p[0].0, p[0].1);
    pb.cubic_to(p[1].0, p[1].1, p[2].0, p[2].1, p[3].0, p[3].1);
    pb.finish()
}

fn mouth() -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(92.0, 86.0);
    pb.quad_to(96.0, 91.0, 100.0, 86.0);
    pb.quad_to(104.0, 91.0, 108.0, 86.0);
    pb.finish()
}
