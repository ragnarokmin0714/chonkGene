//! The model format (`assets/species/<id>/model.ron`) and its checked,
//! resolved form.

use std::collections::HashMap;

use serde::Deserialize;

use crate::math::{V3, v3};
use crate::sdf::Shape;
use crate::{Error, invalid};

/// Hit zones, in hit-test priority order (PLAN §1.2).
pub const ZONES: [&str; 5] = ["belly", "cheek", "paw", "head", "bottom"];

/// Whole-body poses every species can strike.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PoseId {
    Sit,
    /// On its back, paws in the air: giggling.
    BellyUp,
    /// Flattened into a puddle: 厭世.
    Melt,
    /// Paws tucked in, head down: asleep.
    Loaf,
    /// Turned away: sulking.
    Sulk,
    /// Walking, turned the way it goes: one diagonal pair of legs forward.
    WalkA,
    /// Walking: the other pair forward.
    WalkB,
    /// Sitting, head turned to the pet's left.
    LookLeft,
    /// Sitting, head turned to the pet's right.
    LookRight,
    /// A play bow: front low, bottom up.
    Stretch,
    /// Sitting up, head tipped back.
    Yawn,
    /// Sitting, one front paw (or wing) raised.
    Wave,
}

impl PoseId {
    pub const ALL: [PoseId; 12] = [
        PoseId::Sit,
        PoseId::BellyUp,
        PoseId::Melt,
        PoseId::Loaf,
        PoseId::Sulk,
        PoseId::WalkA,
        PoseId::WalkB,
        PoseId::LookLeft,
        PoseId::LookRight,
        PoseId::Stretch,
        PoseId::Yawn,
        PoseId::Wave,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PoseId::Sit => "sit",
            PoseId::BellyUp => "belly_up",
            PoseId::Melt => "melt",
            PoseId::Loaf => "loaf",
            PoseId::Sulk => "sulk",
            PoseId::WalkA => "walk_a",
            PoseId::WalkB => "walk_b",
            PoseId::LookLeft => "look_left",
            PoseId::LookRight => "look_right",
            PoseId::Stretch => "stretch",
            PoseId::Yawn => "yawn",
            PoseId::Wave => "wave",
        }
    }
}

/// What the face shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Look {
    Open,
    /// Mid-blink.
    Half,
    Shut,
    /// Content ^ ^.
    Happy,
    /// ^ ^ with the mouth wide open.
    Laugh,
    /// Half-lidded and flat-mouthed.
    FedUp,
    /// Eyes squeezed shut, mouth a small frown.
    Pout,
    /// Closed, relaxed curves.
    Asleep,
    /// Eyes squeezed shut, mouth a tall open O.
    Yawn,
}

// ---- the file format ---------------------------------------------------------

#[derive(Deserialize)]
struct ModelFile {
    species: String,
    /// Colour slots.
    slots: Vec<String>,
    /// The two slots that stand for a coat in menus and settings; the first
    /// two slots unless named (a white duck needs a visible outline colour).
    #[serde(default)]
    swatch: Option<(String, String)>,
    coats: Vec<CoatFile>,
    #[serde(default)]
    pivots: HashMap<String, [f32; 3]>,
    /// Bone → the bone it hangs from (head from the body, back legs from
    /// the hips): a parent's move carries its children. Unlisted bones hang
    /// from the whole pet.
    #[serde(default)]
    parents: HashMap<String, String>,
    parts: Vec<PartFile>,
    face: FaceFile,
    /// Species-specific poses, replacing the shared ones by name.
    #[serde(default)]
    poses: HashMap<String, PoseFile>,
    #[serde(default)]
    gait: GaitFile,
    /// Soft spots that wobble on spring bones of their own.
    #[serde(default)]
    jiggles: Vec<JiggleFile>,
}

#[derive(Deserialize)]
struct JiggleFile {
    name: String,
    parent: String,
    at: [f32; 3],
    radius: f32,
    share: f32,
}

/// How a species walks (see [`Gait`]).
#[derive(Deserialize)]
#[serde(default)]
struct GaitFile {
    swing: f32,
    lift: f32,
    roll: f32,
    twist: f32,
    nod: f32,
    diagonal: bool,
}

impl Default for GaitFile {
    fn default() -> Self {
        Self {
            swing: 26.0,
            lift: 0.07,
            roll: 3.0,
            twist: 7.0,
            nod: 4.0,
            diagonal: true,
        }
    }
}

#[derive(Deserialize)]
struct CoatFile {
    name: String,
    colors: HashMap<String, String>,
}

#[derive(Deserialize)]
enum ShapeFile {
    Ellipsoid([f32; 3]),
    Capsule {
        to: [f32; 3],
        r: f32,
    },
    Torus {
        major: f32,
        minor: f32,
    },
    Cone {
        r1: f32,
        r2: f32,
        h: f32,
        #[serde(default = "one")]
        flat: f32,
    },
}

fn one() -> f32 {
    1.0
}

fn root() -> String {
    "root".into()
}

#[derive(Deserialize)]
struct PartFile {
    name: String,
    #[serde(default = "root")]
    bone: String,
    shape: ShapeFile,
    #[serde(default)]
    at: [f32; 3],
    /// Degrees: roll (z), then pitch (x), then turn (y).
    #[serde(default)]
    rot: [f32; 3],
    /// Smooth-union radius with the parts before it.
    #[serde(default)]
    blend: f32,
    color: String,
    #[serde(default)]
    gloss: f32,
    /// Plush fuzz: 1.0 for fur and feathers, 0.0 for beaks and noses.
    #[serde(default = "one")]
    fuzz: f32,
    /// How much the body value widens it (0.0 not at all, 1.0 like the belly).
    #[serde(default)]
    chonk: f32,
    /// Also place a copy mirrored across x = 0.
    #[serde(default)]
    mirror: bool,
    zone: String,
    #[serde(default)]
    marks: Vec<MarkFile>,
}

#[derive(Deserialize)]
struct MarkFile {
    at: [f32; 3],
    radii: [f32; 3],
    color: String,
    #[serde(default = "soft")]
    soft: f32,
    #[serde(default)]
    zone: Option<String>,
    #[serde(default)]
    mirror: bool,
}

fn soft() -> f32 {
    0.15
}

#[derive(Deserialize)]
struct SpotFile {
    at: [f32; 2],
    r: [f32; 2],
    color: String,
}

#[derive(Deserialize)]
struct FaceFile {
    /// The ellipsoid part the eyes are painted on.
    on: String,
    /// The ellipsoid part the mouth is painted on; none for birds.
    #[serde(default)]
    mouth_on: Option<String>,
    /// Where the snot bubble hangs, in the `on` part's frame.
    nose: [f32; 3],
    ink: String,
    eye: EyeFile,
    #[serde(default)]
    brows: Option<SpotFile>,
    blush: SpotFile,
    #[serde(default)]
    whiskers: bool,
    /// A colour slot for a flick of eyeliner at each eye's outer corner
    /// (a fox spirit's red liner).
    #[serde(default)]
    liner: Option<String>,
    /// A colour slot for a flame painted on the forehead (a fox spirit's
    /// mark).
    #[serde(default)]
    forehead: Option<String>,
}

#[derive(Deserialize)]
struct EyeFile {
    at: [f32; 2],
    r: [f32; 2],
    /// A coloured iris and the pupil's size within it (owls, cats).
    #[serde(default)]
    iris: Option<(String, f32)>,
}

#[derive(Deserialize)]
struct MoveFile {
    #[serde(default)]
    shift: [f32; 3],
    #[serde(default)]
    rot: [f32; 3],
    #[serde(default = "ones")]
    scale: [f32; 3],
    /// Drop the bone's parts until their lowest point rests on the floor
    /// (tucked paws), whatever the species' proportions.
    #[serde(default)]
    rest: bool,
}

impl Default for MoveFile {
    fn default() -> Self {
        Self {
            shift: [0.0; 3],
            rot: [0.0; 3],
            scale: ones(),
            rest: false,
        }
    }
}

fn yes() -> bool {
    true
}

fn ones() -> [f32; 3] {
    [1.0; 3]
}

#[derive(Deserialize, Default)]
struct PoseFile {
    #[serde(default)]
    root: MoveFile,
    /// Settle the whole pet onto the floor after posing it.
    #[serde(default = "yes")]
    rest: bool,
    /// Keyed by bone name. Mirrored copies of parts move rigidly with their
    /// bone, unless `"<bone>.mirror"` gives them a move of their own (one
    /// paw waving, both wings up). That move is written as if for the +x
    /// side and applied mirrored, about the mirrored pivot, so the same
    /// values on both make the two sides do the same thing.
    #[serde(default)]
    bones: HashMap<String, MoveFile>,
}

// ---- resolved ------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Coat {
    pub name: String,
    /// One colour per slot, straight RGBA.
    pub colors: Vec<[u8; 4]>,
    swatch: (usize, usize),
}

impl Coat {
    /// The two colours that stand for this coat in menus and settings.
    pub fn swatch(&self) -> ([u8; 4], [u8; 4]) {
        (self.colors[self.swatch.0], self.colors[self.swatch.1])
    }
}

#[derive(Clone, Debug)]
pub struct Mark {
    pub at: V3,
    pub radii: V3,
    pub slot: usize,
    pub soft: f32,
    pub zone: Option<u8>,
    pub mirror: bool,
}

#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub bone: usize,
    pub shape: Shape,
    pub at: V3,
    pub rot: V3,
    pub blend: f32,
    pub slot: usize,
    pub gloss: f32,
    pub fuzz: f32,
    pub chonk: f32,
    pub mirror: bool,
    pub zone: u8,
    pub marks: Vec<Mark>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub at: (f32, f32),
    pub r: (f32, f32),
    pub slot: usize,
}

#[derive(Clone, Debug)]
pub struct Face {
    pub on: usize,
    pub mouth_on: Option<usize>,
    pub nose: V3,
    pub ink: usize,
    pub eye_at: (f32, f32),
    pub eye_r: (f32, f32),
    /// Iris colour slot and the pupil's share of the eye.
    pub iris: Option<(usize, f32)>,
    pub brows: Option<Spot>,
    pub blush: Spot,
    pub whiskers: bool,
    /// Eyeliner colour slot.
    pub liner: Option<usize>,
    /// Forehead flame colour slot.
    pub forehead: Option<usize>,
}

/// How a species walks: the walk is worked out from these every frame (legs
/// lifting and planting, the body rocking and twisting, the head nodding),
/// between the walk poses' turned-aside stance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gait {
    /// How far each leg swings forward and back, degrees.
    pub swing: f32,
    /// How high a stepping foot lifts, world units.
    pub lift: f32,
    /// The chest rolling onto the planted side, degrees.
    pub roll: f32,
    /// The hips turning against the chest, degrees.
    pub twist: f32,
    /// The head nodding with each step (a little behind it), degrees.
    pub nod: f32,
    /// Four legs in diagonal pairs (a trot); else two legs in turn.
    pub diagonal: bool,
}

/// A soft spot (a belly, a bottom, a chin) that wobbles on a spring bone
/// of its own, hung from bone `parent`: skin within `radius` of `at` passes
/// up to `share` of its weight on the parent to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Jiggle {
    pub name: String,
    pub parent: usize,
    pub at: V3,
    pub radius: f32,
    pub share: f32,
}

/// A bone's move in a pose: scale and turn about its pivot, then shift.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub shift: V3,
    pub rot: V3,
    pub scale: V3,
    /// How far to lower the bone's parts onto the floor: 1.0 all the way,
    /// 0.0 not at all (in between while blending poses).
    pub rest: f32,
}

impl Default for Move {
    fn default() -> Self {
        Self {
            shift: V3::ZERO,
            rot: V3::ZERO,
            scale: V3::ONE,
            rest: 0.0,
        }
    }
}

impl Move {
    /// The same move seen in a mirror (across x = 0).
    pub fn mirrored(&self) -> Move {
        Move {
            shift: self.shift.flip_x(),
            rot: V3 {
                x: self.rot.x,
                y: -self.rot.y,
                z: -self.rot.z,
            },
            ..*self
        }
    }

    /// `t` of the way from `self` to `to`. Turns blend per axis, which is
    /// fine for the modest angles between poses.
    pub fn blend(&self, to: &Move, t: f32) -> Move {
        Move {
            shift: self.shift.lerp(to.shift, t),
            rot: self.rot.lerp(to.rot, t),
            scale: self.scale.lerp(to.scale, t),
            rest: self.rest + (to.rest - self.rest) * t,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pose {
    /// The whole pet, about the origin.
    pub root: Move,
    /// One per bone.
    pub bones: Vec<Move>,
    /// One per bone: a move for the mirrored copies only, if the pose has one.
    pub twins: Vec<Option<Move>>,
}

impl Pose {
    /// `t` of the way from `self` to `to` (same model): every bone moves
    /// smoothly, so a pet can turn its head or lie down instead of cutting
    /// between poses.
    pub fn blend(&self, to: &Pose, t: f32) -> Pose {
        let n = self.bones.len().max(to.bones.len());
        let twins = (0..n)
            .map(|i| {
                let either = self.twins.get(i).copied().flatten().is_some()
                    || to.twins.get(i).copied().flatten().is_some();
                either.then(|| self.bone(i, true).blend(&to.bone(i, true), t))
            })
            .collect();
        Pose {
            root: self.root.blend(&to.root, t),
            bones: (0..n)
                .map(|i| self.bone(i, false).blend(&to.bone(i, false), t))
                .collect(),
            twins,
        }
    }

    /// The pose seen in a mirror. Blending a pose into its mirror turns the
    /// pet round in 3D, through facing you. Paired limbs swap sides; a part
    /// off to one side (a Shiba's tail) stays on its own side, as on a real
    /// pet turning round.
    pub fn mirrored(&self) -> Pose {
        let n = self.bones.len();
        let (bones, twins) = (0..n)
            .map(|i| match self.twins.get(i).copied().flatten() {
                // Each side takes the other's move, as written for +x.
                Some(twin) => (twin, Some(self.bones[i])),
                None => (self.bones[i].mirrored(), None),
            })
            .unzip();
        Pose {
            root: self.root.mirrored(),
            bones,
            twins,
        }
    }

    /// The move for a part on bone `bone`; `mirrored` for its mirrored copy.
    pub fn bone(&self, bone: usize, mirrored: bool) -> Move {
        let twin = if mirrored {
            self.twins.get(bone).copied().flatten()
        } else {
            None
        };
        twin.or_else(|| self.bones.get(bone).copied())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
pub struct Model {
    pub species: String,
    pub slots: Vec<String>,
    pub coats: Vec<Coat>,
    pub bones: Vec<String>,
    pub pivots: Vec<V3>,
    /// One per bone: the bone it hangs from.
    pub parents: Vec<Option<usize>>,
    /// Bones, parents before children.
    pub order: Vec<usize>,
    pub parts: Vec<Part>,
    pub face: Face,
    pub poses: Vec<Pose>,
    pub gait: Gait,
    pub jiggles: Vec<Jiggle>,
}

impl Model {
    /// Parse a model and the shared poses it may override.
    pub fn parse(model: &str, shared_poses: &str) -> Result<Self, Error> {
        let file: ModelFile = ron::from_str(model).map_err(|e| invalid(format!("model: {e}")))?;
        let shared: HashMap<String, PoseFile> =
            ron::from_str(shared_poses).map_err(|e| invalid(format!("poses: {e}")))?;
        resolve(file, shared)
    }

    pub fn coat(&self, name: &str) -> Option<usize> {
        self.coats.iter().position(|c| c.name == name)
    }

    pub fn pose(&self, id: PoseId) -> &Pose {
        &self.poses[id as usize]
    }

    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b == name)
    }

    pub fn part(&self, name: &str) -> Option<usize> {
        self.parts.iter().position(|p| p.name == name)
    }
}

fn resolve(file: ModelFile, shared: HashMap<String, PoseFile>) -> Result<Model, Error> {
    let species = file.species;
    let ctx = |msg: String| invalid(format!("{species}: {msg}"));
    let slot = |name: &str| {
        file.slots
            .iter()
            .position(|s| s == name)
            .ok_or_else(|| ctx(format!("unknown colour slot {name:?}")))
    };
    let zone = |name: &str| {
        ZONES
            .iter()
            .position(|z| *z == name)
            .map(|i| i as u8)
            .ok_or_else(|| ctx(format!("unknown zone {name:?}")))
    };
    if file.slots.len() < 2 {
        return Err(ctx("needs at least two colour slots".into()));
    }
    if file.coats.is_empty() {
        return Err(ctx("needs at least one coat".into()));
    }

    let swatch = match &file.swatch {
        Some((a, b)) => (slot(a)?, slot(b)?),
        None => (0, 1),
    };
    let mut coats = Vec::new();
    for c in &file.coats {
        let mut colors = Vec::new();
        for s in &file.slots {
            let hex = c
                .colors
                .get(s)
                .ok_or_else(|| ctx(format!("coat {:?} has no {s:?}", c.name)))?;
            colors.push(parse_hex(hex).ok_or_else(|| ctx(format!("bad colour {hex:?}")))?);
        }
        if c.colors.len() != file.slots.len() {
            return Err(ctx(format!("coat {:?} names an unknown slot", c.name)));
        }
        coats.push(Coat {
            name: c.name.clone(),
            colors,
            swatch,
        });
    }

    // Bones: root first, then every bone a part, pivot or parent names.
    let mut bones = vec!["root".to_string()];
    let mut named: Vec<&String> = file.parts.iter().map(|p| &p.bone).collect();
    let mut keys: Vec<&String> = file.pivots.keys().chain(file.parents.keys()).collect();
    keys.sort();
    named.extend(keys);
    for name in named {
        if !bones.contains(name) {
            bones.push(name.clone());
        }
    }
    let bone_index = |name: &str| bones.iter().position(|b| b == name);
    let mut parents = vec![None; bones.len()];
    for (child, parent) in &file.parents {
        let (Some(c), Some(p)) = (bone_index(child), bone_index(parent)) else {
            return Err(ctx(format!("parent {parent:?} of {child:?} is no bone")));
        };
        parents[c] = Some(p);
    }
    // Parents before children; a loop is an error.
    let mut order = Vec::new();
    let mut state = vec![0u8; bones.len()];
    fn visit(
        b: usize,
        parents: &[Option<usize>],
        state: &mut [u8],
        order: &mut Vec<usize>,
    ) -> bool {
        match state[b] {
            2 => return true,
            1 => return false,
            _ => {}
        }
        state[b] = 1;
        if let Some(p) = parents[b]
            && !visit(p, parents, state, order)
        {
            return false;
        }
        state[b] = 2;
        order.push(b);
        true
    }
    for (b, name) in bones.iter().enumerate() {
        if !visit(b, &parents, &mut state, &mut order) {
            return Err(ctx(format!("bone {name:?} hangs from itself")));
        }
    }
    let pivots = bones
        .iter()
        .map(|b| file.pivots.get(b).map_or(V3::ZERO, |p| V3::from_array(*p)))
        .collect();

    let mut parts = Vec::new();
    for p in &file.parts {
        let shape = match p.shape {
            ShapeFile::Ellipsoid(r) => Shape::Ellipsoid(V3::from_array(r)),
            ShapeFile::Capsule { to, r } => Shape::Capsule {
                to: V3::from_array(to),
                r,
            },
            ShapeFile::Torus { major, minor } => Shape::Torus { major, minor },
            ShapeFile::Cone { r1, r2, h, flat } => Shape::Cone { r1, r2, h, flat },
        };
        let positive = match shape {
            Shape::Ellipsoid(r) => r.min_elem() > 0.0,
            Shape::Capsule { to, r } => r > 0.0 && to.length() > 0.0,
            Shape::Torus { major, minor } => major > 0.0 && minor > 0.0,
            Shape::Cone { r1, r2, h, flat } => r1 > 0.0 && r2 > 0.0 && h > 0.0 && flat >= 1.0,
        };
        if !positive || p.blend < 0.0 {
            return Err(ctx(format!("part {:?} has a degenerate shape", p.name)));
        }
        let marks = p
            .marks
            .iter()
            .map(|m| {
                Ok(Mark {
                    at: V3::from_array(m.at),
                    radii: V3::from_array(m.radii),
                    slot: slot(&m.color)?,
                    soft: m.soft.clamp(0.01, 1.0),
                    zone: m.zone.as_deref().map(zone).transpose()?,
                    mirror: m.mirror,
                })
            })
            .collect::<Result<_, Error>>()?;
        parts.push(Part {
            name: p.name.clone(),
            bone: bones.iter().position(|b| *b == p.bone).unwrap_or(0),
            shape,
            at: V3::from_array(p.at),
            rot: V3::from_array(p.rot),
            blend: p.blend,
            slot: slot(&p.color)?,
            gloss: p.gloss,
            fuzz: p.fuzz,
            chonk: p.chonk,
            mirror: p.mirror,
            zone: zone(&p.zone)?,
            marks,
        });
    }
    let part = |name: &str| {
        let i = parts
            .iter()
            .position(|p| p.name == name)
            .ok_or_else(|| ctx(format!("face names unknown part {name:?}")))?;
        match parts[i].shape {
            Shape::Ellipsoid(_) => Ok(i),
            _ => Err(ctx(format!("face part {name:?} must be an ellipsoid"))),
        }
    };
    let f = &file.face;
    let spot = |s: &SpotFile| -> Result<Spot, Error> {
        Ok(Spot {
            at: (s.at[0], s.at[1]),
            r: (s.r[0], s.r[1]),
            slot: slot(&s.color)?,
        })
    };
    let face = Face {
        on: part(&f.on)?,
        mouth_on: f.mouth_on.as_deref().map(part).transpose()?,
        nose: V3::from_array(f.nose),
        ink: slot(&f.ink)?,
        eye_at: (f.eye.at[0], f.eye.at[1]),
        eye_r: (f.eye.r[0], f.eye.r[1]),
        iris: f
            .eye
            .iris
            .as_ref()
            .map(|(s, k)| slot(s).map(|s| (s, k.clamp(0.1, 1.0))))
            .transpose()?,
        brows: f.brows.as_ref().map(spot).transpose()?,
        blush: spot(&f.blush)?,
        whiskers: f.whiskers,
        liner: f.liner.as_deref().map(slot).transpose()?,
        forehead: f.forehead.as_deref().map(slot).transpose()?,
    };

    let mut poses = Vec::new();
    for id in PoseId::ALL {
        let pf = file
            .poses
            .get(id.name())
            .or_else(|| shared.get(id.name()))
            .ok_or_else(|| ctx(format!("no pose {:?}", id.name())))?;
        let mv = |m: &MoveFile| Move {
            shift: V3::from_array(m.shift),
            rot: V3::from_array(m.rot),
            scale: V3::from_array(m.scale),
            rest: if m.rest { 1.0 } else { 0.0 },
        };
        let mut pose = Pose {
            root: Move {
                rest: if pf.rest { 1.0 } else { 0.0 },
                ..mv(&pf.root)
            },
            bones: vec![Move::default(); bones.len()],
            twins: vec![None; bones.len()],
        };
        for (key, m) in &pf.bones {
            let (name, twin) = match key.strip_suffix(".mirror") {
                Some(name) => (name, true),
                None => (key.as_str(), false),
            };
            // Shared poses may move bones this species lacks (a bird's legs).
            if let Some(i) = bones.iter().position(|b| b == name) {
                let m = mv(m);
                if m.scale.min_elem() <= 0.0 {
                    return Err(ctx(format!(
                        "pose {:?} scales {key:?} to nothing",
                        id.name()
                    )));
                }
                if twin {
                    pose.twins[i] = Some(m);
                } else {
                    pose.bones[i] = m;
                }
            }
        }
        poses.push(pose);
    }

    let mut jiggles = Vec::new();
    for j in &file.jiggles {
        let parent = bones.iter().position(|b| *b == j.parent).ok_or_else(|| {
            ctx(format!(
                "jiggle {:?} hangs from no bone {:?}",
                j.name, j.parent
            ))
        })?;
        if bones.contains(&j.name) || jiggles.iter().any(|o: &Jiggle| o.name == j.name) {
            return Err(ctx(format!("jiggle {:?} is already a bone", j.name)));
        }
        jiggles.push(Jiggle {
            name: j.name.clone(),
            parent,
            at: V3::from_array(j.at),
            radius: j.radius,
            share: j.share.clamp(0.0, 1.0),
        });
    }

    Ok(Model {
        species,
        slots: file.slots,
        coats,
        bones,
        pivots,
        parents,
        order,
        gait: Gait {
            swing: file.gait.swing,
            lift: file.gait.lift,
            roll: file.gait.roll,
            twist: file.gait.twist,
            nod: file.gait.nod,
            diagonal: file.gait.diagonal,
        },
        parts,
        face,
        poses,
        jiggles,
    })
}

/// `#RRGGBB` or `#RRGGBBAA`.
pub fn parse_hex(hex: &str) -> Option<[u8; 4]> {
    let h = hex.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

pub(crate) fn rgb(c: [u8; 4]) -> V3 {
    v3(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0
}
