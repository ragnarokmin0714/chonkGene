//! Procedural animation (PLAN §3.3): breathing, blinking, tail and the belly
//! spring, reduced to a few numbers every rig understands.

use crate::Zone;
use crate::spring::Spring;

const BREATH_PERIOD: f32 = 3.2;
const BLINK_SECS: f32 = 0.12;
const GIGGLE_EVERY: f32 = 0.22;

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
    /// Tail angle, degrees.
    pub tail: f32,
    /// Walking bob, design units (negative is up).
    pub bob: f32,
    /// Swing while held, degrees.
    pub sway: f32,
    /// 1.0 faces right, −1.0 faces left.
    pub facing: f32,
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
    pub sway: f32,
    pub facing: f32,
}

impl Animator {
    pub fn new(seed: u64, chonk: f32) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        let t = rng.f32() * BREATH_PERIOD;
        Self {
            t,
            body: Spring::for_chonk(chonk),
            belly: Spring::new(220.0, 7.0),
            next_blink: t + 1.0 + rng.f32() * 3.0,
            blink_until: 0.0,
            next_giggle: 0.0,
            rng,
            expression: Expression::Neutral,
            walking: false,
            sway: 0.0,
            facing: 1.0,
        }
    }

    pub fn set_chonk(&mut self, chonk: f32) {
        let (pos, vel) = (self.body.pos, self.body.vel);
        self.body = Spring::for_chonk(chonk);
        self.body.pos = pos;
        self.body.vel = vel;
    }

    pub fn poke(&mut self, zone: Zone) {
        let (body, belly) = match zone {
            Zone::Belly => (6.0, 10.0),
            Zone::Bottom => (5.0, 3.0),
            Zone::Cheek | Zone::Head => (2.5, 0.0),
            Zone::Paw => (1.5, 0.0),
        };
        self.body.kick(body);
        self.belly.kick(belly);
    }

    /// A landing squashes the body in proportion to the impact.
    pub fn land(&mut self, speed: f32) {
        self.body.kick((speed / 120.0).min(12.0));
    }

    pub fn is_settling(&self) -> bool {
        !self.body.is_resting() || !self.belly.is_resting()
    }

    /// Frames per second this pet needs now (PLAN §5).
    pub fn fps(&self, moving: bool) -> u32 {
        if moving || self.is_settling() || self.expression == Expression::Giggle {
            60
        } else if self.expression == Expression::Asleep {
            5
        } else if self.walking {
            30
        } else {
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
    }

    pub fn params(&self) -> PoseParams {
        let tau = std::f32::consts::TAU;
        let (wag_hz, wag_deg) = match self.expression {
            Expression::Giggle => (4.0, 16.0),
            Expression::FedUp => (0.5, 3.0),
            Expression::Sulk | Expression::Asleep => (0.0, 0.0),
            _ if self.walking => (3.0, 14.0),
            _ => (1.2, 8.0),
        };
        let blinking = self.t < self.blink_until;
        PoseParams {
            breath: if self.expression == Expression::Asleep {
                1.6 * (self.t * tau / (BREATH_PERIOD * 1.6)).sin()
            } else {
                (self.t * tau / BREATH_PERIOD).sin()
            },
            squash: self.body.pos.clamp(-0.4, 0.4),
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
            tail: match self.expression {
                Expression::Sulk => 12.0,
                Expression::Asleep => 4.0,
                _ => (self.t * tau * wag_hz).sin() * wag_deg,
            },
            bob: if self.walking {
                -(self.t * 9.0).sin().abs() * 6.0
            } else {
                0.0
            },
            sway: self.sway,
            facing: self.facing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_runs_at_ten_fps_and_a_poke_raises_it() {
        let mut a = Animator::new(1, 0.33);
        assert_eq!(a.fps(false), 10);
        a.poke(Zone::Belly);
        assert_eq!(a.fps(false), 60);
        for _ in 0..300 {
            a.step(1.0 / 60.0);
        }
        assert_eq!(a.fps(false), 10);
    }

    #[test]
    fn giggling_keeps_the_belly_bouncing_at_sixty_fps() {
        let mut a = Animator::new(3, 0.33);
        a.expression = Expression::Giggle;
        for _ in 0..240 {
            a.step(1.0 / 60.0);
            assert_eq!(a.fps(false), 60);
        }
        assert!(a.params().eyes_open < 0.5);
    }

    #[test]
    fn fed_up_is_half_lidded_and_slow() {
        let mut a = Animator::new(4, 0.33);
        a.expression = Expression::FedUp;
        a.step(0.5);
        assert_eq!(a.fps(false), 10);
        assert!(a.params().tail.abs() <= 3.0);
    }

    #[test]
    fn blinks_every_few_seconds() {
        let mut a = Animator::new(2, 0.33);
        let mut shut_frames = 0;
        for _ in 0..(60 * 12) {
            a.step(1.0 / 60.0);
            if a.params().eyes_open < 0.5 {
                shut_frames += 1;
            }
        }
        // 12 s holds 2–3 blinks of ~7 frames each.
        assert!((10..=30).contains(&shut_frames), "{shut_frames}");
    }
}
