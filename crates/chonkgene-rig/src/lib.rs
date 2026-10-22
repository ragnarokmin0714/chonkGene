//! Rig data for chonkGene pets (docs/PLAN.md §3.3).
//!
//! A rig is parts (paths) hung on bones. Body shape is a morph between the
//! 標準 and 麻糬 targets; everything that moves per frame is a bone transform,
//! so paths are rebuilt only when the body value changes.

mod hit;
mod model;

use std::fmt;

pub use model::*;
use tiny_skia_path::{Path, PathBuilder, Transform};

/// Bump when `CompiledRig` changes shape.
pub const FORMAT: u16 = 1;

/// Rigs shipped in the binary, compiled by `cargo xtask art`.
pub mod builtin {
    pub const SHIBA: &[u8] = include_bytes!("../compiled/shiba.rig");
}

#[derive(Debug)]
pub enum Error {
    Decode(postcard::Error),
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Decode(e) => write!(f, "cannot decode rig: {e}"),
            Error::Invalid(msg) => write!(f, "invalid rig: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

fn invalid(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

/// A bone's local transform, applied around its pivot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Local {
    pub translate: (f32, f32),
    /// Degrees.
    pub rotate: f32,
    pub scale: (f32, f32),
}

impl Default for Local {
    fn default() -> Self {
        Self {
            translate: (0.0, 0.0),
            rotate: 0.0,
            scale: (1.0, 1.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub bones: Vec<Local>,
}

/// Part paths at one body value.
#[derive(Clone, Debug)]
pub struct Shape {
    pub chonk: f32,
    pub paths: Vec<Option<Path>>,
}

#[derive(Clone, Debug)]
pub struct Rig {
    data: CompiledRig,
}

impl Rig {
    pub fn load(bytes: &[u8]) -> Result<Self, Error> {
        let data: CompiledRig = postcard::from_bytes(bytes).map_err(Error::Decode)?;
        Self::from_compiled(data)
    }

    pub fn from_compiled(data: CompiledRig) -> Result<Self, Error> {
        validate(&data)?;
        Ok(Self { data })
    }

    pub fn data(&self) -> &CompiledRig {
        &self.data
    }

    pub fn size(&self) -> (f32, f32) {
        self.data.size
    }

    pub fn ground_y(&self) -> f32 {
        self.data.ground_y
    }

    pub fn bone(&self, name: &str) -> Option<usize> {
        self.data.bones.iter().position(|b| b.name == name)
    }

    pub fn coat(&self, name: &str) -> Option<usize> {
        self.data.coats.iter().position(|c| c.name == name)
    }

    pub fn anchor(&self, name: &str) -> Option<(f32, f32)> {
        self.data
            .anchors
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| *p)
    }

    pub fn rest_pose(&self) -> Pose {
        Pose {
            bones: vec![Local::default(); self.data.bones.len()],
        }
    }

    /// Part paths at body value `chonk` (0.0 標準 … 1.0 麻糬).
    pub fn shape(&self, chonk: f32) -> Shape {
        let c = chonk.clamp(0.0, 1.0);
        let paths = self
            .data
            .parts
            .iter()
            .map(|part| {
                let mut pts = part
                    .standard
                    .iter()
                    .zip(&part.mochi)
                    .map(|(a, b)| (a.0 + (b.0 - a.0) * c, a.1 + (b.1 - a.1) * c));
                let mut pb = PathBuilder::new();
                for verb in &part.verbs {
                    match verb {
                        Verb::Move => {
                            let p = pts.next()?;
                            pb.move_to(p.0, p.1);
                        }
                        Verb::Line => {
                            let p = pts.next()?;
                            pb.line_to(p.0, p.1);
                        }
                        Verb::Quad => {
                            let (c1, p) = (pts.next()?, pts.next()?);
                            pb.quad_to(c1.0, c1.1, p.0, p.1);
                        }
                        Verb::Cubic => {
                            let (c1, c2, p) = (pts.next()?, pts.next()?, pts.next()?);
                            pb.cubic_to(c1.0, c1.1, c2.0, c2.1, p.0, p.1);
                        }
                        Verb::Close => pb.close(),
                    }
                }
                pb.finish()
            })
            .collect();
        Shape { chonk: c, paths }
    }

    /// World transform of every bone (design space), written into `out`.
    pub fn world_into(&self, pose: &Pose, out: &mut Vec<Transform>) {
        out.clear();
        for (i, bone) in self.data.bones.iter().enumerate() {
            let l = pose.bones.get(i).copied().unwrap_or_default();
            let (px, py) = bone.pivot;
            let local = Transform::from_translate(px + l.translate.0, py + l.translate.1)
                .pre_rotate(l.rotate)
                .pre_scale(l.scale.0, l.scale.1)
                .pre_translate(-px, -py);
            let world = match bone.parent {
                Some(p) => out[usize::from(p)].pre_concat(local),
                None => local,
            };
            out.push(world);
        }
    }

    /// The zone under `point` (design space), front-most rules first.
    pub fn hit(&self, shape: &Shape, world: &[Transform], point: (f32, f32)) -> Option<&str> {
        self.data.zones.iter().find_map(|zone| {
            zone.parts
                .iter()
                .any(|&i| {
                    let i = usize::from(i);
                    let part = &self.data.parts[i];
                    match (&shape.paths[i], world.get(usize::from(part.bone))) {
                        (Some(path), Some(ts)) => hit::contains(path, *ts, part.stroke, point),
                        _ => false,
                    }
                })
                .then_some(zone.name.as_str())
        })
    }
}

fn validate(r: &CompiledRig) -> Result<(), Error> {
    if r.format != FORMAT {
        return Err(invalid(format!("format {} (expected {FORMAT})", r.format)));
    }
    if r.slots.is_empty() || r.coats.is_empty() || r.bones.is_empty() {
        return Err(invalid("needs slots, coats and bones"));
    }
    for coat in &r.coats {
        if coat.colors.len() != r.slots.len() {
            return Err(invalid(format!(
                "coat {} does not cover every slot",
                coat.name
            )));
        }
    }
    for (i, bone) in r.bones.iter().enumerate() {
        if bone.parent.is_some_and(|p| usize::from(p) >= i) {
            return Err(invalid(format!(
                "bone {} comes before its parent",
                bone.name
            )));
        }
    }
    for part in &r.parts {
        if usize::from(part.bone) >= r.bones.len() || usize::from(part.slot) >= r.slots.len() {
            return Err(invalid(format!(
                "part {} points at a missing bone or slot",
                part.id
            )));
        }
        let needed: usize = part.verbs.iter().map(|v| v.points()).sum();
        if part.standard.len() != needed || part.mochi.len() != needed {
            return Err(invalid(format!(
                "part {} has the wrong number of points",
                part.id
            )));
        }
        if part.verbs.first() != Some(&Verb::Move) {
            return Err(invalid(format!("part {} must start with a move", part.id)));
        }
    }
    for zone in &r.zones {
        if zone.parts.iter().any(|&p| usize::from(p) >= r.parts.len()) {
            return Err(invalid(format!(
                "zone {} points at a missing part",
                zone.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shiba() -> Rig {
        Rig::load(builtin::SHIBA).expect("the built-in Shiba loads")
    }

    #[test]
    fn the_builtin_shiba_has_the_conventional_bones_and_zones() {
        let rig = shiba();
        for bone in ["root", "body", "belly", "head", "tail", "eye_l", "eye_r"] {
            assert!(rig.bone(bone).is_some(), "missing bone {bone}");
        }
        let zones: Vec<_> = rig.data().zones.iter().map(|z| z.name.as_str()).collect();
        assert_eq!(zones, ["belly", "cheek", "paw", "head", "bottom"]);
        assert!(rig.coat("red").is_some());
        assert!(rig.anchor("bubble").is_some());
    }

    #[test]
    fn every_part_builds_at_every_body_value() {
        let rig = shiba();
        for chonk in [0.0, 0.33, 0.66, 1.0] {
            let shape = rig.shape(chonk);
            assert!(shape.paths.iter().all(Option::is_some));
        }
    }

    #[test]
    fn mochi_is_wider_than_standard() {
        let rig = shiba();
        let body = rig
            .data()
            .parts
            .iter()
            .position(|p| p.id == "body")
            .unwrap();
        let w = |c| rig.shape(c).paths[body].as_ref().unwrap().bounds().width();
        assert!(w(1.0) > w(0.0) * 1.2);
    }

    #[test]
    fn zones_are_found_where_the_parts_are() {
        let rig = shiba();
        let shape = rig.shape(0.33);
        let mut world = Vec::new();
        rig.world_into(&rig.rest_pose(), &mut world);
        assert_eq!(rig.hit(&shape, &world, (100.0, 140.0)), Some("belly"));
        assert_eq!(rig.hit(&shape, &world, (100.0, 40.0)), Some("head"));
        assert_eq!(rig.hit(&shape, &world, (3.0, 3.0)), None);
    }

    #[test]
    fn a_moved_bone_moves_its_hit_zone() {
        let rig = shiba();
        let shape = rig.shape(0.33);
        let mut pose = rig.rest_pose();
        pose.bones[rig.bone("root").unwrap()].translate = (0.0, -40.0);
        let mut world = Vec::new();
        rig.world_into(&pose, &mut world);
        assert_eq!(rig.hit(&shape, &world, (100.0, 100.0)), Some("belly"));
    }

    #[test]
    fn corrupt_data_is_rejected() {
        let mut data = shiba().data().clone();
        data.parts[0].mochi.pop();
        assert!(Rig::from_compiled(data).is_err());
        assert!(Rig::load(&[1, 2, 3]).is_err());
    }
}
