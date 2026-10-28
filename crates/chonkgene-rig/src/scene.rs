//! A posed pet as one signed distance field, and what its surface looks like.

use crate::face;
use crate::math::{M3, V3, mix, smoothstep, v3};
use crate::model::{Look, Model, Move, Pose, PoseId, rgb};
use crate::sdf::{Shape, blend_weight, smin};

/// Body value (0.0 標準 … 1.0 麻糬) → how much wider the belly gets.
pub fn fat(chonk: f32) -> f32 {
    0.85 + 0.55 * chonk.clamp(0.0, 1.0)
}

/// Which parts a scene holds. The tail is drawn as its own layer so a frame
/// can wag it without ray-marching the pet again (PLAN §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    All,
    /// Everything but the parts on the `tail` bone.
    Body,
    /// Only the parts on the `tail` bone.
    Tail,
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
    /// Moved by the pose's mirrored-copy move for its bone.
    twin: bool,
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

/// A placed part, read-only.
pub struct PartView<'s> {
    pub shape: Shape,
    /// World → part frame: `a · p + b`.
    pub a: M3,
    pub b: V3,
    /// Multiplies the part's distances to keep them safe to march.
    pub k: f32,
    pub blend: f32,
    /// World-space sphere holding the part.
    pub center: V3,
    pub radius: f32,
    pub part: &'s crate::Part,
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
    layer: Layer,
    face: Option<usize>,
    mouth: Option<usize>,
    nose: V3,
    /// Where the tail joins the body, and whether it is nearer the camera
    /// than the body's middle, in world space; none without a tail bone.
    tail: Option<(V3, bool)>,
    /// Sphere around the whole pet.
    pub center: V3,
    pub radius: f32,
}

impl<'m> Scene<'m> {
    pub fn new(
        model: &'m Model,
        coat: usize,
        chonk: f32,
        pose: PoseId,
        look: Look,
        layer: Layer,
    ) -> Self {
        Self::posed(model, coat, chonk, model.pose(pose), look, layer)
    }

    /// A scene in any pose, such as one blended between two of the model's.
    pub fn posed(
        model: &'m Model,
        coat: usize,
        chonk: f32,
        pose: &Pose,
        look: Look,
        layer: Layer,
    ) -> Self {
        let palette = model.coats[coat.min(model.coats.len() - 1)]
            .colors
            .iter()
            .map(|c| rgb(*c))
            .collect();
        let fat = fat(chonk);
        let root = Affine::of(pose.root, V3::ZERO);
        // Every bone's place in the pet: its own move about its pivot, after
        // its parent's turn and shift (parents come first in `model.order`;
        // a parent's scale squashes only its own parts, so a melting body
        // does not flatten the head). A bone's mirrored side may have a move
        // of its own (a twin): written for +x, applied mirrored about the
        // mirrored pivot, under the same parent.
        let n = model.bones.len();
        let mut plain = vec![Affine::IDENTITY; n];
        let mut rigid = vec![Affine::IDENTITY; n];
        let mut twins = vec![None; n];
        for &b in &model.order {
            let parent = model.parents[b].map_or(Affine::IDENTITY, |p| rigid[p]);
            let pivot = model.pivots[b];
            let mv = pose.bone(b, false);
            plain[b] = parent.then(&Affine::of(mv, pivot));
            rigid[b] = parent.then(&Affine::of(
                Move {
                    scale: V3::ONE,
                    ..mv
                },
                pivot,
            ));
            twins[b] = pose
                .twins
                .get(b)
                .copied()
                .flatten()
                .map(|t| parent.then(&Affine::of(t.mirrored(), pivot.flip_x())));
        }
        let mut placed = Vec::new();
        let (mut face, mut mouth) = (0, None);
        for (i, part) in model.parts.iter().enumerate() {
            let f = 1.0 + part.chonk * (fat - 1.0);
            let shape = part.shape.widen(f);
            let at = v3(part.at.x * f, part.at.y, part.at.z);
            let rp = M3::euler(part.rot);
            for flip in [false, true] {
                if flip && !part.mirror {
                    continue;
                }
                // A mirrored copy moves rigidly with its bone (both ears turn
                // with the head), unless the pose gives its side a move of
                // its own.
                let twin = if flip { twins[part.bone] } else { None };
                let bone = twin.unwrap_or(plain[part.bone]);
                // Part frame → world: placed by `at` and `rot`, flipped
                // across x for the mirrored copy, then the bone, then the
                // whole pet.
                let fx = if flip { -1.0 } else { 1.0 };
                let flip_m = M3::diag(v3(fx, 1.0, 1.0));
                let place = Affine {
                    m: flip_m.mul(rp),
                    t: v3(at.x * fx, at.y, at.z),
                };
                let world = root.then(&bone).then(&place);
                let inv = world.inverse();
                if i == model.face.on && !flip {
                    face = placed.len();
                }
                if Some(i) == model.face.mouth_on && !flip {
                    mouth = Some(placed.len());
                }
                let (smin, smax) = world.scale_range();
                placed.push(Placed {
                    part: i,
                    shape,
                    a: inv.m,
                    b: inv.t,
                    k: smin,
                    blend: part.blend,
                    center: world.t,
                    radius: shape.bound() * smax + part.blend,
                    bone: part.bone,
                    twin: twin.is_some(),
                    smax,
                });
            }
        }
        // Where the tail bone ends up: where the pose puts its pivot, carried
        // along by the settling below like the tail's parts.
        let tail_bone = model.bones.iter().position(|b| b == "tail");
        let tail_part = |placed: &[Placed]| {
            tail_bone
                .and_then(|b| placed.iter().find(|p| p.bone == b))
                .map(|p| p.center)
        };
        let unsettled = tail_part(&placed);
        // Settle what the pose asks to rest on the floor: bones first (tucked
        // paws), then the whole pet.
        for (bi, mv) in pose.bones.iter().enumerate() {
            if mv.rest > 0.0 {
                settle(
                    placed.iter_mut().filter(|p| p.bone == bi && !p.twin),
                    mv.rest,
                );
            }
            if let Some(m) = pose
                .twins
                .get(bi)
                .copied()
                .flatten()
                .filter(|m| m.rest > 0.0)
            {
                settle(placed.iter_mut().filter(|p| p.bone == bi && p.twin), m.rest);
            }
        }
        if pose.root.rest > 0.0 {
            settle(placed.iter_mut(), pose.root.rest);
        }
        let tail_root =
            tail_bone
                .zip(unsettled)
                .zip(tail_part(&placed))
                .map(|((b, before), after)| {
                    root.then(&plain[b]).apply(model.pivots[b]) + (after - before)
                });

        let tail = tail_root.map(|root| {
            let body: Vec<V3> = placed
                .iter()
                .filter(|p| Some(p.bone) != tail_bone)
                .map(|p| p.center)
                .collect();
            let mid = body.iter().fold(V3::ZERO, |a, c| a + *c) / body.len().max(1) as f32;
            (root, root.z > mid.z)
        });
        let nose = {
            let fp = &placed[face];
            inverse(&fp.a).apply(model.face.nose - fp.b)
        };
        let keep = |p: &Placed| match layer {
            Layer::All => true,
            Layer::Body => Some(p.bone) != tail_bone,
            Layer::Tail => Some(p.bone) == tail_bone,
        };
        let index = |at: usize, placed: &[Placed]| {
            keep(&placed[at]).then(|| placed[..at].iter().filter(|p| keep(p)).count())
        };
        let (face, mouth) = (index(face, &placed), mouth.and_then(|m| index(m, &placed)));
        placed.retain(keep);
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
            layer,
            face,
            mouth,
            nose,
            tail,
            center,
            radius,
        }
    }

    pub fn layer(&self) -> Layer {
        self.layer
    }

    /// Where the tail joins the body, and whether it is in front of it.
    pub fn tail(&self) -> Option<(V3, bool)> {
        self.tail
    }

    /// Does the scene hold anything to draw?
    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
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
        let mut s = self.coat_at(p);
        if let Some(fp) = self.face.map(|i| &self.placed[i]) {
            let fq = fp.local(p);
            if fp.shape.distance(fq).abs() < 0.05 {
                s.color = face::eyes(self, fp.shape, fq, s.color, px);
            }
        }
        if let Some(m) = self.mouth.map(|i| &self.placed[i]) {
            let mq = m.local(p);
            if m.shape.distance(mq).abs() < 0.04 {
                s.color = face::mouth(self, m.shape, mq, s.color, px);
            }
        }
        s
    }

    /// [`Scene::surface`] without the painted face: the coat and its marks.
    pub fn coat_at(&self, p: V3) -> Surface {
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

    /// The coat's colours, one per slot.
    pub fn palette(&self) -> &[V3] {
        &self.palette
    }

    pub fn look(&self) -> Look {
        self.look
    }

    /// Every placed part in blend order, as a ray-marcher elsewhere (the
    /// GPU) needs it.
    pub fn parts(&self) -> impl Iterator<Item = PartView<'_>> {
        self.placed.iter().map(|p| PartView {
            shape: p.shape,
            a: p.a,
            b: p.b,
            k: p.k,
            blend: p.blend,
            center: p.center,
            radius: p.radius,
            part: &self.model.parts[p.part],
        })
    }

    /// Indices into [`Scene::parts`] of the parts the eyes and the mouth
    /// are painted on, if this layer holds them.
    pub fn face_parts(&self) -> (Option<usize>, Option<usize>) {
        (self.face, self.mouth)
    }

    /// Where the snot bubble hangs, in world space.
    pub fn nose(&self) -> V3 {
        self.nose
    }
}

/// An affine map: `m · p + t`.
#[derive(Clone, Copy, Debug)]
struct Affine {
    m: M3,
    t: V3,
}

impl Affine {
    const IDENTITY: Affine = Affine {
        m: M3::IDENTITY,
        t: V3::ZERO,
    };

    /// A pose move: scale and turn about `pivot`, then shift.
    fn of(mv: Move, pivot: V3) -> Self {
        let m = M3::euler(mv.rot).mul(M3::diag(mv.scale));
        Affine {
            m,
            t: pivot + mv.shift - m.apply(pivot),
        }
    }

    fn apply(&self, p: V3) -> V3 {
        self.m.apply(p) + self.t
    }

    /// `self` after `inner`: apply `inner` first.
    fn then(&self, inner: &Affine) -> Affine {
        Affine {
            m: self.m.mul(inner.m),
            t: self.apply(inner.t),
        }
    }

    fn inverse(&self) -> Affine {
        let m = inverse(&self.m);
        Affine {
            m,
            t: -m.apply(self.t),
        }
    }

    /// How much it shrinks and stretches lengths, at most each way (the
    /// columns' lengths: exact for the turn-then-scale moves bones make).
    fn scale_range(&self) -> (f32, f32) {
        let cols = self.m.transpose().r.map(V3::length);
        (
            cols.iter().copied().fold(f32::MAX, f32::min),
            cols.iter().copied().fold(0.0, f32::max),
        )
    }
}

/// Lift (or drop) parts together until the lowest touches the floor, or
/// `amount` of the way there.
fn settle<'a>(parts: impl Iterator<Item = &'a mut Placed>, amount: f32) {
    let mut parts: Vec<&mut Placed> = parts.collect();
    let low = parts.iter().map(|p| p.lowest()).fold(f32::MAX, f32::min);
    if low < f32::MAX {
        for p in &mut parts {
            p.lift((crate::FLOOR - low) * amount);
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
