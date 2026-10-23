//! A posed pet as one signed distance field, and what its surface looks like.

use crate::face;
use crate::math::{M3, V3, mix, smoothstep, v3};
use crate::model::{Look, Model, PoseId, rgb};
use crate::sdf::{Shape, blend_weight, smin};

/// Body value (0.0 標準 … 1.0 麻糬) → how much wider the belly gets.
pub fn fat(chonk: f32) -> f32 {
    0.85 + 0.55 * chonk.clamp(0.0, 1.0)
}

/// One placed copy of a part.
#[derive(Clone, Debug)]
struct Placed {
    part: usize,
    shape: Shape,
    /// World → part frame: `a · p + b`.
    a: M3,
    b: V3,
    /// Bone scale squeezes distances; multiplying by its smallest axis keeps
    /// them safe to march.
    k: f32,
    blend: f32,
    /// World-space sphere holding the part; `center` is its origin.
    center: V3,
    radius: f32,
    bone: usize,
    /// Largest bone scale, for radii that grow with it.
    smax: f32,
}

impl Placed {
    /// World height of the part's lowest point (exact for ellipsoids and
    /// capsules, close for tori and cones).
    fn lowest(&self) -> f32 {
        let l = inverse(&self.a);
        let c = self.center;
        match self.shape {
            Shape::Ellipsoid(r) => {
                let row = l.r[1];
                c.y - (row.mul_elem(r)).length()
            }
            Shape::Capsule { to, r } => c.y.min(c.y + l.r[1].dot(to)) - r * self.smax,
            Shape::Torus { major, minor } => {
                let axis = l.apply(v3(0.0, 1.0, 0.0)).normalize();
                c.y - (major * (1.0 - axis.y * axis.y).max(0.0).sqrt() + minor) * self.smax
            }
            Shape::Cone { r1, r2, h, .. } => {
                (c.y - r1 * self.smax).min(c.y + l.r[1].y * h - r2 * self.smax)
            }
        }
    }

    /// Move the part up by `dy` in world space.
    fn lift(&mut self, dy: f32) {
        self.center.y += dy;
        self.b = self.b - self.a.apply(v3(0.0, dy, 0.0));
    }

    fn local(&self, p: V3) -> V3 {
        self.a.apply(p) + self.b
    }
    fn distance(&self, p: V3) -> f32 {
        self.shape.distance(self.local(p)) * self.k
    }
}

/// What the surface is like at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    /// Straight RGB, 0.0–1.0.
    pub color: V3,
    pub gloss: f32,
    pub fuzz: f32,
    /// Index into [`crate::ZONES`].
    pub zone: u8,
}

pub struct Scene<'m> {
    model: &'m Model,
    palette: Vec<V3>,
    placed: Vec<Placed>,
    look: Look,
    face: usize,
    mouth: Option<usize>,
    /// Sphere around the whole pet.
    pub center: V3,
    pub radius: f32,
}

impl<'m> Scene<'m> {
    pub fn new(model: &'m Model, coat: usize, chonk: f32, pose: PoseId, look: Look) -> Self {
        let palette = model.coats[coat.min(model.coats.len() - 1)]
            .colors
            .iter()
            .map(|c| rgb(*c))
            .collect();
        let pose = model.pose(pose);
        let fat = fat(chonk);
        let mut placed = Vec::new();
        let (mut face, mut mouth) = (0, None);
        for (i, part) in model.parts.iter().enumerate() {
            let f = 1.0 + part.chonk * (fat - 1.0);
            let shape = part.shape.widen(f);
            let at = v3(part.at.x * f, part.at.y, part.at.z);
            let bone = pose.bones.get(part.bone).copied().unwrap_or_default();
            let pivot = model.pivots[part.bone];
            let root = pose.root;
            let rp = M3::euler(part.rot);
            let rb = M3::euler(bone.rot);
            let rr = M3::euler(root.rot);
            for flip in [false, true] {
                if flip && !part.mirror {
                    continue;
                }
                let fx = if flip { -1.0 } else { 1.0 };
                let local = |p: V3| {
                    let q = rr.transpose().apply(p - root.shift);
                    let q = rb
                        .transpose()
                        .apply(q - pivot - bone.shift)
                        .div_elem(bone.scale)
                        + pivot;
                    let q = v3(q.x * fx, q.y, q.z);
                    rp.transpose().apply(q - at)
                };
                let b = local(V3::ZERO);
                let cols =
                    [v3(1.0, 0.0, 0.0), v3(0.0, 1.0, 0.0), v3(0.0, 0.0, 1.0)].map(|e| local(e) - b);
                let a = M3 {
                    r: [cols[0], cols[1], cols[2]],
                }
                .transpose();
                // The part's origin, carried forward to world space.
                let origin = v3(at.x * fx, at.y, at.z);
                let in_bone = pivot + rb.apply((origin - pivot).mul_elem(bone.scale)) + bone.shift;
                let center = rr.apply(in_bone) + root.shift;
                if i == model.face.on && !flip {
                    face = placed.len();
                }
                if Some(i) == model.face.mouth_on && !flip {
                    mouth = Some(placed.len());
                }
                placed.push(Placed {
                    part: i,
                    shape,
                    a,
                    b,
                    k: bone.scale.min_elem(),
                    blend: part.blend,
                    center,
                    radius: shape.bound() * bone.scale.max_elem() + part.blend,
                    bone: part.bone,
                    smax: bone.scale.max_elem(),
                });
            }
        }
        // Settle what the pose asks to rest on the floor: bones first (tucked
        // paws), then the whole pet.
        for (bi, mv) in pose.bones.iter().enumerate() {
            if mv.rest {
                settle(placed.iter_mut().filter(|p| p.bone == bi));
            }
        }
        if pose.root.rest {
            settle(placed.iter_mut());
        }
        let (mut lo, mut hi) = (
            v3(f32::MAX, f32::MAX, f32::MAX),
            v3(f32::MIN, f32::MIN, f32::MIN),
        );
        for p in &placed {
            let r = v3(p.radius, p.radius, p.radius);
            let (a, b) = (p.center - r, p.center + r);
            lo = v3(lo.x.min(a.x), lo.y.min(a.y), lo.z.min(a.z));
            hi = v3(hi.x.max(b.x), hi.y.max(b.y), hi.z.max(b.z));
        }
        let center = (lo + hi) * 0.5;
        let radius = placed
            .iter()
            .map(|p| (p.center - center).length() + p.radius)
            .fold(0.0, f32::max);
        Self {
            model,
            palette,
            placed,
            look,
            face,
            mouth,
            center,
            radius,
        }
    }

    pub fn model(&self) -> &Model {
        self.model
    }

    /// Signed distance from `p` to the pet.
    pub fn distance(&self, p: V3) -> f32 {
        let mut d = f32::MAX;
        for pl in &self.placed {
            // A part farther than the blend radius beyond the current surface
            // cannot change the union: skip it on its bound alone.
            if (p - pl.center).length() - pl.radius >= d + pl.blend {
                continue;
            }
            let dp = pl.distance(p);
            d = if d == f32::MAX {
                dp
            } else {
                smin(d, dp, pl.blend)
            };
        }
        d
    }

    /// Outward surface normal at `p`.
    pub fn normal(&self, p: V3) -> V3 {
        let e = 0.0015;
        let k = [
            v3(1.0, -1.0, -1.0),
            v3(-1.0, -1.0, 1.0),
            v3(-1.0, 1.0, -1.0),
            v3(1.0, 1.0, 1.0),
        ];
        k.iter()
            .fold(V3::ZERO, |n, k| n + *k * self.distance(p + *k * e))
            .normalize()
    }

    /// Colour, gloss, fuzz and hit zone at surface point `p`; `px` is the
    /// size of a pixel in world units, for anti-aliasing the painted face.
    pub fn surface(&self, p: V3, px: f32) -> Surface {
        let mut d = f32::MAX;
        let mut color = V3::ZERO;
        let mut gloss = 0.0;
        let mut fuzz = 0.0;
        let mut zone = 0;
        let mut nearest = f32::MAX;
        for pl in &self.placed {
            if (p - pl.center).length() - pl.radius >= d + pl.blend {
                continue;
            }
            let q = pl.local(p);
            let dp = pl.shape.distance(q) * pl.k;
            let part = &self.model.parts[pl.part];
            let (c, z) = self.paint(part, q);
            if d == f32::MAX {
                (d, color, gloss, fuzz) = (dp, c, part.gloss, part.fuzz);
            } else {
                let h = blend_weight(d, dp, pl.blend);
                color = c.lerp(color, h);
                gloss = mix(part.gloss, gloss, h);
                fuzz = mix(part.fuzz, fuzz, h);
                d = smin(d, dp, pl.blend);
            }
            if dp < nearest {
                nearest = dp;
                zone = z;
            }
        }
        let fp = &self.placed[self.face];
        let fq = fp.local(p);
        if fp.shape.distance(fq).abs() < 0.05 {
            color = face::eyes(self, fp.shape, fq, color, px);
        }
        if let Some(m) = self.mouth.map(|i| &self.placed[i]) {
            let mq = m.local(p);
            if m.shape.distance(mq).abs() < 0.04 {
                color = face::mouth(self, m.shape, mq, color, px);
            }
        }
        Surface {
            color,
            gloss,
            fuzz,
            zone,
        }
    }

    /// A part's own colour at `q` (its frame), with its marks, and its zone.
    fn paint(&self, part: &crate::Part, q: V3) -> (V3, u8) {
        let mut c = self.palette[part.slot];
        let mut zone = part.zone;
        for m in &part.marks {
            let mut cover = mark(q, m.at, m.radii, m.soft);
            if m.mirror {
                cover = cover.max(mark(q, m.at.flip_x(), m.radii, m.soft));
            }
            if cover > 0.0 {
                c = c.lerp(self.palette[m.slot], cover);
                if cover > 0.5
                    && let Some(z) = m.zone
                {
                    zone = z;
                }
            }
        }
        (c, zone)
    }

    pub(crate) fn color(&self, slot: usize) -> V3 {
        self.palette[slot]
    }

    pub(crate) fn look(&self) -> Look {
        self.look
    }

    /// Where the snot bubble hangs, in world space.
    pub fn nose(&self) -> V3 {
        let fp = &self.placed[self.face];
        inverse(&fp.a).apply(self.model.face.nose - fp.b)
    }
}

/// Lift (or drop) parts together until the lowest touches the floor.
fn settle<'a>(parts: impl Iterator<Item = &'a mut Placed>) {
    let mut parts: Vec<&mut Placed> = parts.collect();
    let low = parts.iter().map(|p| p.lowest()).fold(f32::MAX, f32::min);
    if low < f32::MAX {
        for p in &mut parts {
            p.lift(crate::FLOOR - low);
        }
    }
}

/// How much of a soft ellipsoidal mark covers `q`.
fn mark(q: V3, at: V3, radii: V3, soft: f32) -> f32 {
    1.0 - smoothstep(1.0 - soft, 1.0, (q - at).div_elem(radii).length())
}

/// General 3×3 inverse (bone scale makes the frame non-orthonormal).
fn inverse(m: &M3) -> M3 {
    let [a, b, c] = m.r;
    let cross = |u: V3, v: V3| {
        v3(
            u.y * v.z - u.z * v.y,
            u.z * v.x - u.x * v.z,
            u.x * v.y - u.y * v.x,
        )
    };
    let (r0, r1, r2) = (cross(b, c), cross(c, a), cross(a, b));
    let det = a.dot(r0);
    M3 {
        r: [r0 / det, r1 / det, r2 / det],
    }
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_undoes_a_scaled_turn() {
        let m = M3::euler(v3(20.0, 35.0, -10.0)).mul(M3::diag(v3(1.0, 0.6, 1.4)));
        let p = v3(0.3, -0.2, 0.9);
        assert!((inverse(&m).apply(m.apply(p)) - p).length() < 1e-5);
    }
}
