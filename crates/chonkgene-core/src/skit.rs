//! Irrational mode (PLAN §1.4): two pets squabble over nothing and always
//! make up. The director picks when and who, and turns a skit into simple
//! commands the app carries out.

use std::collections::HashMap;

use crate::anim::Expression;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Stare-off, a chase, making up.
    GlareChase,
    /// 雞同鴨講: chick and duckling talk past each other.
    TalkPast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Approach,
    Glare,
    Chase,
    Talk(u32),
    Shrug,
    MakeUp,
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
                if (a.x - b.x).abs() <= MEET || now >= s.until {
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

    #[test]
    fn a_squabble_always_ends_in_making_up() {
        let cast = actors([Species::Shiba, Species::Fold], [100.0, 900.0]);
        let mut dir = Director::default();
        let t = started(&mut dir, &cast);
        let lines = play(&mut dir, cast, t);
        assert_eq!(lines.first(), Some(&Line::SkitGlare));
        assert_eq!(lines.last(), Some(&Line::SkitMakeUp));
        assert!(!dir.involves(1) && !dir.involves(2));
    }

    #[test]
    fn breaking_it_up_makes_them_make_up_at_once() {
        let cast = actors([Species::Shiba, Species::Owl], [100.0, 900.0]);
        let mut dir = Director::default();
        let t = started(&mut dir, &cast);
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
