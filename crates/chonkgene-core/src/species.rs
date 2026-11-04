//! The six species and their defaults (PLAN §1.1, §1.6, §1.12).

use crate::Personality;
use crate::anim::{Act, Habits};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Species {
    Shiba,
    Munchkin,
    Fold,
    Owl,
    Chick,
    Duck,
    Fox,
}

/// How a species goes after a toy (see [`Species::plays_like`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Play {
    /// Crouch, wiggle, pounce.
    Stalk,
    /// Pounce and swat.
    Pounce,
    /// Peck at it low, flap and hop at it high.
    Peck,
}

/// What a species does when shaken awake (PLAN §1.12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grumpiness {
    /// Tears back and forth along the taskbar.
    Zoomies,
    /// Groggy and crooked.
    Dizzy,
    /// One swat, then back to sleep facing away.
    Swat,
    /// Feathers puff up.
    PuffUp,
    /// Spins and plops down.
    Spin,
}

impl Species {
    pub const ALL: [Species; 7] = [
        Species::Shiba,
        Species::Munchkin,
        Species::Fold,
        Species::Owl,
        Species::Chick,
        Species::Duck,
        Species::Fox,
    ];

    /// Stable id used in rigs, saves and line tables.
    pub fn id(self) -> &'static str {
        match self {
            Species::Shiba => "shiba",
            Species::Munchkin => "munchkin",
            Species::Fold => "fold",
            Species::Owl => "owl",
            Species::Chick => "chick",
            Species::Duck => "duck",
            Species::Fox => "fox",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    /// How much it rocks from foot to foot when it walks, 0.0–1.0: birds
    /// waddle, short legs sway, dogs trot.
    pub fn waddle(self) -> f32 {
        match self {
            Species::Shiba | Species::Fold | Species::Fox => 0.35,
            Species::Munchkin => 0.55,
            Species::Owl => 0.7,
            Species::Chick | Species::Duck => 1.0,
        }
    }

    /// Some of its blinks are slow ones: cats (a sign of trust) and the owl.
    pub fn slow_blinks(self) -> bool {
        matches!(self, Species::Munchkin | Species::Fold | Species::Owl)
    }

    /// How keen it is on the grass stalk (狗尾草) on the cursor, 0.0–1.0:
    /// cats can't resist it, the fox and the Shiba chase it, the chick and
    /// duckling go after it a bit, the owl only watches it go by.
    pub fn toy_keen(self) -> f32 {
        match self {
            Species::Munchkin | Species::Fold => 1.0,
            Species::Fox => 0.9,
            Species::Shiba => 0.8,
            Species::Chick | Species::Duck => 0.5,
            Species::Owl => 0.0,
        }
    }

    /// What it does about a toy in reach: cats crouch with their bottoms
    /// up and pounce, dogs and the fox pounce and swat, birds peck at it
    /// low and flap and hop at it high.
    pub fn plays_like(self) -> Play {
        match self {
            Species::Munchkin | Species::Fold => Play::Stalk,
            Species::Shiba | Species::Fox => Play::Pounce,
            Species::Chick | Species::Duck | Species::Owl => Play::Peck,
        }
    }

    /// The little things it does at rest, with how often (see
    /// [`Animator::with_habits`](crate::anim::Animator::with_habits)): dogs
    /// dig and scratch, cats wash, birds preen, peck and flap; everyone
    /// glances about, stretches, yawns, waves and shakes.
    pub fn habits(self) -> Habits {
        match self {
            Species::Shiba => &[
                (Act::LookLeft, 13),
                (Act::LookRight, 13),
                (Act::Stretch, 10),
                (Act::Yawn, 8),
                (Act::Wave, 6),
                (Act::Dig, 18),
                (Act::Scratch, 18),
                (Act::Shake, 12),
            ],
            Species::Munchkin | Species::Fold => &[
                (Act::LookLeft, 13),
                (Act::LookRight, 13),
                (Act::Stretch, 12),
                (Act::Yawn, 10),
                (Act::Wave, 6),
                (Act::Scratch, 14),
                (Act::Groom, 24),
                (Act::Shake, 6),
            ],
            Species::Fox => &[
                (Act::LookLeft, 12),
                (Act::LookRight, 12),
                (Act::Stretch, 10),
                (Act::Yawn, 6),
                (Act::Wave, 6),
                (Act::Dig, 12),
                (Act::Scratch, 12),
                (Act::Groom, 18),
                (Act::Shake, 10),
            ],
            Species::Chick => &[
                (Act::LookLeft, 12),
                (Act::LookRight, 12),
                (Act::Stretch, 8),
                (Act::Yawn, 6),
                (Act::Wave, 6),
                (Act::Peck, 26),
                (Act::Preen, 14),
                (Act::Flap, 10),
                (Act::Shake, 6),
            ],
            Species::Duck => &[
                (Act::LookLeft, 12),
                (Act::LookRight, 12),
                (Act::Stretch, 8),
                (Act::Yawn, 6),
                (Act::Wave, 6),
                (Act::Preen, 22),
                (Act::Flap, 16),
                (Act::Peck, 10),
                (Act::Shake, 8),
            ],
            Species::Owl => &[
                (Act::LookLeft, 18),
                (Act::LookRight, 18),
                (Act::Stretch, 8),
                (Act::Yawn, 12),
                (Act::Wave, 6),
                (Act::Preen, 18),
                (Act::Flap, 10),
                (Act::Shake, 10),
            ],
        }
    }

    /// Display name (zh-TW).
    pub fn label(self) -> &'static str {
        match self {
            Species::Shiba => "柴犬",
            Species::Munchkin => "曼赤肯貓",
            Species::Fold => "蘇格蘭折耳貓",
            Species::Owl => "貓頭鷹",
            Species::Chick => "小雞",
            Species::Duck => "小鴨",
            Species::Fox => "九尾狐",
        }
    }

    pub fn default_name(self) -> &'static str {
        match self {
            Species::Shiba => "阿柴",
            Species::Munchkin => "奶茶",
            Species::Fold => "芋圓",
            Species::Owl => "咕咕",
            Species::Chick => "蛋黃",
            Species::Duck => "湯圓",
            Species::Fox => "小九",
        }
    }

    /// Default 「牠怎麼叫你」.
    pub fn default_you(self) -> &'static str {
        match self {
            Species::Shiba => "主人",
            Species::Munchkin => "好朋友",
            Species::Fold => "奴才",
            Species::Owl => "同學",
            Species::Chick => "老大",
            Species::Duck => "領隊",
            Species::Fox => "小主人",
        }
    }

    pub fn personality(self) -> Personality {
        let (ticklish, patience, energy) = match self {
            Species::Shiba => (0.8, 0.5, 0.8),
            Species::Munchkin => (0.8, 0.8, 0.5),
            Species::Fold => (0.2, 0.2, 0.2),
            Species::Owl => (0.5, 0.5, 0.5),
            Species::Chick => (0.5, 0.2, 0.9),
            Species::Duck => (0.5, 0.8, 0.3),
            // A trickster: quick to giggle, quick to tire of you, lively.
            Species::Fox => (0.6, 0.4, 0.7),
        };
        Personality {
            ticklish,
            patience,
            energy,
        }
    }

    /// Local hours the pet sleeps: from, until (may wrap past midnight).
    pub fn sleep_hours(self) -> (f32, f32) {
        match self {
            Species::Owl => (9.0, 16.0),
            Species::Chick => (21.0, 5.5),
            // A fox spirit keeps late hours.
            Species::Fox => (1.0, 9.0),
            _ => (23.0, 7.0),
        }
    }

    pub fn grumpiness(self) -> Grumpiness {
        match self {
            Species::Shiba => Grumpiness::Zoomies,
            Species::Munchkin => Grumpiness::Dizzy,
            Species::Fold => Grumpiness::Swat,
            Species::Owl | Species::Chick => Grumpiness::PuffUp,
            Species::Duck | Species::Fox => Grumpiness::Spin,
        }
    }

    /// After its grumpiness, does it go straight back to sleep?
    pub fn sleeps_after_grumpiness(self) -> bool {
        matches!(self, Species::Fold | Species::Owl | Species::Duck)
    }

    /// Favourite snack (PLAN §1.13); the owl's is still open (§10).
    pub fn snack(self) -> &'static str {
        match self {
            Species::Shiba => "肉乾",
            Species::Munchkin | Species::Fold => "罐罐",
            Species::Owl => "小餅乾",
            Species::Chick => "玉米粒",
            Species::Duck => "豌豆",
            // Fried tofu, the fox's favourite in Japanese lore.
            Species::Fox => "豆皮壽司",
        }
    }
}

/// Is `hour` (0–24, local) inside `from..until`, wrapping past midnight?
pub fn in_hours(hour: f32, (from, until): (f32, f32)) -> bool {
    if from <= until {
        (from..until).contains(&hour)
    } else {
        hour >= from || hour < until
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for s in Species::ALL {
            assert_eq!(Species::from_id(s.id()), Some(s));
        }
    }

    #[test]
    fn night_hours_wrap_past_midnight() {
        assert!(in_hours(23.5, (23.0, 7.0)));
        assert!(in_hours(3.0, (23.0, 7.0)));
        assert!(!in_hours(12.0, (23.0, 7.0)));
        assert!(in_hours(12.0, Species::Owl.sleep_hours()));
        assert!(!in_hours(22.0, Species::Owl.sleep_hours()));
    }
}
