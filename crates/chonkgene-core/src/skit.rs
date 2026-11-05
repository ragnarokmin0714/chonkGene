//! Irrational mode (PLAN §1.4): two pets squabble over nothing and always
//! make up. The director picks when and who, and turns a skit into simple
//! commands the app carries out.

use std::collections::HashMap;

use crate::anim::{Act, Expression};
use crate::mind::Line;
use crate::species::Species;

/// Pets this close (pixels between centres) are face to face.
const MEET: f32 = 150.0;
const APPROACH_SECS: f64 = 7.0;
const GLARE_SECS: f64 = 2.6;
const CHASE_SECS: f64 = 3.6;
const TALK_EVERY: f64 = 1.3;
const TALK_ROUNDS: u32 = 4;
const SHRUG_SECS: f64 = 1.8;
const MAKE_UP_SECS: f64 = 3.0;
/// Playing, pets come this close (pixels between centres): their bodies
/// touch, the windows overlapping.
const CONTACT: f32 = 95.0;
/// Each turn of a play-fight, seconds, and how many turns.
const PLAY_TURN: f64 = 1.9;
const PLAY_TURNS: u32 = 6;
/// The same pair rests at least this long between skits.
const PAIR_COOLDOWN: f64 = 600.0;

/// A pet as the director sees it.
#[derive(Clone, Copy, Debug)]
pub struct Actor {
    pub id: u64,
    pub species: Species,
    /// Horizontal centre on screen, pixels.
    pub x: f32,
    /// Awake, not held, not grumpy, not busy: free to squabble.
    pub available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    /// Walk towards `x` at `pace` times the normal speed.
    Walk {
        id: u64,
        x: f32,
        pace: f32,
    },
    /// Stand still facing `x`.
    Face {
        id: u64,
        x: f32,
    },
    Say {
        id: u64,
        line: Line,
    },
    Express {
        id: u64,
        expression: Expression,
    },
    /// Do this gesture now.
    Act {
        id: u64,
        act: Act,
    },
    /// A pounce: one big hop.
    Hop {
        id: u64,
    },
}

/// How each species plays at fighting (PLAN §8, Roger's fourth pass,
/// item 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    /// A play bow, then zoomies round and back (the Shiba).
    BowZoom,
    /// One swat, then the back turned (the Fold).
    SwatTurn,
    /// Rolls onto its back and cannot roll back (the Munchkin).
    RollStuck,
    /// Pecks and hops at it (the chick).
    PeckHop,
    /// Quacks and runs off (the duckling).
    QuackFlee,
    /// Puffs up and stares (the owl).
    PuffStare,
    /// Swats with its tails, the bell ringing; tickles with a tail tip
    /// (the fox).
    TailSwat,
}

impl Species {
    pub fn play_move(self) -> Move {
        match self {
            Species::Shiba => Move::BowZoom,
            Species::Fold => Move::SwatTurn,
            Species::Munchkin => Move::RollStuck,
            Species::Chick => Move::PeckHop,
            Species::Duck => Move::QuackFlee,
            Species::Owl => Move::PuffStare,
            Species::Fox => Move::TailSwat,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Stare-off, a chase, making up.
    GlareChase,
    /// 雞同鴨講: chick and duckling talk past each other.
    TalkPast,
    /// A play-fight: up close, in turns, each its own way.
    Play,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Approach,
    Glare,
    Chase,
    Talk(u32),
    Shrug,
    MakeUp,
    /// Turn `n` of a play-fight.
    Play(u32),
}

#[derive(Clone, Copy, Debug)]
struct Skit {
    kind: Kind,
    a: u64,
    b: u64,
    phase: Phase,
    until: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Director {
    next_at: Option<f64>,
    skit: Option<Skit>,
    rested: HashMap<(u64, u64), f64>,
}

/// Mean seconds between skits for 偶爾 / 常常 / 超常.
fn mean_gap(frequency: u8) -> f64 {
    match frequency {
        0 => 900.0,
        1 => 360.0,
        _ => 120.0,
    }
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

impl Director {
    /// Is this pet in a skit right now? The app leaves its walking to us.
    pub fn involves(&self, id: u64) -> bool {
        self.skit.is_some_and(|s| s.a == id || s.b == id)
    }

    /// 勸架 (PLAN §1.4): clicking either pet ends the skit in making up.
    pub fn interrupt(&mut self, id: u64, now: f64, actors: &[Actor]) -> Vec<Cmd> {
        match self.skit {
            Some(s) if (s.a == id || s.b == id) && s.phase != Phase::MakeUp => {
                self.make_up(now, actors)
            }
            _ => Vec::new(),
        }
    }

    fn make_up(&mut self, now: f64, actors: &[Actor]) -> Vec<Cmd> {
        let Some(s) = &mut self.skit else {
            return Vec::new();
        };
        s.phase = Phase::MakeUp;
        s.until = now + MAKE_UP_SECS;
        let (a, b) = (s.a, s.b);
        let x = |id| actors.iter().find(|p| p.id == id).map_or(0.0, |p| p.x);
        vec![
            Cmd::Face { id: a, x: x(b) },
            Cmd::Face { id: b, x: x(a) },
            Cmd::Say {
                id: a,
                line: Line::SkitMakeUp,
            },
            Cmd::Express {
                id: a,
                expression: Expression::Happy,
            },
            Cmd::Express {
                id: b,
                expression: Expression::Happy,
            },
        ]
    }

    pub fn tick(
        &mut self,
        now: f64,
        enabled: bool,
        frequency: u8,
        actors: &[Actor],
        rng: &mut fastrand::Rng,
    ) -> Vec<Cmd> {
        let gap = mean_gap(frequency);
        let next_at = *self
            .next_at
            .get_or_insert(now + gap * (0.5 + f64::from(rng.f32())));
        let Some(mut s) = self.skit else {
            if !enabled || now < next_at {
                return Vec::new();
            }
            self.next_at = Some(now + gap * (0.5 + f64::from(rng.f32())));
            return self.start(now, actors, rng);
        };

        let find = |id| actors.iter().find(|p| p.id == id).copied();
        let (Some(a), Some(b)) = (find(s.a), find(s.b)) else {
            // One of them left the desktop: the skit just ends.
            self.skit = None;
            return Vec::new();
        };
        if s.phase != Phase::MakeUp && (!a.available || !b.available) {
            return self.make_up(now, actors);
        }

        let mut out = Vec::new();
        match s.phase {
            Phase::Approach => {
                let meet = if s.kind == Kind::Play { CONTACT } else { MEET };
                if (a.x - b.x).abs() <= meet || now >= s.until {
                    out.extend([
                        Cmd::Face { id: a.id, x: b.x },
                        Cmd::Face { id: b.id, x: a.x },
                    ]);
                    match s.kind {
                        Kind::GlareChase => {
                            s.phase = Phase::Glare;
                            s.until = now + GLARE_SECS;
                            out.push(Cmd::Say {
                                id: a.id,
                                line: Line::SkitGlare,
                            });
                            out.push(Cmd::Express {
                                id: b.id,
                                expression: Expression::FedUp,
                            });
                        }
                        Kind::TalkPast => {
                            s.phase = Phase::Talk(0);
                            s.until = now;
                        }
                        Kind::Play => {
                            s.phase = Phase::Play(0);
                            s.until = now;
                            out.push(Cmd::Say {
                                id: a.id,
                                line: Line::SkitPlay,
                            });
                        }
                    }
                } else {
                    let mid = (a.x + b.x) / 2.0;
                    out.extend([
                        Cmd::Walk {
                            id: a.id,
                            x: mid,
                            pace: 1.6,
                        },
                        Cmd::Walk {
                            id: b.id,
                            x: mid,
                            pace: 1.6,
                        },
                    ]);
                }
            }
            Phase::Glare if now >= s.until => {
                s.phase = Phase::Chase;
                s.until = now + CHASE_SECS;
                out.extend([
                    Cmd::Say {
                        id: b.id,
                        line: Line::SkitHuff,
                    },
                    Cmd::Say {
                        id: a.id,
                        line: Line::SkitChase,
                    },
                ]);
            }
            Phase::Chase => {
                if now >= s.until {
                    self.skit = Some(s);
                    return self.make_up(now, actors);
                }
                // b runs away from a; a follows a little slower.
                let away = if b.x >= a.x { b.x + 400.0 } else { b.x - 400.0 };
                out.extend([
                    Cmd::Walk {
                        id: b.id,
                        x: away,
                        pace: 2.6,
                    },
                    Cmd::Walk {
                        id: a.id,
                        x: b.x,
                        pace: 2.4,
                    },
                ]);
            }
            Phase::Talk(round) if now >= s.until => {
                if round >= TALK_ROUNDS {
                    s.phase = Phase::Shrug;
                    s.until = now + SHRUG_SECS;
                    out.extend([
                        Cmd::Say {
                            id: a.id,
                            line: Line::SkitShrug,
                        },
                        Cmd::Say {
                            id: b.id,
                            line: Line::SkitShrug,
                        },
                    ]);
                } else {
                    let speaker = if round % 2 == 0 { a.id } else { b.id };
                    out.push(Cmd::Say {
                        id: speaker,
                        line: Line::SkitTalk,
                    });
                    s.phase = Phase::Talk(round + 1);
                    s.until = now + TALK_EVERY;
                }
            }
            Phase::Play(turn) if now >= s.until => {
                if turn >= PLAY_TURNS {
                    // Played out: both pleased, no making up needed.
                    self.rested.insert(pair(s.a, s.b), now);
                    self.skit = None;
                    return vec![
                        Cmd::Face { id: a.id, x: b.x },
                        Cmd::Face { id: b.id, x: a.x },
                        Cmd::Express {
                            id: a.id,
                            expression: Expression::Happy,
                        },
                        Cmd::Express {
                            id: b.id,
                            expression: Expression::Happy,
                        },
                    ];
                }
                let (me, other) = if turn % 2 == 0 { (a, b) } else { (b, a) };
                out.extend(play_turn(me, other));
                s.phase = Phase::Play(turn + 1);
                s.until = now + PLAY_TURN;
            }
            Phase::Play(_) => {
                // Between turns, the zoomies and the flight come back.
                for (p, q) in [(a, b), (b, a)] {
                    if (p.x - q.x).abs() > 2.5 * CONTACT
                        && matches!(p.species.play_move(), Move::BowZoom | Move::QuackFlee)
                        && now >= s.until - 0.9
                    {
                        out.push(Cmd::Walk {
                            id: p.id,
                            x: q.x,
                            pace: 2.6,
                        });
                    }
                }
            }
            Phase::Shrug if now >= s.until => {
                self.skit = Some(s);
                return self.make_up(now, actors);
            }
            Phase::MakeUp if now >= s.until => {
                self.rested.insert(pair(s.a, s.b), now);
                self.skit = None;
                return out;
            }
            _ => {}
        }
        self.skit = Some(s);
        out
    }

    fn start(&mut self, now: f64, actors: &[Actor], rng: &mut fastrand::Rng) -> Vec<Cmd> {
        let free: Vec<&Actor> = actors.iter().filter(|p| p.available).collect();
        let mut pairs = Vec::new();
        for (i, a) in free.iter().enumerate() {
            for b in &free[i + 1..] {
                let rested = self
                    .rested
                    .get(&pair(a.id, b.id))
                    .is_none_or(|t| now - t >= PAIR_COOLDOWN);
                if rested {
                    pairs.push((*a, *b));
                }
            }
        }
        if pairs.is_empty() {
            return Vec::new();
        }
        // A chick and a duckling present always get to talk past each other.
        let talk = pairs.iter().position(|(a, b)| {
            matches!(
                (a.species, b.species),
                (Species::Chick, Species::Duck) | (Species::Duck, Species::Chick)
            )
        });
        let (kind, (a, b)) = match talk {
            Some(i) if rng.bool() => (Kind::TalkPast, pairs[i]),
            // Half the time they play instead of squabbling.
            _ if rng.bool() => (Kind::Play, pairs[rng.usize(..pairs.len())]),
            _ => (Kind::GlareChase, pairs[rng.usize(..pairs.len())]),
        };
        self.skit = Some(Skit {
            kind,
            a: a.id,
            b: b.id,
            phase: Phase::Approach,
            until: now + APPROACH_SECS,
        });
        let mid = (a.x + b.x) / 2.0;
        vec![
            Cmd::Walk {
                id: a.id,
                x: mid,
                pace: 1.6,
            },
            Cmd::Walk {
                id: b.id,
                x: mid,
                pace: 1.6,
            },
        ]
    }
}

/// One pet's turn at a play-fight against `other`: its own move, and how
/// the other takes it.
fn play_turn(me: Actor, other: Actor) -> Vec<Cmd> {
    let away = |d: f32| if me.x >= other.x { me.x + d } else { me.x - d };
    let mut out = match me.species.play_move() {
        Move::BowZoom => vec![
            Cmd::Act {
                id: me.id,
                act: Act::Stretch,
            },
            Cmd::Walk {
                id: me.id,
                x: away(320.0),
                pace: 3.4,
            },
        ],
        Move::SwatTurn => vec![
            Cmd::Act {
                id: me.id,
                act: Act::Paw,
            },
            Cmd::Face {
                id: me.id,
                x: away(100.0),
            },
        ],
        Move::RollStuck => vec![Cmd::Express {
            id: me.id,
            expression: Expression::Giggle,
        }],
        Move::PeckHop => vec![
            Cmd::Act {
                id: me.id,
                act: Act::Peck,
            },
            Cmd::Hop { id: me.id },
        ],
        Move::QuackFlee => vec![
            Cmd::Say {
                id: me.id,
                line: Line::SkitHuff,
            },
            Cmd::Walk {
                id: me.id,
                x: away(260.0),
                pace: 2.8,
            },
        ],
        Move::PuffStare => vec![
            Cmd::Act {
                id: me.id,
                act: Act::Flap,
            },
            Cmd::Express {
                id: me.id,
                expression: Expression::FedUp,
            },
        ],
        Move::TailSwat => vec![Cmd::Act {
            id: me.id,
            act: Act::Swish,
        }],
    };
    // The fox's tail tip tickles: the other one giggles. Anyone else
    // gets a hop of surprise.
    out.push(if me.species == Species::Fox {
        Cmd::Express {
            id: other.id,
            expression: Expression::Giggle,
        }
    } else {
        Cmd::Hop { id: other.id }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actors(species: [Species; 2], xs: [f32; 2]) -> Vec<Actor> {
        (0..2)
            .map(|i| Actor {
                id: i as u64 + 1,
                species: species[i],
                x: xs[i],
                available: true,
            })
            .collect()
    }

    /// Run a skit to the end, moving walkers 4 px per tick; returns every line said.
    fn play(dir: &mut Director, mut cast: Vec<Actor>, start: f64) -> Vec<Line> {
        let mut rng = fastrand::Rng::with_seed(3);
        let mut lines = Vec::new();
        let mut t = start;
        for _ in 0..3000 {
            for cmd in dir.tick(t, true, 2, &cast, &mut rng) {
                match cmd {
                    Cmd::Walk { id, x, .. } => {
                        let p = cast.iter_mut().find(|p| p.id == id).unwrap();
                        p.x += (x - p.x).clamp(-4.0, 4.0);
                    }
                    Cmd::Say { line, .. } => lines.push(line),
                    _ => {}
                }
            }
            if !dir.involves(1) && !lines.is_empty() {
                break;
            }
            t += 0.05;
        }
        lines
    }

    fn started(dir: &mut Director, cast: &[Actor]) -> f64 {
        let mut rng = fastrand::Rng::with_seed(3);
        let mut t = 0.0;
        while !dir.involves(1) {
            dir.tick(t, true, 2, cast, &mut rng);
            t += 1.0;
            assert!(t < 10_000.0, "never started");
        }
        t
    }

    /// A director that has just started a skit of `kind` between pets 1
    /// and 2 (trying seeds until one does), and when.
    fn started_as(kind: Kind, cast: &[Actor]) -> (Director, f64) {
        for seed in 0..50 {
            let mut rng = fastrand::Rng::with_seed(seed);
            let mut dir = Director::default();
            let mut t = 0.0;
            while !dir.involves(1) {
                dir.tick(t, true, 2, cast, &mut rng);
                t += 1.0;
            }
            if dir.skit.is_some_and(|s| s.kind == kind) {
                return (dir, t);
            }
        }
        panic!("never started a {kind:?}");
    }

    #[test]
    fn a_squabble_always_ends_in_making_up() {
        let cast = actors([Species::Shiba, Species::Fold], [100.0, 900.0]);
        let (mut dir, t) = started_as(Kind::GlareChase, &cast);
        let lines = play(&mut dir, cast, t);
        assert_eq!(lines.first(), Some(&Line::SkitGlare));
        assert_eq!(lines.last(), Some(&Line::SkitMakeUp));
        assert!(!dir.involves(1) && !dir.involves(2));
    }

    #[test]
    fn a_play_fight_is_up_close_in_turns_and_ends_happy() {
        let cast = actors([Species::Fox, Species::Munchkin], [100.0, 900.0]);
        let (mut dir, t) = started_as(Kind::Play, &cast);
        let mut cast = cast;
        let mut rng = fastrand::Rng::with_seed(3);
        let (mut closest, mut swats, mut giggles, mut happy) = (f32::MAX, 0, 0, 0);
        let mut now = t;
        for _ in 0..4000 {
            for cmd in dir.tick(now, true, 2, &cast, &mut rng) {
                match cmd {
                    Cmd::Walk { id, x, .. } => {
                        let p = cast.iter_mut().find(|p| p.id == id).unwrap();
                        p.x += (x - p.x).clamp(-4.0, 4.0);
                    }
                    Cmd::Act {
                        id: 1,
                        act: Act::Swish,
                    } => swats += 1,
                    Cmd::Express {
                        id: 2,
                        expression: Expression::Giggle,
                    } => giggles += 1,
                    Cmd::Express {
                        expression: Expression::Happy,
                        ..
                    } => happy += 1,
                    _ => {}
                }
            }
            closest = closest.min((cast[0].x - cast[1].x).abs());
            if !dir.involves(1) {
                break;
            }
            now += 0.05;
        }
        assert!(!dir.involves(1), "never ended");
        assert!(closest <= CONTACT, "never closer than {closest}");
        assert_eq!(swats, 3, "the fox swatted {swats} times");
        assert!(giggles >= 3, "tickled {giggles} times");
        assert_eq!(happy, 2);
    }

    #[test]
    fn breaking_it_up_makes_them_make_up_at_once() {
        let cast = actors([Species::Shiba, Species::Owl], [100.0, 900.0]);
        let (mut dir, t) = started_as(Kind::GlareChase, &cast);
        let cmds = dir.interrupt(2, t + 1.0, &cast);
        assert!(cmds.contains(&Cmd::Say {
            id: 1,
            line: Line::SkitMakeUp
        }));
    }

    #[test]
    fn nothing_happens_when_off_or_alone() {
        let mut rng = fastrand::Rng::with_seed(1);
        let mut dir = Director::default();
        let cast = actors([Species::Shiba, Species::Fold], [100.0, 900.0]);
        for t in 0..20_000 {
            assert!(dir.tick(f64::from(t), false, 2, &cast, &mut rng).is_empty());
        }
        let alone = &cast[..1];
        for t in 0..20_000 {
            assert!(dir.tick(f64::from(t), true, 2, alone, &mut rng).is_empty());
        }
    }

    #[test]
    fn chick_and_duckling_sometimes_talk_past_each_other() {
        let mut talked = false;
        for seed in 0..20 {
            let cast = actors([Species::Chick, Species::Duck], [100.0, 300.0]);
            let mut dir = Director::default();
            let mut rng = fastrand::Rng::with_seed(seed);
            let mut t = 0.0;
            while !dir.involves(1) {
                dir.tick(t, true, 2, &cast, &mut rng);
                t += 1.0;
            }
            if play(&mut dir, cast, t).contains(&Line::SkitTalk) {
                talked = true;
                break;
            }
        }
        assert!(talked);
    }

    #[test]
    fn the_same_pair_rests_between_skits() {
        let cast = actors([Species::Shiba, Species::Fold], [100.0, 900.0]);
        let mut dir = Director::default();
        let t = started(&mut dir, &cast);
        play(&mut dir, cast.clone(), t);
        let mut rng = fastrand::Rng::with_seed(5);
        let end = t + 60.0;
        for dt in 0..300 {
            dir.tick(end + f64::from(dt), true, 2, &cast, &mut rng);
            assert!(!dir.involves(1), "started again too soon");
        }
    }
}
