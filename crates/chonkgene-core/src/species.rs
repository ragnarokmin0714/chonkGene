//! The six species and their defaults (PLAN §1.1, §1.6, §1.12).

use crate::anim::{Act, Habits};
use crate::i18n::{mark, tr};
use crate::{Personality, Zone};

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

/// What a light pat on the bottom gets (see [`Species::bottom_pat`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BottomPat {
    /// 電梯屁股 (a cat): rump up, tail straight up, the hind paws treading.
    ButtUp,
    /// The Shiba: a hind leg kicking at the base of its tail, a pleased
    /// look back.
    Kick,
    /// The chick: a startled hop a step forward.
    Hop,
    /// The duckling: the hop, then off at a quicker waddle, quacking.
    HopAway,
    /// The owl: feathers puffed, its head turned right round to stare at
    /// you over its back, unimpressed.
    LookRound,
    /// The fox (her beast form; her human form has no bottom to pat): she
    /// turns, glares and swats your hand with her tails, 「放肆！」. A
    /// glare, no blush.
    Swat,
}

/// How much a species likes a hand on a zone (PLAN §8, Roger's sixteenth
/// pass, item 4: 「摸動物不同部位時，會有其他反應」).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fondness {
    /// A flinch, a look back, a step away; kept at, a grudge.
    Dislikes,
    /// As any touch was before.
    Fine,
    /// Leans into the hand, a small heart.
    Likes,
    /// Eyes shut in bliss, hearts, purring, kneading, a hind leg going.
    Loves,
}

impl Species {
    /// How it takes a hand on `zone`: Roger's table (PLAN §8). Paws and
    /// wings share a column, as do the back and the bottom.
    pub fn fondness(self, zone: Zone) -> Fondness {
        use Fondness::{Dislikes as No, Fine as Ok, Likes as Yes, Loves as Love};
        use Zone::*;
        // Head, chin, cheek, belly, paw (front or hind) or wing, back
        // (shoulders too) or bottom, tail.
        let row = match self {
            Species::Shiba => [Yes, Love, Yes, Love, No, Love, No],
            Species::Munchkin => [Love, Love, Love, Love, No, Yes, No],
            Species::Fold => [Love, Love, Yes, Love, No, Yes, No],
            Species::Owl => [Love, Yes, Love, Ok, No, Ok, No],
            Species::Chick => [Love, Yes, Yes, Ok, No, Love, Ok],
            Species::Duck => [Love, Yes, Ok, Ok, No, Love, Yes],
            Species::Fox => [Love, Yes, Yes, Ok, No, Ok, Love],
        };
        row[match zone {
            Head => 0,
            Chin => 1,
            Cheek => 2,
            Belly => 3,
            Paw | Foot | Wing => 4,
            Back | Shoulder | Bottom => 5,
            Tail => 6,
        }]
    }

    /// How it takes a light pat on the bottom (PLAN §8, Roger's
    /// twenty-seventh pass, item 6 (b); the owl's is ours, his list left
    /// it out).
    pub fn bottom_pat(self) -> BottomPat {
        match self {
            Species::Munchkin | Species::Fold => BottomPat::ButtUp,
            Species::Shiba => BottomPat::Kick,
            Species::Chick => BottomPat::Hop,
            Species::Duck => BottomPat::HopAway,
            Species::Owl => BottomPat::LookRound,
            Species::Fox => BottomPat::Swat,
        }
    }

    /// Its belly loved, but rubbed this long it grabs the hand and bunny
    /// kicks (Roger: the 曼赤肯 only), seconds.
    pub fn belly_trap(self) -> Option<f32> {
        (self == Species::Munchkin).then_some(3.0)
    }

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

    /// What it does pleased, smiling at you (see
    /// [`Animator::with_joys`](crate::anim::Animator::with_joys)): dogs and
    /// the fox wiggle, hop and spin, cats knead, birds hop, flap and
    /// dance from foot to foot.
    pub fn joys(self) -> Habits {
        match self {
            Species::Shiba => &[
                (Act::Wiggle, 32),
                (Act::Bounce, 24),
                (Act::Spin, 18),
                (Act::Wave, 12),
                (Act::Stretch, 8),
            ],
            Species::Fox => &[
                (Act::Wiggle, 22),
                (Act::Spin, 26),
                (Act::Bounce, 20),
                (Act::Wave, 12),
                (Act::Groom, 8),
            ],
            Species::Munchkin => &[
                (Act::Knead, 30),
                (Act::Bounce, 16),
                (Act::Wiggle, 14),
                (Act::Wave, 12),
                (Act::Groom, 10),
            ],
            Species::Fold => &[
                (Act::Knead, 36),
                (Act::Wave, 14),
                (Act::Groom, 14),
                (Act::Bounce, 10),
                (Act::Stretch, 8),
            ],
            Species::Chick => &[
                (Act::Bounce, 30),
                (Act::Flap, 24),
                (Act::Wiggle, 22),
                (Act::Spin, 10),
            ],
            Species::Duck => &[
                (Act::Wiggle, 30),
                (Act::Flap, 22),
                (Act::Bounce, 18),
                (Act::Spin, 10),
            ],
            Species::Owl => &[
                (Act::Wiggle, 22),
                (Act::Flap, 22),
                (Act::Bounce, 12),
                (Act::LookLeft, 10),
                (Act::LookRight, 10),
            ],
        }
    }

    /// Display name (zh-TW).
    pub fn label(self) -> &'static str {
        match self {
            Species::Shiba => tr("柴犬"),
            Species::Munchkin => tr("曼赤肯貓"),
            Species::Fold => tr("蘇格蘭折耳貓"),
            Species::Owl => tr("貓頭鷹"),
            Species::Chick => tr("小雞"),
            Species::Duck => tr("小鴨"),
            Species::Fox => tr("九尾狐"),
        }
    }

    /// Its default name in the language shown now.
    pub fn default_name(self) -> &'static str {
        tr(self.default_name_zh())
    }

    /// Its default name as the code writes it (the key of its
    /// translations): a save still holding it in any language was never
    /// renamed (`PetSave::localize_defaults`).
    pub fn default_name_zh(self) -> &'static str {
        match self {
            Species::Shiba => mark("阿柴"),
            Species::Munchkin => mark("奶茶"),
            Species::Fold => mark("芋圓"),
            Species::Owl => mark("咕咕"),
            Species::Chick => mark("蛋黃"),
            Species::Duck => mark("湯圓"),
            Species::Fox => mark("小九"),
        }
    }

    /// Default 「牠怎麼叫你」, in the language shown now.
    pub fn default_you(self) -> &'static str {
        tr(self.default_you_zh())
    }

    /// Default 「牠怎麼叫你」 as the code writes it (see [`Self::default_name_zh`]).
    pub fn default_you_zh(self) -> &'static str {
        match self {
            Species::Shiba => mark("主人"),
            Species::Munchkin => mark("好朋友"),
            Species::Fold => mark("奴才"),
            Species::Owl => mark("同學"),
            Species::Chick => mark("老大"),
            Species::Duck => mark("領隊"),
            Species::Fox => mark("小主人"),
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
            Species::Shiba => tr("肉乾"),
            Species::Munchkin | Species::Fold => tr("罐罐"),
            Species::Owl => tr("小餅乾"),
            Species::Chick => tr("玉米粒"),
            Species::Duck => tr("豌豆"),
            // Fried tofu, the fox's favourite in Japanese lore.
            Species::Fox => tr("豆皮壽司"),
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
