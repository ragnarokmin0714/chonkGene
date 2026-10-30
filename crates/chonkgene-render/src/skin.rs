//! Skinned-mesh spike: the skeleton a baked pet mesh hangs on, and a
//! four-legged walk worked out every frame. The legs step one at a time
//! (left hind, left fore, right hind, right fore) with each planted foot
//! keeping pace with the floor, the chest and hips rock against each other,
//! tails sway like seaweed as a wave runs down their segments, and the
//! belly, bottom and chin wobble on springs of their own.

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

/// Four-legged models for the spike, by species id.
pub const QUAD_SPECIES: [&str; 4] = ["shiba", "fox", "munchkin", "fold"];

/// A four-legged model (`shiba`, `fox`, `munchkin`, `fold`) and its skeleton.
pub fn quad(id: &str) -> Result<(Model, Skeleton), String> {
    let src = match id {
        "shiba" => include_str!("../../../assets/species/shiba/quad.ron"),
        "fox" => include_str!("../../../assets/species/fox/quad.ron"),
        "munchkin" => include_str!("../../../assets/species/munchkin/quad.ron"),
        "fold" => include_str!("../../../assets/species/fold/quad.ron"),
        _ => return Err(format!("no four-legged model {id:?}")),
    };
    let model = Model::parse(src, builtin::POSES).map_err(|e| e.to_string())?;
    let sk = Skeleton::new(&model);
    Ok((model, sk))
}

/// Where the whole pet turns: the floor under the middle of its body.
pub const ROOT_PIVOT: V3 = v3(0.0, FLOOR, -0.1);

#[derive(Clone)]
pub struct Skeleton {
    pub names: Vec<String>,
    pub parents: Vec<Option<usize>>,
    pub pivots: Vec<V3>,
    /// Parents before children.
    order: Vec<usize>,
    /// Jiggle bone, centre, radius, share (see [`chonkgene_rig::Jiggle`]).
    pub jiggles: Vec<(usize, V3, f32, f32)>,
}

impl Skeleton {
    /// The model's bones and pivots, then a bone for each of its jiggles.
    pub fn new(model: &Model) -> Self {
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
        let mut jiggles = Vec::new();
        for j in &model.jiggles {
            names.push(j.name.clone());
            parents.push(Some(j.parent));
            pivots.push(j.at);
            order.push(names.len() - 1);
            jiggles.push((names.len() - 1, j.at, j.radius, j.share));
        }
        Self {
            names,
            parents,
            pivots,
            order,
            jiggles,
        }
    }

    /// Tails as chains of bones, base first: each starts at a bone named
    /// `tail…` hanging from a bone that is not, and runs on through its one
    /// `tail…` child.
    pub fn tails(&self) -> Vec<Vec<usize>> {
        let is_tail = |b: usize| self.names[b].starts_with("tail");
        let child =
            |b: usize| (0..self.names.len()).find(|&c| self.parents[c] == Some(b) && is_tail(c));
        (0..self.names.len())
            .filter(|&b| is_tail(b) && !self.parents[b].is_some_and(is_tail))
            .map(|base| {
                let mut chain = vec![base];
                while let Some(c) = child(*chain.last().unwrap_or(&base)) {
                    chain.push(c);
                }
                chain
            })
            .collect()
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
/// How long the legs are the swing is set for (floor to hip pivot, world
/// units; the Shiba's): shorter legs swing wider to keep pace with the
/// window, longer ones narrower.
const LEG_REF: f32 = 0.59;
const DUTY: f32 = 0.62;
const LIFT: f32 = 0.07;
/// Where each leg is in the step cycle (a walk, one foot at a time), and
/// whether it is a front leg.
const LEGS: [(&str, f32, bool); 4] = [
    ("leg_bl", 0.0, false),
    ("leg_fl", 0.25, true),
    ("leg_br", 0.5, false),
    ("leg_fr", 0.75, true),
];
/// The chest's rise and fall, world units; twice a step cycle.
const BOB: f32 = 0.02;
const ROLL: f32 = 3.0;
const TWIST: f32 = 4.0;
/// How far the pet turns side-on while walking, and at rest (toward you).
const WALK_YAW: f32 = 72.0;
const REST_YAW: f32 = 28.0;
/// Sitting, it turns almost to face you, so the tails fanned out behind it
/// show on both sides of its head.
const SIT_YAW: f32 = 8.0;

/// Sitting: the body tips back this far (degrees) about the front paws,
/// which brings the bottom down onto the floor; the hind legs fold
/// forward under it.
const SIT_PITCH: f32 = 24.0;
const SIT_HIND: f32 = -68.0;
/// Where the front paws stand, the point the body tips back about.
const FRONT_PAWS: V3 = v3(0.0, FLOOR, 0.5);
/// Sitting, several tails open behind the pet like a fan, so all of them
/// show: spread up to this far either side of straight up (degrees), and
/// leaning back this much. A single tail just keeps its place against the
/// body tipping back.
const SIT_FAN: f32 = 95.0;
const SIT_FAN_BACK: f32 = 0.35;
/// The fan leaves a gap this wide either side of straight up, where the
/// head and ears would hide a tail.
const SIT_FAN_GAP: f32 = 24.0;
/// The bottom's waddle side to side at full pace, world units, and the
/// bounce it gets plopping down into a sit (and a smaller one getting up),
/// world units a second.
const BUTT_WADDLE: f32 = 0.05;
const BUTT_PLOP: f32 = 1.8;

/// Tail swish: how far each segment bends against the one before, degrees,
/// standing and walking. The bend runs down the tail as a wave, each
/// segment `WAVE_LAG` radians behind the one before, so a tail sways like
/// seaweed in a current rather than swinging stiff from its root.
const SWISH_REST: f32 = 12.0;
const SWISH_WALK: f32 = 13.0;
const WAVE_LAG: f32 = 0.9;
/// A long tail's wave still spans this many radians from root to tip, so
/// a tail of many short segments ripples no more than one of a few.
const WAVE_SPAN: f32 = 7.5;
/// Swishes a second, standing and at full pace.
const SWISH_HZ: (f32, f32) = (0.6, 1.6);

/// A tail: its bones from the base out, with a sideways swish and an
/// up-and-down bounce spring for each, and where in the swish it starts.
struct Tail {
    bones: Vec<usize>,
    swish: Vec<Spring>,
    bounce: Vec<Spring>,
    /// The base's turn about the vertical: flung out by the body turning.
    fling: Spring,
    offset: f32,
    /// The turn (degrees, as [`M3::euler`]) that takes the root from where
    /// it rests to its place in the fan, sitting.
    sit_turn: V3,
}

/// A four-legged walk, worked out frame by frame.
pub struct Walk {
    phase: f32,
    /// 0 standing … 1 full pace.
    pub speed: f32,
    yaw: Spring,
    /// Where the tails are in their swish, cycles.
    swish: f32,
    tails: Vec<Tail>,
    belly: Spring,
    butt: Spring,
    butt_z: Spring,
    butt_x: Spring,
    chin: Spring,
    /// The chest's height the last two frames, and the pace last frame, for
    /// the shakes they give the soft parts.
    bob: [f32; 2],
    nod: [f32; 2],
    pace: f32,
    /// The turn's speed last frame: turning flings the tails the other way.
    yaw_v: f32,
    /// Asked to sit, and how far down it is (0 standing … 1 sitting).
    sitting: bool,
    /// The head's extra turn, and where it is heading.
    look: Spring,
    look_goal: f32,
    sit: Spring,
    legs: [(usize, f32, bool); 4],
    /// Leg swing either way at full pace, degrees: [`SWING`] for the legs
    /// of [`LEG_REF`], wider for shorter ones.
    swing: f32,
    bones: Bones,
    n: usize,
}

struct Bones {
    body: usize,
    hips: usize,
    head: usize,
    belly: Option<usize>,
    butt: Option<usize>,
    chin: Option<usize>,
}

impl Walk {
    pub fn new(sk: &Skeleton, facing: f32) -> Result<Self, String> {
        let need = |n: &str| sk.bone(n).ok_or_else(|| format!("no bone {n:?}"));
        let mut legs = [(0, 0.0, false); 4];
        for (l, (name, offset, front)) in legs.iter_mut().zip(LEGS) {
            *l = (need(name)?, offset, front);
        }
        let leg_len = legs.iter().map(|l| sk.pivots[l.0].y - FLOOR).sum::<f32>() / 4.0;
        let swing = SWING * (LEG_REF / leg_len.max(0.2)).clamp(0.8, 2.0);
        // Several tails swish out of step, so they ripple rather than
        // sway as one.
        let chains = sk.tails();
        let turns = fan(sk, &chains);
        let tails = chains
            .into_iter()
            .zip(turns)
            .enumerate()
            .map(|(i, (bones, sit_turn))| Tail {
                swish: vec![Spring::default(); bones.len()],
                bounce: vec![Spring::default(); bones.len()],
                bones,
                fling: Spring::default(),
                offset: i as f32 * 0.37,
                sit_turn,
            })
            .collect();
        Ok(Self {
            phase: 0.0,
            speed: 0.0,
            yaw: Spring {
                x: facing * REST_YAW,
                v: 0.0,
            },
            swish: 0.0,
            tails,
            belly: Spring::default(),
            butt: Spring::default(),
            butt_z: Spring::default(),
            butt_x: Spring::default(),
            chin: Spring::default(),
            bob: [0.0; 2],
            nod: [0.0; 2],
            pace: 0.0,
            yaw_v: 0.0,
            sitting: false,
            look: Spring::default(),
            look_goal: 0.0,
            sit: Spring::default(),
            legs,
            swing,
            bones: Bones {
                body: need("body")?,
                hips: need("hips")?,
                head: need("head")?,
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

    /// Sit down (or get up); walking gets it up first.
    pub fn sit(&mut self, on: bool) {
        self.sitting = on;
    }

    /// Turn the head this far (degrees, + to the pet's left) on top of
    /// what the walk does with it; it gets there smoothly.
    pub fn look(&mut self, deg: f32) {
        self.look_goal = deg;
    }

    /// How far down into a sit, 0 standing … 1 sitting.
    pub fn sat(&self) -> f32 {
        self.sit.x.clamp(0.0, 1.0)
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
        let goal = if walking { 1.0 } else { 0.0 };
        self.speed += (goal - self.speed).clamp(-dt * 4.0, dt * 4.0);
        let s = self.speed;
        if s > 0.0 {
            self.phase = (self.phase + dt * STEP_HZ * (0.4 + 0.6 * s)).fract();
        }
        let yaw = self.yaw.step(
            facing
                * if walking {
                    WALK_YAW
                } else if self.sitting {
                    SIT_YAW
                } else {
                    REST_YAW
                },
            0.0,
            1.3,
            1.0,
            dt,
        );
        let ph = self.phase;
        // How far into a sit, worked out first: the tails and the bottom
        // need it too.
        let was_sat = self.sit.x;
        let goal = if self.sitting && !walking { 1.0 } else { 0.0 };
        let sit = self.sit.step(goal, 0.0, 1.2, 1.0, dt).clamp(0.0, 1.0);
        let mut j = vec![Joint::default(); self.n];
        let b = &self.bones;

        // Legs: on the floor, a straight sweep back (the foot stays put
        // while the body passes over it); off it, an eased swing forward
        // with the foot lifted.
        for &(leg, offset, _) in &self.legs {
            let psi = (ph + offset).fract();
            let (fwd, lift) = if psi < DUTY {
                (1.0 - 2.0 * psi / DUTY, 0.0)
            } else {
                let t = (psi - DUTY) / (1.0 - DUTY);
                let e = t * t * (3.0 - 2.0 * t);
                (-1.0 + 2.0 * e, (PI * t).sin())
            };
            j[leg] = Joint {
                rot: v3(-self.swing * s * fwd, 0.0, 0.0),
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
        let look = self.look.step(self.look_goal, 0.0, 1.6, 1.0, dt);
        j[b.head] = Joint {
            rot: v3(nod, -yaw * (0.45 - 0.3 * s) + look, -0.6 * roll),
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

        // Tails: a wave runs from root to tip, faster and wider at pace;
        // the root bounces with the steps, and turning flings it out.
        self.swish = (self.swish + dt * (SWISH_HZ.0 + (SWISH_HZ.1 - SWISH_HZ.0) * s)).fract();
        let amp = SWISH_REST + (SWISH_WALK - SWISH_REST) * s;
        for tail in &mut self.tails {
            let fling = tail.fling.step(0.0, -0.5 * yaw_acc, 1.8, 0.35, dt);
            let at = TAU * (self.swish + tail.offset);
            // The bend is shared out over the segments, so a long tail (a
            // Shiba's seven-segment ring) bends no more in all than a short
            // one and keeps its shape.
            let amp = amp * 3.0 / tail.bones.len().max(3) as f32;
            for (k, &bone) in tail.bones.iter().enumerate() {
                let lag = WAVE_LAG.min(WAVE_SPAN / tail.bones.len() as f32) * k as f32;
                // Sideways, the travelling wave; up and down, a smaller one
                // a quarter behind, plus a bounce with the steps at the root.
                let sw_target = amp * (at - lag).sin();
                let mut bo_target = 0.4 * amp * (at - lag - 1.6).sin();
                if k == 0 {
                    bo_target += 5.0 * s * (2.0 * TAU * ph - 0.8).sin();
                }
                // Springs smooth the wave as the pace (and so its speed and
                // size) changes.
                let sw = tail.swish[k].step(sw_target, 0.0, 3.0, 0.5, dt);
                let bo = tail.bounce[k].step(bo_target, 0.0, 3.0, 0.5, dt);
                j[bone] = Joint {
                    rot: v3(bo, if k == 0 { fling } else { 0.0 }, sw),
                    shift: V3::ZERO,
                };
                if k == 0 {
                    j[bone].rot = j[bone].rot + tail.sit_turn * sit;
                }
            }
        }
        if let Some(i) = b.belly {
            let y = self.belly.step(0.0, -acc, 3.6, 0.16, dt);
            j[i].shift = v3(0.0, y.clamp(-0.12, 0.12), 0.0);
        }
        if let Some(i) = b.butt {
            // Plopping down into a sit squashes the bottom, and it bounces;
            // getting up gives it a smaller one.
            if was_sat < 0.85 && sit >= 0.85 {
                self.butt.v -= BUTT_PLOP;
            } else if was_sat > 0.5 && sit <= 0.5 {
                self.butt.v += 0.5 * BUTT_PLOP;
            }
            let y = self.butt.step(0.0, -acc, 3.2, 0.16, dt);
            let z = self.butt_z.step(0.0, -0.6 * pace_acc, 3.0, 0.2, dt);
            // Walking, it waddles side to side a beat behind the hips.
            let x = self
                .butt_x
                .step(BUTT_WADDLE * s * (TAU * ph).sin(), 0.0, 2.6, 0.15, dt);
            j[i].shift = v3(
                x.clamp(-0.1, 0.1),
                y.clamp(-0.12, 0.12),
                z.clamp(-0.08, 0.08),
            );
        }
        if let Some(i) = b.chin {
            let y = self.chin.step(0.0, -acc - 0.02 * nod_acc, 4.0, 0.18, dt);
            j[i].shift = v3(0.0, y.clamp(-0.09, 0.09), 0.0);
        }

        // Sitting: the body tips back about the front paws until the
        // bottom is on the floor, so the paws stay where they stand (the
        // front legs lean back a little, turning only a third of the way
        // back to upright); the hind legs fold forward under the body, and
        // the head lifts to look up. Tipping about the paws (before the
        // turn, so the pet does not swing round them) is a tip about the
        // root plus a shift.
        if sit > 0.0 {
            let pitch = -SIT_PITCH * sit;
            j[0].rot.x += pitch;
            let d = FRONT_PAWS - ROOT_PIVOT;
            let tipped = d - M3::rot_x(pitch.to_radians()).apply(d);
            j[0].shift = j[0].shift + M3::rot_y(yaw.to_radians()).apply(tipped);
            for &(leg, _, front) in &self.legs {
                j[leg].rot.x += if front {
                    0.3 * SIT_PITCH * sit
                } else {
                    SIT_HIND * sit
                };
            }
            j[b.head].rot.x += 0.85 * SIT_PITCH * sit;
        }
        j
    }
}

/// Where each tail's root turns to when the pet sits (see [`SIT_FAN`]),
/// degrees as [`M3::euler`], in the hips' frame. Several tails take places
/// in a fan in the order they lie left to right, so none cross; one tail
/// only rises against the body tipping back.
fn fan(sk: &Skeleton, chains: &[Vec<usize>]) -> Vec<V3> {
    let dirs: Vec<V3> = chains
        .iter()
        .map(|c| match c[..] {
            [a, b, ..] => (sk.pivots[b] - sk.pivots[a]).normalize(),
            _ => v3(0.0, 1.0, 0.0),
        })
        .collect();
    if chains.len() < 2 {
        return vec![v3(SIT_PITCH, 0.0, 0.0); chains.len()];
    }
    let mut order: Vec<usize> = (0..chains.len()).collect();
    order.sort_by(|&a, &b| {
        dirs[a]
            .x
            .atan2(dirs[a].y)
            .total_cmp(&dirs[b].x.atan2(dirs[b].y))
    });
    let mut turns = vec![V3::ZERO; chains.len()];
    // Evenly along the two arcs either side of the gap.
    let arc = SIT_FAN - SIT_FAN_GAP;
    for (slot, &t) in order.iter().enumerate() {
        let along = 2.0 * arc * slot as f32 / (chains.len() - 1) as f32;
        let across = if along <= arc {
            -SIT_FAN + along
        } else {
            SIT_FAN_GAP + along - arc
        };
        let (s, c) = across.to_radians().sin_cos();
        // Where it should point with the body tipped back, in the body's
        // own frame: tipped forward by as much.
        let world = v3(s, c, -SIT_FAN_BACK).normalize();
        let want = M3::rot_x(SIT_PITCH.to_radians()).apply(world);
        turns[t] = aim(dirs[t], want);
    }
    turns
}

fn cross(a: V3, b: V3) -> V3 {
    v3(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// The turn (degrees, as [`M3::euler`]) that points `from` along `to`:
/// the shortest turn between them (Rodrigues), read back as Euler angles.
fn aim(from: V3, to: V3) -> V3 {
    let (from, to) = (from.normalize(), to.normalize());
    let axis = cross(from, to);
    let (sin, cos) = (axis.length(), from.dot(to));
    let k = if sin > 1e-6 {
        axis / sin
    } else if cos > 0.0 {
        return V3::ZERO;
    } else {
        // Opposite ways: half a turn about any axis square to `from`.
        let other = if from.x.abs() < 0.9 {
            v3(1.0, 0.0, 0.0)
        } else {
            v3(0.0, 1.0, 0.0)
        };
        cross(from, other).normalize()
    };
    let kk = |i: usize, j: usize| [k.x, k.y, k.z][i] * [k.x, k.y, k.z][j];
    let skew = [[0.0, -k.z, k.y], [k.z, 0.0, -k.x], [-k.y, k.x, 0.0]];
    let m = |i: usize, j: usize| {
        let id = if i == j { 1.0 } else { 0.0 };
        id * cos + sin * skew[i][j] + (1.0 - cos) * kk(i, j)
    };
    // M3::euler is Ry · Rx · Rz: m12 = −sin x, (m10, m11) ∝ (sin z, cos z),
    // (m02, m22) ∝ (sin y, cos y).
    let x = (-m(1, 2)).clamp(-1.0, 1.0).asin();
    let z = m(1, 0).atan2(m(1, 1));
    let y = m(0, 2).atan2(m(2, 2));
    v3(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aim_turns_any_direction_onto_any_other() {
        // Straight back onto sideways is the case two angles cannot reach.
        let dirs = [
            v3(0.0, 0.0, -1.0),
            v3(1.0, 0.0, 0.0),
            v3(0.0, 1.0, 0.0),
            v3(0.16, -0.01, -0.99),
            v3(-0.86, 0.24, -0.46),
            v3(0.7, 0.62, 0.36),
        ];
        for from in dirs {
            for to in dirs.iter().map(|d| v3(d.z, d.x, -d.y)).chain(dirs) {
                let got = M3::euler(aim(from, to)).apply(from.normalize());
                let miss = (got - to.normalize()).length();
                assert!(miss < 1e-3, "{from:?} → {to:?} landed {got:?}");
            }
        }
    }
}
