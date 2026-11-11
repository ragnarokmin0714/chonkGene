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
/// A squabble has its ups and downs (Roger, 2026-11-06: making up at once
/// made no sense): rounds of huffing before the chase, sulking back to
/// back after it, then one comes over to say sorry, is turned down once,
/// and is forgiven.
const ARGUE_ROUNDS: u32 = 3;
const ARGUE_EVERY: f64 = 1.3;
const SULK_SECS: f64 = 5.0;
const SORRY_SECS: f64 = 3.0;
const TURNED_DOWN_SECS: f64 = 2.0;
/// Playing, pets come this close (pixels between centres): their bodies
/// touch, the windows overlapping.
const CONTACT: f32 = 95.0;
/// Each turn of a play-fight, seconds, and how many turns.
const PLAY_TURN: f64 = 1.9;
const PLAY_TURNS: u32 = 6;
/// The same pair rests at least this long between skits.
const PAIR_COOLDOWN: f64 = 180.0;
/// Partway through a play-fight (before this turn) they tussle: a cloud of
/// dust rolling about for this long.
const TUSSLE_TURN: u32 = 3;
const TUSSLE_SECS: f32 = 2.6;
/// They run at each other for a tussle at most this long.
const GATHER_SECS: f64 = 3.0;
/// The fox's spell (定身術): casting, then the others held up in the air.
const CAST_SECS: f64 = 1.3;
const FLOAT_SECS: f32 = 3.6;
/// Of skits with the fox free to start one, this share are a spell.
const SPELL_SHARE: f32 = 0.35;
/// Called over by the fox (招喚術): they come running for this long, and
/// line up this far apart either side of it.
const CALL_SECS: f64 = 5.0;
const CALL_GAP: f32 = 80.0;
/// Approaching, two pets still this far apart when the time is up give it
/// up: across the screens they glared and huffed at no one (Roger: they
/// seemed to squabble with themselves).
const TOO_FAR: f32 = 2.0 * MEET;

/// A pet as the director sees it.
#[derive(Clone, Copy, Debug)]
pub struct Actor {
    pub id: u64,
    pub species: Species,
    /// Horizontal centre on screen, pixels.
    pub x: f32,
    /// Awake, not held, not grumpy, not busy: free to squabble.
    pub available: bool,
    /// Asleep where it lies (not held, not falling): the fox's 定身術
    /// floats it up in a dream bubble without waking it.
    pub asleep: bool,
    /// Its own energy (see [`crate::Personality`]): bold or timid when the
    /// fox casts.
    pub energy: f32,
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
    /// Tussle with `with` for `secs`: the two vanish into one cloud of dust
    /// at `id`, paws and stars poking out, and come out of it panting.
    Tussle {
        id: u64,
        with: u64,
        secs: f32,
    },
    /// The fox casts a spell: up on its hind legs, tails fanned, the bell
    /// ringing, a ring of fox-fire round its feet.
    Cast {
        id: u64,
    },
    /// Held up in the air by the spell, frozen mid-pose in a bubble of
    /// light, for `secs`; then it drops softly onto its feet.
    Freeze {
        id: u64,
        secs: f32,
    },
    /// Just put down by the spell, frightened: it jumps back from the fox
    /// (at `from`), starts, and sweats.
    Scare {
        id: u64,
        from: f32,
    },
    /// Blessed by the fox (祝福術): hearts, a happy hop.
    Bless {
        id: u64,
    },
    /// Near the fox as it casts, it sees the fire go up (at `from`) and
    /// shows how it takes it.
    Awe {
        id: u64,
        from: f32,
        how: Awe,
    },
}

impl Cmd {
    /// The pet it is for.
    pub fn id(&self) -> u64 {
        match *self {
            Cmd::Walk { id, .. }
            | Cmd::Face { id, .. }
            | Cmd::Say { id, .. }
            | Cmd::Express { id, .. }
            | Cmd::Act { id, .. }
            | Cmd::Hop { id }
            | Cmd::Tussle { id, .. }
            | Cmd::Cast { id }
            | Cmd::Freeze { id, .. }
            | Cmd::Scare { id, .. }
            | Cmd::Awe { id, .. }
            | Cmd::Bless { id } => id,
        }
    }
}

/// How a pet near the fox takes its spell's fire going up (Roger's
/// fourteenth pass: some frightened, some excited).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Awe {
    /// Timid: shrinks back, ears flat, a drop of sweat.
    Shrink,
    /// Bold or playful: hops and bounces, thrilled.
    Bounce,
    /// A cat: crouches, bottom wiggling, and pounces at the flames.
    Crouch,
}

impl Awe {
    /// How `species` with `energy` takes it: a cat with any go in it
    /// stalks the flames; otherwise the lively are thrilled, the rest
    /// frightened.
    pub fn of(species: Species, energy: f32) -> Self {
        let cat = matches!(species, Species::Munchkin | Species::Fold);
        if cat && energy >= AWE_CAT {
            Awe::Crouch
        } else if energy >= AWE_BOLD {
            Awe::Bounce
        } else {
            Awe::Shrink
        }
    }
}

/// A cat with this much energy or more stalks the fox's flames...
const AWE_CAT: f32 = 0.35;
/// ...and anyone this lively is thrilled by them; the rest shrink back.
const AWE_BOLD: f32 = 0.6;
/// Pets this near the fox (pixels between middles) see its spell go up.
const AWE_REACH: f32 = 700.0;

/// The fox's spells (PLAN §8, Roger's eleventh and thirteenth passes): the
/// director casts 定身術 now and then by itself; all of them can be asked
/// for from the fox's menu (施法…).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spell {
    /// 定身術: every other pet held up in the air for a moment.
    Freeze,
    /// 祝福術: hearts for everyone.
    Bless,
    /// 招喚術: everyone comes running to the fox.
    Call,
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
    /// A play-fight: up close, in turns, each its own way, with a tussle
    /// in the middle.
    Play,
    /// The fox's 定身術: every other pet held up in the air for a moment
    /// (the owl stares it back: the fox floats instead).
    Spell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Approach,
    Glare,
    Chase,
    Talk(u32),
    /// Huffing at each other in turn, round `n`.
    Argue(u32),
    /// Back to back, sulking.
    Sulk,
    /// One comes over to say sorry; at step 1 it was turned down once and
    /// offers a paw.
    Sorry(u32),
    Shrug,
    MakeUp,
    /// Turn `n` of a play-fight.
    Play(u32),
    /// Running into each other for a tussle, before turn `n` of a
    /// play-fight.
    Gather(u32),
    /// Tussling, before turn `n` of a play-fight.
    Tussle(u32),
    /// The fox casting.
    Cast,
    /// Pets held up by the spell.
    Float,
    /// Pets running to the fox, called.
    Called,
}

#[derive(Clone, Copy, Debug)]
struct Skit {
    kind: Kind,
    a: u64,
    b: u64,
    phase: Phase,
    until: f64,
    /// Which spell, for [`Kind::Spell`].
    spell: Spell,
}

#[derive(Clone, Debug, Default)]
pub struct Director {
    next_at: Option<f64>,
    skit: Option<Skit>,
    rested: HashMap<(u64, u64), f64>,
    /// The next skit is of this kind (see [`Director::rush`]).
    forced: Option<Kind>,
    /// Called over by the fox (招喚術): theirs to walk while it lasts.
    called: Vec<u64>,
}

/// Mean seconds between skits for 偶爾 / 常常 / 超常 (Roger: they hardly
/// ever played; more often since 2026-11-06).
fn mean_gap(frequency: u8) -> f64 {
    match frequency {
        0 => 600.0,
        1 => 180.0,
        _ => 60.0,
    }
}

fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

impl Director {
    /// For trying skits out (`CHONKGENE_SKIT`): the next one starts within
    /// seconds, and is a `spell`, a `play`-fight or a `squabble` as named
    /// (anything else: as it comes). True if the name was known.
    pub fn rush(&mut self, kind: &str) -> bool {
        self.next_at = Some(3.0);
        self.forced = match kind {
            "spell" => Some(Kind::Spell),
            "play" => Some(Kind::Play),
            "squabble" => Some(Kind::GlareChase),
            _ => None,
        };
        self.forced.is_some()
    }

    /// Is this pet in a skit right now? The app leaves its walking to us.
    pub fn involves(&self, id: u64) -> bool {
        self.skit.is_some_and(|s| {
            s.a == id || s.b == id || (s.phase == Phase::Called && self.called.contains(&id))
        })
    }

    /// The skit under way, for the log: what, who, and how far along.
    pub fn describe(&self) -> Option<String> {
        let s = self.skit?;
        let spell = if s.kind == Kind::Spell {
            format!(" {:?}", s.spell)
        } else {
            String::new()
        };
        let called = if s.phase == Phase::Called {
            format!(", called {:?}", self.called)
        } else {
            String::new()
        };
        Some(format!(
            "{:?}{spell} of {} and {}, {:?}{called}",
            s.kind, s.a, s.b, s.phase
        ))
    }

    /// Is `id` in a play-fight (not a squabble)?
    pub fn playing(&self, id: u64) -> bool {
        self.involves(id) && self.skit.is_some_and(|s| s.kind == Kind::Play)
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
        // Held up by a spell, or sulking in the skit, a pet is not free,
        // but the skit goes on.
        let own_mood = matches!(s.phase, Phase::Argue(_) | Phase::Sulk | Phase::Sorry(_));
        if !own_mood
            && !matches!(s.phase, Phase::MakeUp | Phase::Float)
            && (!a.available || !b.available)
        {
            return self.make_up(now, actors);
        }

        let mut out = Vec::new();
        match s.phase {
            Phase::Approach => {
                let meet = if s.kind == Kind::Play { CONTACT } else { MEET };
                if now >= s.until && (a.x - b.x).abs() > TOO_FAR {
                    // Could not get to each other (a gap, an edge, other
                    // screens): nothing comes of it.
                    self.rested.insert(pair(s.a, s.b), now);
                    self.skit = None;
                    return Vec::new();
                }
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
                        // A spell starts casting at once, never approaching.
                        Kind::Spell => {}
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
                s.phase = Phase::Argue(0);
                s.until = now;
            }
            Phase::Argue(round) if now >= s.until => {
                if round >= ARGUE_ROUNDS {
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
                } else {
                    // Turn and turn about, each huffier: a stamp of a hop.
                    let (me, other) = if round % 2 == 0 { (b, a) } else { (a, b) };
                    out.extend([
                        Cmd::Face {
                            id: me.id,
                            x: other.x,
                        },
                        Cmd::Say {
                            id: me.id,
                            line: Line::SkitHuff,
                        },
                        Cmd::Hop { id: me.id },
                        Cmd::Express {
                            id: me.id,
                            expression: Expression::FedUp,
                        },
                    ]);
                    s.phase = Phase::Argue(round + 1);
                    s.until = now + ARGUE_EVERY;
                }
            }
            Phase::Chase => {
                if now >= s.until {
                    // Run out of steam: back to back, sulking.
                    s.phase = Phase::Sulk;
                    s.until = now + SULK_SECS;
                    for (p, q) in [(a, b), (b, a)] {
                        out.extend([
                            Cmd::Face {
                                id: p.id,
                                x: 2.0 * p.x - q.x,
                            },
                            Cmd::Express {
                                id: p.id,
                                expression: Expression::Sulk,
                            },
                        ]);
                    }
                    self.skit = Some(s);
                    return out;
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
            Phase::Play(turn) if now >= s.until && turn == TUSSLE_TURN => {
                s.phase = Phase::Gather(turn);
                s.until = now + GATHER_SECS;
            }
            Phase::Gather(turn) => {
                if (a.x - b.x).abs() <= CONTACT || now >= s.until {
                    // Into a tussle, the two rolling about in one cloud.
                    out.push(Cmd::Tussle {
                        id: a.id,
                        with: b.id,
                        secs: TUSSLE_SECS,
                    });
                    s.phase = Phase::Tussle(turn);
                    s.until = now + f64::from(TUSSLE_SECS) + 0.4;
                } else {
                    // At each other, full tilt.
                    out.extend([
                        Cmd::Walk {
                            id: a.id,
                            x: b.x,
                            pace: 2.6,
                        },
                        Cmd::Walk {
                            id: b.id,
                            x: a.x,
                            pace: 2.6,
                        },
                    ]);
                }
            }
            Phase::Tussle(turn) if now >= s.until => {
                s.phase = Phase::Play(turn + 1);
                s.until = now;
            }
            Phase::Cast if now >= s.until && s.spell == Spell::Bless => {
                self.rested.insert(pair(s.a, s.b), now);
                self.skit = None;
                let mut out = vec![Cmd::Express {
                    id: a.id,
                    expression: Expression::Happy,
                }];
                for p in actors.iter().filter(|p| p.available && p.id != a.id) {
                    out.push(Cmd::Bless { id: p.id });
                }
                return out;
            }
            Phase::Cast if now >= s.until && s.spell == Spell::Call => {
                s.phase = Phase::Called;
                s.until = now + CALL_SECS;
                let cmds = call(a, actors);
                self.called = cmds.iter().map(Cmd::id).collect();
                self.called.dedup();
                out.extend(cmds);
            }
            Phase::Called => {
                if now >= s.until {
                    // Here: everyone pleased, facing the fox.
                    self.rested.insert(pair(s.a, s.b), now);
                    self.skit = None;
                    let mut out = vec![Cmd::Express {
                        id: a.id,
                        expression: Expression::Giggle,
                    }];
                    for p in actors.iter().filter(|p| p.available && p.id != a.id) {
                        out.push(Cmd::Face { id: p.id, x: a.x });
                        out.push(Cmd::Express {
                            id: p.id,
                            expression: Expression::Happy,
                        });
                    }
                    return out;
                }
                // Anyone free by now comes too (one still cowering from
                // the cast at first).
                let cmds = call(a, actors);
                for id in cmds.iter().map(Cmd::id) {
                    if !self.called.contains(&id) {
                        self.called.push(id);
                    }
                }
                out.extend(cmds);
            }
            Phase::Cast if now >= s.until => {
                // The owl stares the spell back: the fox floats instead.
                let bounced = b.species == Species::Owl && b.id != a.id;
                let secs = FLOAT_SECS;
                if bounced {
                    out.push(Cmd::Freeze { id: a.id, secs });
                    out.push(Cmd::Say {
                        id: a.id,
                        line: Line::SkitFloated,
                    });
                } else {
                    for p in actors
                        .iter()
                        .filter(|p| (p.available || p.asleep) && p.id != a.id)
                    {
                        out.push(Cmd::Freeze { id: p.id, secs });
                    }
                    if b.id != a.id {
                        out.push(Cmd::Say {
                            id: b.id,
                            line: Line::SkitFloated,
                        });
                    }
                }
                s.phase = Phase::Float;
                s.until = now + f64::from(secs) + 0.6;
            }
            Phase::Float if now >= s.until => {
                // Down again: the fox giggles; the rest got a fright
                // (Roger: they should look a bit scared), and the one it
                // picked on says so. Bounced back by the owl, the fox is
                // the one put out, and the owl pleased with itself.
                self.rested.insert(pair(s.a, s.b), now);
                self.skit = None;
                if b.species == Species::Owl && b.id != a.id {
                    return vec![
                        Cmd::Express {
                            id: a.id,
                            expression: Expression::FedUp,
                        },
                        Cmd::Express {
                            id: b.id,
                            expression: Expression::Happy,
                        },
                    ];
                }
                let mut out = vec![Cmd::Express {
                    id: a.id,
                    expression: Expression::Giggle,
                }];
                for p in actors.iter().filter(|p| p.id != a.id && p.available) {
                    out.push(Cmd::Scare {
                        id: p.id,
                        from: a.x,
                    });
                }
                if b.id != a.id {
                    out.push(Cmd::Say {
                        id: b.id,
                        line: Line::SkitScared,
                    });
                }
                return out;
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
            Phase::Sulk if now >= s.until => {
                // The one that chased comes over, sorry.
                s.phase = Phase::Sorry(0);
                s.until = now + SORRY_SECS;
                out.extend([
                    Cmd::Express {
                        id: a.id,
                        expression: Expression::Neutral,
                    },
                    Cmd::Say {
                        id: a.id,
                        line: Line::SkitSorry,
                    },
                ]);
            }
            Phase::Sorry(0) if now >= s.until => {
                // Turned down once: a huff over the shoulder. It offers a
                // paw.
                s.phase = Phase::Sorry(1);
                s.until = now + TURNED_DOWN_SECS;
                out.extend([
                    Cmd::Say {
                        id: b.id,
                        line: Line::SkitHuff,
                    },
                    Cmd::Face { id: a.id, x: b.x },
                    Cmd::Act {
                        id: a.id,
                        act: Act::Paw,
                    },
                ]);
            }
            Phase::Sorry(0) => {
                if (a.x - b.x).abs() > MEET {
                    out.push(Cmd::Walk {
                        id: a.id,
                        x: b.x,
                        pace: 0.7,
                    });
                }
            }
            Phase::Sorry(_) if now >= s.until => {
                // Forgiven: face to face, a happy hop each.
                s.phase = Phase::MakeUp;
                s.until = now + MAKE_UP_SECS;
                out.extend([
                    Cmd::Face { id: a.id, x: b.x },
                    Cmd::Face { id: b.id, x: a.x },
                    Cmd::Say {
                        id: b.id,
                        line: Line::SkitForgive,
                    },
                    Cmd::Express {
                        id: a.id,
                        expression: Expression::Happy,
                    },
                    Cmd::Express {
                        id: b.id,
                        expression: Expression::Happy,
                    },
                ]);
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

    /// The fox casts `spell` now, asked from its menu: nothing if it is
    /// not free or another skit is on.
    pub fn cast(&mut self, fox: u64, spell: Spell, now: f64, actors: &[Actor]) -> Vec<Cmd> {
        let Some(f) = actors.iter().find(|p| p.id == fox).copied() else {
            return Vec::new();
        };
        if self.skit.is_some() || !f.available {
            return Vec::new();
        }
        // The nearest one free is the one it picks on; alone, it casts all
        // the same.
        let other = actors
            .iter()
            .filter(|p| p.id != fox && p.available)
            .min_by(|p, q| (p.x - f.x).abs().total_cmp(&(q.x - f.x).abs()))
            .copied()
            .unwrap_or(f);
        self.begin_spell(now, actors, f, other, spell)
    }

    fn begin_spell(
        &mut self,
        now: f64,
        actors: &[Actor],
        fox: Actor,
        other: Actor,
        spell: Spell,
    ) -> Vec<Cmd> {
        self.skit = Some(Skit {
            kind: Kind::Spell,
            a: fox.id,
            b: other.id,
            phase: Phase::Cast,
            until: now + CAST_SECS,
            spell,
        });
        let line = match spell {
            Spell::Freeze => Line::SkitSpell,
            Spell::Bless => Line::SpellBless,
            Spell::Call => Line::SpellCall,
        };
        let mut out = vec![
            Cmd::Face {
                id: fox.id,
                x: other.x,
            },
            Cmd::Cast { id: fox.id },
            Cmd::Say { id: fox.id, line },
        ];
        // The others near it see the fire go up, each in its own way; the
        // owl it picks on stares it out instead (below).
        let stares = spell == Spell::Freeze && other.species == Species::Owl;
        out.extend(
            actors
                .iter()
                .filter(|p| p.id != fox.id && p.available && (p.x - fox.x).abs() <= AWE_REACH)
                .filter(|p| !(stares && p.id == other.id))
                .map(|p| Cmd::Awe {
                    id: p.id,
                    from: fox.x,
                    how: Awe::of(p.species, p.energy),
                }),
        );
        // The owl puffs up and stares.
        if stares && other.id != fox.id {
            out.extend([
                Cmd::Act {
                    id: other.id,
                    act: Act::Flap,
                },
                Cmd::Express {
                    id: other.id,
                    expression: Expression::FedUp,
                },
            ]);
        }
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
        // The fox, now and then, casts a spell on the rest instead.
        let fox = pairs.iter().find_map(|&(a, b)| {
            [(a, b), (b, a)]
                .into_iter()
                .find(|(f, _)| f.species == Species::Fox)
        });
        // Asked for a spell with no fox free yet: wait for one.
        if self.forced == Some(Kind::Spell) && fox.is_none() {
            self.next_at = Some(now + 1.0);
            return Vec::new();
        }
        let forced = self.forced.take();
        let spell = match forced {
            Some(kind) => kind == Kind::Spell,
            None => rng.f32() < SPELL_SHARE,
        };
        if let Some((fox, other)) = fox
            && spell
        {
            return self.begin_spell(now, actors, *fox, *other, Spell::Freeze);
        }
        // A chick and a duckling present always get to talk past each other.
        let talk = pairs.iter().position(|(a, b)| {
            matches!(
                (a.species, b.species),
                (Species::Chick, Species::Duck) | (Species::Duck, Species::Chick)
            )
        });
        let (kind, (a, b)) = match talk {
            _ if matches!(forced, Some(Kind::Play | Kind::GlareChase)) => (
                forced.unwrap_or(Kind::Play),
                pairs[rng.usize(..pairs.len())],
            ),
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
            spell: Spell::Freeze,
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

/// Called over by `fox` (招喚術): every other pet free runs to a place in a
/// line either side of it, the nearest closest.
fn call(fox: Actor, actors: &[Actor]) -> Vec<Cmd> {
    let mut others: Vec<&Actor> = actors
        .iter()
        .filter(|p| p.available && p.id != fox.id)
        .collect();
    others.sort_by(|p, q| (p.x - fox.x).abs().total_cmp(&(q.x - fox.x).abs()));
    let mut taken = [0u32; 2];
    let mut out = Vec::new();
    for p in others {
        let side = if p.x >= fox.x { 1 } else { 0 };
        let k = taken[side] as f32;
        taken[side] += 1;
        let sign = if side == 1 { 1.0 } else { -1.0 };
        let x = fox.x + sign * (CONTACT + CALL_GAP * k);
        if (p.x - x).abs() > 30.0 {
            out.push(Cmd::Walk {
                id: p.id,
                x,
                pace: 2.2,
            });
        } else {
            out.push(Cmd::Face { id: p.id, x: fox.x });
        }
    }
    out
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
                asleep: false,
                energy: 0.5,
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
        let (dir, t, _) = started_with(kind, cast);
        (dir, t)
    }

    /// [`started_as`], with the commands it started with.
    fn started_with(kind: Kind, cast: &[Actor]) -> (Director, f64, Vec<Cmd>) {
        for seed in 0..50 {
            let mut rng = fastrand::Rng::with_seed(seed);
            let mut dir = Director::default();
            let mut t = 0.0;
            let mut first = Vec::new();
            while !dir.involves(1) {
                first = dir.tick(t, true, 2, cast, &mut rng);
                t += 1.0;
            }
            if dir.skit.is_some_and(|s| s.kind == kind) {
                return (dir, t, first);
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
        assert_eq!(lines.last(), Some(&Line::SkitForgive));
        assert!(!dir.involves(1) && !dir.involves(2));
        // Ups and downs (Roger: making up at once made no sense): huffing,
        // the chase, sorry, turned down once, then forgiven.
        let at = |l: Line| lines.iter().position(|&x| x == l).unwrap_or(usize::MAX);
        let huffs = lines.iter().filter(|&&l| l == Line::SkitHuff).count();
        assert!(huffs >= ARGUE_ROUNDS as usize + 2, "{lines:?}");
        assert!(at(Line::SkitChase) < at(Line::SkitSorry), "{lines:?}");
        assert!(at(Line::SkitSorry) < at(Line::SkitForgive), "{lines:?}");
        let last_huff = lines.iter().rposition(|&l| l == Line::SkitHuff).unwrap();
        assert!(
            last_huff > at(Line::SkitSorry),
            "never turned down: {lines:?}"
        );
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

    /// Every command of a skit to its end, the walkers moved 4 px a tick.
    fn run(dir: &mut Director, cast: &mut [Actor], start: f64) -> Vec<Cmd> {
        let mut rng = fastrand::Rng::with_seed(3);
        let mut all = Vec::new();
        let mut now = start;
        for _ in 0..4000 {
            for cmd in dir.tick(now, true, 2, cast, &mut rng) {
                if let Cmd::Walk { id, x, .. } = cmd {
                    let p = cast.iter_mut().find(|p| p.id == id).unwrap();
                    p.x += (x - p.x).clamp(-4.0, 4.0);
                }
                all.push(cmd);
            }
            if !dir.involves(1) {
                break;
            }
            now += 0.05;
        }
        assert!(!dir.involves(1), "never ended");
        all
    }

    #[test]
    fn a_play_fight_has_one_tussle_in_the_middle_up_close() {
        let mut cast = actors([Species::Shiba, Species::Fold], [100.0, 900.0]);
        let (mut dir, t) = started_as(Kind::Play, &cast);
        let mut rng = fastrand::Rng::with_seed(3);
        let (mut tussles, mut turns_before, mut apart) = (0, 0, f32::NAN);
        let mut now = t;
        for _ in 0..4000 {
            for cmd in dir.tick(now, true, 2, &cast, &mut rng) {
                match cmd {
                    Cmd::Walk { id, x, .. } => {
                        let p = cast.iter_mut().find(|p| p.id == id).unwrap();
                        p.x += (x - p.x).clamp(-4.0, 4.0);
                    }
                    Cmd::Act { .. } if tussles == 0 => turns_before += 1,
                    Cmd::Tussle { id: 1, with: 2, .. } => {
                        tussles += 1;
                        apart = (cast[0].x - cast[1].x).abs();
                    }
                    _ => {}
                }
            }
            if !dir.involves(1) {
                break;
            }
            now += 0.05;
        }
        assert_eq!(tussles, 1);
        assert!(turns_before >= 1, "the tussle came before any turn");
        // The Shiba's zoomies take it far off; they run back together.
        assert!(apart <= CONTACT, "tussled {apart} px apart");
    }

    #[test]
    fn the_fox_spell_holds_everyone_else_up_and_gives_them_a_fright() {
        let mut cast = actors([Species::Fox, Species::Munchkin], [100.0, 900.0]);
        cast.push(Actor {
            id: 3,
            species: Species::Shiba,
            x: 500.0,
            available: true,
            asleep: false,
            energy: 0.5,
        });
        let (mut dir, t, mut cmds) = started_with(Kind::Spell, &cast);
        cmds.extend(run(&mut dir, &mut cast, t));
        assert!(cmds.contains(&Cmd::Cast { id: 1 }));
        assert!(cmds.contains(&Cmd::Say {
            id: 1,
            line: Line::SkitSpell
        }));
        let held: Vec<u64> = cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::Freeze { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(held, vec![2, 3]);
        // Put down, they got a fright (Roger, 2026-11-06), the one picked
        // on says so; the fox giggles.
        for id in [2, 3] {
            assert!(cmds.contains(&Cmd::Scare { id, from: 100.0 }), "{cmds:?}");
        }
        assert_eq!(
            cmds.last(),
            Some(&Cmd::Say {
                id: 2,
                line: Line::SkitScared
            })
        );
        assert!(cmds.contains(&Cmd::Express {
            id: 1,
            expression: Expression::Giggle
        }));
        // Nobody walks: a spell is cast from where the fox stands.
        assert!(!cmds.iter().any(|c| matches!(c, Cmd::Walk { .. })));
    }

    /// Roger's fourteenth pass: as the fox casts, the pets near it react
    /// by personality: the timid shrink back, the lively are thrilled, a
    /// cat stalks the flames; one far off does not see it.
    #[test]
    fn pets_near_the_fox_take_its_spell_each_in_their_own_way() {
        let pet = |id, species: Species, x, energy| Actor {
            id,
            species,
            x,
            available: true,
            asleep: false,
            energy,
        };
        let cast = vec![
            pet(1, Species::Fox, 500.0, 0.7),
            pet(2, Species::Fold, 300.0, 0.2),
            pet(3, Species::Munchkin, 700.0, 0.5),
            pet(4, Species::Shiba, 900.0, 0.8),
            pet(5, Species::Duck, 1400.0, 0.3),
        ];
        let cmds = Director::default().cast(1, Spell::Bless, 0.0, &cast);
        let awe = |id| {
            cmds.iter().find_map(|c| match *c {
                Cmd::Awe { id: i, from, how } if i == id => Some((from, how)),
                _ => None,
            })
        };
        assert_eq!(awe(2), Some((500.0, Awe::Shrink)));
        assert_eq!(awe(3), Some((500.0, Awe::Crouch)));
        assert_eq!(awe(4), Some((500.0, Awe::Bounce)));
        assert_eq!(awe(5), None, "too far off to see it");
        assert_eq!(awe(1), None, "the fox itself");
        // A sluggish cat is frightened, not stalking.
        assert_eq!(Awe::of(Species::Fold, 0.2), Awe::Shrink);
        // The owl it picks on stares the spell back instead.
        let owl = vec![
            pet(1, Species::Fox, 500.0, 0.7),
            pet(2, Species::Owl, 600.0, 0.5),
        ];
        let cmds = Director::default().cast(1, Spell::Freeze, 0.0, &owl);
        assert!(
            !cmds.iter().any(|c| matches!(c, Cmd::Awe { .. })),
            "{cmds:?}"
        );
    }

    /// A pet asleep floats up too, in a dream bubble, and is put back down
    /// without waking: no fright for it.
    #[test]
    fn the_spell_floats_a_sleeper_and_lets_it_sleep_on() {
        let mut cast = actors([Species::Fox, Species::Shiba], [100.0, 900.0]);
        cast.push(Actor {
            id: 3,
            species: Species::Fold,
            x: 500.0,
            available: false,
            asleep: true,
            energy: 0.5,
        });
        let mut dir = Director::default();
        dir.cast(1, Spell::Freeze, 0.0, &cast);
        let cmds = until_over(&mut dir, &mut cast, 0.0);
        assert!(cmds.contains(&Cmd::Freeze {
            id: 3,
            secs: FLOAT_SECS
        }));
        assert!(!cmds.iter().any(|c| matches!(c, Cmd::Scare { id: 3, .. })));
        assert!(cmds.iter().any(|c| matches!(c, Cmd::Scare { id: 2, .. })));
    }

    #[test]
    fn the_owl_stares_the_spell_back_and_the_fox_floats() {
        let mut cast = actors([Species::Fox, Species::Owl], [100.0, 900.0]);
        let (mut dir, t, mut cmds) = started_with(Kind::Spell, &cast);
        cmds.extend(run(&mut dir, &mut cast, t));
        let held: Vec<u64> = cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::Freeze { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(held, vec![1]);
        assert!(cmds.contains(&Cmd::Act {
            id: 2,
            act: Act::Flap
        }));
    }

    /// Every command from ticking `dir` until the skit is over.
    fn until_over(dir: &mut Director, cast: &mut [Actor], start: f64) -> Vec<Cmd> {
        let mut rng = fastrand::Rng::with_seed(3);
        let mut all = Vec::new();
        let mut t = start;
        while dir.skit.is_some() {
            for cmd in dir.tick(t, true, 2, cast, &mut rng) {
                if let Cmd::Walk { id, x, .. } = cmd {
                    let p = cast.iter_mut().find(|p| p.id == id).unwrap();
                    p.x += (x - p.x).clamp(-8.0, 8.0);
                }
                all.push(cmd);
            }
            t += 0.05;
            assert!(t < start + 60.0, "never ended");
        }
        all
    }

    #[test]
    fn the_fox_casts_what_its_menu_asks_for() {
        let three = || {
            let mut cast = actors([Species::Fox, Species::Shiba], [500.0, 900.0]);
            cast.push(Actor {
                id: 3,
                species: Species::Chick,
                x: 100.0,
                available: true,
                asleep: false,
                energy: 0.5,
            });
            cast
        };
        // 祝福術: hearts for everyone else, at once after the cast.
        let mut cast = three();
        let mut dir = Director::default();
        let first = dir.cast(1, Spell::Bless, 0.0, &cast);
        assert!(first.contains(&Cmd::Say {
            id: 1,
            line: Line::SpellBless
        }));
        let cmds = until_over(&mut dir, &mut cast, 0.0);
        assert!(cmds.contains(&Cmd::Bless { id: 2 }) && cmds.contains(&Cmd::Bless { id: 3 }));
        // 招喚術: they come running and stand either side of the fox.
        let mut cast = three();
        let mut dir = Director::default();
        dir.cast(1, Spell::Call, 0.0, &cast);
        until_over(&mut dir, &mut cast, 0.0);
        let (shiba, chick) = (cast[1].x, cast[2].x);
        assert!(
            (shiba - (500.0 + CONTACT)).abs() <= 30.0,
            "Shiba at {shiba}"
        );
        assert!(
            (chick - (500.0 - CONTACT)).abs() <= 30.0,
            "chick at {chick}"
        );
        // Called, they are the director's to walk (their own wandering
        // cut the walks short), and one still busy when the cast ends
        // (cowering from it) comes once it is free.
        let mut cast = three();
        cast[2].available = false;
        let mut dir = Director::default();
        dir.cast(1, Spell::Call, 0.0, &cast);
        let mut rng = fastrand::Rng::with_seed(3);
        let mut t = 0.0;
        while dir.skit.is_some_and(|s| s.phase != Phase::Called) {
            dir.tick(t, true, 2, &cast, &mut rng);
            t += 0.05;
        }
        assert!(dir.involves(2) && !dir.involves(3));
        assert!(dir.describe().is_some_and(|d| d.contains("Called")));
        cast[2].available = true;
        dir.tick(t, true, 2, &cast, &mut rng);
        assert!(dir.involves(3));
        until_over(&mut dir, &mut cast, t);
        assert!(
            (cast[2].x - (500.0 - CONTACT)).abs() <= 30.0,
            "chick at {}",
            cast[2].x
        );
        assert!(!dir.involves(2) && dir.describe().is_none());
        // Asked while another skit is on, or while busy: nothing.
        let mut busy = three();
        busy[0].available = false;
        assert!(
            Director::default()
                .cast(1, Spell::Freeze, 0.0, &busy)
                .is_empty()
        );
        // Alone, it casts all the same and nobody floats.
        let mut alone = vec![three()[0]];
        let mut dir = Director::default();
        assert!(!dir.cast(1, Spell::Freeze, 0.0, &alone).is_empty());
        let cmds = until_over(&mut dir, &mut alone, 0.0);
        assert!(
            !cmds
                .iter()
                .any(|c| matches!(c, Cmd::Freeze { .. } | Cmd::Say { .. }))
        );
    }

    /// Roger saw pets squabble with themselves: two that cannot get to
    /// each other in time give it up instead of glaring across the screens.
    #[test]
    fn pets_too_far_apart_give_a_squabble_up() {
        let cast = actors([Species::Shiba, Species::Fold], [100.0, 1900.0]);
        let (mut dir, t) = started_as(Kind::GlareChase, &cast);
        // Stuck where they are (an edge, a gap): nobody walks.
        let mut rng = fastrand::Rng::with_seed(3);
        let mut said = Vec::new();
        let mut now = t;
        while dir.skit.is_some() && now < t + 30.0 {
            said.extend(dir.tick(now, true, 2, &cast, &mut rng));
            now += 0.05;
        }
        assert!(dir.skit.is_none());
        assert!(
            !said.iter().any(|c| matches!(c, Cmd::Say { .. })),
            "{said:?}"
        );
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
        for dt in 0..(PAIR_COOLDOWN as u32 - 61) {
            dir.tick(end + f64::from(dt), true, 2, &cast, &mut rng);
            assert!(!dir.involves(1), "started again too soon");
        }
    }
}
