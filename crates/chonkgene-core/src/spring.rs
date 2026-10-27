//! Damped springs for jiggly flesh (PLAN §1.2): displacement `target` (0
//! unless something is pressing on it) at rest.

/// Longest physics substep; keeps the spring stable at 10 fps frames.
const MAX_SUBSTEP: f32 = 1.0 / 120.0;
const REST_POS: f32 = 0.002;
const REST_VEL: f32 = 0.02;
/// Below this the wobble moves a body by about a pixel.
const WOBBLE_POS: f32 = 0.02;
const WOBBLE_VEL: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub pos: f32,
    pub vel: f32,
    /// Where it rests: a finger holding a belly in moves this.
    pub target: f32,
    stiffness: f32,
    damping: f32,
}

impl Spring {
    pub fn new(stiffness: f32, damping: f32) -> Self {
        Self {
            pos: 0.0,
            vel: 0.0,
            target: 0.0,
            stiffness,
            damping,
        }
    }

    /// A belly for a body value in 0.0 (標準) – 1.0 (麻糬): chonkier means more
    /// mass, so it wobbles slower and longer.
    pub fn for_chonk(chonk: f32) -> Self {
        let mass = 1.0 + 0.8 * chonk.clamp(0.0, 1.0);
        Self::new(180.0 / mass, 9.0 / mass.sqrt())
    }

    pub fn kick(&mut self, velocity: f32) {
        self.vel += velocity;
    }

    pub fn step(&mut self, dt: f32) {
        let mut left = dt.clamp(0.0, 0.25);
        while left > 0.0 {
            let h = left.min(MAX_SUBSTEP);
            // Semi-implicit Euler.
            self.vel += (-self.stiffness * (self.pos - self.target) - self.damping * self.vel) * h;
            self.pos += self.vel * h;
            left -= h;
        }
        if self.is_resting() {
            self.pos = self.target;
            self.vel = 0.0;
        }
    }

    /// Still visibly moving (see [`WOBBLE_POS`]).
    pub fn is_wobbling(&self) -> bool {
        (self.pos - self.target).abs() > WOBBLE_POS || self.vel.abs() > WOBBLE_VEL
    }

    pub fn is_resting(&self) -> bool {
        (self.pos - self.target).abs() < REST_POS && self.vel.abs() < REST_VEL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mut s: Spring, dt: f32, secs: f32) -> (Spring, f32) {
        let mut peak: f32 = 0.0;
        let mut t = 0.0;
        while t < secs {
            s.step(dt);
            peak = peak.max(s.pos.abs());
            t += dt;
        }
        (s, peak)
    }

    #[test]
    fn settles_after_a_kick() {
        let mut s = Spring::for_chonk(0.33);
        s.kick(9.0);
        let (s, peak) = run(s, 1.0 / 60.0, 3.0);
        assert!(peak > 0.2, "a poke should visibly squash, peak {peak}");
        assert!(s.is_resting());
    }

    #[test]
    fn stays_stable_at_low_frame_rates() {
        let mut s = Spring::for_chonk(0.0);
        s.kick(9.0);
        let (s, peak) = run(s, 1.0 / 10.0, 4.0);
        assert!(peak < 1.0, "blew up: peak {peak}");
        assert!(s.is_resting());
    }

    #[test]
    fn rests_where_it_is_held() {
        let mut s = Spring::for_chonk(0.33);
        s.target = 0.25;
        let (s, _) = run(s, 1.0 / 60.0, 3.0);
        assert!(s.is_resting());
        assert_eq!(s.pos, 0.25);
    }

    #[test]
    fn chonkier_bellies_wobble_longer() {
        let settle = |chonk| {
            let mut s = Spring::for_chonk(chonk);
            s.kick(9.0);
            let mut t = 0.0;
            while !s.is_resting() {
                s.step(1.0 / 60.0);
                t += 1.0 / 60.0;
            }
            t
        };
        assert!(settle(1.0) > settle(0.0));
    }
}
