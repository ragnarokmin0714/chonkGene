//! Procedural animation (PLAN §3.3): breathing, blinking, the belly spring,
//! the walk cycle, turning round, hops, idle gestures and tail wags, reduced
//! to a few numbers every renderer understands.

use crate::Zone;
use crate::spring::Spring;

const BREATH_PERIOD: f32 = 3.2;
const BLINK_SECS: f32 = 0.12;
const GIGGLE_EVERY: f32 = 0.22;
/// Steps a second at walking pace.
const STEP_HZ: f32 = 2.2;
/// Turning round: long enough to see a 3D turn through facing you (a
/// squash-flip when frames are pre-rendered sprites).
const TURN_SECS: f32 = 0.3;
/// Hops fall at this many design pixels/s².
const HOP_GRAVITY: f32 = 1800.0;
/// How far a held finger squashes the body, in spring units.
const PRESS_SQUASH: f32 = 0.28;
/// A steady walk shows this many distinct frames a step, so a renderer can
/// draw each once and copy it after (PLAN §5).
pub const BEATS: u8 = 12;

/// How the face and tail read. Placeholder for the keyframed poses (belly-up
/// laughing, playing dead, …) that need real art (PLAN §3.3, §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expression {
    Neutral,
    /// Squinting, bouncing with giggles, tail going wild.
    Giggle,
    /// Half-lidded, limp tail: 厭世.
    FedUp,
    /// Eyes shut, tail down: the overdone finale.
    Sulk,
    /// Content squint after making up or a head pat.
    Happy,
    /// Eyes shut, tail still, slow breathing; the renderer adds the snot bubble.
    Asleep,
}

/// What the body is doing besides its expression: a walking step or an idle
/// gesture. Each has its own pose in the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Rest,
    /// Walking: one diagonal pair of legs forward…
    StepA,
    /// …then the other.
    StepB,
    LookLeft,
    LookRight,
    /// A play bow.
    Stretch,
    Yawn,
    Wave,
}

/// How long a gesture that has a fixed length lasts, seconds: the idle
/// stretch, yawn and wave. None for the rest (looks vary, steps and rest
/// have no length).
pub fn gesture_secs(act: Act) -> Option<f32> {
    match act {
        Act::Stretch => Some(1.9),
        Act::Yawn => Some(1.7),
        Act::Wave => Some(1.6),
        _ => None,
    }
}

/// Inputs a renderer maps onto bones by convention (see chonkgene-render).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseParams {
    /// −1.0 … 1.0 over a breath.
    pub breath: f32,
    /// Whole-body squash: > 0 flattens, < 0 stretches.
    pub squash: f32,
    /// Extra belly jiggle on top of `squash`.
    pub belly: f32,
    /// 1.0 open, 0.0 shut.
    pub eyes_open: f32,
    /// Tail wag away from the pose's own tail, degrees.
    pub tail: f32,
    /// Walking bob and hops, design units (negative is up).
    pub bob: f32,
    /// Lean about the feet, degrees: swinging while held, waddling.
    pub sway: f32,
    /// 1.0 faces right, −1.0 faces left; in between mid-turn, squeezed thin.
    pub facing: f32,
    pub act: Act,
    /// Set on a steady walk: which of the step's [`BEATS`] this frame is.
    /// Everything else then follows from `act`, `beat` and `facing`, so
    /// frames with the same three look the same.
    pub beat: Option<u8>,
    /// Where the walk is in its two-step cycle, 0.0 … 2.0: step A, then
    /// step B (snapped to the beat on a steady walk); 0.0 standing.
    pub stride: f32,
    /// Seconds on the animation's clock, for motion drawn from it (a waving
    /// paw, blending between poses).
    pub clock: f32,
    /// Walking speed against the usual (zoomies run faster, a skit may
    /// slow it): legs drawn every frame step to match.
    pub pace: f32,
}

impl Default for PoseParams {
    fn default() -> Self {
        Self {
            breath: 0.0,
            squash: 0.0,
            belly: 0.0,
            eyes_open: 1.0,
            tail: 0.0,
            bob: 0.0,
            sway: 0.0,
            facing: 1.0,
            act: Act::Rest,
            beat: None,
            stride: 0.0,
            clock: 0.0,
            pace: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Animator {
    t: f32,
    body: Spring,
    belly: Spring,
    next_blink: f32,
    blink_until: f32,
    next_giggle: f32,
    rng: fastrand::Rng,
    pub expression: Expression,
    pub walking: bool,
    /// Walking speed multiplier (zoomies run faster).
    pub pace: f32,
    pub sway: f32,
    /// The way the pet wants to face; the shown facing turns towards it.
    pub facing: f32,
    /// Pop into a new pose to hide the cut: frames cut from pre-rendered
    /// sprites need it; frames drawn on the GPU blend instead.
    pub pops: bool,
    turn: f32,
    /// Species gait, 0.0 (trots) – 1.0 (waddles).
    waddle: f32,
    /// Steps taken on this walk, fractional.
    stride: f32,
    /// Height above the floor (negative is up) and its speed.
    hop: f32,
    hop_vel: f32,
    hops_left: u8,
    pressed: bool,
    /// The gesture under way, when it started and when it ends.
    gesture: Option<(Act, f32, f32)>,
    next_gesture: f32,
    /// The current or next burst of idle wagging.
    wag: (f32, f32),
    next_huff: f32,
}

impl Animator {
    /// `waddle` is the species gait (see `Species::waddle`).
    pub fn new(seed: u64, chonk: f32, waddle: f32) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        let t = rng.f32() * BREATH_PERIOD;
        let wag_at = t + 3.0 + rng.f32() * 5.0;
        Self {
            t,
            body: Spring::for_chonk(chonk),
            belly: Spring::new(220.0, 7.0),
            next_blink: t + 1.0 + rng.f32() * 3.0,
            blink_until: 0.0,
            next_giggle: 0.0,
            next_gesture: t + 4.0 + rng.f32() * 5.0,
            wag: (wag_at, wag_at + 1.5),
            next_huff: 0.0,
            rng,
            expression: Expression::Neutral,
            walking: false,
            pace: 1.0,
            sway: 0.0,
            facing: 1.0,
            pops: true,
            turn: 1.0,
            waddle: waddle.clamp(0.0, 1.0),
            stride: 0.0,
            hop: 0.0,
            hop_vel: 0.0,
            hops_left: 0,
            pressed: false,
            gesture: None,
        }
    }

    /// The same animator, popping into new poses or not (see `pops`).
    pub fn with_pops(mut self, pops: bool) -> Self {
        self.pops = pops;
        self
    }

    pub fn set_chonk(&mut self, chonk: f32) {
        let (pos, vel, target) = (self.body.pos, self.body.vel, self.body.target);
        self.body = Spring::for_chonk(chonk);
        self.body.pos = pos;
        self.body.vel = vel;
        self.body.target = target;
    }

    /// A finger is down on the pet: it squishes in under it, ready to pop
    /// back when the finger lets go ([`Animator::poke`]).
    pub fn press(&mut self) {
        self.pressed = true;
        self.body.target = PRESS_SQUASH;
    }

    /// The press turned into a lift: let go of the squish and stretch.
    pub fn lift(&mut self) {
        self.pressed = false;
        self.body.target = 0.0;
        self.body.kick(-3.0);
    }

    /// The finger lets go. A belly held in springs back out by itself; a
    /// quick tap that had no time to squash it kicks it instead.
    pub fn poke(&mut self, zone: Zone) {
        let held = (self.body.pos / PRESS_SQUASH).clamp(0.0, 1.0);
        self.pressed = false;
        self.body.target = 0.0;
        let (body, belly) = match zone {
            Zone::Belly => (6.0, 10.0),
            Zone::Bottom => (5.0, 3.0),
            Zone::Cheek | Zone::Head => (2.5, 0.0),
            Zone::Paw => (1.5, 0.0),
        };
        self.body.kick(body * (1.0 - held));
        self.belly.kick(belly);
        // A poke in the bottom makes it jump.
        if zone == Zone::Bottom {
            self.jump(220.0, 0);
        }
    }

    /// A happy double hop (a pat, a snack).
    pub fn cheer(&mut self) {
        self.jump(250.0, 1);
    }

    fn jump(&mut self, speed: f32, again: u8) {
        if !self.hopping() {
            self.hop_vel = -speed;
            self.hops_left = again;
        }
    }

    fn hopping(&self) -> bool {
        self.hop < 0.0 || self.hop_vel != 0.0
    }

    fn turning(&self) -> bool {
        self.turn != self.facing.signum()
    }

    /// Start a gesture now if the pet is free for it (a wave hello).
    pub fn gesture(&mut self, act: Act) {
        if self.free() {
            self.start(act);
        }
    }

    /// Free for an idle gesture: sitting about with nothing else going on.
    fn free(&self) -> bool {
        !self.walking
            && !self.pressed
            && !self.hopping()
            && matches!(self.expression, Expression::Neutral | Expression::Happy)
    }

    fn start(&mut self, act: Act) {
        let secs = match act {
            Act::LookLeft | Act::LookRight => 1.4 + self.rng.f32() * 0.8,
            _ => match gesture_secs(act) {
                Some(secs) => secs,
                None => return,
            },
        };
        self.gesture = Some((act, self.t, self.t + secs));
        // A pop hides the cut to the new pose, where poses cut.
        if self.pops {
            self.body.kick(0.8);
        }
        self.body.target = match act {
            Act::Yawn => -0.14,
            Act::Stretch => 0.06,
            _ => 0.0,
        };
    }

    fn pick_gesture(&mut self) -> Act {
        match self.rng.u32(0..100) {
            0..30 => Act::LookLeft,
            30..60 => Act::LookRight,
            60..75 => Act::Stretch,
            75..90 => Act::Yawn,
            _ => Act::Wave,
        }
    }

    /// A landing squashes the body in proportion to the impact.
    pub fn land(&mut self, speed: f32) {
        self.body.kick((speed / 120.0).min(12.0));
    }

    pub fn is_settling(&self) -> bool {
        !self.body.is_resting() || !self.belly.is_resting()
    }

    /// Wobbling enough to need every frame; the last of a wobble is a pixel
    /// or so, and half the frames show it as well.
    fn is_wobbling(&self) -> bool {
        self.body.is_wobbling() || self.belly.is_wobbling()
    }

    fn wagging(&self) -> bool {
        self.params().tail != 0.0
    }

    /// Frames per second this pet needs now (PLAN §5).
    pub fn fps(&self, moving: bool) -> u32 {
        let waving = self.gesture.is_some_and(|(act, ..)| act == Act::Wave);
        if moving
            || self.is_wobbling()
            || self.expression == Expression::Giggle
            || self.turning()
            || self.hopping()
        {
            60
        } else if self.walking || waving || self.wagging() {
            30
        } else if self.expression == Expression::Asleep {
            5
        } else {
            // Glancing, stretching and yawning hold still poses, and the last
            // of a wobble moves less than a pixel.
            10
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.t += dt;
        self.body.step(dt);
        self.belly.step(dt);
        self.sway *= (1.0 - 6.0 * dt).max(0.0);
        // Settle exactly, so a resting pose stops changing (the renderer caches on it).
        if self.sway.abs() < 0.01 {
            self.sway = 0.0;
        }
        if self.expression == Expression::Giggle && self.t >= self.next_giggle {
            self.body.kick(2.5);
            self.next_giggle = self.t + GIGGLE_EVERY;
        }
        if self.t >= self.next_blink {
            self.blink_until = self.t + BLINK_SECS;
            self.next_blink = self.t + 3.5 + self.rng.f32() * 2.0;
        }

        // Turning round: squeeze thin, flip, fill out, with a little hop.
        let goal = self.facing.signum();
        if self.turn != goal {
            if self.turn.abs() == 1.0 {
                self.jump(110.0, 0);
            }
            let reach = 2.0 * dt / TURN_SECS;
            self.turn = if (goal - self.turn).abs() <= reach {
                goal
            } else {
                self.turn + reach * goal
            };
        }

        if self.walking {
            self.stride += STEP_HZ * self.pace * dt;
        } else {
            self.stride = 0.0;
        }

        if self.hopping() {
            self.hop_vel += HOP_GRAVITY * dt;
            self.hop += self.hop_vel * dt;
            if self.hop >= 0.0 {
                self.body.kick((self.hop_vel / 90.0).min(4.0));
                self.hop = 0.0;
                self.hop_vel = 0.0;
                if self.hops_left > 0 {
                    let again = self.hops_left - 1;
                    self.jump(200.0, again);
                }
            }
        }

        // Idle gestures: now and then, while nothing else is going on.
        if let Some((act, _, until)) = self.gesture {
            if !self.free() || self.t >= until {
                self.gesture = None;
                self.body.target = if self.pressed { PRESS_SQUASH } else { 0.0 };
                self.next_gesture = self.t + 5.0 + self.rng.f32() * 7.0;
                // A yawn ends with a little shake of the head.
                if act == Act::Yawn {
                    self.sway = 6.0;
                }
            }
        } else if self.free()
            && self.expression == Expression::Neutral
            && self.t >= self.next_gesture
        {
            let act = self.pick_gesture();
            self.start(act);
        }

        // Bursts of wagging at rest; sulking flicks the tail now and then.
        if self.t >= self.wag.1 {
            let (len, gap) = match self.expression {
                Expression::Sulk => (0.7, 2.0 + self.rng.f32() * 3.0),
                _ => (1.2 + self.rng.f32() * 0.8, 4.0 + self.rng.f32() * 7.0),
            };
            let start = self.t + gap;
            self.wag = (start, start + len);
        }
        // Sulking: a huffy 哼 every few seconds.
        if self.expression == Expression::Sulk && self.t >= self.next_huff {
            self.body.kick(-1.4);
            self.next_huff = self.t + 2.5 + self.rng.f32() * 2.0;
        }
    }

    /// Where the walk is: which step and how far through it (0.0–1.0).
    fn step_phase(&self) -> (bool, f32) {
        let n = self.stride.floor();
        ((n as i64) % 2 == 0, self.stride - n)
    }

    fn tail(&self) -> f32 {
        let tau = std::f32::consts::TAU;
        let wave = |hz: f32, deg: f32| (self.t * tau * hz).sin() * deg;
        let burst = |hz: f32, deg: f32| {
            let (from, to) = self.wag;
            if self.t < from || self.t >= to {
                return 0.0;
            }
            let k = (self.t - from) / (to - from);
            (k * std::f32::consts::PI).sin() * ((self.t - from) * tau * hz).sin() * deg
        };
        match self.expression {
            Expression::Giggle => wave(4.0, 16.0),
            Expression::Happy => wave(3.5, 18.0),
            Expression::FedUp => wave(0.5, 3.0),
            Expression::Sulk => burst(1.4, 10.0),
            Expression::Asleep => 0.0,
            // One wag a step, in time with the feet (see `params`).
            Expression::Neutral if self.walking => {
                let (_, p) = self.step_phase();
                (p * tau).sin() * 14.0
            }
            Expression::Neutral => burst(2.5, 12.0),
        }
    }

    pub fn params(&self) -> PoseParams {
        let tau = std::f32::consts::TAU;
        let pi = std::f32::consts::PI;
        let blinking = self.t < self.blink_until;

        let (mut act, mut bob, mut roll, mut footfall) = (Act::Rest, 0.0, 0.0, 0.0);
        // A steady walk with nothing else going on repeats the same few
        // frames: snap it to them.
        let steady = self.walking
            && self.expression == Expression::Neutral
            && !self.is_settling()
            && !self.turning()
            && !self.hopping()
            && self.sway == 0.0;
        let (mut beat, mut stride) = (None, 0.0);
        if self.walking {
            // Each step lifts the body in an arc and rocks it onto the
            // planted foot; it lands with a small squash.
            let (a, mut p) = self.step_phase();
            if steady {
                let n = ((p * f32::from(BEATS)) as u8).min(BEATS - 1);
                p = f32::from(n) / f32::from(BEATS);
                beat = Some(n);
            }
            let side = if a { 1.0 } else { -1.0 };
            act = if a { Act::StepA } else { Act::StepB };
            stride = if a { p } else { 1.0 + p };
            bob = -(pi * p).sin() * (7.0 + 5.0 * self.waddle);
            roll = side * (pi * p).sin() * (3.0 + 6.0 * self.waddle);
            footfall = (0.06 + 0.12 * self.waddle) * (1.0 - p / 0.22).max(0.0).powi(2);
        } else if let Some((g, since, _)) = self.gesture {
            act = g;
            if g == Act::Wave {
                roll = 4.0 * ((self.t - since) * tau * 2.2).sin();
            }
        }
        if self.expression == Expression::Giggle {
            // Rolling about with laughter.
            roll += 6.0 * (self.t * tau * 1.8).sin();
        }
        let tail = match beat {
            Some(n) => (f32::from(n) / f32::from(BEATS) * tau).sin() * 14.0,
            None => self.tail(),
        };
        PoseParams {
            breath: if beat.is_some() {
                0.0
            } else if self.expression == Expression::Asleep {
                1.6 * (self.t * tau / (BREATH_PERIOD * 1.6)).sin()
            } else {
                (self.t * tau / BREATH_PERIOD).sin()
            },
            squash: (self.body.pos + footfall).clamp(-0.4, 0.4),
            belly: self.belly.pos.clamp(-0.4, 0.4),
            eyes_open: match self.expression {
                Expression::Giggle | Expression::Happy => 0.18,
                Expression::Sulk => 0.12,
                Expression::Asleep => 0.06,
                Expression::FedUp if blinking => 0.12,
                Expression::FedUp => 0.5,
                Expression::Neutral if blinking => 0.12,
                Expression::Neutral => 1.0,
            },
            tail,
            bob: bob + self.hop,
            sway: self.sway + roll,
            facing: self.turn,
            act,
            beat,
            stride,
            clock: self.t,
            pace: self.pace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn run(a: &mut Animator, secs: f32, mut each: impl FnMut(&Animator)) {
        for _ in 0..(secs / DT).round() as usize {
            a.step(DT);
            each(a);
        }
    }

    #[test]
    fn idle_runs_at_ten_fps_and_a_poke_raises_it() {
        let mut a = Animator::new(1, 0.33, 0.35);
        assert_eq!(a.fps(false), 10);
        a.poke(Zone::Belly);
        assert_eq!(a.fps(false), 60);
        run(&mut a, 2.5, |_| {});
        assert!(!a.is_settling());
        assert_eq!(a.fps(false), 10);
    }

    /// Idle pets are mostly still, so frames can be cached (PLAN §5): over
    /// ten minutes they average well under the walking rate.
    #[test]
    fn idle_pets_mostly_run_slow() {
        for seed in 0..4 {
            let mut a = Animator::new(seed, 0.33, 0.35);
            let (mut t, mut frames) = (0.0, 0);
            while t < 600.0 {
                let dt = 1.0 / a.fps(false) as f32;
                a.step(dt);
                t += dt;
                frames += 1;
            }
            let fps = frames as f32 / 600.0;
            assert!(fps < 18.0, "seed {seed}: {fps:.1} fps");
        }
    }

    #[test]
    fn giggling_keeps_the_belly_bouncing_at_sixty_fps() {
        let mut a = Animator::new(3, 0.33, 0.35);
        a.expression = Expression::Giggle;
        run(&mut a, 4.0, |a| assert_eq!(a.fps(false), 60));
        assert!(a.params().eyes_open < 0.5);
    }

    #[test]
    fn fed_up_is_half_lidded_and_slow() {
        let mut a = Animator::new(4, 0.33, 0.35);
        a.expression = Expression::FedUp;
        a.step(0.5);
        assert!(a.fps(false) <= 30);
        assert!(a.params().tail.abs() <= 3.0);
    }

    #[test]
    fn blinks_every_few_seconds() {
        let mut a = Animator::new(2, 0.33, 0.35);
        let mut shut_frames = 0;
        run(&mut a, 12.0, |a| {
            if a.params().eyes_open < 0.5 {
                shut_frames += 1;
            }
        });
        // 12 s holds 2–3 blinks of ~7 frames each.
        assert!((10..=30).contains(&shut_frames), "{shut_frames}");
    }

    #[test]
    fn walking_alternates_steps_rocks_and_bobs() {
        let mut a = Animator::new(6, 0.33, 1.0);
        a.walking = true;
        let (mut steps, mut lowest, mut rolls) = (Vec::new(), 0.0f32, (0.0f32, 0.0f32));
        run(&mut a, 2.0, |a| {
            let p = a.params();
            if steps.last() != Some(&p.act) {
                steps.push(p.act);
            }
            lowest = lowest.min(p.bob);
            rolls = (rolls.0.min(p.sway), rolls.1.max(p.sway));
        });
        assert!(steps.len() >= 4, "{steps:?}");
        assert!(steps.windows(2).all(|w| w[0] != w[1]));
        assert!(steps.iter().all(|s| matches!(s, Act::StepA | Act::StepB)));
        assert!(lowest < -6.0, "bob {lowest}");
        assert!(rolls.0 < -5.0 && rolls.1 > 5.0, "a duck waddles: {rolls:?}");
        a.walking = false;
        a.step(DT);
        assert_eq!(a.params().act, Act::Rest);
    }

    /// A steady walk is a loop of a few frames; the same beat looks the same.
    #[test]
    fn a_steady_walk_repeats_its_beats() {
        let mut a = Animator::new(13, 0.33, 0.35);
        a.walking = true;
        let mut seen: Vec<PoseParams> = Vec::new();
        run(&mut a, 5.0, |a| {
            let p = a.params();
            let beat = p.beat.expect("steady");
            if let Some(q) = seen.iter().find(|q| q.beat == Some(beat) && q.act == p.act) {
                assert_eq!(
                    PoseParams {
                        eyes_open: q.eyes_open,
                        clock: q.clock,
                        ..p
                    },
                    *q
                );
            } else {
                seen.push(p);
            }
        });
        assert!(seen.len() <= 2 * usize::from(BEATS));
        a.poke(Zone::Belly);
        a.step(DT);
        assert_eq!(a.params().beat, None, "a poke is not part of the loop");
    }

    #[test]
    fn birds_waddle_more_than_dogs() {
        let widest = |waddle| {
            let mut a = Animator::new(6, 0.33, waddle);
            a.walking = true;
            let mut w = 0.0f32;
            run(&mut a, 2.0, |a| w = w.max(a.params().sway.abs()));
            w
        };
        assert!(widest(1.0) > widest(0.35) * 1.5);
    }

    #[test]
    fn turning_round_squeezes_through_thin() {
        let mut a = Animator::new(7, 0.33, 0.35);
        a.facing = -1.0;
        let mut thinnest = 1.0f32;
        run(&mut a, 0.4, |a| {
            thinnest = thinnest.min(a.params().facing.abs())
        });
        assert!(thinnest < 0.2, "{thinnest}");
        assert_eq!(a.params().facing, -1.0);
    }

    #[test]
    fn a_held_finger_squashes_and_letting_go_pops_back() {
        let mut a = Animator::new(8, 0.33, 0.35);
        a.press();
        run(&mut a, 0.5, |_| {});
        let held = a.params().squash;
        assert!(held > 0.2, "held {held}");
        a.poke(Zone::Head);
        let mut stretched = 0.0f32;
        run(&mut a, 0.6, |a| {
            stretched = stretched.min(a.params().squash)
        });
        assert!(
            stretched < -0.05,
            "should overshoot into a stretch: {stretched}"
        );
    }

    #[test]
    fn a_quick_tap_still_squashes() {
        let mut a = Animator::new(8, 0.33, 0.35);
        a.press();
        a.poke(Zone::Belly);
        let mut most = 0.0f32;
        run(&mut a, 0.3, |a| most = most.max(a.params().squash));
        assert!(most > 0.2, "{most}");
    }

    #[test]
    fn cheering_hops_twice_and_lands() {
        let mut a = Animator::new(9, 0.33, 0.35);
        a.cheer();
        let (mut high, mut takeoffs, mut up) = (0.0f32, 0, false);
        run(&mut a, 1.5, |a| {
            let bob = a.params().bob;
            high = high.min(bob);
            if bob < 0.0 && !up {
                takeoffs += 1;
            }
            up = bob < 0.0;
        });
        assert_eq!(takeoffs, 2);
        assert!(high < -14.0, "{high}");
        assert_eq!(a.params().bob, 0.0);
    }

    #[test]
    fn idle_pets_glance_stretch_and_yawn_but_not_while_walking() {
        let mut a = Animator::new(10, 0.33, 0.35);
        let mut seen = Vec::new();
        run(&mut a, 90.0, |a| {
            let act = a.params().act;
            if act != Act::Rest && !seen.contains(&act) {
                seen.push(act);
            }
        });
        assert!(seen.len() >= 3, "{seen:?}");
        a.walking = true;
        run(&mut a, 20.0, |a| {
            assert!(matches!(a.params().act, Act::StepA | Act::StepB));
        });
    }

    #[test]
    fn a_wave_hello_starts_at_once() {
        let mut a = Animator::new(11, 0.33, 0.35);
        a.gesture(Act::Wave);
        a.step(DT);
        assert_eq!(a.params().act, Act::Wave);
        run(&mut a, 2.0, |_| {});
        assert_eq!(a.params().act, Act::Rest);
    }

    #[test]
    fn the_tail_rests_between_bursts_of_wagging() {
        let mut a = Animator::new(12, 0.33, 0.35);
        let (mut still, mut wag) = (0, 0);
        run(&mut a, 30.0, |a| {
            if a.params().tail == 0.0 {
                still += 1;
            } else {
                wag += 1;
            }
        });
        assert!(wag > 60, "wags now and then: {wag}");
        assert!(still > wag, "rests more than it wags: {still} vs {wag}");
    }
}
