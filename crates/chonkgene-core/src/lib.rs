//! Pet logic for chonkGene (docs/PLAN.md §3.1): pure, no IO, seedable.
//!
//! Time is passed in as seconds (`f64`) so every rule can be tested without
//! a clock.

pub mod activity;
pub mod anim;
pub mod i18n;
pub mod mind;
pub mod motion;
pub mod play;
pub mod shake;
pub mod skit;
pub mod sound;
pub mod species;
pub mod spring;
pub mod terrain;
pub mod tickle;

/// Where a poke landed (PLAN §1.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    Belly,
    Cheek,
    /// A front paw (the fox's human form: a hand), the one it gives to
    /// shake (PLAN §8, Roger's twenty-seventh pass, item 2).
    Paw,
    Bottom,
    Head,
    /// Under the chin (Roger, 2026-11-03: chin scratches).
    Chin,
    Tail,
    /// The top of the body (a bird's back too), apart from the bottom
    /// (PLAN §8, Roger's sixteenth pass, item 4).
    Back,
    /// A bird's wing, apart from its feet.
    Wing,
    /// The top of the body between the neck and the front legs, a bird's
    /// wing root, the human form's shoulders: 環抱 (PLAN §8, Roger's
    /// twenty-seventh pass, item 2).
    Shoulder,
    /// A hind paw, a bird's foot, the human form's legs: 舉高高.
    Foot,
}

impl Zone {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "belly" => Self::Belly,
            "cheek" => Self::Cheek,
            "paw" => Self::Paw,
            "bottom" => Self::Bottom,
            "head" => Self::Head,
            "chin" => Self::Chin,
            "tail" => Self::Tail,
            "back" => Self::Back,
            "wing" => Self::Wing,
            "shoulder" => Self::Shoulder,
            "foot" => Self::Foot,
            _ => return None,
        })
    }
}

/// Per-pet personality, each 0.0–1.0 (PLAN §1.1, §1.6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Personality {
    pub ticklish: f32,
    pub patience: f32,
    pub energy: f32,
}

impl Personality {
    /// Species defaults: high ticklish, medium patience, high energy.
    pub const SHIBA: Self = Self {
        ticklish: 0.8,
        patience: 0.5,
        energy: 0.8,
    };
}
