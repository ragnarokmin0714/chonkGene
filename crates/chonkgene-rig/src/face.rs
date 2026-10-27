//! The painted face: eyes, brows, blush and whiskers on the head; the mouth
//! on the muzzle. Everything is drawn in "face space": the direction from the
//! part's centre, squashed onto a unit sphere, seen from the front.

use crate::math::{V3, smoothstep, v3};
use crate::model::{Look, Spot};
use crate::scene::Scene;
use crate::sdf::Shape;

type P = (f32, f32);

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}
fn len(a: P) -> f32 {
    (a.0 * a.0 + a.1 * a.1).sqrt()
}
fn ell(uv: P, c: P, r: P) -> f32 {
    len(((uv.0 - c.0) / r.0, (uv.1 - c.1) / r.1)) - 1.0
}
/// Distance to a ring of radius `r`, half-width `w`.
fn arc(uv: P, c: P, r: f32, w: f32) -> f32 {
    (len(sub(uv, c)) - r).abs() - w
}
fn segment(uv: P, a: P, b: P, w: f32) -> f32 {
    let (pa, ba) = (sub(uv, a), sub(b, a));
    let h = ((pa.0 * ba.0 + pa.1 * ba.1) / (ba.0 * ba.0 + ba.1 * ba.1)).clamp(0.0, 1.0);
    len((pa.0 - ba.0 * h, pa.1 - ba.1 * h)) - w
}
/// Coverage of a shape at signed distance `d`, smoothed over `aa`.
fn ink(d: f32, aa: f32) -> f32 {
    1.0 - smoothstep(-aa, aa, d)
}

/// Face-space position of `q` on an ellipsoid part, if it faces forward.
fn face_uv(shape: Shape, q: V3, min_z: f32) -> Option<(P, f32)> {
    let Shape::Ellipsoid(r) = shape else {
        return None;
    };
    let n = q.div_elem(r).normalize();
    (n.z >= min_z).then_some(((n.x, n.y), r.y))
}

pub fn eyes(scene: &Scene, shape: Shape, q: V3, base: V3, px: f32) -> V3 {
    let Some((uv, ry)) = face_uv(shape, q, 0.15) else {
        return base;
    };
    let face = &scene.model().face;
    let aa = px / ry;
    let ink_c = scene.color(face.ink);
    let white = v3(1.0, 1.0, 1.0);
    let mut c = base;
    let spot = |c: V3, s: &Spot, side: f32, alpha: f32, soft: f32| {
        let d = ell(uv, (s.at.0 * side, s.at.1), s.r);
        c.lerp(scene.color(s.slot), alpha * ink(d, aa * soft))
    };
    for side in [-1.0, 1.0] {
        let ec = (face.eye_at.0 * side, face.eye_at.1);
        let (rx, ry) = face.eye_r;
        if let Some(b) = &face.brows {
            c = spot(c, b, side, 1.0, 1.0);
        }
        c = spot(c, &face.blush, side, 0.55, 8.0);
        if face.whiskers {
            for i in 0..3 {
                let y = -0.31 + 0.06 * i as f32;
                let a = (0.40 * side, y);
                let b = (0.66 * side, y + 0.05 * (i as f32 - 1.0));
                c = c.lerp(ink_c, 0.45 * ink(segment(uv, a, b, 0.007), aa));
            }
        }
        let w = 0.17 * ry;
        // Plain eyes are all pupil; owls and cats can have a coloured iris.
        let (iris_c, pupil) = face
            .iris
            .map_or((ink_c, 1.0), |(slot, k)| (scene.color(slot), k));
        match scene.look() {
            Look::Open => {
                c = c.lerp(iris_c, ink(ell(uv, ec, (rx, ry)), aa));
                if pupil < 1.0 {
                    c = c.lerp(ink_c, ink(ell(uv, ec, (rx * pupil, ry * pupil)), aa));
                }
                let hl = (ec.0 - 0.30 * rx, ec.1 + 0.36 * ry);
                c = c.lerp(white, ink(len(sub(uv, hl)) - 0.38 * rx, aa));
                let hl2 = (ec.0 + 0.35 * rx, ec.1 - 0.36 * ry);
                c = c.lerp(white, ink(len(sub(uv, hl2)) - 0.17 * rx, aa));
            }
            Look::Half | Look::FedUp => {
                // The lid comes down to the middle of the eye, drawn as a line.
                let lid = ec.1 + 0.05 * ry;
                let d = ell(uv, ec, (rx, ry)).max(uv.1 - lid);
                c = c.lerp(iris_c, ink(d, aa));
                if pupil < 1.0 {
                    let d = ell(uv, ec, (rx * pupil, ry * pupil)).max(uv.1 - lid);
                    c = c.lerp(ink_c, ink(d, aa));
                }
                let hl = (ec.0 - 0.25 * rx, ec.1 - 0.35 * ry);
                c = c.lerp(white, ink(len(sub(uv, hl)) - 0.16 * rx, aa));
                let l = ((uv.1 - lid).abs() - 0.13 * ry).max((uv.0 - ec.0).abs() - 1.05 * rx);
                c = c.lerp(ink_c, ink(l, aa));
            }
            Look::Shut | Look::Asleep => {
                // ‿ : the lower half of a ring.
                let d =
                    arc(uv, (ec.0, ec.1 + 0.5 * ry), 0.75 * rx, w).max(uv.1 - (ec.1 + 0.2 * ry));
                c = c.lerp(ink_c, ink(d, aa));
            }
            Look::Happy | Look::Laugh => {
                // ^ : the upper half of a ring.
                let d = arc(uv, (ec.0, ec.1 - 0.4 * ry), 0.75 * rx, w)
                    .max(-(uv.1 - (ec.1 - 0.25 * ry)));
                c = c.lerp(ink_c, ink(d, aa));
            }
            Look::Pout | Look::Yawn => {
                // > < : squeezed shut.
                let tip = (ec.0 - 0.45 * rx * side, ec.1);
                let a = (ec.0 + 0.55 * rx * side, ec.1 + 0.5 * ry);
                let b = (ec.0 + 0.55 * rx * side, ec.1 - 0.5 * ry);
                let d = segment(uv, a, tip, w).min(segment(uv, b, tip, w));
                c = c.lerp(ink_c, ink(d, aa));
            }
        }
    }
    c
}

pub fn mouth(scene: &Scene, shape: Shape, q: V3, base: V3, px: f32) -> V3 {
    let Some((uv, ry)) = face_uv(shape, q, 0.3) else {
        return base;
    };
    let ink_c = scene.color(scene.model().face.ink);
    let aa = px / ry;
    let mut c = base;
    match scene.look() {
        Look::Laugh => {
            let m = (len(((uv.0) / 1.0, (uv.1 + 0.18) / 0.85)) - 0.30).max(uv.1 + 0.18);
            c = c.lerp(v3(0.55, 0.18, 0.22), ink(m, aa));
            let t = (len(sub(uv, (0.0, -0.48))) - 0.16).max(m);
            c = c.lerp(v3(1.0, 0.52, 0.56), ink(t, aa));
        }
        Look::Yawn => {
            // A tall O with the tongue at the bottom.
            let m = len((uv.0 / 0.7, uv.1 + 0.3)) - 0.3;
            c = c.lerp(v3(0.50, 0.16, 0.20), ink(m, aa));
            let t = (len(sub(uv, (0.0, -0.58))) - 0.15).max(m);
            c = c.lerp(v3(1.0, 0.52, 0.56), ink(t, aa));
        }
        Look::FedUp => {
            let d = ((uv.1 + 0.22).abs() - 0.022).max(uv.0.abs() - 0.13);
            c = c.lerp(ink_c, ink(d, aa));
        }
        Look::Pout => {
            // A small ∩.
            let d = arc(uv, (0.0, -0.32), 0.10, 0.022).max(-(uv.1 + 0.30));
            c = c.lerp(ink_c, ink(d, aa));
        }
        _ => {
            // ω
            for s in [-1.0, 1.0] {
                let d = arc(uv, (0.15 * s, -0.10), 0.15, 0.022).max(uv.1 + 0.10);
                c = c.lerp(ink_c, ink(d, aa));
            }
        }
    }
    // The line from the nose down to the mouth.
    let ridge = (uv.0.abs() - 0.022).max((uv.1 + 0.10).max(-uv.1 - 0.02));
    c.lerp(ink_c, ink(ridge, aa))
}
