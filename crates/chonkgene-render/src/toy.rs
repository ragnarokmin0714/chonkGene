//! The foxtail grass (狗尾草) held on the cursor to tease the pets with
//! (PLAN §8, Roger's sixth pass): a thin stem from the cursor, bowing down
//! to a fuzzy seed head that swings as the cursor moves and wobbles to a
//! rest.

use std::f32::consts::TAU;

use tiny_skia::{FillRule, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// The picture, pixels at 100%, and where in it the cursor holds the stem.
pub const SIZE: (f32, f32) = (150.0, 140.0);
pub const GRIP: (f32, f32) = (40.0, 14.0);
/// Grip to the tip of the head, and the head's length, pixels at 100%.
const LENGTH: f32 = 92.0;
const HEAD: f32 = 36.0;
/// Where the stem points at rest (radians below the horizontal, to the
/// right), and how far either way it may swing.
const REST: f32 = 0.85;
const SWING: (f32, f32) = (0.0, 1.9);
/// The stem's springiness (hertz) and damping: it wobbles a few times.
const HZ: f32 = 1.6;
const DAMPING: f32 = 0.18;

const STEM: [u8; 4] = [0x7F, 0xA6, 0x45, 255];
const HEAD_FILL: [u8; 4] = [0xB9, 0xC9, 0x5A, 255];
const BRISTLE: [u8; 4] = [0xE3, 0xEB, 0xA6, 220];

fn paint(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

/// The stalk's swing.
#[derive(Clone, Debug)]
pub struct Stalk {
    /// The stem's angle below the horizontal, radians, and its speed.
    angle: f32,
    spin: f32,
    /// The grip's last two places (screen pixels), for its acceleration.
    grip: Option<[(f32, f32); 2]>,
}

impl Default for Stalk {
    fn default() -> Self {
        Self {
            angle: REST,
            spin: 0.0,
            grip: None,
        }
    }
}

impl Stalk {
    /// The grip moved to `at` (screen pixels) over `dt` seconds: the head
    /// lags behind the hand and swings back. True while it still moves.
    pub fn step(&mut self, at: (f32, f32), dt: f32, scale: f32) -> bool {
        let dt = dt.clamp(1e-3, 0.05);
        let (ax, ay) = match self.grip {
            Some([last, before]) => (
                (at.0 - 2.0 * last.0 + before.0) / (dt * dt),
                (at.1 - 2.0 * last.1 + before.1) / (dt * dt),
            ),
            None => (0.0, 0.0),
        };
        self.grip = Some([at, self.grip.map_or(at, |g| g[0])]);
        // A pendulum on a springy stem: the hand's acceleration pushes the
        // head the other way, along the way it can swing.
        let length = (LENGTH - 0.5 * HEAD) * scale;
        let push = (ax * self.angle.sin() - ay * self.angle.cos()) / length;
        let w = TAU * HZ;
        let acc = -w * w * (self.angle - REST) - 2.0 * DAMPING * w * self.spin
            + push.clamp(-400.0, 400.0);
        self.spin += acc * dt;
        self.angle += self.spin * dt;
        if !(SWING.0..=SWING.1).contains(&self.angle) {
            self.angle = self.angle.clamp(SWING.0, SWING.1);
            self.spin *= -0.3;
        }
        self.spin.abs() > 0.02 || (self.angle - REST).abs() > 0.002
    }

    /// The middle of the seed head, from the grip, pixels at `scale`: what
    /// the pets go for.
    pub fn head(&self, scale: f32) -> (f32, f32) {
        let r = (LENGTH - 0.5 * HEAD) * scale;
        (r * self.angle.cos(), r * self.angle.sin())
    }

    /// Draw the stalk into a picture of [`SIZE`] at `scale`, held at
    /// [`GRIP`].
    pub fn draw(&self, pm: &mut Pixmap, scale: f32) {
        pm.fill(tiny_skia::Color::TRANSPARENT);
        let (gx, gy) = (GRIP.0 * scale, GRIP.1 * scale);
        let (c, s) = (self.angle.cos(), self.angle.sin());
        let base = (LENGTH - HEAD) * scale;
        // The stem bows up a little on its way to the head.
        let bow = self.angle - 0.35;
        let mut stem = PathBuilder::new();
        stem.move_to(gx, gy);
        stem.quad_to(
            gx + 0.5 * base * bow.cos(),
            gy + 0.5 * base * bow.sin(),
            gx + base * c,
            gy + base * s,
        );
        if let Some(path) = stem.finish() {
            let stroke = Stroke {
                width: 2.2 * scale,
                line_cap: LineCap::Round,
                ..Stroke::default()
            };
            pm.stroke_path(&path, &paint(STEM), &stroke, Transform::identity(), None);
        }
        // The head: a fuzzy plume along the stem's direction, fattest a
        // little past its middle, bristles sticking out all along it.
        let along = Transform::from_row(c, s, -s, c, gx + base * c, gy + base * s);
        let len = HEAD * scale;
        if let Some(body) = PathBuilder::from_oval(
            tiny_skia::Rect::from_xywh(0.0, -5.0 * scale, len, 10.0 * scale).unwrap_or_else(|| {
                tiny_skia::Rect::from_xywh(0.0, -1.0, 2.0, 2.0).expect("a unit rect")
            }),
        ) {
            pm.fill_path(&body, &paint(HEAD_FILL), FillRule::Winding, along, None);
        }
        let bristle = Stroke {
            width: 0.9 * scale,
            line_cap: LineCap::Round,
            ..Stroke::default()
        };
        let mut hairs = PathBuilder::new();
        let n = 28;
        for k in 0..n {
            let t = k as f32 / (n - 1) as f32;
            let x = t * len;
            // Thickest past the middle, thinning to the tip.
            let fat = (std::f32::consts::PI * t.powf(0.8)).sin() * 8.0 * scale + 1.5 * scale;
            for side in [-1.0, 1.0] {
                let lean = 0.45 + 0.25 * ((k as f32 * 1.7).sin());
                hairs.move_to(x, side * 2.0 * scale);
                hairs.line_to(x + lean * fat, side * fat);
            }
        }
        // A tuft at the tip.
        hairs.move_to(len, 0.0);
        hairs.line_to(len + 4.0 * scale, -scale);
        hairs.move_to(len, 0.0);
        hairs.line_to(len + 3.5 * scale, 1.5 * scale);
        if let Some(path) = hairs.finish() {
            pm.stroke_path(&path, &paint(BRISTLE), &bristle, along, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_lags_behind_the_hand_and_swings_back_to_rest() {
        let mut s = Stalk::default();
        let rest = s.head(1.0);
        // A quick flick of the hand to the right, then holding still.
        let mut x = 0.0;
        for _ in 0..6 {
            x += 25.0;
            s.step((x, 0.0), 1.0 / 60.0, 1.0);
        }
        let lagged = s.head(1.0);
        assert!(lagged.0 < rest.0 - 5.0, "{lagged:?} against {rest:?}");
        let mut still = true;
        for _ in 0..600 {
            still = !s.step((x, 0.0), 1.0 / 60.0, 1.0);
        }
        assert!(still, "still swinging after 10 s");
        let back = s.head(1.0);
        assert!((back.0 - rest.0).abs() < 0.5 && (back.1 - rest.1).abs() < 0.5);
    }

    #[test]
    fn the_stalk_stays_inside_its_picture() {
        for scale in [1.0, 1.5, 2.0] {
            let w = (SIZE.0 * scale).ceil() as u32;
            let h = (SIZE.1 * scale).ceil() as u32;
            let mut pm = Pixmap::new(w, h).unwrap();
            for angle in [SWING.0, REST, SWING.1] {
                let s = Stalk {
                    angle,
                    ..Stalk::default()
                };
                s.draw(&mut pm, scale);
                let d = pm.data();
                for y in 0..h {
                    for x in [0, w - 1] {
                        assert_eq!(d[((y * w + x) * 4 + 3) as usize], 0, "touches a side");
                    }
                }
                for x in 0..w {
                    for y in [0, h - 1] {
                        assert_eq!(
                            d[((y * w + x) * 4 + 3) as usize],
                            0,
                            "touches the top or bottom"
                        );
                    }
                }
                let drawn = d.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
                assert!(drawn > 200, "drew only {drawn} pixels");
            }
        }
    }
}
