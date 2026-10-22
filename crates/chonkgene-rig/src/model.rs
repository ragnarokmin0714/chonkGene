//! The two rig formats: `RigSource` is what an artist writes next to the SVGs
//! (`assets/species/<id>/rig.ron`); `CompiledRig` is what `cargo xtask art`
//! produces and the app embeds.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `rig.ron`. Part ids name elements in `standard.svg` and `mochi.svg`.
#[derive(Debug, Deserialize)]
pub struct RigSource {
    pub species: String,
    /// Design box, width × height.
    pub size: (f32, f32),
    /// Where the feet touch the ground, in design units.
    pub ground_y: f32,
    pub slots: Vec<String>,
    pub coats: Vec<CoatSource>,
    pub bones: Vec<BoneSource>,
    pub parts: Vec<PartSource>,
    pub zones: Vec<ZoneSource>,
    pub anchors: BTreeMap<String, (f32, f32)>,
}

#[derive(Debug, Deserialize)]
pub struct CoatSource {
    pub name: String,
    /// Slot name → `#RRGGBB` or `#RRGGBBAA`.
    pub colors: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct BoneSource {
    pub name: String,
    pub parent: Option<String>,
    pub pivot: (f32, f32),
}

#[derive(Debug, Deserialize)]
pub struct PartSource {
    pub id: String,
    pub bone: String,
    pub slot: String,
}

#[derive(Debug, Deserialize)]
pub struct ZoneSource {
    pub name: String,
    pub parts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompiledRig {
    pub format: u16,
    pub species: String,
    pub size: (f32, f32),
    pub ground_y: f32,
    pub slots: Vec<String>,
    pub coats: Vec<Coat>,
    /// Parents always come before their children.
    pub bones: Vec<Bone>,
    /// Back to front.
    pub parts: Vec<Part>,
    /// In hit-test priority order.
    pub zones: Vec<Zone>,
    pub anchors: Vec<(String, (f32, f32))>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Coat {
    pub name: String,
    /// Premultiplied-free RGBA per slot, indexed like `CompiledRig::slots`.
    pub colors: Vec<[u8; 4]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    pub parent: Option<u16>,
    pub pivot: (f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verb {
    Move,
    Line,
    Quad,
    Cubic,
    Close,
}

impl Verb {
    pub fn points(self) -> usize {
        match self {
            Verb::Move | Verb::Line => 1,
            Verb::Quad => 2,
            Verb::Cubic => 3,
            Verb::Close => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: String,
    pub bone: u16,
    pub slot: u16,
    /// Stroke width for line parts (tail, mouth); `None` means filled.
    pub stroke: Option<f32>,
    pub verbs: Vec<Verb>,
    /// Points at body value 0.0 (標準) and 1.0 (麻糬); same length.
    pub standard: Vec<(f32, f32)>,
    pub mochi: Vec<(f32, f32)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub name: String,
    pub parts: Vec<u16>,
}
