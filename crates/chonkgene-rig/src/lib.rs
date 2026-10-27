//! 3D models for chonkGene pets (docs/PLAN.md §3.3).
//!
//! A pet is a handful of soft primitives (ellipsoids, capsules, tori, cones)
//! hung on bones and blended with smooth unions, so every seam is round. The
//! face is painted onto the head. Poses move bones; the body value widens the
//! parts that carry fat. Everything here is pure: no IO, no drawing.

mod face;
pub mod math;
mod model;
mod scene;
pub mod sdf;

use std::fmt;

pub use model::*;
pub use scene::{Layer, Scene, Surface, fat};

/// World height of the floor every pet stands on.
pub const FLOOR: f32 = -1.1;

/// Models shipped in the binary.
pub mod builtin {
    use super::{Error, Model};

    /// Poses shared by every species unless a model overrides one.
    pub const POSES: &str = include_str!("../../../assets/species/poses.ron");

    /// The model source for a species id (`shiba`, `munchkin`, …).
    pub fn source(id: &str) -> Option<&'static str> {
        Some(match id {
            "shiba" => include_str!("../../../assets/species/shiba/model.ron"),
            "munchkin" => include_str!("../../../assets/species/munchkin/model.ron"),
            "fold" => include_str!("../../../assets/species/fold/model.ron"),
            "owl" => include_str!("../../../assets/species/owl/model.ron"),
            "chick" => include_str!("../../../assets/species/chick/model.ron"),
            "duck" => include_str!("../../../assets/species/duck/model.ron"),
            _ => return None,
        })
    }

    pub fn load(id: &str) -> Result<Model, Error> {
        let src = source(id).ok_or_else(|| super::invalid(format!("no built-in model {id:?}")))?;
        Model::parse(src, POSES)
    }
}

#[derive(Debug)]
pub enum Error {
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Invalid(msg) => write!(f, "invalid model: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_model_loads() {
        for id in ["shiba", "munchkin", "fold", "owl", "chick", "duck"] {
            let m = builtin::load(id).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(m.species, id);
        }
    }

    #[test]
    fn hex_colours_parse() {
        assert_eq!(parse_hex("#E8955A"), Some([0xE8, 0x95, 0x5A, 255]));
        assert_eq!(parse_hex("#F7A9A0DC"), Some([0xF7, 0xA9, 0xA0, 0xDC]));
        assert_eq!(parse_hex("E8955A"), None);
        assert_eq!(parse_hex("#E8955"), None);
    }
}
