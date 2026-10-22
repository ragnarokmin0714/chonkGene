//! The six species and their defaults (PLAN §1.1, §1.6, §1.12).

use crate::Personality;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Species {
    Shiba,
    Munchkin,
    Fold,
    Owl,
    Chick,
    Duck,
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
    pub const ALL: [Species; 6] = [
        Species::Shiba,
        Species::Munchkin,
        Species::Fold,
        Species::Owl,
        Species::Chick,
        Species::Duck,
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
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
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
            _ => (23.0, 7.0),
        }
    }

    pub fn grumpiness(self) -> Grumpiness {
        match self {
            Species::Shiba => Grumpiness::Zoomies,
            Species::Munchkin => Grumpiness::Dizzy,
            Species::Fold => Grumpiness::Swat,
            Species::Owl | Species::Chick => Grumpiness::PuffUp,
            Species::Duck => Grumpiness::Spin,
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
