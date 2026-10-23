//! Signed distance primitives, in each part's own frame.

use crate::math::{V3, v3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// Radii along x, y, z.
    Ellipsoid(V3),
    /// From the part origin to `to`, radius `r`.
    Capsule { to: V3, r: f32 },
    /// A ring in the xz plane.
    Torus { major: f32, minor: f32 },
    /// Along +y: radius `r1` at the base, `r2` at height `h`; `flat` squeezes
    /// it front to back (ears).
    Cone { r1: f32, r2: f32, h: f32, flat: f32 },
}

impl Shape {
    pub fn distance(&self, p: V3) -> f32 {
        match *self {
            Shape::Ellipsoid(r) => ellipsoid(p, r),
            Shape::Capsule { to, r } => {
                let h = (p.dot(to) / to.dot(to)).clamp(0.0, 1.0);
                (p - to * h).length() - r
            }
            Shape::Torus { major, minor } => {
                let qx = (p.x * p.x + p.z * p.z).sqrt() - major;
                (qx * qx + p.y * p.y).sqrt() - minor
            }
            Shape::Cone { r1, r2, h, flat } => {
                // Squeezing z by `flat` stretches distances by up to `flat`;
                // dividing keeps the field safe to march.
                round_cone(v3(p.x, p.y, p.z * flat), r1, r2, h) / flat
            }
        }
    }

    /// Radius of a sphere around the part origin that holds the whole shape.
    pub fn bound(&self) -> f32 {
        match *self {
            Shape::Ellipsoid(r) => r.max_elem(),
            Shape::Capsule { to, r } => to.length() + r,
            Shape::Torus { major, minor } => major + minor,
            Shape::Cone { r1, r2, h, .. } => r1.max(h + r2),
        }
    }

    /// The same shape `f` times wider and deeper (x and z); used for body fat.
    pub fn widen(&self, f: f32) -> Shape {
        match *self {
            Shape::Ellipsoid(r) => Shape::Ellipsoid(v3(r.x * f, r.y, r.z * f)),
            Shape::Capsule { to, r } => Shape::Capsule {
                to,
                r: r * f.sqrt(),
            },
            Shape::Torus { major, minor } => Shape::Torus {
                major,
                minor: minor * f.sqrt(),
            },
            Shape::Cone { r1, r2, h, flat } => Shape::Cone {
                r1: r1 * f,
                r2,
                h,
                flat,
            },
        }
    }
}

/// Approximate (but marching-safe) ellipsoid distance.
pub fn ellipsoid(p: V3, r: V3) -> f32 {
    let k0 = p.div_elem(r).length();
    let k1 = p.div_elem(r.mul_elem(r)).length();
    if k1 == 0.0 {
        return -r.min_elem();
    }
    k0 * (k0 - 1.0) / k1
}

fn round_cone(p: V3, r1: f32, r2: f32, h: f32) -> f32 {
    let qx = (p.x * p.x + p.z * p.z).sqrt();
    let qy = p.y;
    let b = (r1 - r2) / h;
    let a = (1.0 - b * b).max(0.0).sqrt();
    let k = -b * qx + a * qy;
    if k < 0.0 {
        return (qx * qx + qy * qy).sqrt() - r1;
    }
    if k > a * h {
        return (qx * qx + (qy - h) * (qy - h)).sqrt() - r2;
    }
    a * qx + b * qy - r1
}

/// Blend weight of a smooth union: 1.0 keeps `a`, 0.0 takes `b`.
pub fn blend_weight(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return if a <= b { 1.0 } else { 0.0 };
    }
    (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0)
}

/// Smooth union of two distances with blend radius `k`.
pub fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = blend_weight(a, b, k);
    b + (a - b) * h - k * h * (1.0 - h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_are_zero_on_the_surface() {
        let e = Shape::Ellipsoid(v3(1.0, 0.5, 0.8));
        assert!(e.distance(v3(1.0, 0.0, 0.0)).abs() < 1e-5);
        assert!(e.distance(v3(0.0, 0.5, 0.0)).abs() < 1e-5);
        assert!(e.distance(V3::ZERO) < 0.0);
        let c = Shape::Capsule {
            to: v3(0.0, -1.0, 0.0),
            r: 0.2,
        };
        assert!((c.distance(v3(0.2, -0.5, 0.0))).abs() < 1e-5);
        let t = Shape::Torus {
            major: 0.5,
            minor: 0.1,
        };
        assert!(t.distance(v3(0.6, 0.0, 0.0)).abs() < 1e-5);
        let k = Shape::Cone {
            r1: 0.3,
            r2: 0.1,
            h: 1.0,
            flat: 1.0,
        };
        assert!(k.distance(v3(0.0, 1.1, 0.0)).abs() < 1e-5);
    }

    #[test]
    fn bounds_hold_every_surface_point() {
        let shapes = [
            Shape::Ellipsoid(v3(1.0, 0.5, 0.8)),
            Shape::Capsule {
                to: v3(0.3, -1.0, 0.2),
                r: 0.2,
            },
            Shape::Torus {
                major: 0.5,
                minor: 0.1,
            },
            Shape::Cone {
                r1: 0.3,
                r2: 0.1,
                h: 1.0,
                flat: 2.0,
            },
        ];
        for s in shapes {
            let r = s.bound();
            // Anything just outside the bound sphere is outside the shape.
            for i in 0..64 {
                let a = i as f32 * 0.7;
                let dir = v3(
                    a.sin() * (a * 0.3).cos(),
                    (a * 0.3).sin(),
                    a.cos() * (a * 0.3).cos(),
                );
                assert!(s.distance(dir.normalize() * (r + 1e-3)) > 0.0, "{s:?}");
            }
        }
    }

    #[test]
    fn smooth_union_is_never_above_min() {
        for (a, b) in [(0.1, 0.2), (0.5, -0.1), (0.3, 0.3)] {
            assert!(smin(a, b, 0.2) <= a.min(b) + 1e-6);
            assert_eq!(smin(a, b, 0.0), a.min(b));
        }
    }
}
