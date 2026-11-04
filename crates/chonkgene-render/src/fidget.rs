//! The little movements under every held pose (PLAN §8 "Roger's seventh
//! pass", item 1): sitting, idling, smiling, looking back at you, a pet is
//! never frozen. Besides breathing and the tail's sway, one fidget follows
//! another with short gaps (an ear flick, a sniff, a paw shuffled, a wing
//! shifted, the tongue out), each species from its own list; the ears
//! twitch on clocks of their own, so nothing moves in step.
//!
//! Under all of it the head and body drift a little, slowly and without
//! end ([`Out::calm`]), so even between fidgets nothing stands frozen;
//! during a gesture the tail and ears keep going; a fidget cut short (the
//! pet sets off) fades out rather than snapping back.
//!
//! [`Fidgets`] only says what runs and how far into it; the walks
//! ([`crate::skin::Walk`], [`crate::waddle::Waddle`]) move the bones.

/// One kind of fidget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A burst of quick wagging.
    Wag,
    /// The tip of the tail curls and flicks (cats).
    TipFlick,
    /// The fox's tails lift one after another.
    Ripple,
    /// Nose up, twitching.
    Sniff,
    /// The head tipped to one side.
    HeadTilt,
    /// A front paw lifted and set down again.
    PawShuffle,
    /// A front paw scratching the cheek or chest, sitting.
    PawScratch,
    /// Front paws pressing in turn (a cat kneading).
    Knead,
    /// Short legs paddling in turn (the Munchkin).
    Paddle,
    /// Weight from one side to the other: the belly wobbles.
    WeightShift,
    /// The tongue left out (blep).
    Blep,
    /// Tongue out, panting.
    Pant,
    /// A quick lick of the nose.
    Lick,
    /// A shake of the head that sets the fox's bell ringing.
    Jingle,
    /// A dreaming paw twitch, asleep.
    Dream,
    /// The owl's head swaying side to side, judging distance.
    HeadBob,
    /// Feathers puffed up, then sleeked with a little shiver.
    Ruffle,
    /// One wing half lifted and settled.
    WingShift,
    /// Both wings fluttering a moment (the chick).
    Flutter,
    /// A foot up to scratch the head (birds).
    FootScratch,
    /// Toes re-gripping the floor, one foot then the other (the owl).
    ToeGrip,
    /// The bill dabbling (the duckling).
    Dabble,
    /// The tail waggled quickly (the duckling).
    Waggle,
    /// A quick shake of the head (the duckling).
    HeadShake,
}

impl Kind {
    /// How long it runs, seconds: shortest and longest.
    fn secs(self) -> (f32, f32) {
        match self {
            Self::Wag => (1.0, 2.2),
            Self::TipFlick => (0.6, 1.0),
            Self::Ripple => (1.2, 1.6),
            Self::Sniff => (0.8, 1.4),
            Self::HeadTilt => (1.2, 2.4),
            Self::PawShuffle => (0.5, 0.8),
            Self::PawScratch => (1.2, 1.8),
            Self::Knead => (1.8, 3.0),
            Self::Paddle => (1.0, 1.6),
            Self::WeightShift => (1.0, 1.8),
            Self::Blep => (1.6, 2.6),
            Self::Pant => (1.8, 3.2),
            Self::Lick => (0.35, 0.5),
            Self::Jingle => (0.5, 0.8),
            Self::Dream => (0.4, 0.8),
            Self::HeadBob => (1.0, 1.6),
            Self::Ruffle => (0.9, 1.3),
            Self::WingShift => (0.7, 1.1),
            Self::Flutter => (0.4, 0.7),
            Self::FootScratch => (1.0, 1.5),
            Self::ToeGrip => (0.6, 0.9),
            Self::Dabble => (0.6, 1.0),
            Self::Waggle => (0.5, 0.8),
            Self::HeadShake => (0.4, 0.6),
        }
    }

    /// Quick enough to need smooth frames while it runs.
    pub fn quick(self) -> bool {
        matches!(
            self,
            Self::TipFlick
                | Self::Sniff
                | Self::PawScratch
                | Self::Knead
                | Self::Paddle
                | Self::Pant
                | Self::Lick
                | Self::Jingle
                | Self::Ruffle
                | Self::Flutter
                | Self::FootScratch
                | Self::Dabble
                | Self::Waggle
                | Self::HeadShake
                | Self::Wag
        )
    }

    /// Moves only the tail: it can go on while a gesture moves the rest.
    pub fn light(self) -> bool {
        matches!(
            self,
            Self::Wag | Self::TipFlick | Self::Ripple | Self::Waggle
        )
    }

    /// Shows the tongue (the face draws it).
    pub fn tongue(self) -> Option<Tongue> {
        match self {
            Self::Blep | Self::Lick => Some(Tongue::Blep),
            Self::Pant => Some(Tongue::Pant),
            _ => None,
        }
    }
}

/// The tongue out: a blep (or a lick), or panting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tongue {
    Blep,
    Pant,
}

/// Fidgets with their weights.
type Kit = &'static [(Kind, u32)];

/// Each species' fidgets awake, with weights, and asleep.
fn kit(species: &str) -> (Kit, Kit) {
    use Kind::*;
    match species {
        "shiba" => (
            &[
                (Wag, 5),
                (Pant, 4),
                (Sniff, 4),
                (HeadTilt, 3),
                (PawShuffle, 3),
                (WeightShift, 3),
            ],
            &[(Dream, 3), (TipFlick, 1)],
        ),
        "fold" => (
            &[
                (TipFlick, 5),
                (Lick, 3),
                (PawScratch, 3),
                (Knead, 3),
                (HeadTilt, 2),
                (Sniff, 2),
                (WeightShift, 2),
            ],
            &[(TipFlick, 3), (Dream, 1)],
        ),
        "munchkin" => (
            &[
                (TipFlick, 4),
                (Blep, 4),
                (Paddle, 3),
                (WeightShift, 3),
                (Sniff, 2),
                (HeadTilt, 2),
                (PawScratch, 2),
            ],
            &[(TipFlick, 3), (Dream, 2)],
        ),
        "fox" => (
            &[
                (Ripple, 5),
                (Jingle, 3),
                (Sniff, 3),
                (Lick, 3),
                (HeadTilt, 2),
                (PawShuffle, 2),
                (WeightShift, 2),
            ],
            &[(Ripple, 2), (Dream, 2)],
        ),
        "owl" => (
            &[
                (HeadBob, 5),
                (Ruffle, 4),
                (WingShift, 3),
                (ToeGrip, 3),
                (HeadTilt, 4),
            ],
            &[(Ruffle, 1), (WingShift, 2)],
        ),
        "chick" => (
            &[
                (HeadTilt, 5),
                (Flutter, 4),
                (FootScratch, 3),
                (Ruffle, 2),
                (WingShift, 2),
            ],
            &[(Ruffle, 2), (WingShift, 1)],
        ),
        "duck" => (
            &[
                (Waggle, 5),
                (Dabble, 4),
                (HeadShake, 3),
                (WingShift, 3),
                (FootScratch, 2),
                (HeadTilt, 2),
            ],
            &[(Waggle, 1), (WingShift, 2)],
        ),
        _ => (&[(HeadTilt, 1), (WeightShift, 1)], &[(Dream, 1)]),
    }
}

/// Awake, the longest gap between one fidget's end and the next's start,
/// seconds, for the laziest pet (a lively one waits less): with breathing
/// and the tail, nothing is ever still longer than this.
pub const MAX_GAP: f32 = 1.0;
/// Asleep, the gaps: only now and then.
const SLEEP_GAP: (f32, f32) = (1.5, 4.0);
/// An ear twitches every this many seconds, each on its own clock.
const EAR_EVERY: (f32, f32) = (1.6, 4.5);
const EAR_EVERY_ASLEEP: (f32, f32) = (5.0, 11.0);
/// A fidget cut short fades out over this long, seconds.
const FADE_SECS: f32 = 0.3;
/// An ear flick's length, seconds.
pub const EAR_SECS: f32 = 0.45;
/// The ears turn to listen somewhere new every this many seconds.
const SWIVEL_EVERY: (f32, f32) = (2.0, 5.0);

/// A fidget under way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run {
    pub kind: Kind,
    /// Seconds in, and how long it runs.
    pub t: f32,
    pub len: f32,
    /// −1 or +1: which side (a paw, a tilt, a wing).
    pub side: f32,
    /// 1 while it runs; cut short, it falls to 0 over [`FADE_SECS`].
    pub fade: f32,
}

impl Run {
    /// 0 … 1 … 0 over its length, eased in and out (and faded, cut short).
    pub fn e(&self) -> f32 {
        let x = (self.t / self.len).clamp(0.0, 1.0);
        (std::f32::consts::PI * x).sin().sqrt() * self.fade
    }
}

/// What the pet's body is doing, as the fidgets see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    /// A pose held (sitting, idling, a feeling): any fidget.
    Free,
    /// A gesture moves the body: only the tail's.
    Busy,
    /// Walking: none.
    Off,
}

/// What the fidgets come to this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Out {
    pub run: Option<Run>,
    /// Each ear's twitch (left, right): seconds into it, if one is on.
    pub ears: [Option<f32>; 2],
    /// Where the ears listen: −1 … 1 (+ toward the pet's left), eased.
    pub swivel: f32,
    /// The slow drift under everything: its clock (seconds), and how much
    /// of it shows (0 walking … 1 at rest, eased; less asleep or busy).
    pub t: f32,
    pub calm: f32,
}

impl Out {
    /// Needs smooth frames: a quick fidget or an ear flick is on.
    pub fn quick(&self) -> bool {
        self.run.is_some_and(|r| r.kind.quick()) || self.ears.iter().any(Option::is_some)
    }

    pub fn tongue(&self) -> Option<Tongue> {
        self.run.and_then(|r| r.kind.tongue())
    }

    /// The fidget under way if it is `kind`: how far in (0 … 1 … 0),
    /// seconds in, and its side.
    pub fn get(&self, kind: Kind) -> Option<(f32, f32, f32)> {
        self.run
            .filter(|r| r.kind == kind)
            .map(|r| (r.e(), r.t, r.side))
    }
}

/// The fidget clock of one pet.
pub struct Fidgets {
    awake: Kit,
    asleep: Kit,
    /// 0 lazy … 1 lively ([`chonkgene_core::Personality::energy`]).
    energy: f32,
    seed: u32,
    t: f32,
    run: Option<Run>,
    last: Option<Kind>,
    next: f32,
    ears: [(f32, Option<f32>); 2],
    swivel: (f32, f32, f32),
    calm: f32,
    /// Off: none at all (tests and looks that compare poses).
    off: bool,
}

impl Fidgets {
    pub fn new(species: &str, seed: u32) -> Self {
        let (awake, asleep) = kit(species);
        let mut f = Self {
            awake,
            asleep,
            energy: 0.5,
            seed: seed | 1,
            t: 0.0,
            run: None,
            last: None,
            next: 0.0,
            ears: [(0.0, None); 2],
            swivel: (0.0, 0.0, 0.0),
            calm: 0.0,
            off: false,
        };
        f.next = f.between(0.3, MAX_GAP);
        for k in 0..2 {
            f.ears[k].0 = f.between(0.5, EAR_EVERY.1);
        }
        f.swivel.2 = f.between(SWIVEL_EVERY.0, SWIVEL_EVERY.1);
        // Each pet drifts on a clock of its own.
        f.t = f.between(0.0, 60.0);
        f
    }

    /// How lively the pet is (0 … 1): a lively one fidgets more often.
    pub fn set_energy(&mut self, energy: f32) {
        self.energy = energy.clamp(0.0, 1.0);
    }

    /// No fidgets at all from now on, so poses can be compared (tests,
    /// looks of a single pose).
    pub fn stop(&mut self) {
        self.off = true;
    }

    /// The fidgets this species does awake.
    pub fn kinds(&self) -> impl Iterator<Item = Kind> + '_ {
        self.awake.iter().map(|k| k.0)
    }

    fn roll(&mut self) -> f32 {
        // xorshift32
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn between(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.roll()
    }

    /// Pick one from `kit` by weight, not the one just done if there is
    /// another.
    fn pick(&mut self, kit: &[(Kind, u32)]) -> Kind {
        let last = self.last;
        let total: u32 = kit
            .iter()
            .filter(|k| kit.len() < 2 || Some(k.0) != last)
            .map(|k| k.1)
            .sum();
        let mut at = self.roll() * total as f32;
        for &(kind, w) in kit {
            if kit.len() >= 2 && Some(kind) == last {
                continue;
            }
            at -= w as f32;
            if at < 0.0 {
                return kind;
            }
        }
        kit[0].0
    }

    /// Advance `dt` seconds, with the body doing `hold`; `asleep` keeps to
    /// the quiet ones.
    pub fn step(&mut self, dt: f32, hold: Hold, asleep: bool) -> Out {
        if self.off {
            return Out::default();
        }
        self.t += dt;
        let allowed = |kind: Kind| match hold {
            Hold::Free => !asleep || asleep_ok(kind),
            Hold::Busy => kind.light(),
            Hold::Off => false,
        };
        if let Some(r) = &mut self.run {
            // A walk or a gesture takes over the body: it fades out from
            // where it was.
            if !allowed(r.kind) || r.fade < 1.0 {
                r.fade -= dt / FADE_SECS;
            } else {
                r.t += dt;
            }
            if r.t >= r.len || r.fade <= 0.0 {
                self.run = None;
                let gap = if asleep {
                    self.between(SLEEP_GAP.0, SLEEP_GAP.1)
                } else {
                    // A lively pet waits at most two thirds as long.
                    self.between(0.15, MAX_GAP * (1.0 - 0.35 * self.energy))
                };
                self.next = self.t + gap;
            }
        }
        let kit: Vec<(Kind, u32)> = if asleep { self.asleep } else { self.awake }
            .iter()
            .copied()
            .filter(|&(k, _)| allowed(k))
            .collect();
        if self.run.is_none() && !kit.is_empty() && self.t >= self.next {
            let kind = self.pick(&kit);
            let (lo, hi) = kind.secs();
            let len = self.between(lo, hi);
            let side = if self.roll() < 0.5 { -1.0 } else { 1.0 };
            self.run = Some(Run {
                kind,
                t: 0.0,
                len,
                side,
                fade: 1.0,
            });
            self.last = Some(kind);
        }
        // The ears, each on its own clock, walking too.
        let every = if asleep { EAR_EVERY_ASLEEP } else { EAR_EVERY };
        let mut ears = [None; 2];
        for (k, ear) in ears.iter_mut().enumerate() {
            if let Some(since) = self.ears[k].1 {
                let t = self.t - since;
                if t < EAR_SECS {
                    *ear = Some(t);
                } else {
                    self.ears[k].1 = None;
                    self.ears[k].0 = self.t + self.between(every.0, every.1);
                }
            } else if self.t >= self.ears[k].0 {
                self.ears[k].1 = Some(self.t);
                *ear = Some(0.0);
                // Now and then both ears go together.
                let other = 1 - k;
                if self.ears[other].1.is_none() && self.roll() < 0.25 {
                    self.ears[other].1 = Some(self.t);
                }
            }
        }
        // The ears listen somewhere new now and then, gliding there.
        if self.t >= self.swivel.2 {
            self.swivel.1 = if asleep { 0.0 } else { self.between(-1.0, 1.0) };
            self.swivel.2 = self.t + self.between(SWIVEL_EVERY.0, SWIVEL_EVERY.1);
        }
        self.swivel.0 += (self.swivel.1 - self.swivel.0) * (dt * 2.5).min(1.0);
        let calm = match (hold, asleep) {
            (Hold::Off, _) => 0.0,
            (_, true) => 0.35,
            (Hold::Busy, false) => 0.5,
            (Hold::Free, false) => 1.0,
        };
        self.calm += (calm - self.calm) * (dt * 2.0).min(1.0);
        Out {
            run: self.run,
            ears,
            swivel: self.swivel.0,
            t: self.t,
            calm: self.calm,
        }
    }
}

/// Fidgets that may go on as the pet drops off.
fn asleep_ok(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Dream | Kind::TipFlick | Kind::Ripple | Kind::Ruffle | Kind::WingShift | Kind::Waggle
    )
}

/// An ear's twitch at `t` seconds into it: a quick flap out that rings
/// down, −1 … 1.
pub fn ear_flick(t: f32) -> f32 {
    use std::f32::consts::TAU;
    let fade = (1.0 - t / EAR_SECS).clamp(0.0, 1.0);
    (TAU * 7.0 * t).sin() * fade * fade
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 30.0;

    #[test]
    fn awake_one_fidget_follows_another_with_short_gaps() {
        for species in ["shiba", "fold", "munchkin", "fox", "owl", "chick", "duck"] {
            let mut f = Fidgets::new(species, 7);
            f.set_energy(0.0);
            let (mut gap, mut worst) = (0.0f32, 0.0f32);
            let mut seen = std::collections::HashSet::new();
            for _ in 0..(120.0 / DT) as usize {
                let out = f.step(DT, Hold::Free, false);
                match out.run {
                    Some(r) => {
                        seen.insert(r.kind);
                        gap = 0.0;
                    }
                    None => gap += DT,
                }
                worst = worst.max(gap);
            }
            assert!(worst <= MAX_GAP + 2.0 * DT, "{species}: still {worst:.2} s");
            let all: std::collections::HashSet<Kind> = f.kinds().collect();
            assert_eq!(seen, all, "{species} skipped some of its fidgets");
        }
    }

    #[test]
    fn no_fidget_while_walking_and_only_quiet_ones_asleep() {
        let mut f = Fidgets::new("shiba", 3);
        for _ in 0..(20.0 / DT) as usize {
            assert_eq!(f.step(DT, Hold::Off, false).run, None);
        }
        for _ in 0..(60.0 / DT) as usize {
            if let Some(r) = f.step(DT, Hold::Free, true).run {
                assert!(matches!(r.kind, Kind::Dream | Kind::TipFlick), "{r:?}");
            }
        }
    }

    #[test]
    fn in_a_gesture_only_the_tail_fidgets_and_a_cut_fidget_fades() {
        let mut f = Fidgets::new("shiba", 5);
        let mut seen = false;
        for _ in 0..(60.0 / DT) as usize {
            if let Some(r) = f.step(DT, Hold::Busy, false).run {
                assert!(r.kind.light(), "{r:?} in a gesture");
                seen = true;
            }
        }
        assert!(seen, "the tail kept still through every gesture");
        // A sniff under way, then the pet sets off: it fades, no snap.
        let mut f = Fidgets::new("fox", 9);
        let run = loop {
            if let Some(r) = f.step(DT, Hold::Free, false).run
                && !r.kind.light()
                && r.t > 0.1
            {
                break r;
            }
        };
        let mut last = run.e();
        let mut frames = 0;
        while let Some(r) = f.step(DT, Hold::Off, false).run {
            assert!(r.e() <= last + 1e-3, "it grew again");
            assert!(last - r.e() < 0.2, "it snapped from {last} to {}", r.e());
            last = r.e();
            frames += 1;
        }
        assert!(frames >= 3, "gone in {frames} frames");
    }

    #[test]
    fn the_drift_shows_at_rest_and_not_walking() {
        let mut f = Fidgets::new("owl", 3);
        let mut calm = 0.0;
        for _ in 0..(3.0 / DT) as usize {
            calm = f.step(DT, Hold::Free, false).calm;
        }
        assert!(calm > 0.95, "{calm}");
        for _ in 0..(3.0 / DT) as usize {
            calm = f.step(DT, Hold::Off, false).calm;
        }
        assert!(calm < 0.05, "{calm}");
    }

    #[test]
    fn the_ears_twitch_on_clocks_of_their_own() {
        let mut f = Fidgets::new("fox", 11);
        let (mut left, mut right, mut apart) = (0, 0, 0);
        for _ in 0..(60.0 / DT) as usize {
            let out = f.step(DT, Hold::Free, false);
            let [l, r] = out.ears.map(|e| e == Some(0.0));
            left += l as u32;
            right += r as u32;
            apart += (l != r) as u32;
        }
        assert!(left >= 10 && right >= 10, "{left} {right}");
        assert!(apart >= 10, "the ears flick in step");
    }
}
