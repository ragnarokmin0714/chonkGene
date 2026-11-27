//! A chin scratch, beat by beat (PLAN §8, Roger's twenty-seventh pass,
//! item 2): two taps on its chin bring your hand in from below for a few
//! seconds, and each species takes it its own way (see
//! [`ChinScratch`]). The same beats move the hand (the app draws it and
//! presses with it) and the pet (the renderer), so the two stay together.

pub use crate::species::ChinScratch;

/// Where a chin scratch is, `t` seconds in (see [`ChinScratch::beats`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChinBeats {
    /// How far your hand has come in, 0 … 1 (it fades in and out).
    pub hand: f32,
    /// The hand is on its chin, pressing in.
    pub on: bool,
    /// Where the hand is, off the bottom of its chin, as shares of a
    /// hand's length: x toward the way it faces, y down.
    pub off: (f32, f32),
    /// The chin lifted further than a hand on it lifts it, 0 … 1: a cat's
    /// going up and up, the chick's head up.
    pub lift: f32,
    /// The kind's own, 0 … 1: the Shiba's jowls pushed up, the 曼赤肯
    /// rubbing its cheek on your knuckles, the 折耳貓 toppled over (and
    /// lying, melted), the owl's face feathers fluffed out, the chick
    /// puffed round, the duckling's nibble, the fox's tail hooking the
    /// hand.
    pub own: f32,
    /// The head's tilt, degrees (+ to its left): the 折耳貓's growing tilt,
    /// the owl's head following your finger round.
    pub roll: f32,
    /// The head's nod, degrees (+ down): the owl's, with its tilt, going
    /// round in a circle after your finger.
    pub nod: f32,
    /// The tail going, 0 … 1: the Shiba's curl, the duckling's bottom
    /// waggling, the fox's tails swaying slowly.
    pub wag: f32,
    /// The fox: how far the hand has gone from her chin to the tip of the
    /// tail hooking it, 0 … 1.
    pub to_tail: f32,
}

/// The hand coming in from below and going again, seconds.
const IN: f32 = 0.35;
const OUT: f32 = 0.35;
/// The Shiba: four pats up under its double chin, then two squeezes.
const PAT: f32 = 0.4;
const PATS: f32 = 4.0;
const SQUEEZE: f32 = 0.6;
const SQUEEZES: f32 = 2.0;
/// A cat: each stroke of the knuckles up its throat to the chin.
const STROKE: f32 = 0.55;
/// The 曼赤肯: four strokes, then its cheek rubbed on your knuckles.
const STROKES: f32 = 4.0;
const RUB: f32 = 1.3;
/// The 折耳貓: tilting while stroked, then toppling, lying melted a while,
/// and up again.
const TILT: f32 = 2.55;
const TOPPLE: f32 = 0.8;
const MELT: f32 = 2.0;
const GET_UP: f32 = 0.7;
/// How far the 折耳貓's head tilts before it topples, degrees.
const FOLD_TILT: f32 = 38.0;
/// The owl: the finger round under its face disc twice, each turn this
/// long; the head going round with it, tipped over from this far to this
/// far, degrees (the renderer shares it between the head and the body: a
/// round owl's head turned much further tears its skin), and nodding a
/// little less.
const TURN: f32 = 1.6;
const TURNS: f32 = 2.0;
const OWL_ROLL: (f32, f32) = (25.0, 60.0);
/// The chick: scratched this long, then puffed a little longer once the
/// hand has gone, and back down.
const PUFF: f32 = 2.4;
const PUFF_HOLD: f32 = 0.5;
const PUFF_DOWN: f32 = 0.5;
/// The duckling: the finger held at its bill this long, its nibbles in
/// bursts of quick pecks.
const NIBBLE: f32 = 2.8;
const NIBBLE_HZ: f32 = 6.0;
/// The fox: scratched, the hand starting to go, her tail hooking it and
/// bringing it back, scratched again, and the tail letting go.
const FOX_SCRATCH: f32 = 2.4;
const FOX_STROKE: f32 = 0.7;
const FOX_HELD: f32 = 1.2;
const FOX_AGAIN: f32 = 0.9;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let u = ((x - a) / (b - a)).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// A cat's knuckles stroking up its throat, `u` strokes in: down the
/// throat and back up to the chin (shares of a hand's length).
fn stroke(u: f32) -> (f32, f32) {
    (0.0, 0.1 * (0.5 - 0.5 * (std::f32::consts::TAU * u).cos()))
}

impl ChinScratch {
    /// How long your hand is there, seconds.
    pub fn hand_secs(self) -> f32 {
        IN + OUT
            + match self {
                ChinScratch::Jowls => PATS * PAT + SQUEEZES * SQUEEZE,
                ChinScratch::Knuckles => STROKES * STROKE + RUB,
                // It goes as the cat topples, a little after it starts.
                ChinScratch::Topple => TILT + 0.15,
                ChinScratch::Round => TURNS * TURN,
                ChinScratch::Puff => PUFF,
                ChinScratch::Nibble => NIBBLE,
                ChinScratch::Hook => FOX_SCRATCH + FOX_HELD + 0.35 + FOX_AGAIN,
            }
    }

    /// How long the whole of it lasts, the pet's part after the hand
    /// included (the 折耳貓 lying melted, the owl's head coming back
    /// round), seconds.
    pub fn secs(self) -> f32 {
        let hand = self.hand_secs();
        match self {
            ChinScratch::Topple => IN + TILT + TOPPLE + MELT + GET_UP,
            ChinScratch::Round => hand + 0.4,
            ChinScratch::Puff => hand + PUFF_HOLD + PUFF_DOWN,
            ChinScratch::Nibble => hand + 0.3,
            ChinScratch::Hook => hand + 0.4,
            ChinScratch::Jowls | ChinScratch::Knuckles => hand,
        }
    }

    /// When what it says comes: once it is being scratched, seconds.
    pub fn say_at(self) -> f32 {
        match self {
            // As its head starts to go.
            ChinScratch::Topple => IN + 1.2,
            _ => IN + 0.6,
        }
    }

    /// Where the scratch is `t` seconds in; none before or after it.
    pub fn beats(self, t: f32) -> Option<ChinBeats> {
        use std::f32::consts::{PI, TAU};
        if !(0.0..self.secs()).contains(&t) {
            return None;
        }
        let hand_secs = self.hand_secs();
        let came = smooth(0.0, IN, t);
        let gone = smooth(hand_secs - OUT, hand_secs, t);
        let mut b = ChinBeats {
            hand: came * (1.0 - gone),
            on: t >= 0.6 * IN && t < hand_secs - 0.6 * OUT,
            // Up from below as it comes, down and away as it goes.
            off: (0.0, 0.5 * ((1.0 - came) + gone)),
            ..ChinBeats::default()
        };
        // Scratching from when the hand arrives to when it goes.
        let busy = smooth(IN, IN + 0.3, t) * (1.0 - smooth(hand_secs - OUT, hand_secs, t));
        let from = t - IN;
        match self {
            ChinScratch::Jowls => {
                b.wag = busy;
                let squeezes = IN + PATS * PAT;
                if (IN..squeezes).contains(&t) {
                    // Each pat: pressed up a moment, the hand dropping
                    // away and back.
                    let u = (from / PAT).fract();
                    let drop = if u < 0.55 {
                        0.0
                    } else {
                        0.22 * (PI * (u - 0.55) / 0.45).sin()
                    };
                    b.off.1 += drop;
                    b.on = drop < 0.04;
                    b.own = if b.on { 0.6 } else { 0.0 };
                } else if t >= squeezes && (t - squeezes) / SQUEEZE < SQUEEZES {
                    // Each squeeze: pushed up higher and held, then let go.
                    let u = ((t - squeezes) / SQUEEZE).fract();
                    if u < 0.75 {
                        let push = (PI * u / 0.75).sin();
                        b.off.1 -= 0.06 * push;
                        b.own = 0.6 + 0.4 * push;
                    } else {
                        b.off.1 += 0.18 * (PI * (u - 0.75) / 0.25).sin();
                        b.on = false;
                    }
                } else if t >= squeezes {
                    // The last squeeze let go: the hand goes from there.
                    b.on = false;
                }
            }
            ChinScratch::Knuckles => {
                let rub_from = IN + STROKES * STROKE;
                let s = stroke(from / STROKE);
                if t < rub_from {
                    b.off.1 += s.1 * busy;
                }
                // The chin up and up with each stroke, coming down to rub
                // its cheek on your knuckles.
                let rub = smooth(rub_from, rub_from + 0.25, t)
                    * (1.0 - smooth(hand_secs - OUT - 0.2, hand_secs - OUT, t));
                b.lift = smooth(IN, rub_from, t) * (1.0 - 0.7 * rub);
                b.own = rub;
            }
            ChinScratch::Topple => {
                let topple = IN + TILT;
                let s = stroke(from / STROKE);
                b.off.1 += s.1 * busy;
                let tilt = smooth(IN, topple, t);
                b.lift = tilt;
                // Up again, the tilt goes with the rest.
                let up = 1.0 - smooth(topple + TOPPLE + MELT, self.secs(), t);
                b.roll = FOLD_TILT * tilt * up;
                b.own = smooth(topple, topple + TOPPLE, t) * up;
                // The hand follows it a little way down as it goes.
                b.off.0 -= 0.25 * smooth(topple, hand_secs, t);
            }
            ChinScratch::Round => {
                let end = IN + TURNS * TURN;
                let phase = TAU * (from / TURN).clamp(0.0, TURNS);
                let r = 0.1 * busy;
                b.off.0 += r * phase.sin();
                b.off.1 += r * (1.0 - phase.cos());
                let amp = OWL_ROLL.0 + (OWL_ROLL.1 - OWL_ROLL.0) * smooth(IN, end - 0.25, t);
                // Its head stays where the finger left it, and comes back
                // round once the hand has gone.
                let back = 1.0 - smooth(end, self.secs(), t);
                let round = amp * smooth(IN, IN + 0.4, t) * back;
                b.roll = round * phase.sin();
                b.nod = 0.4 * round * phase.cos();
                b.own = smooth(IN, IN + 0.85, t) * back;
                b.lift = 0.3 * busy;
            }
            ChinScratch::Puff => {
                b.off.0 += 0.03 * (TAU * 3.0 * t).sin() * busy;
                let down = hand_secs + PUFF_HOLD;
                b.own = smooth(IN + 0.15, IN + 0.95, t) * (1.0 - smooth(down, self.secs(), t));
                b.lift = smooth(IN, IN + 0.65, t) * (1.0 - smooth(hand_secs, down, t));
            }
            ChinScratch::Nibble => {
                b.wag = smooth(IN, IN + 0.35, t) * (1.0 - smooth(hand_secs, self.secs(), t));
                // Bursts of quick nibbles, a rest between: 0.6 s on, 0.3 off.
                let u = (from - 0.15) / 0.9;
                let peck = if u >= 0.0 && u.fract() < 0.67 {
                    (PI * NIBBLE_HZ * t).sin().abs()
                } else {
                    0.0
                };
                b.own = peck * busy;
            }
            ChinScratch::Hook => {
                b.wag = smooth(IN, IN + 0.5, t) * (1.0 - smooth(hand_secs, self.secs(), t));
                let leave = IN + FOX_SCRATCH;
                let free = leave + FOX_HELD;
                // The hand slips off her chin and away toward her side,
                // where a tail sweeps round and curls over it; held there
                // a while (it tugs), it gives in and goes back to her chin.
                b.to_tail = smooth(leave, leave + 0.45, t) * (1.0 - smooth(free, free + 0.35, t));
                b.own = smooth(leave + 0.1, leave + 0.45, t) * (1.0 - smooth(free, free + 0.4, t));
                if b.to_tail > 0.99 {
                    b.off.0 += 0.03 * (TAU * 5.0 * t).sin();
                }
                let strokes = busy * (1.0 - b.to_tail);
                b.off.1 += 0.6 * stroke(from / FOX_STROKE).1 * strokes;
                b.on = b.on && b.to_tail < 0.1;
                b.lift = 0.4 * strokes;
            }
        }
        Some(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Species;

    fn frames(kind: ChinScratch) -> Vec<ChinBeats> {
        (0..)
            .map(|k| k as f32 / 120.0)
            .take_while(|&t| t < kind.secs())
            .map(|t| kind.beats(t).expect("under way"))
            .collect()
    }

    /// Times the hand comes down onto the chin.
    fn downs(f: &[ChinBeats]) -> usize {
        f.windows(2).filter(|w| w[1].on && !w[0].on).count()
    }

    /// Roger's twenty-seventh pass, item 2: every species' chin scratch
    /// brings your hand in and takes it away again, on the chin between,
    /// and each does its own thing with it: the Shiba's jowls patted and
    /// squeezed, the 曼赤肯's chin up and up and then its cheek on your
    /// knuckles, the 折耳貓 tilting until it topples, the owl's head going
    /// round nearly upside down, the chick puffed, the duckling nibbling,
    /// the fox's tail catching the hand as it goes.
    #[test]
    fn each_chin_scratch_is_its_own() {
        for s in Species::ALL {
            let kind = s.chin_scratch();
            let f = frames(kind);
            assert!(kind.beats(-0.01).is_none() && kind.beats(kind.secs()).is_none());
            assert!(f[0].hand < 0.05 && !f[0].on, "{s:?} comes in");
            assert!(f.iter().any(|b| b.hand > 0.99 && b.on), "{s:?} on its chin");
            let last_hand = kind.hand_secs();
            assert!(kind.beats(last_hand - 0.01).unwrap().hand < 0.05, "{s:?}");
            assert!(
                kind.say_at() < last_hand,
                "{s:?} speaks with the hand there"
            );
            assert!(kind.secs() >= last_hand, "{s:?}");
        }
        let most = |kind: ChinScratch, get: fn(&ChinBeats) -> f32| {
            frames(kind).iter().map(get).fold(0.0f32, f32::max)
        };
        let jowls = frames(ChinScratch::Jowls);
        assert_eq!(downs(&jowls), 6, "four pats and two squeezes");
        assert!(most(ChinScratch::Jowls, |b| b.own) > 0.95);
        assert!(most(ChinScratch::Jowls, |b| b.wag) > 0.95, "the curl going");
        // The 曼赤肯: the chin highest just before the rub, lower in it.
        let k = ChinScratch::Knuckles;
        let rub_at = IN + STROKES * STROKE;
        assert!(k.beats(rub_at - 0.01).unwrap().lift > 0.95);
        let rubbing = k.beats(rub_at + 0.6).unwrap();
        assert!(rubbing.own > 0.95 && rubbing.lift < 0.5, "{rubbing:?}");
        // The 折耳貓: the hand gone before it lies, melted, then up again.
        let fold = ChinScratch::Topple;
        let lying = fold.beats(IN + TILT + TOPPLE + 0.5 * MELT).unwrap();
        assert!(lying.own > 0.99 && lying.hand == 0.0, "{lying:?}");
        assert!(lying.roll > 0.9 * FOLD_TILT, "{lying:?}");
        assert!(fold.beats(fold.secs() - 0.01).unwrap().own < 0.01);
        // The owl: round both ways, further over at the last.
        let (left, right) = (
            most(ChinScratch::Round, |b| b.roll),
            most(ChinScratch::Round, |b| -b.roll),
        );
        assert!(
            left.max(right) > 55.0 && left.min(right) > 45.0,
            "{left} {right}"
        );
        let nods = most(ChinScratch::Round, |b| b.nod.abs());
        assert!(nods > 20.0, "round, not side to side: nods {nods}");
        let owl = ChinScratch::Round;
        assert!(
            owl.beats(owl.secs() - 0.01).unwrap().roll.abs() < 2.0,
            "back round"
        );
        assert!(most(ChinScratch::Puff, |b| b.own) > 0.99, "puffed round");
        assert!(
            ChinScratch::Puff
                .beats(ChinScratch::Puff.hand_secs() + 0.2)
                .unwrap()
                .own
                > 0.9
        );
        let nibbles = frames(ChinScratch::Nibble);
        let pecks = nibbles
            .windows(3)
            .filter(|w| w[1].own > 0.9 && w[1].own >= w[0].own && w[1].own > w[2].own)
            .count();
        assert!(pecks >= 10, "{pecks} nibbles");
        assert!(
            most(ChinScratch::Nibble, |b| b.wag) > 0.99,
            "its bottom waggling"
        );
        // The fox: off her chin as it goes, to the tail that holds it, and
        // back on.
        let fox = frames(ChinScratch::Hook);
        assert_eq!(downs(&fox), 2, "caught and brought back");
        let hooked = ChinScratch::Hook.beats(IN + FOX_SCRATCH + 0.8).unwrap();
        assert!(
            hooked.own > 0.99 && hooked.to_tail > 0.99 && !hooked.on,
            "{hooked:?}"
        );
        let again = ChinScratch::Hook
            .beats(IN + FOX_SCRATCH + FOX_HELD + 0.6)
            .unwrap();
        assert!(
            again.on && again.to_tail == 0.0 && again.own < 0.05,
            "{again:?}"
        );
    }
}
