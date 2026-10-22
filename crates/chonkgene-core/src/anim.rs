//! Procedural animation (PLAN §3.3): breathing, blinking, tail and the belly
//! spring, reduced to a few numbers every rig understands.

use crate::Zone;
use crate::spring::Spring;

const BREATH_PERIOD: f32 = 3.2;
const BLINK_SECS: f32 = 0.12;

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
    rng: fastrand::Rng,
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
            rng,
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
        if moving || self.is_settling() {
            60
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
        if self.t >= self.next_blink {
            self.blink_until = self.t + BLINK_SECS;
            self.next_blink = self.t + 3.5 + self.rng.f32() * 2.0;
        }
    }

    pub fn params(&self) -> PoseParams {
        let tau = std::f32::consts::TAU;
        let (wag_hz, wag_deg) = if self.walking {
            (3.0, 14.0)
        } else {
            (1.2, 8.0)
        };
        PoseParams {
            breath: (self.t * tau / BREATH_PERIOD).sin(),
            squash: self.body.pos.clamp(-0.4, 0.4),
            belly: self.belly.pos.clamp(-0.4, 0.4),
            eyes_open: if self.t < self.blink_until { 0.12 } else { 1.0 },
            tail: (self.t * tau * wag_hz).sin() * wag_deg,
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
