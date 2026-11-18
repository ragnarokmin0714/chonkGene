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
/// Distance to a tapered capsule from `a` (radius `r1`) to `b` (radius
/// `r2`): not exact, but fine for painting.
fn tapered(uv: P, a: P, b: P, r1: f32, r2: f32) -> f32 {
    let (pa, ba) = (sub(uv, a), sub(b, a));
    let h = ((pa.0 * ba.0 + pa.1 * ba.1) / (ba.0 * ba.0 + ba.1 * ba.1)).clamp(0.0, 1.0);
    len((pa.0 - ba.0 * h, pa.1 - ba.1 * h)) - (r1 + (r2 - r1) * h)
}

/// Distance to a leaf from `a` to `b`, `w` wide at its middle and coming
/// to a point at both ends: not exact, but fine for painting.
fn leaf(uv: P, a: P, b: P, w: f32) -> f32 {
    let (pa, ba) = (sub(uv, a), sub(b, a));
    let h = ((pa.0 * ba.0 + pa.1 * ba.1) / (ba.0 * ba.0 + ba.1 * ba.1)).clamp(0.0, 1.0);
    let r = w * (std::f32::consts::PI * h).sin().powf(0.7) + 0.003;
    len((pa.0 - ba.0 * h, pa.1 - ba.1 * h)) - r
}

/// The fox's marks at rest (Roger's picture 施法狀態, PLAN §8 twenty-third
/// pass, item 3), the same in paint.wgsl `fox_marks`: on the forehead three
/// leaves fanned up from one point, the middle one tallest; on each cheek
/// two long strokes tapering back toward the ear. Returns the coverage of
/// the marks, and how far out toward the ears it is (0 … 1: the strokes
/// pale toward their ends).
pub(crate) fn fox_marks(uv: P, aa: f32) -> (f32, f32) {
    let mut d = leaf(uv, (0.0, 0.45), (0.0, 0.71), 0.05);
    for side in [-1.0, 1.0] {
        d = d.min(leaf(uv, (0.03 * side, 0.44), (0.15 * side, 0.62), 0.038));
        for (a, b, r) in [
            ((0.5, -0.2), (0.88, -0.1), 0.04),
            ((0.52, -0.31), (0.86, -0.25), 0.034),
        ] {
            d = d.min(tapered(uv, (a.0 * side, a.1), (b.0 * side, b.1), r, 0.005));
        }
    }
    (ink(d, aa), smoothstep(0.55, 0.86, uv.0.abs()))
}

/// The fox's seal in her human form, the same in paint.wgsl `seal_marks`,
/// placed from the eyes (`eye`: their middle x, y and radii) as Roger's
/// sheet has it (the turnaround's front view, measured): low on the
/// forehead between the brows a small crest (two wings opening up from a
/// point a little below the tops of the eyes, a drop between them); a leaf
/// over the inner half of each eye for a brow, slanting up and out; two
/// short stripes on each cheek under the eye, rising outward, thick at the
/// outer end.
pub(crate) fn seal_marks(uv: P, aa: f32, eye: (f32, f32, f32, f32)) -> f32 {
    let (ex, ey, rx, ry) = eye;
    let foot = ey + 0.55 * ry;
    let mut d = tapered(uv, (0.0, foot + 0.04), (0.0, foot + 0.2), 0.026, 0.003);
    for side in [-1.0, 1.0] {
        d = d.min(leaf(
            uv,
            (0.012 * side, foot),
            (0.085 * side, foot + 0.17),
            0.024,
        ));
        let b0 = ((ex - 1.45 * rx) * side, ey + 1.32 * ry);
        let b1 = ((ex + 0.2 * rx) * side, ey + 1.93 * ry);
        d = d.min(leaf(uv, b0, b1, 0.042));
        for i in 0..2 {
            let fi = i as f32;
            let outer = (
                (ex + (0.42 - 0.08 * fi) * rx) * side,
                ey - (1.31 + 0.3 * fi) * ry,
            );
            let inner = (
                (ex - (0.58 - 0.12 * fi) * rx) * side,
                ey - (1.66 + 0.3 * fi) * ry,
            );
            d = d.min(tapered(uv, outer, inner, 0.034 - 0.006 * fi, 0.005));
        }
    }
    ink(d, aa)
}

/// A big chibi eye (the fox's human form), the same in paint.wgsl
/// `chibi_eye`: the white, a gradient iris (dark at the top, light at the
/// bottom) with a darker rim, a big pupil, a heavy upper lash line that
/// flicks up and out at the outer corner, two highlights (the same way on
/// both eyes, as one light makes them).
fn chibi_eye(c: V3, uv: P, ec: P, r: P, side: f32, (iris, ink_c): (V3, V3), aa: f32) -> V3 {
    let (rx, ry) = r;
    let white = v3(1.0, 1.0, 1.0);
    let open = ell(uv, ec, r);
    let mut c = c.lerp(v3(0.98, 0.97, 0.99), ink(open, aa));
    let ic = (ec.0, ec.1 - 0.04 * ry);
    let ir = (0.8 * rx, 0.95 * ry);
    let di = ell(uv, ic, ir).max(open);
    let t = smoothstep(-ir.1, ir.1, uv.1 - ic.1);
    let shade = iris.lerp(white, 0.4).lerp(iris * 0.45, t);
    c = c.lerp(shade, ink(di, aa));
    let rim = (ell(uv, ic, ir).abs() * ir.0 - 0.012).max(open);
    c = c.lerp(iris * 0.35, ink(rim, aa));
    c = c.lerp(
        (iris * 0.18).lerp(ink_c, 0.5),
        ink(ell(uv, ic, (0.46 * rx, 0.52 * ry)).max(open), aa),
    );
    // The lash line: thick along the top of the opening, thinning down
    // its sides, and the flick out of the outer corner.
    let top = smoothstep(ec.1 - 0.35 * ry, ec.1 + 0.7 * ry, uv.1);
    let lash = (open.abs() * rx - (0.004 + 0.03 * top)).max(ec.1 - 0.2 * ry - uv.1);
    let flick = tapered(
        uv,
        (ec.0 + 0.8 * rx * side, ec.1 + 0.45 * ry),
        (ec.0 + (rx + 0.1) * side, ec.1 + 0.75 * ry),
        0.03,
        0.004,
    );
    c = c.lerp(ink_c, ink(lash.min(flick), aa));
    let hl = (ec.0 + 0.3 * rx, ec.1 + 0.32 * ry);
    c = c.lerp(white, ink(len(sub(uv, hl)) - 0.24 * rx, aa));
    let hl2 = (ec.0 - 0.28 * rx, ec.1 - 0.38 * ry);
    c.lerp(white, 0.85 * ink(len(sub(uv, hl2)) - 0.11 * rx, aa))
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
        if let Some(slot) = face.liner {
            // A flick out of the outer corner, up and away, under the eye
            // so the eye covers where it starts.
            let (a, b) = liner(ec, (rx, ry), side);
            c = c.lerp(scene.color(slot), ink(segment(uv, a, b, 0.018), aa));
        }
        let w = 0.17 * ry;
        // Plain eyes are all pupil; owls and cats can have a coloured iris.
        let (iris_c, pupil) = face
            .iris
            .map_or((ink_c, 1.0), |(slot, k)| (scene.color(slot), k));
        match scene.look() {
            Look::Open | Look::Blep if face.chibi => {
                c = chibi_eye(c, uv, ec, (rx, ry), side, (iris_c, ink_c), aa);
            }
            Look::Open | Look::Blep => {
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
            Look::Happy | Look::Laugh | Look::Pant => {
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
    if let Some(slot) = face.forehead {
        let mark = scene.color(slot);
        if face.seal {
            c = c.lerp(
                mark,
                seal_marks(
                    uv,
                    aa,
                    (face.eye_at.0, face.eye_at.1, face.eye_r.0, face.eye_r.1),
                ),
            );
        } else {
            let (whole, out) = fox_marks(uv, aa);
            c = c.lerp(mark.lerp(white, 0.35 * out), whole);
        }
    }
    c
}

/// The eyeliner flick by an eye at `ec` with radii `r` on `side`: from the
/// outer corner, out and up.
fn liner(ec: (f32, f32), r: (f32, f32), side: f32) -> ((f32, f32), (f32, f32)) {
    (
        (ec.0 + 0.85 * r.0 * side, ec.1 + 0.1 * r.1),
        (ec.0 + (r.0 + 0.13) * side, ec.1 + 0.55 * r.1 + 0.05),
    )
}

/// A tongue from `a` down to `b`, `r` wide, a darker groove down its middle.
fn tongue(c: V3, uv: P, a: P, b: P, r: f32, aa: f32) -> V3 {
    let t = segment(uv, a, b, r);
    let c = c.lerp(v3(1.0, 0.52, 0.56), ink(t, aa));
    let groove = segment(uv, (a.0, a.1 - 0.02), (b.0, b.1 + 0.6 * r), 0.012).max(t);
    c.lerp(v3(0.86, 0.36, 0.42), ink(groove, aa))
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
        Look::Blep => {
            // ω, the tip of the tongue peeping out under it.
            for s in [-1.0, 1.0] {
                let d = arc(uv, (0.15 * s, -0.10), 0.15, 0.022).max(uv.1 + 0.10);
                c = c.lerp(ink_c, ink(d, aa));
            }
            c = tongue(c, uv, (0.0, -0.2), (0.0, -0.44), 0.13, aa);
        }
        Look::Pant => {
            // A small open mouth, the tongue hanging out of it.
            let m = (len((uv.0 / 1.1, (uv.1 + 0.22) / 0.8)) - 0.26).max(uv.1 + 0.12);
            c = c.lerp(v3(0.55, 0.18, 0.22), ink(m, aa));
            c = tongue(c, uv, (0.0, -0.28), (0.0, -0.64), 0.17, aa);
        }
        _ if scene.model().face.chibi => return chibi_smile(c, uv, ink_c, aa),
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

/// A child's mouth at rest (the fox's human form, Roger's sheet): a wide,
/// shallow closed smile (the ends just turned up), a blush of pink lip
/// under it; no line up to the nose. The same in paint.wgsl `chibi_smile`.
fn chibi_smile(c: V3, uv: P, ink_c: V3, aa: f32) -> V3 {
    let lip = ell(uv, (0.0, -0.32), (0.34, 0.16));
    let c = c.lerp(v3(0.96, 0.62, 0.62), 0.3 * ink(lip, aa * 6.0));
    let line = arc(uv, (0.0, 1.0), 1.12, 0.05)
        .max(uv.1 - 0.1)
        .max(uv.0.abs() - 0.44);
    c.lerp(ink_c.lerp(v3(0.75, 0.35, 0.35), 0.4), ink(line, aa))
}
