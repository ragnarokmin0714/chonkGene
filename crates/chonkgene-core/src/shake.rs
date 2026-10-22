//! "Shake" while dragging (PLAN §1.2): 4+ left-right reversals within 1.2 s.

use std::collections::VecDeque;

const WINDOW: f64 = 1.2;
const REVERSALS: usize = 4;
/// Horizontal moves smaller than this (pixels) do not count as a direction.
const DEAD_ZONE: f32 = 3.0;

#[derive(Clone, Debug, Default)]
pub struct ShakeDetector {
    last_x: Option<f32>,
    dir: i8,
    reversals: VecDeque<f64>,
    fired: bool,
}

impl ShakeDetector {
    /// Feed the pet's x position; true exactly once per drag, when it becomes a shake.
    pub fn feed(&mut self, x: f32, now: f64) -> bool {
        let Some(last) = self.last_x else {
            self.last_x = Some(x);
            return false;
        };
        let step = x - last;
        if step.abs() > DEAD_ZONE {
            let dir = if step > 0.0 { 1 } else { -1 };
            if self.dir != 0 && dir != self.dir {
                self.reversals.push_back(now);
            }
            self.dir = dir;
            self.last_x = Some(x);
        }
        while self.reversals.front().is_some_and(|t| now - t > WINDOW) {
            self.reversals.pop_front();
        }
        if !self.fired && self.reversals.len() >= REVERSALS {
            self.fired = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiggle(d: &mut ShakeDetector, swings: usize, period: f64) -> usize {
        let mut fired = 0;
        let mut t = 0.0;
        for i in 0..=swings {
            let x = if i % 2 == 0 { 0.0 } else { 60.0 };
            for step in 0..6 {
                let frac = step as f32 / 5.0;
                let prev = if i % 2 == 0 { 60.0 } else { 0.0 };
                if d.feed(prev + (x - prev) * frac, t) {
                    fired += 1;
                }
                t += period / 6.0;
            }
        }
        fired
    }

    #[test]
    fn fast_back_and_forth_is_a_shake_once() {
        let mut d = ShakeDetector::default();
        assert_eq!(wiggle(&mut d, 10, 0.15), 1);
    }

    #[test]
    fn slow_swings_are_not_a_shake() {
        let mut d = ShakeDetector::default();
        assert_eq!(wiggle(&mut d, 10, 0.6), 0);
    }

    #[test]
    fn a_straight_drag_is_not_a_shake() {
        let mut d = ShakeDetector::default();
        assert!((0..100).all(|i| !d.feed(i as f32 * 5.0, f64::from(i) * 0.01)));
    }
}
