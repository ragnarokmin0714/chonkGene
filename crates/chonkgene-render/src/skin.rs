//! Skinned-mesh spike: the skeleton a baked pet mesh hangs on, and a
//! four-legged walk worked out every frame. The legs step one at a time
//! (left hind, left fore, right hind, right fore) with each planted foot
//! keeping pace with the floor, the chest and hips rock against each other,
//! and the tail, belly, bottom and chin ride springs of their own.

use chonkgene_rig::math::{M3, V3, v3};
use chonkgene_rig::{FLOOR, Model, builtin};

/// A bone's move: a turn about its pivot (degrees, as [`M3::euler`]), then
/// a shift.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Joint {
    pub rot: V3,
    pub shift: V3,
}

/// An affine map: `m · p + t`.
#[derive(Clone, Copy, Debug)]
pub struct Affine {
    pub m: M3,
    pub t: V3,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        m: M3::IDENTITY,
        t: V3::ZERO,
    };

    /// `self` after `o`.
    fn then(&self, o: &Affine) -> Affine {
        Affine {
            m: self.m.mul(o.m),
            t: self.m.apply(o.t) + self.t,
        }
    }

    pub fn apply(&self, p: V3) -> V3 {
        self.m.apply(p) + self.t
    }

    /// The three rows of `[m | t]`, as the shader reads a bone.
    pub fn rows(&self) -> [[f32; 4]; 3] {
        [0, 1, 2].map(|i| {
            let r = self.m.r[i];
            [r.x, r.y, r.z, [self.t.x, self.t.y, self.t.z][i]]
        })
    }
}

/// A soft spot (a belly, a bottom, a chin) that wobbles on a spring bone of
/// its own, hung from `parent`; vertices within `radius` of `at` pass up to
/// `share` of their weight to it.
pub struct Jiggle {
    pub name: &'static str,
    pub parent: &'static str,
    pub at: V3,
    pub radius: f32,
    pub share: f32,
}

/// The far end of a bone (a tail's tip) as a bone of its own, turning about
/// `pivot`: weight passes to it from `near` to `far` away from the parent's
/// pivot.
pub struct Tip {
    pub name: &'static str,
    pub parent: &'static str,
    pub pivot: V3,
    pub near: f32,
    pub far: f32,
}

/// The four-legged Shiba and its skeleton.
pub fn shiba_quad() -> Result<(Model, Skeleton), String> {
    let model = Model::parse(
        include_str!("../../../assets/species/shiba/quad.ron"),
        builtin::POSES,
    )
    .map_err(|e| e.to_string())?;
    let sk = Skeleton::new(&model, SHIBA_TIPS, SHIBA_JIGGLES)?;
    Ok((model, sk))
}

/// The four-legged Shiba's extra bones.
pub const SHIBA_TIPS: &[Tip] = &[Tip {
    name: "tail_tip",
    parent: "tail",
    pivot: v3(0.06, 0.22, -0.86),
    near: 0.14,
    far: 0.36,
}];

pub const SHIBA_JIGGLES: &[Jiggle] = &[
    Jiggle {
        name: "belly",
        parent: "body",
        at: v3(0.0, -0.72, 0.2),
        radius: 0.55,
        share: 0.85,
    },
    Jiggle {
        name: "butt",
        parent: "hips",
        at: v3(0.0, -0.3, -1.0),
        radius: 0.52,
        share: 0.8,
    },
    Jiggle {
        name: "chin",
        parent: "head",
        at: v3(0.0, -0.2, 1.4),
        radius: 0.55,
        share: 0.8,
    },
];

/// The four-legged Shiba's shadow: radii across and along its body.
pub const SHIBA_SHADOW: (f32, f32) = (0.62, 1.15);

/// Where the whole pet turns: the floor under the middle of its body.
pub const ROOT_PIVOT: V3 = v3(0.0, FLOOR, -0.1);

pub struct Skeleton {
    pub names: Vec<String>,
    pub parents: Vec<Option<usize>>,
    pub pivots: Vec<V3>,
    /// Parents before children.
    order: Vec<usize>,
    /// Bone, centre, radius, share.
    pub jiggles: Vec<(usize, V3, f32, f32)>,
    /// Parent bone, tip bone, near, far.
    pub tips: Vec<(usize, usize, f32, f32)>,
}

impl Skeleton {
    /// The model's bones and pivots, plus `tips` and `jiggles`.
    pub fn new(model: &Model, tips: &[Tip], jiggles: &[Jiggle]) -> Result<Self, String> {
        let mut names = model.bones.clone();
        let mut parents = model.parents.clone();
        let mut pivots = model.pivots.clone();
        pivots[0] = ROOT_PIVOT;
        // Bone 0 is the root: every other bone without a parent hangs from
        // it, so turning the root turns the pet.
        for p in parents.iter_mut().skip(1) {
            p.get_or_insert(0);
        }
        let mut order = vec![0];
        order.extend(model.order.iter().copied().filter(|&b| b != 0));
        let find = |names: &[String], n: &str| {
            names
                .iter()
                .position(|b| b == n)
                .ok_or_else(|| format!("no bone {n:?}"))
        };
        let mut add = |names: &mut Vec<String>, name: &str, parent: usize, pivot: V3| {
            names.push(name.to_string());
            parents.push(Some(parent));
            pivots.push(pivot);
            order.push(names.len() - 1);
            names.len() - 1
        };
        let mut tip_bones = Vec::new();
        for t in tips {
            let p = find(&names, t.parent)?;
            let b = add(&mut names, t.name, p, t.pivot);
            tip_bones.push((p, b, t.near, t.far));
        }
        let mut jiggle_bones = Vec::new();
        for j in jiggles {
            let p = find(&names, j.parent)?;
            let b = add(&mut names, j.name, p, j.at);
            jiggle_bones.push((b, j.at, j.radius, j.share));
        }
        Ok(Self {
            names,
            parents,
            pivots,
            order,
            jiggles: jiggle_bones,
            tips: tip_bones,
        })
    }

    pub fn bone(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|b| b == name)
    }

    /// Every bone's rest → posed map for `joints` (one per bone; bone 0,
    /// the root, moves the whole pet about the floor under it).
    pub fn pose(&self, joints: &[Joint]) -> Vec<Affine> {
        let mut out = vec![Affine::IDENTITY; self.names.len()];
        for &b in &self.order {
            let parent = self.parents[b].map_or(Affine::IDENTITY, |p| out[p]);
            let j = joints.get(b).copied().unwrap_or_default();
            let pivot = self.pivots[b];
            let m = M3::euler(j.rot);
            let local = Affine {
                m,
                t: pivot + j.shift - m.apply(pivot),
            };
            out[b] = parent.then(&local);
        }
        out
    }
}

/// A damped spring: `x` chases `target`, shaken by `push` (an acceleration).
#[derive(Clone, Copy, Debug, Default)]
struct Spring {
    x: f32,
    v: f32,
}

impl Spring {
    fn step(&mut self, target: f32, push: f32, hz: f32, damping: f32, dt: f32) -> f32 {
        let w = std::f32::consts::TAU * hz;
        let a = -w * w * (self.x - target) - 2.0 * damping * w * self.v + push;
        self.v += a * dt;
        self.x += self.v * dt;
        self.x
    }
}

/// Steps a second at full walking pace.
const STEP_HZ: f32 = 2.0;
/// Leg swing either way, degrees: with legs 0.45 long and 62% of each step
/// on the floor, a planted foot slides back at 1.0 world units a second,
/// the pace the pet's window moves (60 px/s at 100%).
const SWING: f32 = 20.0;
const DUTY: f32 = 0.62;
const LIFT: f32 = 0.07;
/// Where each leg is in the step cycle: a walk, one foot at a time.
const LEGS: [(&str, f32); 4] = [
    ("leg_bl", 0.0),
    ("leg_fl", 0.25),
    ("leg_br", 0.5),
    ("leg_fr", 0.75),
];
/// The chest's rise and fall, world units; twice a step cycle.
const BOB: f32 = 0.02;
const ROLL: f32 = 3.0;
const TWIST: f32 = 4.0;
/// How far the pet turns side-on while walking, and at rest (toward you).
const WALK_YAW: f32 = 72.0;
const REST_YAW: f32 = 28.0;

/// A four-legged walk, worked out frame by frame.
pub struct Walk {
    phase: f32,
    /// 0 standing … 1 full pace.
    pub speed: f32,
    yaw: Spring,
    clock: f32,
    tail: Spring,
    tip: Spring,
    belly: Spring,
    butt: Spring,
    butt_z: Spring,
    chin: Spring,
    /// The chest's height the last two frames, and the pace last frame, for
    /// the shakes they give the soft parts.
    bob: [f32; 2],
    nod: [f32; 2],
    pace: f32,
    /// The turn's speed last frame: turning flings the tail the other way.
    yaw_v: f32,
    legs: [(usize, f32); 4],
    bones: Bones,
    n: usize,
}

struct Bones {
    body: usize,
    hips: usize,
    head: usize,
    tail: usize,
    tail_tip: Option<usize>,
    belly: Option<usize>,
    butt: Option<usize>,
    chin: Option<usize>,
}

impl Walk {
    pub fn new(sk: &Skeleton, facing: f32) -> Result<Self, String> {
        let need = |n: &str| sk.bone(n).ok_or_else(|| format!("no bone {n:?}"));
        let mut legs = [(0, 0.0); 4];
        for (l, (name, offset)) in legs.iter_mut().zip(LEGS) {
            *l = (need(name)?, offset);
        }
        Ok(Self {
            phase: 0.0,
            speed: 0.0,
            yaw: Spring {
                x: facing * REST_YAW,
                v: 0.0,
            },
            clock: 0.0,
            tail: Spring::default(),
            tip: Spring::default(),
            belly: Spring::default(),
            butt: Spring::default(),
            butt_z: Spring::default(),
            chin: Spring::default(),
            bob: [0.0; 2],
            nod: [0.0; 2],
            pace: 0.0,
            yaw_v: 0.0,
            legs,
            bones: Bones {
                body: need("body")?,
                hips: need("hips")?,
                head: need("head")?,
                tail: need("tail")?,
                tail_tip: sk.bone("tail_tip"),
                belly: sk.bone("belly"),
                butt: sk.bone("butt"),
                chin: sk.bone("chin"),
            },
            n: sk.names.len(),
        })
    }

    /// Jolt the soft parts (a poke in the belly), in world units a second:
    /// the belly most, the bottom and the chin wobbling along.
    pub fn poke(&mut self, kick: f32) {
        self.belly.v += kick;
        self.butt.v += kick * 0.7;
        self.chin.v += kick * 0.6;
    }

    /// The pet's turn about the vertical, degrees (0: facing you).
    pub fn yaw(&self) -> f32 {
        self.yaw.x
    }

    /// Advance `dt` seconds, walking or standing, facing +1 (right) or −1
    /// (left); the joints for this frame.
    pub fn step(&mut self, dt: f32, walking: bool, facing: f32) -> Vec<Joint> {
        use std::f32::consts::{PI, TAU};
        let dt = dt.max(1e-4);
        self.clock += dt;
        let goal = if walking { 1.0 } else { 0.0 };
        self.speed += (goal - self.speed).clamp(-dt * 4.0, dt * 4.0);
        let s = self.speed;
        if s > 0.0 {
            self.phase = (self.phase + dt * STEP_HZ * (0.4 + 0.6 * s)).fract();
        }
        let yaw = self.yaw.step(
            facing * if walking { WALK_YAW } else { REST_YAW },
            0.0,
            1.3,
            1.0,
            dt,
        );
        let ph = self.phase;
        let mut j = vec![Joint::default(); self.n];
        let b = &self.bones;

        // Legs: on the floor, a straight sweep back (the foot stays put
        // while the body passes over it); off it, an eased swing forward
        // with the foot lifted.
        for &(leg, offset) in &self.legs {
            let psi = (ph + offset).fract();
            let (fwd, lift) = if psi < DUTY {
                (1.0 - 2.0 * psi / DUTY, 0.0)
            } else {
                let t = (psi - DUTY) / (1.0 - DUTY);
                let e = t * t * (3.0 - 2.0 * t);
                (-1.0 + 2.0 * e, (PI * t).sin())
            };
            j[leg] = Joint {
                rot: v3(-SWING * s * fwd, 0.0, 0.0),
                shift: v3(0.0, LIFT * s * lift, 0.0),
            };
        }

        // The spine: the chest rises over each planted leg and rolls onto
        // it, the hips roll and twist the other way.
        let bob = -BOB * s * (2.0 * TAU * ph).cos();
        let roll = ROLL * s * (TAU * ph).sin();
        j[0] = Joint {
            rot: v3(0.0, yaw, 0.0),
            shift: v3(0.0, bob, 0.0),
        };
        j[b.body] = Joint {
            rot: v3(0.0, -0.4 * TWIST * s * (TAU * ph).cos(), roll),
            shift: V3::ZERO,
        };
        j[b.hips] = Joint {
            rot: v3(0.0, TWIST * s * (TAU * ph).cos(), -2.0 * roll),
            shift: V3::ZERO,
        };
        // The head stays steadier than the body (a little nod, a beat
        // late) and turns toward you, more so at rest.
        let nod = 3.0 * s * (2.0 * TAU * ph - 0.9).sin();
        j[b.head] = Joint {
            rot: v3(nod, -yaw * (0.45 - 0.3 * s), -0.6 * roll),
            shift: V3::ZERO,
        };

        // Shakes for the soft parts: the chest's vertical acceleration, the
        // nod's, and speeding up or slowing down.
        let acc = (bob - 2.0 * self.bob[0] + self.bob[1]) / (dt * dt);
        self.bob = [bob, self.bob[0]];
        let nod_acc = (nod - 2.0 * self.nod[0] + self.nod[1]) / (dt * dt);
        self.nod = [nod, self.nod[0]];
        let pace_acc = (s - self.pace) / dt;
        self.pace = s;
        let yaw_acc = (self.yaw.v - self.yaw_v) / dt;
        self.yaw_v = self.yaw.v;

        let tail = self.tail.step(
            16.0 * s * (TAU * ph - 0.6).sin() + 6.0 * (1.0 - s) * (TAU * 0.4 * self.clock).sin(),
            -yaw_acc,
            2.6,
            0.3,
            dt,
        );
        j[b.tail] = Joint {
            rot: v3(0.0, tail, 0.0),
            shift: V3::ZERO,
        };
        if let Some(t) = b.tail_tip {
            let tip = self.tip.step(0.7 * tail, 0.0, 3.2, 0.25, dt);
            j[t] = Joint {
                rot: v3(0.0, tip, 0.0),
                shift: V3::ZERO,
            };
        }
        if let Some(i) = b.belly {
            let y = self.belly.step(0.0, -acc, 3.6, 0.16, dt);
            j[i].shift = v3(0.0, y.clamp(-0.12, 0.12), 0.0);
        }
        if let Some(i) = b.butt {
            let y = self.butt.step(0.0, -acc, 3.2, 0.16, dt);
            let z = self.butt_z.step(0.0, -0.6 * pace_acc, 3.0, 0.2, dt);
            j[i].shift = v3(0.0, y.clamp(-0.12, 0.12), z.clamp(-0.08, 0.08));
        }
        if let Some(i) = b.chin {
            let y = self.chin.step(0.0, -acc - 0.02 * nod_acc, 4.0, 0.18, dt);
            j[i].shift = v3(0.0, y.clamp(-0.09, 0.09), 0.0);
        }
        j
    }
}
