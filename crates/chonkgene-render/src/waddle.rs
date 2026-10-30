//! A two-legged walk for the birds' skinned meshes (chick, duckling, owl),
//! worked out frame by frame like the four-legged [`Walk`]: the legs take
//! turns, each planted foot keeping pace with the floor; the body rolls
//! onto the planted leg and rises over it, the head stays steadier, the
//! wings swing out a little with each step and the tail wags. Belly,
//! bottom and chin wobble on springs of their own.
//!
//! [`Mover`] is either walk, so the renderer need not care which it has.

use std::f32::consts::{PI, TAU};

use chonkgene_rig::math::{V3, v3};
use chonkgene_rig::{FLOOR, Model};

use crate::skin::{Joint, REST_YAW, SIT_YAW, Skeleton, Spring, WALK_YAW, Walk};

/// The pace the pet's window moves at, world units a second (60 px/s at
/// 100%): a planted foot slides back at this speed.
const PACE: f32 = 1.034;
/// The share of a step a foot is on the floor.
const DUTY: f32 = 0.55;
/// How far a sitting bird's legs fold forward (degrees) and how far it
/// sinks, world units.
const SIT_LEGS: f32 = 75.0;
const SIT_DROP: f32 = 0.1;
/// Body rise over a planted leg, world units; the body's slide over it.
const BOB: f32 = 0.03;
const SLIDE: f32 = 0.045;

/// Walks either way.
pub enum Mover {
    Quad(Box<Walk>),
    Biped(Box<Waddle>),
}

impl Mover {
    /// The walk that fits `sk`: a bird (a `leg_l` and a `leg_r`) waddles,
    /// anything on four legs walks.
    pub fn new(sk: &Skeleton, facing: f32, model: &Model) -> Result<Self, String> {
        if sk.bone("leg_l").is_some() {
            Waddle::new(sk, facing, model).map(|w| Self::Biped(Box::new(w)))
        } else {
            Walk::new(sk, facing).map(|w| Self::Quad(Box::new(w)))
        }
    }

    pub fn poke(&mut self, kick: f32) {
        match self {
            Self::Quad(w) => w.poke(kick),
            Self::Biped(w) => w.poke(kick),
        }
    }

    pub fn sit(&mut self, on: bool) {
        match self {
            Self::Quad(w) => w.sit(on),
            Self::Biped(w) => w.sit(on),
        }
    }

    pub fn look(&mut self, deg: f32) {
        match self {
            Self::Quad(w) => w.look(deg),
            Self::Biped(w) => w.look(deg),
        }
    }

    /// 0 standing … 1 full pace.
    pub fn speed(&self) -> f32 {
        match self {
            Self::Quad(w) => w.speed,
            Self::Biped(w) => w.speed,
        }
    }

    pub fn yaw(&self) -> f32 {
        match self {
            Self::Quad(w) => w.yaw(),
            Self::Biped(w) => w.yaw(),
        }
    }

    pub fn step(&mut self, dt: f32, walking: bool, facing: f32) -> Vec<Joint> {
        match self {
            Self::Quad(w) => w.step(dt, walking, facing),
            Self::Biped(w) => w.step(dt, walking, facing),
        }
    }
}

/// A two-legged waddle.
pub struct Waddle {
    phase: f32,
    /// 0 standing … 1 full pace.
    pub speed: f32,
    yaw: Spring,
    sitting: bool,
    sit: Spring,
    look: Spring,
    look_goal: f32,
    belly: Spring,
    butt: Spring,
    butt_x: Spring,
    chin: Spring,
    bob: [f32; 2],
    nod: [f32; 2],
    pace: f32,
    swish: f32,
    legs: [usize; 2],
    wings: [Option<usize>; 2],
    tail: Option<usize>,
    body: usize,
    head: usize,
    soft: [Option<usize>; 3],
    /// Leg swing (degrees), lift (world units), roll of the body (degrees),
    /// the head's nod (degrees), and steps a second at full pace.
    swing: f32,
    lift: f32,
    roll: f32,
    nod_amp: f32,
    hz: f32,
    n: usize,
}

impl Waddle {
    pub fn new(sk: &Skeleton, facing: f32, model: &Model) -> Result<Self, String> {
        let need = |n: &str| sk.bone(n).ok_or_else(|| format!("no bone {n:?}"));
        let legs = [need("leg_l")?, need("leg_r")?];
        let g = &model.gait;
        let leg_len = legs.iter().map(|&l| sk.pivots[l].y - FLOOR).sum::<f32>() / 2.0;
        // A foot slides back 2·L·sin(swing) while planted, for DUTY of a
        // cycle: pick the cycle rate that makes that the window's pace.
        let slide = 2.0 * leg_len.max(0.15) * g.swing.to_radians().sin();
        let hz = (PACE * DUTY / slide.max(0.05)).clamp(1.2, 3.2);
        Ok(Self {
            phase: 0.0,
            speed: 0.0,
            yaw: Spring {
                x: facing * REST_YAW,
                v: 0.0,
            },
            sitting: false,
            sit: Spring::default(),
            look: Spring::default(),
            look_goal: 0.0,
            belly: Spring::default(),
            butt: Spring::default(),
            butt_x: Spring::default(),
            chin: Spring::default(),
            bob: [0.0; 2],
            nod: [0.0; 2],
            pace: 0.0,
            swish: 0.0,
            legs,
            wings: [sk.bone("wing_l"), sk.bone("wing_r")],
            tail: sk.bone("tail"),
            body: need("body")?,
            head: need("head")?,
            soft: [sk.bone("belly"), sk.bone("butt"), sk.bone("chin")],
            swing: g.swing,
            lift: g.lift,
            roll: g.roll,
            nod_amp: g.nod,
            hz,
            n: sk.names.len(),
        })
    }

    /// Jolt the soft parts (a poke in the belly), world units a second.
    pub fn poke(&mut self, kick: f32) {
        self.belly.v += kick;
        self.butt.v += kick * 0.7;
        self.chin.v += kick * 0.6;
    }

    pub fn sit(&mut self, on: bool) {
        self.sitting = on;
    }

    pub fn look(&mut self, deg: f32) {
        self.look_goal = deg;
    }

    pub fn yaw(&self) -> f32 {
        self.yaw.x
    }

    /// Advance `dt` seconds, walking or standing, facing +1 (right) or −1
    /// (left); the joints for this frame.
    pub fn step(&mut self, dt: f32, walking: bool, facing: f32) -> Vec<Joint> {
        let dt = dt.max(1e-4);
        let goal = if walking { 1.0 } else { 0.0 };
        self.speed += (goal - self.speed).clamp(-dt * 4.0, dt * 4.0);
        let s = self.speed;
        if s > 0.0 {
            self.phase = (self.phase + dt * self.hz * (0.4 + 0.6 * s)).fract();
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
        let sit_goal = if self.sitting && !walking { 1.0 } else { 0.0 };
        let sit = self.sit.step(sit_goal, 0.0, 1.2, 1.0, dt).clamp(0.0, 1.0);
        let mut j = vec![Joint::default(); self.n];

        // Legs, in turn: on the floor a straight sweep back, off it an
        // eased swing forward with the foot lifted. Sitting folds them
        // forward under the body.
        for (k, &leg) in self.legs.iter().enumerate() {
            let psi = (ph + 0.5 * k as f32).fract();
            let (fwd, up) = if psi < DUTY {
                (1.0 - 2.0 * psi / DUTY, 0.0)
            } else {
                let t = (psi - DUTY) / (1.0 - DUTY);
                let e = t * t * (3.0 - 2.0 * t);
                (-1.0 + 2.0 * e, (PI * t).sin())
            };
            j[leg] = Joint {
                rot: v3(-self.swing * s * fwd - SIT_LEGS * sit, 0.0, 0.0),
                shift: v3(0.0, self.lift * s * up, 0.0),
            };
        }

        // The body: onto the planted leg (the left, +x, while ph is in the
        // first half), up over it twice a cycle, a little forward.
        let side = (TAU * ph).sin();
        let roll = -self.roll * s * side;
        let bob = BOB * s * (0.5 - 0.5 * (2.0 * TAU * ph).cos());
        j[0] = Joint {
            rot: v3(0.0, yaw, 0.0),
            shift: v3(0.0, -SIT_DROP * sit, 0.0),
        };
        j[self.body] = Joint {
            rot: v3(3.0 * s, 0.0, roll),
            shift: v3(SLIDE * s * side, bob, 0.0),
        };
        // The head stays steadier than the body, nodding a beat late, and
        // turns toward you, more so at rest.
        let nod = self.nod_amp * s * (2.0 * TAU * ph - 0.9).sin();
        let look = self.look.step(self.look_goal, 0.0, 1.6, 1.0, dt);
        j[self.head] = Joint {
            rot: v3(nod, -yaw * (0.45 - 0.3 * s) + look, -0.7 * roll),
            shift: V3::ZERO,
        };

        // Wings: out a little, more with each step, puffed up sitting.
        let flap = 5.0 + 12.0 * s * (0.5 + 0.5 * (2.0 * TAU * ph + 1.0).sin()) + 8.0 * sit;
        for (side, wing) in self.wings.iter().enumerate() {
            if let Some(w) = *wing {
                let sign = if side == 0 { 1.0 } else { -1.0 };
                j[w].rot = v3(0.0, 0.0, sign * flap);
            }
        }
        // The tail wags faster at pace, and bobs with the steps.
        self.swish = (self.swish + dt * (0.8 + 1.6 * s)).fract();
        if let Some(t) = self.tail {
            j[t].rot = v3(
                -3.0 * s * (2.0 * TAU * ph).sin(),
                0.0,
                (8.0 + 6.0 * s) * (TAU * self.swish).sin(),
            );
        }

        // Shakes for the soft parts: the body's vertical acceleration, the
        // nod's, and speeding up or slowing down.
        let acc = (bob - 2.0 * self.bob[0] + self.bob[1]) / (dt * dt);
        self.bob = [bob, self.bob[0]];
        let nod_acc = (nod - 2.0 * self.nod[0] + self.nod[1]) / (dt * dt);
        self.nod = [nod, self.nod[0]];
        let _pace_acc = (s - self.pace) / dt;
        self.pace = s;
        if let Some(i) = self.soft[0] {
            let y = self.belly.step(0.0, -acc, 3.6, 0.16, dt);
            j[i].shift = v3(0.0, y.clamp(-0.12, 0.12), 0.0);
        }
        if let Some(i) = self.soft[1] {
            let y = self.butt.step(0.0, -acc, 3.2, 0.16, dt);
            let x = self
                .butt_x
                .step(0.05 * s * (TAU * ph).sin(), 0.0, 2.6, 0.15, dt);
            j[i].shift = v3(x.clamp(-0.1, 0.1), y.clamp(-0.12, 0.12), 0.0);
        }
        if let Some(i) = self.soft[2] {
            let y = self.chin.step(0.0, -acc - 0.02 * nod_acc, 4.0, 0.18, dt);
            j[i].shift = v3(0.0, y.clamp(-0.09, 0.09), 0.0);
        }
        j
    }
}
