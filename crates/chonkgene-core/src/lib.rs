//! Pet logic for chonkGene (docs/PLAN.md §3.1): pure, no IO, seedable.
//!
//! Time is passed in as seconds (`f64`) so every rule can be tested without
//! a clock.

pub mod anim;
pub mod mind;
pub mod motion;
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
    Paw,
    Bottom,
    Head,
}

impl Zone {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "belly" => Self::Belly,
            "cheek" => Self::Cheek,
            "paw" => Self::Paw,
            "bottom" => Self::Bottom,
            "head" => Self::Head,
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
