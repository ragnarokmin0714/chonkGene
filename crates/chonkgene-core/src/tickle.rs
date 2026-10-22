//! Poke escalation (PLAN §1.2): tickle builds up with quick pokes, decays
//! with a 2 s half-life, and splits into laughing or fed up by personality.

use crate::{Personality, Zone};

const HALF_LIFE: f64 = 2.0;
const BIG_AT: f32 = 2.5;
const OVERDONE_AT: f32 = 6.0;
/// Below this the pet counts as calm again.
const CALM_BELOW: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Belly-up laughing.
    Laugh,
    /// 厭世.
    FedUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Calm,
    Small,
    Big(Route),
    /// The species finale; pokes are ignored until it has calmed down.
    Overdone {
        since_ms: u64,
    },
}

/// What the caller should show after a poke or an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reaction {
    Small,
    Laugh,
    FedUp,
    Overdone,
    /// Poked while overdone: a cold reply, nothing else changes.
    Ignored,
    /// Tickle decayed to nothing.
    CalmedDown,
    /// Back from the finale: comes over to nuzzle the cursor.
    MadeUp,
}

#[derive(Clone, Debug)]
pub struct Tickle {
    level: f32,
    stage: Stage,
    last: f64,
}

impl Default for Tickle {
    fn default() -> Self {
        Self {
            level: 0.0,
            stage: Stage::Calm,
            last: 0.0,
        }
    }
}

fn zone_weight(zone: Zone) -> f32 {
    match zone {
        Zone::Belly => 1.0,
        Zone::Bottom => 0.8,
        Zone::Cheek => 0.7,
        Zone::Paw => 0.5,
        Zone::Head => 0.4,
    }
}

/// How long the finale lasts: 1–3 minutes, shorter for patient pets.
pub fn sulk_secs(p: &Personality) -> f64 {
    60.0 + 120.0 * f64::from(1.0 - p.patience.clamp(0.0, 1.0))
}

impl Tickle {
    pub fn stage(&self) -> Stage {
        self.stage
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    fn decay_to(&mut self, now: f64) {
        let dt = (now - self.last).max(0.0);
        self.level *= 0.5_f64.powf(dt / HALF_LIFE) as f32;
        self.last = now;
    }

    pub fn poke(
        &mut self,
        now: f64,
        zone: Zone,
        personality: &Personality,
        rng: &mut fastrand::Rng,
    ) -> Reaction {
        if let Stage::Overdone { .. } = self.stage {
            return Reaction::Ignored;
        }
        self.decay_to(now);
        self.level += zone_weight(zone);
        if self.level >= OVERDONE_AT {
            self.stage = Stage::Overdone {
                since_ms: (now * 1000.0) as u64,
            };
            return Reaction::Overdone;
        }
        if self.level >= BIG_AT {
            let route = match self.stage {
                // Once a route is picked it holds until the pet calms down.
                Stage::Big(route) => route,
                _ => {
                    let laugh = 0.15 + 0.7 * personality.ticklish.clamp(0.0, 1.0);
                    if rng.f32() < laugh {
                        Route::Laugh
                    } else {
                        Route::FedUp
                    }
                }
            };
            self.stage = Stage::Big(route);
            return match route {
                Route::Laugh => Reaction::Laugh,
                Route::FedUp => Reaction::FedUp,
            };
        }
        if self.stage == Stage::Calm {
            self.stage = Stage::Small;
        }
        Reaction::Small
    }

    /// Call every frame; returns a reaction when the stage ends by itself.
    pub fn update(&mut self, now: f64, personality: &Personality) -> Option<Reaction> {
        match self.stage {
            Stage::Overdone { since_ms } => {
                if now - since_ms as f64 / 1000.0 >= sulk_secs(personality) {
                    *self = Self {
                        last: now,
                        ..Self::default()
                    };
                    return Some(Reaction::MadeUp);
                }
                None
            }
            Stage::Small | Stage::Big(_) => {
                self.decay_to(now);
                (self.level < CALM_BELOW).then(|| {
                    self.level = 0.0;
                    self.stage = Stage::Calm;
                    Reaction::CalmedDown
                })
            }
            Stage::Calm => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Personality = Personality::SHIBA;

    fn pokes(t: &mut Tickle, n: usize, gap: f64, rng: &mut fastrand::Rng) -> Vec<Reaction> {
        (0..n)
            .map(|i| t.poke(i as f64 * gap, Zone::Belly, &P, rng))
            .collect()
    }

    #[test]
    fn one_poke_is_a_small_reaction() {
        let mut t = Tickle::default();
        let mut rng = fastrand::Rng::with_seed(1);
        assert_eq!(pokes(&mut t, 1, 0.0, &mut rng), [Reaction::Small]);
    }

    #[test]
    fn quick_pokes_escalate_to_big_then_overdone() {
        let mut t = Tickle::default();
        let mut rng = fastrand::Rng::with_seed(1);
        // With the 2 s half-life, about nine pokes 0.15 s apart reach the finale.
        let r = pokes(&mut t, 12, 0.15, &mut rng);
        assert_eq!(r[0], Reaction::Small);
        assert!(matches!(r[2], Reaction::Laugh | Reaction::FedUp));
        assert!(r.contains(&Reaction::Overdone));
        assert_eq!(*r.last().unwrap(), Reaction::Ignored);
    }

    #[test]
    fn slow_pokes_never_escalate() {
        let mut t = Tickle::default();
        let mut rng = fastrand::Rng::with_seed(1);
        assert!(
            pokes(&mut t, 20, 6.0, &mut rng)
                .iter()
                .all(|r| *r == Reaction::Small)
        );
    }

    #[test]
    fn the_route_holds_while_big() {
        let mut t = Tickle::default();
        let mut rng = fastrand::Rng::with_seed(3);
        let r = pokes(&mut t, 5, 0.3, &mut rng);
        let bigs: Vec<_> = r
            .iter()
            .filter(|r| matches!(r, Reaction::Laugh | Reaction::FedUp))
            .collect();
        assert!(bigs.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn ticklish_pets_mostly_laugh_and_others_mostly_sigh() {
        let share = |ticklish: f32| {
            let p = Personality { ticklish, ..P };
            let mut rng = fastrand::Rng::with_seed(7);
            let laughs = (0..1000)
                .filter(|_| {
                    let mut t = Tickle::default();
                    (0..3)
                        .map(|i| t.poke(f64::from(i) * 0.1, Zone::Belly, &p, &mut rng))
                        .last()
                        == Some(Reaction::Laugh)
                })
                .count();
            laughs as f32 / 1000.0
        };
        assert!(share(1.0) > 0.75);
        assert!(share(0.0) < 0.25);
    }

    #[test]
    fn calms_down_after_a_while() {
        let mut t = Tickle::default();
        let mut rng = fastrand::Rng::with_seed(1);
        pokes(&mut t, 3, 0.1, &mut rng);
        assert_eq!(t.update(1.0, &P), None);
        assert_eq!(t.update(20.0, &P), Some(Reaction::CalmedDown));
        assert_eq!(t.stage(), Stage::Calm);
    }

    #[test]
    fn overdone_always_makes_up_within_three_minutes() {
        for patience in [0.0, 0.5, 1.0] {
            let p = Personality { patience, ..P };
            let mut t = Tickle::default();
            let mut rng = fastrand::Rng::with_seed(1);
            for i in 0..8 {
                t.poke(f64::from(i) * 0.1, Zone::Belly, &p, &mut rng);
            }
            assert!(matches!(t.stage(), Stage::Overdone { .. }));
            assert_eq!(t.update(59.0, &p), None);
            assert_eq!(t.update(181.0, &p), Some(Reaction::MadeUp));
            assert_eq!(t.stage(), Stage::Calm);
        }
    }
}
