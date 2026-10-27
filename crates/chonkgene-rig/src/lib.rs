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
pub use scene::{Layer, PartView, Scene, Surface, fat};

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

    const TINY: &str = r##"(
        species: "tiny",
        slots: ["fur", "ink"],
        coats: [(name: "a", colors: {"fur": "#FFFFFF", "ink": "#000000"})],
        pivots: {"body": (0, 0, 0), "head": (0, 0.5, 0)},
        parents: {"head": "body"},
        parts: [
            (name: "body", bone: "body", shape: Ellipsoid((0.5, 0.5, 0.5)), color: "fur", zone: "belly"),
            (name: "head", bone: "head", shape: Ellipsoid((0.3, 0.3, 0.3)), at: (0, 1, 0), color: "fur", zone: "head"),
        ],
        face: (on: "head", nose: (0, 0, 0.3), ink: "ink", eye: (at: (0.3, 0), r: (0.1, 0.1)),
            blush: (at: (0.5, 0), r: (0.1, 0.1), color: "fur")),
    )"##;
    const TINY_POSES: &str = r#"{
        "sit": (rest: false), "belly_up": (rest: false), "melt": (rest: false), "loaf": (rest: false),
        "sulk": (rest: false), "walk_a": (rest: false), "walk_b": (rest: false), "look_left": (rest: false),
        "look_right": (rest: false), "stretch": (rest: false), "yawn": (rest: false), "wave": (rest: false),
    }"#;

    #[test]
    fn a_parent_bone_carries_its_children() {
        let model = Model::parse(TINY, TINY_POSES).unwrap();
        let body = model.bone("body").unwrap();
        let mut pose = model.pose(PoseId::Sit).clone();
        pose.bones[body].rot = math::v3(0.0, 0.0, 90.0);
        let scene = Scene::posed(&model, 0, 0.0, &pose, Look::Open, Layer::All);
        let head = scene.parts().nth(1).unwrap().center;
        // (0, 1, 0) turned a quarter about z: (−1, 0, 0).
        assert!(
            (head - math::v3(-1.0, 0.0, 0.0)).length() < 1e-4,
            "{head:?}"
        );
    }

    #[test]
    fn a_bone_cannot_hang_from_itself() {
        let looped = TINY.replace(
            r#"parents: {"head": "body"}"#,
            r#"parents: {"head": "body", "body": "head"}"#,
        );
        assert!(Model::parse(&looped, TINY_POSES).is_err());
    }

    #[test]
    fn hex_colours_parse() {
        assert_eq!(parse_hex("#E8955A"), Some([0xE8, 0x95, 0x5A, 255]));
        assert_eq!(parse_hex("#F7A9A0DC"), Some([0xF7, 0xA9, 0xA0, 0xDC]));
        assert_eq!(parse_hex("E8955A"), None);
        assert_eq!(parse_hex("#E8955"), None);
    }
}
