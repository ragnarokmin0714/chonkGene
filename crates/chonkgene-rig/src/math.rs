//! The little vector maths a signed distance field needs.

use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub const ZERO: V3 = v3(0.0, 0.0, 0.0);
    pub const ONE: V3 = v3(1.0, 1.0, 1.0);

    pub fn from_array(a: [f32; 3]) -> Self {
        v3(a[0], a[1], a[2])
    }
    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn normalize(self) -> V3 {
        let l = self.length();
        if l > 0.0 { self / l } else { self }
    }
    pub fn mul_elem(self, o: V3) -> V3 {
        v3(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    pub fn div_elem(self, o: V3) -> V3 {
        v3(self.x / o.x, self.y / o.y, self.z / o.z)
    }
    pub fn max_elem(self) -> f32 {
        self.x.max(self.y).max(self.z)
    }
    pub fn min_elem(self) -> f32 {
        self.x.min(self.y).min(self.z)
    }
    pub fn lerp(self, o: V3, t: f32) -> V3 {
        self + (o - self) * t
    }
    /// Mirror across the x = 0 plane.
    pub fn flip_x(self) -> V3 {
        v3(-self.x, self.y, self.z)
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, s: f32) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Div<f32> for V3 {
    type Output = V3;
    fn div(self, s: f32) -> V3 {
        v3(self.x / s, self.y / s, self.z / s)
    }
}
impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// A rotation, as three orthonormal rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M3 {
    pub r: [V3; 3],
}

impl M3 {
    pub const IDENTITY: M3 = M3 {
        r: [v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0), v3(0.0, 0.0, 1.0)],
    };

    /// Roll about z, then pitch about x, then turn about y; angles in
    /// degrees. Turning last means a pose's `y` always spins the whole pet
    /// around the vertical, whatever it did before.
    pub fn euler(deg: V3) -> M3 {
        let (x, y, z) = (deg.x.to_radians(), deg.y.to_radians(), deg.z.to_radians());
        M3::rot_y(y).mul(M3::rot_x(x)).mul(M3::rot_z(z))
    }
    pub fn rot_x(a: f32) -> M3 {
        let (s, c) = a.sin_cos();
        M3 {
            r: [v3(1.0, 0.0, 0.0), v3(0.0, c, -s), v3(0.0, s, c)],
        }
    }
    pub fn rot_y(a: f32) -> M3 {
        let (s, c) = a.sin_cos();
        M3 {
            r: [v3(c, 0.0, s), v3(0.0, 1.0, 0.0), v3(-s, 0.0, c)],
        }
    }
    pub fn rot_z(a: f32) -> M3 {
        let (s, c) = a.sin_cos();
        M3 {
            r: [v3(c, -s, 0.0), v3(s, c, 0.0), v3(0.0, 0.0, 1.0)],
        }
    }
    pub fn apply(&self, v: V3) -> V3 {
        v3(self.r[0].dot(v), self.r[1].dot(v), self.r[2].dot(v))
    }
    pub fn transpose(&self) -> M3 {
        let [a, b, c] = self.r;
        M3 {
            r: [v3(a.x, b.x, c.x), v3(a.y, b.y, c.y), v3(a.z, b.z, c.z)],
        }
    }
    pub fn diag(d: V3) -> M3 {
        M3 {
            r: [v3(d.x, 0.0, 0.0), v3(0.0, d.y, 0.0), v3(0.0, 0.0, d.z)],
        }
    }
    pub fn mul(&self, o: M3) -> M3 {
        let t = o.transpose();
        M3 {
            r: [0, 1, 2].map(|i| {
                v3(
                    self.r[i].dot(t.r[0]),
                    self.r[i].dot(t.r[1]),
                    self.r[i].dot(t.r[2]),
                )
            }),
        }
    }
}

pub fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp01((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

pub fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn rotations_turn_the_right_way() {
        // −90° about x swings a leg hanging down (−y) to point forward (+z):
        // the "paws up" poses rely on this sign.
        let down = v3(0.0, -1.0, 0.0);
        assert!(close(
            M3::euler(v3(-90.0, 0.0, 0.0)).apply(down),
            v3(0.0, 0.0, 1.0)
        ));
        let m = M3::euler(v3(30.0, -20.0, 45.0));
        let v = v3(0.3, -0.7, 0.2);
        assert!(close(m.transpose().apply(m.apply(v)), v));
    }
}
