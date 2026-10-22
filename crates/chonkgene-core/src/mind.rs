//! A pet's moods and memory (PLAN §1.2, §1.12, §1.13): pokes and tickle,
//! sleep and the three ways of waking, grudges, affinity, snacks.
//!
//! The mind decides *what* happens and returns it as [`Out`]; the app shows
//! it (lines in bubbles, expressions, actions, sounds).

use crate::anim::Expression;
use crate::species::{Grumpiness, Species, in_hours};
use crate::tickle::{Reaction, Tickle};
use crate::{Personality, Zone};

/// Experience per level.
pub const XP_PER_LEVEL: u32 = 100;
/// Most experience a pet can gain in one day.
pub const XP_PER_DAY: u32 = 150;
pub const SNACKS_PER_DAY: u8 = 3;
/// Two pokes this close together wake a sleeping pet gently.
const WAKE_POKE_GAP: f64 = 2.0;
/// How long the grumpiness act lasts before its follow-up.
const GRUMPY_SECS: f64 = 3.2;
/// Left alone this long, a pet may doze off when the user is idle or it is night.
const QUIET_BEFORE_SLEEP: f64 = 600.0;
/// The user counts as away after this many idle seconds.
const USER_AWAY: f64 = 900.0;
/// A pet sleeps at least this long before waking by itself.
const MIN_SLEEP: f64 = 120.0;

/// Something the pet says; the app looks it up per species (assets/lines).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Line {
    Hello,
    WelcomeBack,
    PokeSmall,
    Laugh,
    FedUp,
    Overdone,
    StillOverdone,
    MadeUp,
    Pat,
    TurnOver,
    Woke,
    Dizzy,
    Grumpy,
    AfterGrumpy,
    Eat,
    Full,
    SleepTalk,
    GrudgePoke,
    GrudgePat,
    Forgive,
    LevelUp,
}

impl Line {
    pub const ALL: [Line; 21] = [
        Line::Hello,
        Line::WelcomeBack,
        Line::PokeSmall,
        Line::Laugh,
        Line::FedUp,
        Line::Overdone,
        Line::StillOverdone,
        Line::MadeUp,
        Line::Pat,
        Line::TurnOver,
        Line::Woke,
        Line::Dizzy,
        Line::Grumpy,
        Line::AfterGrumpy,
        Line::Eat,
        Line::Full,
        Line::SleepTalk,
        Line::GrudgePoke,
        Line::GrudgePat,
        Line::Forgive,
        Line::LevelUp,
    ];

    /// Key in the line tables.
    pub fn key(self) -> &'static str {
        match self {
            Line::Hello => "hello",
            Line::WelcomeBack => "welcome_back",
            Line::PokeSmall => "poke_small",
            Line::Laugh => "laugh",
            Line::FedUp => "fed_up",
            Line::Overdone => "overdone",
            Line::StillOverdone => "still_overdone",
            Line::MadeUp => "made_up",
            Line::Pat => "pat",
            Line::TurnOver => "turn_over",
            Line::Woke => "woke",
            Line::Dizzy => "dizzy",
            Line::Grumpy => "grumpy",
            Line::AfterGrumpy => "after_grumpy",
            Line::Eat => "eat",
            Line::Full => "full",
            Line::SleepTalk => "sleep_talk",
            Line::GrudgePoke => "grudge_poke",
            Line::GrudgePat => "grudge_pat",
            Line::Forgive => "forgive",
            Line::LevelUp => "level_up",
        }
    }
}

/// What the app should show after an event.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Out {
    pub line: Option<Line>,
    pub expression: Option<Expression>,
    /// Start a species grumpiness act (zoomies, swat, …).
    pub act: Option<Grumpiness>,
    pub level_up: bool,
}

impl Out {
    fn say(line: Line) -> Self {
        Self {
            line: Some(line),
            ..Self::default()
        }
    }

    fn with(mut self, expression: Expression) -> Self {
        self.expression = Some(expression);
        self
    }
}

/// Long-term fondness: only ever goes up (PLAN §0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Affinity {
    pub level: u32,
    pub xp: u32,
    /// Local day number of `gained_today`.
    pub day: i64,
    pub gained_today: u32,
}

impl Default for Affinity {
    fn default() -> Self {
        Self {
            level: 1,
            xp: 0,
            day: 0,
            gained_today: 0,
        }
    }
}

impl Affinity {
    /// Add experience within today's cap; true when a level was gained.
    pub fn gain(&mut self, amount: u32, day: i64) -> bool {
        if day != self.day {
            self.day = day;
            self.gained_today = 0;
        }
        let amount = amount.min(XP_PER_DAY.saturating_sub(self.gained_today));
        self.gained_today += amount;
        self.xp += amount;
        let mut up = false;
        while self.xp >= XP_PER_LEVEL {
            self.xp -= XP_PER_LEVEL;
            self.level += 1;
            up = true;
        }
        up
    }

    /// Total experience ever gained; useful to check it never drops.
    pub fn total(&self) -> u64 {
        u64::from(self.level - 1) * u64::from(XP_PER_LEVEL) + u64::from(self.xp)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Sleep {
    since: f64,
    next_talk: f64,
    /// Went back to sleep in a huff; mumbles less.
    huffy: bool,
}

/// What the app knows about the world this tick.
#[derive(Clone, Copy, Debug)]
pub struct World {
    pub now: f64,
    /// Local hour, 0.0–24.0.
    pub hour: f32,
    /// Local day number (for daily caps).
    pub day: i64,
    /// Seconds since the user last touched keyboard or mouse.
    pub user_idle: f64,
}

#[derive(Clone, Debug)]
pub struct Mind {
    pub species: Species,
    pub personality: Personality,
    pub affinity: Affinity,
    snack_day: i64,
    snacks: u8,
    tickle: Tickle,
    sleep: Option<Sleep>,
    grudge_until: Option<f64>,
    /// When the grumpiness act ends, and whether to fall asleep then.
    act_until: Option<(f64, bool)>,
    last_poke: f64,
    last_touch: f64,
}

impl Mind {
    pub fn new(species: Species, personality: Personality, affinity: Affinity) -> Self {
        Self {
            species,
            personality,
            affinity,
            snack_day: i64::MIN,
            snacks: 0,
            tickle: Tickle::default(),
            sleep: None,
            grudge_until: None,
            act_until: None,
            last_poke: f64::NEG_INFINITY,
            last_touch: 0.0,
        }
    }

    pub fn asleep(&self) -> bool {
        self.sleep.is_some()
    }

    pub fn grumpy(&self) -> bool {
        self.grudge_until.is_some()
    }

    /// Busy with a grumpiness act (zoomies, swat, …).
    pub fn acting(&self) -> bool {
        self.act_until.is_some()
    }

    pub fn snacks_left(&self, day: i64) -> u8 {
        if day == self.snack_day {
            SNACKS_PER_DAY.saturating_sub(self.snacks)
        } else {
            SNACKS_PER_DAY
        }
    }

    fn gain(&mut self, out: &mut Out, amount: u32, day: i64) {
        if self.affinity.gain(amount, day) {
            out.level_up = true;
        }
    }

    fn fall_asleep(&mut self, now: f64, huffy: bool, rng: &mut fastrand::Rng) {
        self.sleep = Some(Sleep {
            since: now,
            next_talk: now + 6.0 + f64::from(rng.f32()) * 6.0,
            huffy,
        });
    }

    fn wake_gently(&mut self, w: &World) -> Out {
        self.sleep = None;
        let mut out = Out::say(Line::Woke).with(Expression::Happy);
        self.gain(&mut out, 5, w.day);
        out
    }

    pub fn poke(&mut self, w: &World, zone: Zone, rng: &mut fastrand::Rng) -> Out {
        self.last_touch = w.now;
        let quick = w.now - self.last_poke < WAKE_POKE_GAP;
        self.last_poke = w.now;
        if self.sleep.is_some() {
            return if quick {
                self.wake_gently(w)
            } else {
                Out::say(Line::TurnOver)
            };
        }
        let mut out = Out::default();
        self.gain(&mut out, 2, w.day);
        if self.acting() {
            return out;
        }
        if self.grumpy() {
            out.line = Some(Line::GrudgePoke);
            return out;
        }
        let (line, expression) = match self.tickle.poke(w.now, zone, &self.personality, rng) {
            Reaction::Laugh => (Line::Laugh, Some(Expression::Giggle)),
            Reaction::FedUp => (Line::FedUp, Some(Expression::FedUp)),
            Reaction::Overdone => (Line::Overdone, Some(Expression::Sulk)),
            Reaction::Ignored => (Line::StillOverdone, None),
            _ => (Line::PokeSmall, None),
        };
        out.line = Some(line);
        out.expression = expression;
        out
    }

    pub fn pat(&mut self, w: &World) -> Out {
        self.last_touch = w.now;
        if self.sleep.is_some() {
            return Out::say(Line::TurnOver);
        }
        if self.grumpy() {
            let mut out = Out::say(Line::GrudgePat);
            self.gain(&mut out, 2, w.day);
            return out;
        }
        let mut out = Out::say(Line::Pat).with(Expression::Happy);
        self.gain(&mut out, 10, w.day);
        out
    }

    pub fn feed(&mut self, w: &World) -> Out {
        self.last_touch = w.now;
        if self.sleep.is_some() {
            return Out::say(Line::TurnOver);
        }
        if self.snacks_left(w.day) == 0 {
            return Out::say(Line::Full);
        }
        if w.day != self.snack_day {
            self.snack_day = w.day;
            self.snacks = 0;
        }
        self.snacks += 1;
        // Food helps a grudge pass (PLAN §1.12).
        if let Some(until) = &mut self.grudge_until {
            *until = until.min(w.now + 3.0);
        }
        let mut out = Out::say(Line::Eat).with(Expression::Happy);
        self.gain(&mut out, 25, w.day);
        out
    }

    pub fn put_to_sleep(&mut self, w: &World, rng: &mut fastrand::Rng) -> Out {
        self.tickle = Tickle::default();
        self.act_until = None;
        self.fall_asleep(w.now, false, rng);
        Out::default().with(Expression::Neutral)
    }

    /// 「輕輕叫醒牠」 from the menu.
    pub fn wake(&mut self, w: &World) -> Out {
        self.last_touch = w.now;
        if self.sleep.is_some() {
            self.wake_gently(w)
        } else {
            Out::default()
        }
    }

    pub fn shake(&mut self, w: &World) -> Out {
        self.last_touch = w.now;
        if self.sleep.is_none() {
            return Out::say(Line::Dizzy);
        }
        self.sleep = None;
        // A grudge of 3–5 minutes; patient pets get over it sooner.
        let grudge = 180.0 + 120.0 * f64::from(1.0 - self.personality.patience.clamp(0.0, 1.0));
        self.grudge_until = Some(w.now + grudge);
        self.act_until = Some((w.now + GRUMPY_SECS, self.species.sleeps_after_grumpiness()));
        Out {
            line: Some(Line::Grumpy),
            expression: Some(Expression::FedUp),
            act: Some(self.species.grumpiness()),
            level_up: false,
        }
    }

    /// Call every frame.
    pub fn tick(&mut self, w: &World, rng: &mut fastrand::Rng) -> Out {
        if let Some((until, back_to_sleep)) = self.act_until
            && w.now >= until
        {
            self.act_until = None;
            if back_to_sleep {
                self.fall_asleep(w.now, true, rng);
            }
            return Out::say(Line::AfterGrumpy).with(Expression::FedUp);
        }
        if let Some(until) = self.grudge_until
            && w.now >= until
        {
            self.grudge_until = None;
            if self.sleep.is_none() && !self.acting() {
                return Out::say(Line::Forgive).with(Expression::Happy);
            }
        }
        match self.tickle.update(w.now, &self.personality) {
            Some(Reaction::MadeUp) => return Out::say(Line::MadeUp).with(Expression::Happy),
            Some(Reaction::CalmedDown) => return Out::default().with(Expression::Neutral),
            _ => {}
        }

        let sleepy = in_hours(w.hour, self.species.sleep_hours());
        let left_alone = w.now - self.last_touch > QUIET_BEFORE_SLEEP;
        if let Some(sleep) = &mut self.sleep {
            let slept = w.now - sleep.since;
            if !sleepy && w.user_idle < 60.0 && slept > MIN_SLEEP && rng.f32() < 0.002 {
                self.sleep = None;
                return Out::say(Line::Woke).with(Expression::Neutral);
            }
            if w.now >= sleep.next_talk {
                sleep.next_talk =
                    w.now + if sleep.huffy { 30.0 } else { 9.0 } + f64::from(rng.f32()) * 8.0;
                return Out::say(Line::SleepTalk);
            }
        } else if !self.acting()
            && !self.grumpy()
            && left_alone
            && (sleepy || w.user_idle > USER_AWAY)
            && rng.f32() < 0.004
        {
            self.fall_asleep(w.now, false, rng);
            return Out::default().with(Expression::Neutral);
        }
        Out::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(now: f64) -> World {
        World {
            now,
            hour: 14.0,
            day: 100,
            user_idle: 0.0,
        }
    }

    fn mind() -> Mind {
        let s = Species::Shiba;
        Mind::new(s, s.personality(), Affinity::default())
    }

    fn rng() -> fastrand::Rng {
        fastrand::Rng::with_seed(9)
    }

    #[test]
    fn one_poke_rolls_a_sleeper_over_and_two_wake_it() {
        let (mut m, mut r) = (mind(), rng());
        m.put_to_sleep(&world(0.0), &mut r);
        assert_eq!(
            m.poke(&world(10.0), Zone::Belly, &mut r).line,
            Some(Line::TurnOver)
        );
        assert!(m.asleep());
        assert_eq!(
            m.poke(&world(11.0), Zone::Belly, &mut r).line,
            Some(Line::Woke)
        );
        assert!(!m.asleep() && !m.grumpy());
    }

    #[test]
    fn shaken_awake_means_grumpiness_then_a_grudge_that_ends() {
        let (mut m, mut r) = (mind(), rng());
        m.put_to_sleep(&world(0.0), &mut r);
        let out = m.shake(&world(5.0));
        assert_eq!(out.act, Some(Grumpiness::Zoomies));
        assert!(m.grumpy());
        assert_eq!(m.tick(&world(9.0), &mut r).line, Some(Line::AfterGrumpy));
        assert_eq!(m.pat(&world(10.0)).line, Some(Line::GrudgePat));
        let mut forgiven = false;
        for t in 11..400 {
            if m.tick(&world(f64::from(t)), &mut r).line == Some(Line::Forgive) {
                forgiven = true;
                assert!(t <= 5 + 300);
                break;
            }
        }
        assert!(forgiven);
    }

    #[test]
    fn a_shaken_fold_swats_and_goes_back_to_sleep() {
        let mut r = rng();
        let mut m = Mind::new(
            Species::Fold,
            Species::Fold.personality(),
            Affinity::default(),
        );
        m.put_to_sleep(&world(0.0), &mut r);
        assert_eq!(m.shake(&world(1.0)).act, Some(Grumpiness::Swat));
        m.tick(&world(5.0), &mut r);
        assert!(m.asleep());
    }

    #[test]
    fn snacks_run_out_and_shorten_grudges() {
        let (mut m, mut r) = (mind(), rng());
        m.put_to_sleep(&world(0.0), &mut r);
        m.shake(&world(1.0));
        m.tick(&world(5.0), &mut r);
        assert_eq!(m.feed(&world(6.0)).line, Some(Line::Eat));
        assert_eq!(m.tick(&world(10.0), &mut r).line, Some(Line::Forgive));
        m.feed(&world(11.0));
        m.feed(&world(12.0));
        assert_eq!(m.feed(&world(13.0)).line, Some(Line::Full));
        let tomorrow = World {
            day: 101,
            ..world(14.0)
        };
        assert_eq!(m.feed(&tomorrow).line, Some(Line::Eat));
    }

    #[test]
    fn affinity_never_goes_down_whatever_happens() {
        let (mut m, mut r) = (mind(), rng());
        let mut last = m.affinity.total();
        for i in 0..5000u32 {
            let w = World {
                now: f64::from(i) * 0.7,
                day: 100 + i64::from(i / 900),
                hour: (i % 24) as f32,
                user_idle: f64::from(i % 1000),
            };
            match r.u8(0..6) {
                0 => {
                    m.poke(&w, Zone::Belly, &mut r);
                }
                1 => {
                    m.pat(&w);
                }
                2 => {
                    m.feed(&w);
                }
                3 => {
                    m.shake(&w);
                }
                4 => {
                    m.put_to_sleep(&w, &mut r);
                }
                _ => {
                    m.tick(&w, &mut r);
                }
            }
            assert!(m.affinity.total() >= last, "affinity dropped at step {i}");
            last = m.affinity.total();
        }
        assert!(m.affinity.level > 1);
    }

    #[test]
    fn the_daily_cap_holds() {
        let mut a = Affinity::default();
        for _ in 0..100 {
            a.gain(25, 7);
        }
        assert_eq!(a.total(), u64::from(XP_PER_DAY));
        a.gain(25, 8);
        assert_eq!(a.total(), u64::from(XP_PER_DAY) + 25);
    }

    #[test]
    fn pets_doze_off_at_night_when_left_alone() {
        let (mut m, mut r) = (mind(), rng());
        let night = |t: f64| World {
            hour: 2.0,
            ..world(t)
        };
        let fell_asleep = (700..5000).any(|t| {
            m.tick(&night(f64::from(t)), &mut r);
            m.asleep()
        });
        assert!(fell_asleep);
    }

    #[test]
    fn pets_stay_up_in_the_afternoon_while_the_user_is_busy() {
        let (mut m, mut r) = (mind(), rng());
        for t in 700..5000 {
            m.tick(&world(f64::from(t)), &mut r);
        }
        assert!(!m.asleep());
    }
}
