//! The hand over a pet (Roger, 2026-11-06: something cuter than the
//! system's): a chubby plush hand with short round fingers, a blush on its
//! back and a pink cuff, drawn here rather than shipped as pictures. Open
//! over a pet or a toy, one finger out while pressing or scratching, a fist
//! while carrying.

use tiny_skia::{FillRule, LineCap, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

/// Which hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    /// Over something to touch: open, ready to pat.
    Open,
    /// Pressing or scratching: the index finger out, the rest curled.
    Poke,
    /// Carrying a pet or a toy: a fist.
    Grab,
}

impl Hand {
    pub const ALL: [Hand; 3] = [Hand::Open, Hand::Poke, Hand::Grab];
}

/// The design grid the hand is drawn on, and its size at 100%.
const GRID: f32 = 32.0;
pub const SIZE: f32 = 36.0;

const SKIN: [u8; 4] = [0xFF, 0xE6, 0xD2, 255];
const INK: [u8; 4] = [0x5A, 0x40, 0x38, 255];
const BLUSH: [u8; 4] = [0xFF, 0x9A, 0xAE, 150];
const CUFF: [u8; 4] = [0xF7, 0xA8, 0xBC, 255];
const OUTLINE: f32 = 2.4;

/// A shape of the hand on the design grid.
enum Blob {
    /// Centre and radii.
    Oval(f32, f32, f32, f32),
    /// From, to and radius: a finger.
    Stick(f32, f32, f32, f32, f32),
}

fn blobs(hand: Hand) -> (Vec<Blob>, (f32, f32)) {
    use Blob::{Oval, Stick};
    match hand {
        Hand::Open => (
            vec![
                Oval(16.0, 20.0, 8.6, 7.8),
                Stick(10.0, 15.0, 9.0, 8.0, 2.4),
                Stick(13.8, 14.0, 13.6, 5.6, 2.5),
                Stick(17.8, 14.0, 18.2, 5.8, 2.5),
                Stick(21.4, 15.0, 22.6, 8.6, 2.3),
                Stick(9.0, 21.0, 4.6, 16.4, 2.4),
            ],
            (13.6, 3.2),
        ),
        Hand::Poke => (
            vec![
                Oval(16.6, 20.6, 8.0, 7.4),
                Stick(13.4, 15.0, 13.3, 5.4, 2.6),
                Oval(17.6, 14.6, 2.6, 2.4),
                Oval(20.8, 15.6, 2.5, 2.3),
                Oval(23.0, 18.0, 2.3, 2.2),
                Stick(10.0, 21.0, 7.6, 17.6, 2.4),
            ],
            (13.3, 2.6),
        ),
        Hand::Grab => (
            vec![
                Oval(16.0, 18.6, 9.2, 7.6),
                Oval(9.8, 12.8, 2.7, 2.6),
                Oval(14.0, 11.8, 2.8, 2.7),
                Oval(18.2, 11.8, 2.8, 2.7),
                Oval(22.2, 12.8, 2.7, 2.6),
            ],
            (16.0, 16.0),
        ),
    }
}

fn path(b: &Blob) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    match *b {
        Blob::Oval(x, y, rx, ry) => {
            pb.push_oval(Rect::from_xywh(x - rx, y - ry, 2.0 * rx, 2.0 * ry)?)
        }
        Blob::Stick(x0, y0, x1, y1, _) => {
            pb.move_to(x0, y0);
            pb.line_to(x1, y1);
        }
    }
    pb.finish()
}

fn paint(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

/// Draw one layer of every blob: grown by `grow` (the outline) or not.
fn layer(pm: &mut Pixmap, blobs: &[Blob], c: [u8; 4], grow: f32, ts: Transform) {
    for b in blobs {
        let Some(p) = path(b) else {
            continue;
        };
        match *b {
            Blob::Oval(..) => {
                pm.fill_path(&p, &paint(c), FillRule::Winding, ts, None);
                if grow > 0.0 {
                    let s = Stroke {
                        width: 2.0 * grow,
                        ..Stroke::default()
                    };
                    pm.stroke_path(&p, &paint(c), &s, ts, None);
                }
            }
            Blob::Stick(.., r) => {
                let s = Stroke {
                    width: 2.0 * (r + grow),
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                };
                pm.stroke_path(&p, &paint(c), &s, ts, None);
            }
        }
    }
}

/// The hand at `px` pixels square, as straight-alpha RGBA, and where it
/// points (the hot spot, pixels).
pub fn draw(hand: Hand, px: u32) -> (Vec<u8>, (u16, u16)) {
    let px = px.clamp(16, 256);
    let Some(mut pm) = Pixmap::new(px, px) else {
        return (vec![0; (px * px * 4) as usize], (0, 0));
    };
    let k = px as f32 / GRID;
    let ts = Transform::from_scale(k, k);
    let (shapes, hot) = blobs(hand);
    // The cuff at the wrist, under the hand.
    let cuff = [Blob::Oval(16.0, 28.2, 7.0, 2.8)];
    layer(&mut pm, &cuff, INK, OUTLINE / 2.0 + 0.2, ts);
    layer(&mut pm, &shapes, INK, OUTLINE / 2.0 + 0.2, ts);
    layer(&mut pm, &cuff, CUFF, 0.0, ts);
    layer(&mut pm, &shapes, SKIN, 0.0, ts);
    // A blush on the back of the hand, and creases between the curled
    // fingers of a fist.
    let (bx, by) = match hand {
        Hand::Grab => (16.0, 19.5),
        _ => (16.6, 21.0),
    };
    layer(&mut pm, &[Blob::Oval(bx, by, 3.4, 2.2)], BLUSH, 0.0, ts);
    // Lines between the fingers: open ones apart, a fist's curled.
    let creases: &[(f32, f32, f32, f32)] = match hand {
        Hand::Open => &[
            (11.7, 9.6, 11.9, 14.0),
            (15.8, 8.2, 15.8, 13.6),
            (20.1, 9.2, 19.9, 14.0),
        ],
        Hand::Poke => &[],
        Hand::Grab => &[
            (12.0, 11.2, 12.0, 13.6),
            (16.1, 11.2, 16.1, 13.6),
            (20.2, 11.2, 20.2, 13.6),
        ],
    };
    let crease = Stroke {
        width: 1.1,
        line_cap: LineCap::Round,
        ..Stroke::default()
    };
    for &(x0, y0, x1, y1) in creases {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        if let Some(p) = pb.finish() {
            pm.stroke_path(&p, &paint(INK), &crease, ts, None);
        }
    }
    // Back to straight alpha for the window system.
    let rgba = pm
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    let spot = |v: f32| (v * k).round().clamp(0.0, px as f32 - 1.0) as u16;
    (rgba, (spot(hot.0), spot(hot.1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every hand is drawn, and its hot spot sits on or right by it.
    #[test]
    fn every_hand_is_drawn_round_its_hot_spot() {
        for hand in Hand::ALL {
            for px in [32u32, 36, 72] {
                let (rgba, (hx, hy)) = draw(hand, px);
                let at = |x: u32, y: u32| rgba[((y * px + x) * 4 + 3) as usize];
                let drawn = rgba.chunks(4).filter(|p| p[3] > 200).count();
                assert!(drawn as u32 > px * px / 5, "{hand:?} at {px}: {drawn}");
                let near =
                    (0..=3).any(|d| at(u32::from(hx), (u32::from(hy) + d).min(px - 1)) > 200);
                assert!(near, "{hand:?} at {px}: nothing at ({hx}, {hy})");
            }
        }
    }
}
