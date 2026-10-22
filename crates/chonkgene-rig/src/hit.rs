//! Point-in-part tests for hit zones: curves are flattened to polygons.

use tiny_skia_path::{Path, PathSegment, Point, Transform};

const CURVE_STEPS: usize = 8;

/// Is `point` (design space) inside `path` drawn with `ts`? Stroked parts
/// count within half their width.
pub fn contains(path: &Path, ts: Transform, stroke: Option<f32>, point: (f32, f32)) -> bool {
    let Some(inv) = ts.invert() else {
        return false;
    };
    let mut p = [Point::from_xy(point.0, point.1)];
    inv.map_points(&mut p);
    let (x, y) = (p[0].x, p[0].y);
    let polys = flatten(path);
    match stroke {
        Some(width) => polys.iter().any(|poly| near(poly, x, y, width / 2.0)),
        None => winding(&polys, x, y) != 0,
    }
}

fn flatten(path: &Path) -> Vec<Vec<(f32, f32)>> {
    let mut polys = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let mut last = (0.0, 0.0);
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                if cur.len() > 1 {
                    polys.push(std::mem::take(&mut cur));
                }
                cur.clear();
                last = (p.x, p.y);
                cur.push(last);
            }
            PathSegment::LineTo(p) => {
                last = (p.x, p.y);
                cur.push(last);
            }
            PathSegment::QuadTo(c, p) => {
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * last.0 + 2.0 * u * t * c.x + t * t * p.x,
                        u * u * last.1 + 2.0 * u * t * c.y + t * t * p.y,
                    ));
                }
                last = (p.x, p.y);
            }
            PathSegment::CubicTo(c1, c2, p) => {
                for i in 1..=CURVE_STEPS {
                    let t = i as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    cur.push((
                        a * last.0 + b * c1.x + c * c2.x + d * p.x,
                        a * last.1 + b * c1.y + c * c2.y + d * p.y,
                    ));
                }
                last = (p.x, p.y);
            }
            PathSegment::Close => {
                if cur.len() > 1 {
                    polys.push(std::mem::take(&mut cur));
                }
            }
        }
    }
    if cur.len() > 1 {
        polys.push(cur);
    }
    polys
}

/// Non-zero winding number of `(x, y)` against closed polygons.
fn winding(polys: &[Vec<(f32, f32)>], x: f32, y: f32) -> i32 {
    let mut w = 0;
    for poly in polys {
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            let side = (b.0 - a.0) * (y - a.1) - (x - a.0) * (b.1 - a.1);
            if a.1 <= y && b.1 > y && side > 0.0 {
                w += 1;
            } else if a.1 > y && b.1 <= y && side < 0.0 {
                w -= 1;
            }
        }
    }
    w
}

fn near(poly: &[(f32, f32)], x: f32, y: f32, radius: f32) -> bool {
    poly.windows(2).any(|seg| {
        let (a, b) = (seg[0], seg[1]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((x - a.0) * dx + (y - a.1) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (px, py) = (a.0 + dx * t, a.1 + dy * t);
        (x - px).hypot(y - py) <= radius
    })
}
