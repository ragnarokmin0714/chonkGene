//! What you seem to be doing at the computer, from input timing alone
//! (PLAN §1.3, §6.2–6.3, §8 "Roger's fourth pass", item 4): no hooks and
//! no keys read, only the cursor's position and the system's "seconds since
//! the last input". Input while the cursor stays put is typing (or
//! scrolling); a cursor left still with no input at all is reading, or
//! watching something; a long while with no input is being away.

use std::collections::VecDeque;

/// The cursor still this long with no input at all, seconds: reading.
pub const READING_AFTER: f64 = 20.0;
/// No input this long, seconds: away (the pets may fall asleep, §1.12).
pub const AWAY_AFTER: f64 = 120.0;
/// Typing: this many inputs seen, with the cursor still, over the last
/// [`TYPING_WINDOW`] seconds.
const TYPING_KEYS: usize = 4;
const TYPING_WINDOW: f64 = 4.0;
/// Typing fast: this many over the last [`FAST_WINDOW`] seconds (the
/// clock is sampled a few times a second, so this is a burst at nearly
/// every sample).
const FAST_KEYS: usize = 8;
const FAST_WINDOW: f64 = 2.0;
/// A pause this long ends a stretch of typing, seconds.
const TYPING_PAUSE: f64 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Activity {
    /// Moving the mouse about, or nothing known.
    Busy,
    /// The cursor still, no input: reading or watching, for this long.
    Reading(f64),
    /// Typing (input with the cursor still); `fast` in a quick burst.
    Typing { fast: bool },
    /// No input for a long while.
    Away,
}

/// Keeps an eye on the input timing, sampled a few times a second.
#[derive(Clone, Debug, Default)]
pub struct Watcher {
    cursor: Option<(i32, i32)>,
    /// When the cursor last moved.
    moved: f64,
    /// The idle clock last sample, and when it was read.
    idle: Option<f64>,
    /// Inputs seen with the cursor still (the idle clock went back).
    keys: VecDeque<f64>,
    /// When the present stretch of typing began.
    typing_since: Option<f64>,
}

impl Watcher {
    /// A sample at `now` (seconds): where the cursor is, and the system's
    /// seconds since the last input, if it can tell.
    pub fn feed(&mut self, now: f64, cursor: Option<(i32, i32)>, idle: Option<f64>) {
        let moved = cursor.is_some() && cursor != self.cursor;
        if moved {
            self.cursor = cursor;
            self.moved = now;
        }
        if let Some(idle) = idle {
            // The clock went back since last time: there was input. With
            // the cursor where it was, that was a key (or a wheel).
            let input = self.idle.is_some_and(|was| idle < was) || idle < 0.05;
            if input && !moved && now - self.moved > 0.5 {
                self.keys.push_back(now);
            }
            self.idle = Some(idle);
        }
        while self.keys.front().is_some_and(|&t| now - t > TYPING_WINDOW) {
            self.keys.pop_front();
        }
        let typing = self.keys.len() >= TYPING_KEYS;
        match (typing, self.typing_since) {
            (true, None) => self.typing_since = Some(now),
            (false, Some(_)) if self.keys.back().is_none_or(|&t| now - t > TYPING_PAUSE) => {
                self.typing_since = None;
            }
            _ => {}
        }
    }

    pub fn activity(&self, now: f64) -> Activity {
        let idle = self.idle.unwrap_or(0.0);
        if idle >= AWAY_AFTER {
            return Activity::Away;
        }
        if self.keys.len() >= TYPING_KEYS {
            let fast = self
                .keys
                .iter()
                .filter(|&&t| now - t <= FAST_WINDOW)
                .count()
                >= FAST_KEYS;
            return Activity::Typing { fast };
        }
        let still = now - self.moved;
        if self.idle.is_some() && still >= READING_AFTER && idle >= READING_AFTER.min(still) {
            return Activity::Reading(still);
        }
        Activity::Busy
    }

    /// How long you have been typing with no long pause, seconds.
    pub fn typing_for(&self, now: f64) -> f64 {
        self.typing_since.map_or(0.0, |since| now - since)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Samples five times a second for `secs`: the cursor at `at(t)` and
    /// input whenever `key(t)`.
    fn run(
        w: &mut Watcher,
        from: f64,
        secs: f64,
        at: impl Fn(f64) -> (i32, i32),
        key: impl Fn(f64) -> bool,
        last_input: &mut f64,
    ) -> f64 {
        let mut t = from;
        while t < from + secs {
            t += 0.2;
            let pos = at(t);
            if key(t) || Some(pos) != w.cursor {
                *last_input = t;
            }
            w.feed(t, Some(pos), Some(t - *last_input));
        }
        t
    }

    #[test]
    fn keys_with_a_still_cursor_are_typing_and_a_burst_is_fast() {
        let mut w = Watcher::default();
        let mut last = 0.0;
        let t = run(
            &mut w,
            0.0,
            3.0,
            |t| ((t * 50.0) as i32, 300),
            |_| false,
            &mut last,
        );
        assert_eq!(w.activity(t), Activity::Busy);
        // A key every 0.6 s: typing, not fast.
        let t = run(
            &mut w,
            t,
            6.0,
            |_| (150, 300),
            |t| (t * 5.0).round() as i64 % 3 == 0,
            &mut last,
        );
        assert_eq!(w.activity(t), Activity::Typing { fast: false });
        // A key at every sample: fast.
        let t = run(&mut w, t, 3.0, |_| (150, 300), |_| true, &mut last);
        assert_eq!(w.activity(t), Activity::Typing { fast: true });
        assert!(w.typing_for(t) > 5.0, "typing for {:.1} s", w.typing_for(t));
    }

    #[test]
    fn a_still_cursor_and_no_keys_is_reading_then_away() {
        let mut w = Watcher::default();
        let mut last = 0.0;
        let t = run(
            &mut w,
            0.0,
            2.0,
            |t| ((t * 50.0) as i32, 300),
            |_| false,
            &mut last,
        );
        let t2 = run(&mut w, t, 10.0, |_| (100, 300), |_| false, &mut last);
        assert_eq!(w.activity(t2), Activity::Busy);
        let t3 = run(&mut w, t2, 15.0, |_| (100, 300), |_| false, &mut last);
        assert!(matches!(w.activity(t3), Activity::Reading(s) if s > 20.0));
        let t4 = run(&mut w, t3, 120.0, |_| (100, 300), |_| false, &mut last);
        assert_eq!(w.activity(t4), Activity::Away);
    }

    #[test]
    fn moving_the_mouse_is_never_typing() {
        let mut w = Watcher::default();
        let mut last = 0.0;
        let t = run(
            &mut w,
            0.0,
            8.0,
            |t| ((t * 80.0) as i32, 300),
            |_| true,
            &mut last,
        );
        assert_eq!(w.activity(t), Activity::Busy);
        assert_eq!(w.typing_for(t), 0.0);
    }

    #[test]
    fn without_an_idle_clock_it_never_guesses() {
        let mut w = Watcher::default();
        for k in 0..300 {
            w.feed(f64::from(k) * 0.2, Some((5, 5)), None);
        }
        assert_eq!(w.activity(60.0), Activity::Busy);
    }
}
