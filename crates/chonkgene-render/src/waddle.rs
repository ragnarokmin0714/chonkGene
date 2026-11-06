//! A two-legged walk for the birds' skinned meshes (chick, duckling, owl),
//! worked out frame by frame like the four-legged [`Walk`]: the legs take
//! turns, each planted foot keeping pace with the floor; the body rolls
//! onto the planted leg and rises over it, the head stays steadier, the
//! wings swing out a little with each step and the tail wags. Belly,
//! bottom and chin wobble on springs of their own.
//!
//! [`Mover`] is either walk, so the renderer need not care which it has.

use std::f32::consts::{PI, TAU};

use chonkgene_core::anim::Act;

use chonkgene_rig::math::v3;
use chonkgene_rig::{FLOOR, Model};

use chonkgene_core::Zone;

use crate::fidget::{Fidgets, Kind, Out};
use crate::skin::{
    ATTEND_HZ, ATTEND_YAW, Feel, Gesture, Joint, Levels, MAX_SWING, Mood, Moods, REST_YAW, SIT_YAW,
    SPEED_UP, STILL_GOING, SULK_BACK, SULK_YAW, Skeleton, SleepPose, Spring, TURN_HZ, WALK_YAW,
    Walk, bite, peck, tempo_split,
};
use crate::skin::{drift, hold};

/// The pace the pet's window moves at, world units a second (60 px/s at
/// 100%): a planted foot slides back at this speed.
const PACE: f32 = 1.034;
/// The share of a step a foot is on the floor.
const DUTY: f32 = 0.55;
/// Touched, the share of a turn or tip toward the finger the head takes
/// on its own; the body takes the rest.
const TOUCH_NECK: f32 = 0.5;
/// How far a sitting bird's legs fold forward (degrees) and how far it
/// sinks, world units.
const SIT_LEGS: f32 = 75.0;
const SIT_DROP: f32 = 0.1;
/// A bird that tucks its head in to sleep (`Gait::tuck`): the head sinks
/// into its puffed-up feathers, turns this far toward a shoulder (degrees)
/// and leans over onto it, chin down; the body sinks this much further,
/// hiding the feet.
const TUCK_TURN: f32 = 26.0;
const TUCK_LEAN: f32 = 26.0;
const TUCK_DROP: f32 = 0.08;
/// Body rise over a planted leg, world units; the body's slide over it.
const BOB: f32 = 0.03;
/// How far a swimming bird sinks, world units: its feet and the bottom of
/// its body under the water.
pub const SWIM_SINK: f32 = 0.2;
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
            let mut w = Walk::new(sk, facing)?;
            w.set_species(&model.species);
            Ok(Self::Quad(Box::new(w)))
        }
    }

    pub fn poke(&mut self, kick: f32) {
        match self {
            Self::Quad(w) => w.poke(kick),
            Self::Biped(w) => w.poke(kick),
        }
    }

    /// How lively the pet is (0 … 1) and a seed of its own (see
    /// [`Walk::set_fidgets`]).
    pub fn set_fidgets(&mut self, species: &str, energy: f32, seed: u32) {
        match self {
            Self::Quad(w) => w.set_fidgets(species, energy, seed),
            Self::Biped(w) => {
                w.fidgets = Fidgets::new(species, seed);
                w.fidgets.set_energy(energy);
            }
        }
    }

    /// Pleased or at play (`lively`), out of breath (`puffed`): set every
    /// frame (see [`Fidgets::set_mood`]).
    pub fn set_mood(&mut self, lively: bool, puffed: bool) {
        match self {
            Self::Quad(w) => w.set_mood(lively, puffed),
            Self::Biped(w) => w.fidgets.set_mood(lively, puffed),
        }
    }

    /// A lick of the nose now (four-legged pets).
    pub fn lick(&mut self) {
        if let Self::Quad(w) = self {
            w.lick();
        }
    }

    /// No fidgets: poses can be compared (tests, looks of one pose).
    pub fn still(&mut self) {
        match self {
            Self::Quad(w) => w.still(),
            Self::Biped(w) => w.fidgets.stop(),
        }
    }

    /// Where it is in its breath, −1 … 1 (set every frame).
    pub fn breathe(&mut self, b: f32) {
        match self {
            Self::Quad(w) => w.breathe(b),
            Self::Biped(w) => w.breath = b,
        }
    }

    /// The fidgets this frame.
    pub fn fidget(&self) -> &Out {
        match self {
            Self::Quad(w) => w.fidget(),
            Self::Biped(w) => &w.fidget,
        }
    }

    /// How it feels and where it is touched (set every frame).
    pub fn feel(&mut self, feel: Feel) {
        match self {
            Self::Quad(w) => w.feel(feel),
            Self::Biped(w) => w.feel(feel),
        }
    }

    /// The pose to sleep in (birds always tuck in, or stand: the owl).
    pub fn sleep_pose(&mut self, pose: SleepPose) {
        if let Self::Quad(w) = self {
            w.sleep_pose(pose);
        }
    }

    /// How far to settle the pet onto the floor, and flatten it, this
    /// frame (see [`Walk::ground`]).
    pub fn ground(&self) -> f32 {
        match self {
            Self::Quad(w) => w.ground(),
            Self::Biped(w) => w.ground,
        }
    }

    pub fn flatten(&self) -> f32 {
        match self {
            Self::Quad(w) => w.flatten(),
            Self::Biped(w) => w.flatten,
        }
    }

    /// Lie down to sleep (or get up).
    pub fn lie(&mut self, on: bool) {
        match self {
            Self::Quad(w) => w.lie(on),
            Self::Biped(w) => w.lie(on),
        }
    }

    pub fn sit(&mut self, on: bool) {
        match self {
            Self::Quad(w) => w.sit(on),
            Self::Biped(w) => w.sit(on),
        }
    }

    /// Lie down awake (趴下, a still mouse); a bird sits.
    pub fn rest(&mut self, on: bool) {
        match self {
            Self::Quad(w) => w.rest(on),
            Self::Biped(w) => w.rest(on),
        }
    }

    /// Fly (birds only; the rest have no wings to).
    pub fn fly(&mut self, on: bool) {
        if let Self::Biped(w) = self {
            w.fly(on);
        }
    }

    /// Swim (birds only).
    pub fn swim(&mut self, on: bool) {
        if let Self::Biped(w) = self {
            w.swim(on);
        }
    }

    pub fn look(&mut self, deg: f32) {
        match self {
            Self::Quad(w) => w.look(deg),
            Self::Biped(w) => w.look(deg),
        }
    }

    /// Walk this much faster or slower than usual (see [`Walk::set_pace`]).
    pub fn set_pace(&mut self, pace: f32) {
        match self {
            Self::Quad(w) => w.set_pace(pace),
            Self::Biped(w) => w.set_pace(pace),
        }
    }

    /// The idle gesture under way, if any (set every frame).
    pub fn gesture(&mut self, g: Option<Gesture>) {
        match self {
            Self::Quad(w) => w.gesture(g),
            Self::Biped(w) => w.gesture(g),
        }
    }

    /// 0 standing … 1 full pace.
    pub fn speed(&self) -> f32 {
        match self {
            Self::Quad(w) => w.speed,
            Self::Biped(w) => w.speed,
        }
    }

    /// Still on its way somewhere (see [`Walk::busy`]).
    pub fn busy(&self) -> bool {
        match self {
            Self::Quad(w) => w.busy(),
            Self::Biped(w) => w.busy(),
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
    lying: bool,
    lie: Spring,
    /// Sitting down awake for a while (趴下, a still mouse): no tuck.
    resting: bool,
    /// In the air, and how far into the flying pose; on the water, and how
    /// far down in it.
    flying: bool,
    fly: Spring,
    swimming: bool,
    swim: Spring,
    gesture: Option<Gesture>,
    tempo: f32,
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
    /// See [`chonkgene_rig::Gait::neck`] and [`chonkgene_rig::Gait::tuck`].
    neck: f32,
    tuck: f32,
    n: usize,
    /// Still on its way somewhere (walking, turning, sitting down, getting
    /// up, a gesture): it needs smooth frames.
    busy: bool,
    feel: Feel,
    moods: Moods,
    /// A flick of the tail (degrees about z).
    flick: Spring,
    /// Out: see [`Walk::ground`].
    ground: f32,
    flatten: f32,
    /// The little movements under every held pose, and this frame's.
    fidgets: Fidgets,
    fidget: Out,
    /// −1 … 1 over a breath: the belly rises and falls with it.
    breath: f32,
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
            lying: false,
            lie: Spring::default(),
            resting: false,
            flying: false,
            fly: Spring::default(),
            swimming: false,
            swim: Spring::default(),
            gesture: None,
            tempo: 1.0,
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
            neck: g.neck,
            tuck: g.tuck,
            n: sk.names.len(),
            busy: false,
            feel: Feel::default(),
            moods: Moods::default(),
            flick: Spring::default(),
            ground: 0.0,
            flatten: 0.0,
            fidgets: Fidgets::new(&model.species, 1),
            fidget: Out::default(),
            breath: 0.0,
        })
    }

    pub fn feel(&mut self, feel: Feel) {
        self.feel = feel;
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

    pub fn lie(&mut self, on: bool) {
        self.lying = on;
    }

    /// Settle down awake: a bird just sits, head up.
    pub fn rest(&mut self, on: bool) {
        self.resting = on;
    }

    /// Fly: wings beating, feet tucked up, side on to where it goes.
    pub fn fly(&mut self, on: bool) {
        self.flying = on;
    }

    /// Swim: low in the water, paddling, bobbing on it.
    pub fn swim(&mut self, on: bool) {
        self.swimming = on;
    }

    pub fn look(&mut self, deg: f32) {
        self.look_goal = deg;
    }

    pub fn gesture(&mut self, g: Option<Gesture>) {
        self.gesture = g;
    }

    pub fn set_pace(&mut self, pace: f32) {
        self.tempo = pace.clamp(0.2, 5.0);
    }

    pub fn yaw(&self) -> f32 {
        self.yaw.x
    }

    /// Still on its way somewhere: walking, turning, sitting down or
    /// getting up, or in a gesture.
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// Advance `dt` seconds, walking or standing, facing +1 (right) or −1
    /// (left); the joints for this frame.
    pub fn step(&mut self, dt: f32, walking: bool, facing: f32) -> Vec<Joint> {
        let dt = dt.max(1e-4);
        let goal = if walking { 1.0 } else { 0.0 };
        let ramp = dt * SPEED_UP * self.tempo.max(0.2);
        self.speed += (goal - self.speed).clamp(-ramp, ramp);
        let s = self.speed;
        let (quicker, longer) = tempo_split(self.tempo);
        if s > 0.0 {
            self.phase = (self.phase + dt * self.hz * (0.4 + 0.6 * s) * quicker).fract();
        }
        let going = walking || s > STILL_GOING;
        let eating = self.gesture.is_some_and(|g| g.act == Act::Eat);
        let lv = self.moods.step(&self.feel, dt);
        let f = self
            .fidgets
            .step(dt, hold(going, self.gesture.is_some()), self.lying);
        self.fidget = f;
        // Touched, it turns to face you; a tail or a bottom touched, it only
        // looks back over its shoulder (turning round would hide them).
        let touched = self.feel.gaze.is_some() && !self.feel.behind();
        let yaw = self.yaw.step(
            facing
                * if going || self.flying {
                    WALK_YAW
                } else if self.feel.mood == Mood::Sulk {
                    SULK_YAW
                } else if touched {
                    ATTEND_YAW
                } else if self.sitting || self.lying || eating {
                    SIT_YAW
                } else {
                    REST_YAW
                },
            0.0,
            if going {
                TURN_HZ.1
            } else if touched {
                ATTEND_HZ
            } else {
                TURN_HZ.0
            },
            1.0,
            dt,
        );
        let ph = self.phase;
        // Asleep, a bird settles down just as it sits, and tucks its head.
        let up_hz = if walking { 2.5 } else { 1.2 };
        // Laughing, melted or sulking, a bird sits down for it.
        let moody = matches!(self.feel.mood, Mood::Laugh | Mood::FedUp | Mood::Sulk);
        let sit_goal = if (self.sitting || self.lying || self.resting || moody)
            && !walking
            && !self.flying
            && !self.swimming
        {
            1.0
        } else {
            0.0
        };
        let sit = self.sit.step(sit_goal, 0.0, up_hz, 1.0, dt).clamp(0.0, 1.0);
        let lie_goal = if self.lying && !walking { 1.0 } else { 0.0 };
        let lie = self
            .lie
            .step(lie_goal, 0.0, up_hz * 0.7, 1.0, dt)
            .clamp(0.0, 1.0);
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
                rot: v3(
                    -(self.swing * longer).min(MAX_SWING) * s * fwd - SIT_LEGS * sit,
                    0.0,
                    0.0,
                ),
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
            shift: v3(0.0, -(SIT_DROP + TUCK_DROP * self.tuck * lie) * sit, 0.0),
        };
        j[self.body] = Joint {
            rot: v3(3.0 * s, 0.0, roll),
            shift: v3(SLIDE * s * side, bob, 0.0),
        };
        // The head stays steadier than the body, nodding a beat late, and
        // turns toward you, more so at rest. A bird with no neck to speak
        // of (the owl) turns its body for most of it.
        let nod = self.nod_amp * s * (2.0 * TAU * ph - 0.9).sin();
        let look = self.look.step(self.look_goal, 0.0, 1.6, 1.0, dt);
        let turn = -yaw * (0.45 - 0.3 * s) + look;
        j[self.body].rot.y += (1.0 - self.neck) * turn;
        // Asleep, a bird that tucks in sinks its head into its feathers
        // and lays it over on a shoulder (the one away from you); the rest
        // just droop.
        let tuck = self.tuck * lie;
        j[self.head] = Joint {
            rot: v3(
                nod + 18.0 * lie + 6.0 * tuck,
                self.neck * turn - facing * TUCK_TURN * tuck,
                -0.7 * roll + facing * TUCK_LEAN * tuck,
            ),
            shift: v3(0.0, -0.05 * lie - 0.1 * tuck, -0.04 * tuck),
        };

        // Wings: out a little, more with each step, puffed up sitting.
        // Asleep, the wings fold in close.
        let flap = (5.0 + 12.0 * s * (0.5 + 0.5 * (2.0 * TAU * ph + 1.0).sin()) + 8.0 * sit)
            * (1.0 - 0.8 * lie);
        for (side, wing) in self.wings.iter().enumerate() {
            if let Some(w) = *wing {
                let sign = if side == 0 { 1.0 } else { -1.0 };
                j[w].rot = v3(0.0, 0.0, sign * flap);
            }
        }
        // Tucked in, the wing the head lies on comes up over it a little.
        if tuck > 0.0 {
            let (k, sign) = if facing > 0.0 { (1, -1.0) } else { (0, 1.0) };
            if let Some(w) = self.wings[k] {
                j[w].rot.z += sign * 18.0 * tuck;
                j[w].rot.x -= 10.0 * tuck;
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
            // The belly rises and falls with each breath.
            let y = self.belly.step(-0.03 * self.breath, -acc, 3.6, 0.16, dt);
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
        // Some gestures are done sitting (see `Act::seated`); the rest
        // standing.
        if let Some(g) = self
            .gesture
            .filter(|g| s < 0.5 && (sit < 0.5 || g.act.seated()))
        {
            let e = g.e;
            match g.act {
                // Wings spread wide and the chest out: a stretch.
                Act::Stretch => {
                    j[self.body].rot.x -= 6.0 * e;
                    j[self.head].rot.x -= 10.0 * e;
                    for (side, wing) in self.wings.iter().enumerate() {
                        if let Some(w) = *wing {
                            j[w].rot.z += if side == 0 { 50.0 } else { -50.0 } * e;
                        }
                    }
                }
                Act::Yawn => {
                    j[self.head].rot.x -= 14.0 * e;
                    self.chin.v += 0.5 * e * dt;
                }
                // For each bite, quick pecks at the snack on the floor,
                // then the head comes up and back to swallow, the chin
                // bobbing.
                Act::Eat => {
                    let (dip, chew) = bite(g.t);
                    let pecks = if dip > 0.5 { peck(g.t, 4.5) } else { 0.0 };
                    let down = dip * (0.55 + 0.45 * pecks);
                    j[self.body].rot.x += e * 16.0 * down;
                    j[self.head].rot.x += e * (32.0 * down - 8.0 * chew.abs());
                    if chew.abs() > 0.9 {
                        self.chin.v -= 0.4 * e;
                    }
                }
                // Pecking about at the floor: bursts of quick pecks, a
                // little step's sway between them.
                Act::Peck => {
                    let burst = ((TAU * 0.9 * g.t).sin() > -0.3) as u8 as f32;
                    let down = burst * peck(g.t, 3.6);
                    j[self.body].rot.x += e * 16.0 * down;
                    j[self.head].rot.x += e * 30.0 * down;
                    j[self.body].rot.z += e * 4.0 * (TAU * 0.9 * g.t).sin();
                }
                // Preening: sitting, the head turns back over a wing and
                // nibbles at it, the wing lifting out a little.
                Act::Preen => {
                    let side = -facing;
                    let nibble =
                        (TAU * 6.0 * g.t).sin() * ((TAU * 0.8 * g.t).sin() > 0.0) as u8 as f32;
                    let turn = side * 70.0 * e;
                    j[self.body].rot.y += (1.0 - self.neck) * turn;
                    j[self.head].rot.y += self.neck * turn;
                    j[self.head].rot.x += e * (22.0 + 5.0 * nibble);
                    j[self.head].rot.z -= side * 10.0 * e;
                    let k = if side > 0.0 { 0 } else { 1 };
                    if let Some(w) = self.wings[k] {
                        j[w].rot.z += side * 22.0 * e;
                    }
                }
                // Flapping: both wings beat quickly, the body rising onto
                // its toes and the head up.
                Act::Flap => {
                    let beat = (TAU * 6.5 * g.t).sin();
                    for (side, wing) in self.wings.iter().enumerate() {
                        if let Some(w) = *wing {
                            let sign = if side == 0 { 1.0 } else { -1.0 };
                            j[w].rot.z += sign * e * (40.0 + 35.0 * beat);
                        }
                    }
                    j[self.body].shift.y += e * 0.03 * (1.0 + beat) * 0.5;
                    j[self.head].rot.x -= 8.0 * e;
                    self.belly.v += e * 0.6 * beat * dt * 30.0;
                }
                // A ruffling shake: the body rolls quickly side to side,
                // the head a beat ahead, the wings fluttering.
                Act::Shake => {
                    let fade = 1.0 - (g.t / 1.2).clamp(0.0, 1.0) * 0.6;
                    let w = TAU * 5.5 * g.t;
                    j[self.body].rot.z += e * fade * 12.0 * w.sin();
                    j[self.head].rot.z += e * fade * 10.0 * (w + 0.8).sin();
                    j[self.head].rot.y += e * fade * 10.0 * (w + 0.8).sin();
                    for (side, wing) in self.wings.iter().enumerate() {
                        if let Some(wg) = *wing {
                            let sign = if side == 0 { 1.0 } else { -1.0 };
                            j[wg].rot.z += sign * e * fade * 14.0 * (w * 1.7).sin().abs();
                        }
                    }
                    self.butt_x.v += e * fade * 1.2 * (w - 0.9).cos() * dt * 30.0;
                }
                // Pleased: a little dance from foot to foot, the body
                // rocking, the wings held out, the tail waggling hard.
                Act::Wiggle => {
                    let w = TAU * 2.8 * g.t;
                    j[self.body].rot.z += e * 12.0 * w.sin();
                    j[self.head].rot.z -= e * 8.0 * (w - 0.8).sin();
                    for (k, &leg) in self.legs.iter().enumerate() {
                        let up = if k == 0 { w.sin() } else { -w.sin() }.max(0.0);
                        j[leg].shift.y += e * 0.06 * up;
                    }
                    for (side, wing) in self.wings.iter().enumerate() {
                        if let Some(wg) = *wing {
                            let sign = if side == 0 { 1.0 } else { -1.0 };
                            j[wg].rot.z += sign * e * 22.0;
                        }
                    }
                    if let Some(t) = self.tail {
                        j[t].rot.z += e * 18.0 * (2.0 * w).sin();
                    }
                }
                // Two little hops (the animator lifts it): wings out and
                // fluttering, head up.
                Act::Bounce => {
                    let beat = (TAU * 7.0 * g.t).sin();
                    for (side, wing) in self.wings.iter().enumerate() {
                        if let Some(wg) = *wing {
                            let sign = if side == 0 { 1.0 } else { -1.0 };
                            j[wg].rot.z += sign * e * (24.0 + 14.0 * beat);
                        }
                    }
                    j[self.head].rot.x -= 10.0 * e;
                }
                // A turn right round on the spot, the feet pattering.
                Act::Spin => {
                    let secs = chonkgene_core::anim::gesture_secs(Act::Spin).unwrap_or(1.4);
                    let u = (g.t / secs).clamp(0.0, 1.0);
                    j[0].rot.y += facing * 360.0 * u * u * (3.0 - 2.0 * u);
                    for (k, &leg) in self.legs.iter().enumerate() {
                        let p = TAU * 3.5 * g.t + PI * k as f32;
                        j[leg].rot.x += e * 14.0 * p.sin();
                        j[leg].shift.y += e * 0.05 * p.cos().max(0.0);
                    }
                }
                // 握手 for a bird: a wing held out to you, shaking.
                Act::Paw => {
                    let shake = (TAU * 2.6 * g.t).sin();
                    let (k, sign) = if facing > 0.0 { (0, 1.0) } else { (1, -1.0) };
                    if let Some(w) = self.wings[k] {
                        j[w].rot.z += sign * e * (55.0 + 8.0 * shake);
                        j[w].rot.x -= e * 30.0;
                    }
                    j[self.head].rot.z += facing * 10.0 * e;
                }
                // Typing along: the feet patter in turn.
                Act::Tap => {
                    for (k, &leg) in self.legs.iter().enumerate() {
                        let p = TAU * 4.5 * g.t + PI * k as f32;
                        j[leg].shift.y += e * 0.05 * p.sin().max(0.0);
                    }
                    j[self.head].rot.x += e * 5.0 * (TAU * 4.5 * g.t).sin().abs();
                }
                // The head tipped right over, listening.
                Act::Tilt => {
                    j[self.head].rot.z += facing * 30.0 * e;
                }
                // Beak down at the floor, poking about.
                Act::Sniff | Act::Rub => {
                    let poke = peck(g.t, 2.4);
                    j[self.body].rot.x += e * 12.0;
                    j[self.head].rot.x += e * (20.0 + 10.0 * poke);
                }
                // One wing up, waving.
                Act::Wave => {
                    let wiggle = (TAU * 2.2 * g.t).sin();
                    if let Some(w) = self.wings[0] {
                        j[w].rot.z += 70.0 * e + 12.0 * e * wiggle;
                    }
                }
                _ => {}
            }
        }
        // Flying: the body tips forward, the feet tuck up behind, the
        // wings beat wide and the head stays up looking ahead.
        let fly = self
            .fly
            .step(if self.flying { 1.0 } else { 0.0 }, 0.0, 2.5, 1.0, dt)
            .clamp(0.0, 1.0);
        if fly > 0.0 {
            let beat = (TAU * 4.2 * self.moods.clock()).sin();
            j[self.body].rot.x += 24.0 * fly;
            j[self.head].rot.x -= 22.0 * fly;
            for &leg in &self.legs {
                j[leg].rot.x += 55.0 * fly;
            }
            for (side, wing) in self.wings.iter().enumerate() {
                if let Some(w) = *wing {
                    let sign = if side == 0 { 1.0 } else { -1.0 };
                    j[w].rot.z += sign * fly * (55.0 + 45.0 * beat);
                    j[w].rot.x -= 10.0 * fly * beat;
                }
            }
            j[self.body].shift.y += 0.03 * fly * beat;
        }
        // Swimming: sunk to the waterline (the water is drawn over the
        // rest), the feet paddling under it, bobbing gently.
        let swim = self
            .swim
            .step(if self.swimming { 1.0 } else { 0.0 }, 0.0, 1.2, 1.0, dt)
            .clamp(0.0, 1.0);
        if swim > 0.0 {
            let t = self.moods.clock();
            j[0].shift.y -= SWIM_SINK * swim;
            j[self.body].shift.y += 0.015 * swim * (TAU * 0.9 * t).sin();
            j[self.body].rot.z += 3.0 * swim * (TAU * 0.6 * t).sin();
            // Drifting round a little this way and that on the water.
            j[0].rot.y += 10.0 * swim * (TAU * 0.22 * t + 0.7).sin();
            for (k, &leg) in self.legs.iter().enumerate() {
                let p = TAU * 1.8 * t + PI * k as f32;
                j[leg].rot.x += swim * 28.0 * p.sin();
            }
            if let Some(tl) = self.tail {
                j[tl].rot.z += swim * 10.0 * (TAU * 1.4 * t).sin();
            }
        }
        self.react(&mut j, &lv, dt, facing);
        self.fidgets(&mut j, &f, sit, lv.attend, dt);
        self.busy = s > 0.0
            || self.gesture.is_some()
            || self.yaw.v.abs() > 1.0
            || self.look.v.abs() > 1.0
            || self.sit.v.abs() > 0.02
            || self.lie.v.abs() > 0.02
            || fly > 0.0
            || swim > 0.0
            || lv.attend > 0.01;
        j
    }

    /// A bird's little movements on top of the pose (see
    /// [`crate::fidget`]): the head bobs and tilts, feathers puff up and
    /// sleek, a wing shifts or both flutter, a foot scratches the head or
    /// the toes re-grip, the bill dabbles, the tail waggles.
    fn fidgets(&mut self, j: &mut [Joint], f: &Out, sit: f32, attend: f32, dt: f32) {
        // The slow drift under it all; the wings breathe with it.
        let (dx, dy, dz, body) = drift(f.t);
        j[self.head].rot.x += f.calm * dx;
        j[self.head].rot.y += f.calm * dy * self.neck;
        j[self.body].rot.y += f.calm * dy * (1.0 - self.neck);
        j[self.head].rot.z += f.calm * dz;
        j[self.body].rot.z += f.calm * body;
        for (k, wing) in self.wings.iter().enumerate() {
            if let Some(w) = *wing {
                let sign = if k == 0 { 1.0 } else { -1.0 };
                j[w].rot.z += sign * f.calm * 3.0 * (0.5 + 0.5 * (TAU * 0.3 * f.t).sin());
            }
        }
        self.lively(j, f, dt);
        let Some(run) = f.run else {
            return;
        };
        let (e, t, side) = (run.e(), run.t, run.side);
        let x = (t / run.len).clamp(0.0, 1.0);
        // Touched, its own head tilts and bobs mostly wait: on top of
        // turning to the finger they tipped the head far enough over to
        // crease the body under it (Roger: the chick and the duckling went
        // out of shape scratched).
        let head = j[self.head];
        // Wing 0 is the left (+x): it opens about z the positive way.
        let wing = |side: f32| {
            let k = if side > 0.0 { 0 } else { 1 };
            self.wings[k].map(|w| (w, if k == 0 { 1.0 } else { -1.0 }))
        };
        match run.kind {
            Kind::HeadBob => {
                let w = (TAU * 1.4 * t).sin();
                j[self.head].shift.x += 0.05 * w * e;
                j[self.head].rot.z += 6.0 * w * e;
            }
            Kind::HeadTilt => {
                // An owl turns its head right over.
                let deg = if self.neck < 1.0 { 26.0 } else { 17.0 };
                j[self.head].rot.z += e * (side * deg + 4.0 * (TAU * 0.7 * t).sin());
            }
            Kind::Ruffle => {
                // Puffed up, then a shiver as the feathers sleek down.
                for (k, wing) in self.wings.iter().enumerate() {
                    if let Some(w) = *wing {
                        let sign = if k == 0 { 1.0 } else { -1.0 };
                        j[w].rot.z += sign * 14.0 * e;
                    }
                }
                j[self.body].shift.y += 0.02 * e;
                let shiver = if x > 0.55 { 1.0 - x } else { 0.0 };
                j[self.body].rot.z += 9.0 * shiver * (TAU * 11.0 * t).sin();
                j[self.head].rot.z -= 6.0 * shiver * (TAU * 11.0 * t).sin();
                self.belly.v += 0.6 * shiver * (TAU * 11.0 * t).cos() * dt * 30.0;
            }
            Kind::WingShift => {
                if let Some((w, sign)) = wing(side) {
                    j[w].rot.z += sign * 20.0 * e;
                    j[w].rot.x -= 8.0 * e;
                }
            }
            Kind::Flutter => {
                for (k, wing) in self.wings.iter().enumerate() {
                    if let Some(w) = *wing {
                        let sign = if k == 0 { 1.0 } else { -1.0 };
                        j[w].rot.z += sign * e * (12.0 + 12.0 * (TAU * 10.0 * t).sin());
                    }
                }
            }
            Kind::FootScratch => {
                // Standing on one leg, the other foot comes up to scratch
                // the head, which tips down to meet it.
                let k = if side > 0.0 { 0 } else { 1 };
                let leg = self.legs[k];
                let up = e * (1.0 - 0.6 * sit);
                j[leg].rot.x -= up * (70.0 + 10.0 * (TAU * 7.0 * t).sin());
                j[leg].shift.y += up * 0.1;
                j[self.body].rot.z -= side * 5.0 * up;
                j[self.head].rot.z += side * 16.0 * e;
                j[self.head].rot.x += 10.0 * e;
            }
            Kind::ToeGrip => {
                // Rocking onto the foot that stays down.
                j[self.body].rot.z += 3.0 * e * (TAU * 1.6 * t).sin();
                for (k, &leg) in self.legs.iter().enumerate() {
                    let p = (TAU * 1.6 * t + PI * k as f32).sin().max(0.0);
                    j[leg].shift.y += 0.04 * p * e * (1.0 - sit);
                    j[leg].rot.x -= 10.0 * p * e;
                }
            }
            Kind::Dabble => {
                j[self.head].rot.x += e * (7.0 + 4.0 * (TAU * 7.0 * t).sin());
            }
            Kind::Waggle => {
                let w = (TAU * 6.0 * t).sin();
                if let Some(tail) = self.tail {
                    j[tail].rot.z += 22.0 * w * e;
                }
                self.butt_x.v += 0.8 * w * e * dt * 30.0;
            }
            Kind::HeadShake => {
                let w = (TAU * 7.0 * t).sin() * (1.0 - x);
                j[self.head].rot.y += 16.0 * w * e;
                j[self.head].rot.z += 7.0 * w * e;
            }
            _ => {}
        }
        let k = 1.0 - 0.8 * attend.clamp(0.0, 1.0);
        j[self.head].rot = head.rot + (j[self.head].rot - head.rot) * k;
        j[self.head].shift = head.shift + (j[self.head].shift - head.shift) * k;
    }

    /// Pleased or at play, a bird never stops (PLAN §8 "Roger's tenth
    /// pass", item 3): the wings shift and flutter, each on its own beat,
    /// the tail waggles, the head bobs a little, gestures or not.
    fn lively(&mut self, j: &mut [Joint], f: &Out, dt: f32) {
        let (l, t) = (f.lively, f.t);
        if l <= 0.0 {
            return;
        }
        for (k, wing) in self.wings.iter().enumerate() {
            if let Some(w) = *wing {
                let sign = if k == 0 { 1.0 } else { -1.0 };
                let k = k as f32;
                // Lifting and settling, and now and then a quick flutter.
                let shift = (TAU * 0.8 * t + 2.1 * k).sin().max(0.0);
                let burst = (TAU * 0.3 * t + 1.3 * k).sin().max(0.0).powi(4);
                j[w].rot.z += sign * l * (4.0 + 9.0 * shift + 8.0 * burst * (TAU * 9.0 * t).sin());
            }
        }
        let waggle = (TAU * 2.4 * t).sin();
        if let Some(tail) = self.tail {
            j[tail].rot.z += l * 14.0 * waggle;
        }
        self.butt_x.v += 0.4 * l * waggle * dt * 30.0;
        j[self.head].rot.z += l * 5.0 * (TAU * 0.6 * t + 0.4).sin();
        j[self.head].rot.x += l * 3.0 * (TAU * 1.1 * t).sin();
    }

    /// A bird's answer to a feeling or a touch (see [`Walk`]'s): rocking
    /// back on its bottom kicking its feet laughing, sunk flat with wings
    /// drooping fed up, its back turned sulking, a wing for a high five,
    /// the head leaning into a hand, looking back at its tail.
    fn react(&mut self, j: &mut [Joint], lv: &Levels, dt: f32, facing: f32) {
        let t = lv.t;
        self.ground = 0.0;
        self.flatten = 0.0;
        let turn_head = |j: &mut [Joint], deg: f32, neck: f32, head: usize, body: usize| {
            j[head].rot.y += neck * deg;
            j[body].rot.y += (1.0 - neck) * deg;
        };
        // Looking at the finger. A bird is one round body with the head
        // on top and no neck between: the head turning or tipping on its
        // own creases it (Roger: the chick and the duckling went out of
        // shape scratched), so the body takes a share.
        turn_head(
            j,
            lv.gaze.0,
            self.neck.min(TOUCH_NECK),
            self.head,
            self.body,
        );
        j[self.head].rot.x += TOUCH_NECK * lv.gaze.1;
        j[self.body].rot.x += 0.5 * (1.0 - TOUCH_NECK) * lv.gaze.1;
        // Laughing: back on its bottom, feet kicking, wings flapping.
        let l = lv.laugh.min(1.0);
        if l > 0.0 {
            j[self.body].rot.x -= l * (22.0 + 8.0 * (TAU * 2.2 * t).sin());
            for (k, &leg) in self.legs.iter().enumerate() {
                j[leg].rot.x -= l * (45.0 + 20.0 * (TAU * 3.0 * t + PI * k as f32).sin());
            }
            for (side, wing) in self.wings.iter().enumerate() {
                if let Some(w) = *wing {
                    let sign = if side == 0 { 1.0 } else { -1.0 };
                    j[w].rot.z += sign * l * (30.0 + 25.0 * (TAU * 6.0 * t).sin());
                }
            }
            // The tail would swing down under it and prop it up: it
            // lifts it out of the way.
            if let Some(tail) = self.tail {
                j[tail].rot.x += 30.0 * l;
            }
            self.ground = 0.5 * l;
        }
        // Fed up: sunk low and flattened, head down, wings drooping out.
        let m = lv.melt;
        if m > 0.0 {
            j[0].shift.y -= 0.07 * m;
            j[self.head].rot.x += 22.0 * m;
            for (side, wing) in self.wings.iter().enumerate() {
                if let Some(w) = *wing {
                    j[w].rot.z += if side == 0 { 25.0 } else { -25.0 } * m;
                }
            }
            self.flatten = 0.1 * m;
        }
        // Back turned, glancing round now and then.
        if lv.sulk > 0.0 {
            let glance = (TAU * t / 3.5).sin().max(0.0).powi(4);
            j[0].shift.z -= SULK_BACK * lv.sulk;
            j[self.head].rot.x += 12.0 * lv.sulk * (1.0 - glance);
            turn_head(
                j,
                -facing * 60.0 * lv.sulk * glance,
                self.neck,
                self.head,
                self.body,
            );
        }
        j[self.head].rot.z += 8.0 * lv.content * (TAU * 0.4 * t).sin();
        // A hand on the head, a chin scratch, a cheek: as the others, the
        // body taking its share of the tilt.
        let nod = 10.0 * lv.head - 24.0 * lv.chin + 14.0 * lv.flinch;
        let tilt = 9.0 * lv.head * (TAU * 1.1 * t).sin()
            + 5.0 * lv.chin * (TAU * 0.8 * t).sin()
            + 12.0 * lv.cheek;
        j[self.head].rot.x += TOUCH_NECK * nod;
        j[self.body].rot.x += 0.5 * (1.0 - TOUCH_NECK) * nod;
        j[self.head].rot.z += TOUCH_NECK * tilt;
        j[self.body].rot.z += (1.0 - TOUCH_NECK) * tilt;
        // A belly rub: leaning back, wings fluttering.
        let r = lv.rub.min(1.0);
        if r > 0.0 {
            j[self.body].rot.x -= 10.0 * r;
            for (side, wing) in self.wings.iter().enumerate() {
                if let Some(w) = *wing {
                    let sign = if side == 0 { 1.0 } else { -1.0 };
                    j[w].rot.z += sign * r * (14.0 + 10.0 * (TAU * 5.0 * t).sin());
                }
            }
        }
        // Its tail pulled or poked: a flick, and a look back.
        if lv.flick {
            self.flick.v += 300.0;
        }
        let flick = self.flick.step(0.0, 0.0, 3.5, 0.3, dt);
        if let Some(tail) = self.tail {
            j[tail].rot.z += flick;
            if matches!(self.feel.held, Some((Zone::Tail, ..))) {
                j[tail].rot.x -= 15.0;
            }
        }
        turn_head(j, -facing * 70.0 * lv.back, self.neck, self.head, self.body);
        // A wing (or a foot) touched: up it comes, for a high five.
        if let Some((bone, up)) = lv.paw {
            if let Some(k) = self.wings.iter().position(|w| *w == Some(bone)) {
                let sign = if k == 0 { 1.0 } else { -1.0 };
                j[bone].rot.z += sign * up * (70.0 + 8.0 * (TAU * 2.0 * t).sin());
            } else if self.legs.contains(&bone) {
                j[bone].rot.x -= 40.0 * up;
                j[bone].shift.y += 0.05 * up;
            } else if let Some(w) = self.wings[0] {
                j[w].rot.z += up * 70.0;
            }
        }
    }
}
