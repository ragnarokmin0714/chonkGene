//! Skinned-mesh spike: the skeleton a baked pet mesh hangs on, and a
//! four-legged walk worked out every frame. The legs step one at a time
//! (left hind, left fore, right hind, right fore) with each planted foot
//! keeping pace with the floor, the chest and hips rock against each other,
//! tails sway like seaweed as a wave runs down their segments, and the
//! belly, bottom and chin wobble on springs of their own.

use chonkgene_core::Zone;
use chonkgene_core::anim::Hug;
use chonkgene_core::species::Fondness;
use chonkgene_rig::math::{M3, V3, v3};
use chonkgene_rig::{FLOOR, Model, builtin};

use crate::fidget::{Fidgets, Hold, Kind, Out, ear_flick};

/// A bone's move: a turn about its pivot (degrees, as [`M3::euler`]), then
/// a shift.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Joint {
    pub rot: V3,
    pub shift: V3,
}

/// An affine map: `m · p + t`.
#[derive(Clone, Copy, Debug)]
pub struct Affine {
    pub m: M3,
    pub t: V3,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        m: M3::IDENTITY,
        t: V3::ZERO,
    };

    /// `m` about the point `p`.
    pub fn about(m: M3, p: V3) -> Affine {
        Affine {
            m,
            t: p - m.apply(p),
        }
    }

    /// `self` after `o`.
    pub fn then(&self, o: &Affine) -> Affine {
        Affine {
            m: self.m.mul(o.m),
            t: self.m.apply(o.t) + self.t,
        }
    }

    pub fn apply(&self, p: V3) -> V3 {
        self.m.apply(p) + self.t
    }

    /// The three rows of `[m | t]`, as the shader reads a bone.
    pub fn rows(&self) -> [[f32; 4]; 3] {
        [0, 1, 2].map(|i| {
            let r = self.m.r[i];
            [r.x, r.y, r.z, [self.t.x, self.t.y, self.t.z][i]]
        })
    }
}

/// Four-legged models for the spike, by species id.
pub const QUAD_SPECIES: [&str; 4] = ["shiba", "fox", "munchkin", "fold"];

/// Two-legged (bird) models for the skinned mesh, by species id.
pub const BIPED_SPECIES: [&str; 3] = ["chick", "duck", "owl"];

/// A skinned-mesh model of any species that has one: four-legged
/// ([`quad`]) or a bird ([`biped`]).
pub fn mesh_model(id: &str) -> Result<(Model, Skeleton), String> {
    quad(id).or_else(|_| biped(id))
}

/// A bird's model (`chick`, `duck`, `owl`) and its skeleton.
pub fn biped(id: &str) -> Result<(Model, Skeleton), String> {
    let src = match id {
        "chick" => include_str!("../../../assets/species/chick/biped.ron"),
        "duck" => include_str!("../../../assets/species/duck/biped.ron"),
        "owl" => include_str!("../../../assets/species/owl/biped.ron"),
        _ => return Err(format!("no two-legged model {id:?}")),
    };
    let model = Model::parse(src, builtin::POSES).map_err(|e| e.to_string())?;
    let sk = Skeleton::new(&model);
    Ok((model, sk))
}

/// A four-legged model (`shiba`, `fox`, `munchkin`, `fold`) and its skeleton.
pub fn quad(id: &str) -> Result<(Model, Skeleton), String> {
    let src = match id {
        "shiba" => include_str!("../../../assets/species/shiba/quad.ron"),
        "fox" => include_str!("../../../assets/species/fox/quad.ron"),
        "munchkin" => include_str!("../../../assets/species/munchkin/quad.ron"),
        "fold" => include_str!("../../../assets/species/fold/quad.ron"),
        _ => return Err(format!("no four-legged model {id:?}")),
    };
    let model = Model::parse(src, builtin::POSES).map_err(|e| e.to_string())?;
    let sk = Skeleton::new(&model);
    Ok((model, sk))
}

/// Where the whole pet turns: the floor under the middle of its body.
pub const ROOT_PIVOT: V3 = v3(0.0, FLOOR, -0.1);

#[derive(Clone)]
pub struct Skeleton {
    pub names: Vec<String>,
    pub parents: Vec<Option<usize>>,
    pub pivots: Vec<V3>,
    /// Parents before children.
    order: Vec<usize>,
    /// Jiggle bone, centre, radius, share (see [`chonkgene_rig::Jiggle`]).
    pub jiggles: Vec<(usize, V3, f32, f32)>,
}

impl Skeleton {
    /// The model's bones and pivots, then a bone for each of its jiggles.
    pub fn new(model: &Model) -> Self {
        let mut names = model.bones.clone();
        let mut parents = model.parents.clone();
        let mut pivots = model.pivots.clone();
        pivots[0] = ROOT_PIVOT;
        // Bone 0 is the root: every other bone without a parent hangs from
        // it, so turning the root turns the pet.
        for p in parents.iter_mut().skip(1) {
            p.get_or_insert(0);
        }
        let mut order = vec![0];
        order.extend(model.order.iter().copied().filter(|&b| b != 0));
        let mut jiggles = Vec::new();
        for j in &model.jiggles {
            names.push(j.name.clone());
            parents.push(Some(j.parent));
            pivots.push(j.at);
            order.push(names.len() - 1);
            jiggles.push((names.len() - 1, j.at, j.radius, j.share));
        }
        Self {
            names,
            parents,
            pivots,
            order,
            jiggles,
        }
    }

    /// Tails as chains of bones, base first: each starts at a bone named
    /// `tail…` hanging from a bone that is not, and runs on through its one
    /// `tail…` child.
    pub fn tails(&self) -> Vec<Vec<usize>> {
        let is_tail = |b: usize| self.names[b].starts_with("tail");
        let child =
            |b: usize| (0..self.names.len()).find(|&c| self.parents[c] == Some(b) && is_tail(c));
        (0..self.names.len())
            .filter(|&b| is_tail(b) && !self.parents[b].is_some_and(is_tail))
            .map(|base| {
                let mut chain = vec![base];
                while let Some(c) = child(*chain.last().unwrap_or(&base)) {
                    chain.push(c);
                }
                chain
            })
            .collect()
    }

    pub fn bone(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|b| b == name)
    }

    /// Every bone's rest → posed map for `joints` (one per bone; bone 0,
    /// the root, moves the whole pet about the floor under it).
    pub fn pose(&self, joints: &[Joint]) -> Vec<Affine> {
        let mut out = vec![Affine::IDENTITY; self.names.len()];
        for &b in &self.order {
            let parent = self.parents[b].map_or(Affine::IDENTITY, |p| out[p]);
            let j = joints.get(b).copied().unwrap_or_default();
            let pivot = self.pivots[b];
            let m = M3::euler(j.rot);
            let local = Affine {
                m,
                t: pivot + j.shift - m.apply(pivot),
            };
            out[b] = parent.then(&local);
        }
        out
    }
}

/// An idle gesture under way (stretch, yawn, wave) as the walks take it:
/// how far into it, 0 … 1 … 0 over its length (`e`), and seconds in (`t`).
#[derive(Clone, Copy, Debug)]
pub struct Gesture {
    pub act: chonkgene_core::anim::Act,
    pub e: f32,
    pub t: f32,
}

/// Getting about other than on foot (Roger's eighteenth pass): sliding
/// along on its belly, gliding down spread out, or, a four-legged pet,
/// hanging under a balloon or a bamboo-copter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Travel {
    #[default]
    None,
    Slide,
    Glide,
    Fly,
}

impl Travel {
    /// How much of each of slide, glide and fly it is: the goals the poses
    /// ease toward.
    pub(crate) fn weights(self) -> [f32; 3] {
        match self {
            Travel::None => [0.0; 3],
            Travel::Slide => [1.0, 0.0, 0.0],
            Travel::Glide => [0.0, 1.0, 0.0],
            Travel::Fly => [0.0, 0.0, 1.0],
        }
    }
}

/// A feeling the whole body shows (the face shows it too).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mood {
    #[default]
    Calm,
    /// Laughing: rolls over belly-up toward you, paws pedalling.
    Laugh,
    /// 厭世: melts flat on the floor, legs splayed, chin down.
    FedUp,
    /// The overdone finale: turns its back on you, glancing back now and then.
    Sulk,
    /// Pleased (a pat, made up): sits, head tilting.
    Content,
}

/// How a four-legged pet sleeps (Roger, 2026-11-03: heads stayed up). One
/// is picked each time it falls asleep, and now and then in its sleep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SleepPose {
    /// Lying down, chin on its front paws.
    #[default]
    Chin,
    /// Curled round, nose toward its tail.
    Curl,
    /// Flopped over on its side, legs out.
    Side,
    /// On its back, paws in the air.
    BellyUp,
}

impl SleepPose {
    pub const ALL: [SleepPose; 4] = [Self::Chin, Self::Curl, Self::Side, Self::BellyUp];

    /// The poses a species sleeps in, with weights: dogs flop over, cats
    /// curl up, the fox curls under its tails (on its side or back they
    /// would fan out flat), the Munchkin sleeps belly-up.
    pub fn weights(species: &str) -> [u32; 4] {
        match species {
            "shiba" => [3, 2, 3, 2],
            "fox" => [2, 5, 0, 0],
            "munchkin" => [2, 3, 1, 3],
            "fold" => [2, 4, 1, 1],
            _ => [1, 0, 0, 0],
        }
    }
}

/// How the pet feels and where it is touched, as the walks show it (set
/// every frame).
#[derive(Clone, Copy, Debug, Default)]
pub struct Feel {
    pub mood: Mood,
    /// Look at the finger: turn and tilt, degrees (+ to the pet's left,
    /// + down); none when nobody is touching it.
    pub gaze: Option<(f32, f32)>,
    /// A finger is down on this zone, nearest this bone, this many seconds.
    pub held: Option<(Zone, Option<usize>, f32)>,
    /// The last touch let go: zone, bone, seconds ago.
    pub poked: Option<(Zone, Option<usize>, f32)>,
    /// Being scratched, 0 … 1 (see [`chonkgene_core::anim::Touch::scratch`]).
    pub scratch: f32,
    /// How it likes the spot held or last touched (see
    /// [`chonkgene_core::anim::Touch::fond`]).
    pub fond: Option<Fondness>,
    /// The 曼赤肯's belly trap, 0 … 1 (see
    /// [`chonkgene_core::anim::Touch::trap`]).
    pub trap: f32,
}

impl Feel {
    /// The touch is on its tail, bottom or back.
    pub fn behind(&self) -> bool {
        let zone = self.held.map(|h| h.0).or(self.poked.map(|p| p.0));
        matches!(zone, Some(Zone::Tail | Zone::Bottom | Zone::Back))
    }
}

/// Facing you while touched, degrees off straight on: a little turned, so
/// it still looks round.
pub(crate) const ATTEND_YAW: f32 = 6.0;
/// Laughing (and rolled over for a belly rub, or asleep on its back) it
/// lies side-on to you, so you see it roll; sulking it turns its back.
pub(crate) const UP_YAW: f32 = 85.0;
pub(crate) const SULK_YAW: f32 = 150.0;
/// Sulking, how far back it moves (world units), so its bottom stays in
/// the picture.
pub(crate) const SULK_BACK: f32 = 1.2;
/// Turning to you when touched (spring hertz): about a quarter second.
pub(crate) const ATTEND_HZ: f32 = 3.2;

/// What the feelings and touches come to this frame, worked out the same
/// for both walks.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Levels {
    /// 0 … 1 each, eased.
    pub attend: f32,
    pub laugh: f32,
    pub melt: f32,
    pub sulk: f32,
    pub content: f32,
    /// Rolled over for a belly rub (0 … 1).
    pub rub: f32,
    /// Looking back at a touched tail or bottom.
    pub back: f32,
    /// Where it looks (degrees, turn and tilt), times `attend`.
    pub gaze: (f32, f32),
    /// Leaning into a hand on its head, chin up for a scratch, tilting into
    /// a hand on its cheek (signed: + to its left), each 0 … 1 and fading
    /// back after a poke.
    pub head: f32,
    pub chin: f32,
    pub cheek: f32,
    /// A head flinch from a poke, 0 … 1.
    pub flinch: f32,
    /// A paw held or poked: its bone and how far up (0 … 1).
    pub paw: Option<(usize, f32)>,
    /// A tail or bottom poked this very frame (for a flick).
    pub flick: bool,
    /// A dog's hind leg kicking with a good scratch, 0 … 1.
    pub kick: f32,
    /// Being scratched (eased), 0 … 1: leaning into it, a cat kneading.
    pub scratch: f32,
    /// A hand on a spot it loves, held a moment (eased), 0 … 1: leaning
    /// into it as into a scratch, a cat kneading, eyes shut.
    pub bliss: f32,
    /// A hand on a spot it does not like (eased, fading after), 0 … 1:
    /// ears back, head turned away, leaning off.
    pub shy: f32,
    /// The belly trap (see [`Feel::trap`]): front paws clutching, hind
    /// legs bunny-kicking.
    pub trap: f32,
    /// Startled by a poke just now, 0 … 1, fading.
    pub startle: f32,
    /// Seconds on the walk's own clock.
    pub t: f32,
}

/// The springs behind [`Levels`].
#[derive(Clone, Debug, Default)]
pub(crate) struct Moods {
    scratch: Spring,
    attend: Spring,
    gaze: (Spring, Spring),
    laugh: Spring,
    melt: Spring,
    sulk: Spring,
    content: Spring,
    rub: Spring,
    bliss: Spring,
    shy: Spring,
    back: Spring,
    head: Spring,
    chin: Spring,
    cheek: Spring,
    /// The look follows the finger slowly and settles back gently, never
    /// past (the round birds: a quick look snapped their heads round,
    /// Roger's fourteenth pass).
    soft_gaze: bool,
    /// The last poke seen, seconds ago (to tell a new one).
    seen: f32,
    t: f32,
}

impl Moods {
    /// Moods whose look at a finger is soft (see `soft_gaze`).
    pub fn soft() -> Self {
        Self {
            soft_gaze: true,
            ..Self::default()
        }
    }

    /// Seconds the moods have run (a clock for beats that need one).
    pub fn clock(&self) -> f32 {
        self.t
    }

    pub fn step(&mut self, feel: &Feel, dt: f32) -> Levels {
        use std::f32::consts::TAU;
        self.t += dt;
        let ramp = |held: f32| chonkgene_rig::math::smoothstep(0.12, 0.55, held);
        let held = |z: Zone| match feel.held {
            Some((zone, _, secs)) if zone == z => ramp(secs),
            _ => 0.0,
        };
        // A poke shows for a moment, fading (`short`) or a little longer
        // (`long`: looking back).
        let faded = |z: Zone, secs: f32| match feel.poked {
            Some((zone, _, ago)) if zone == z => (-ago / secs).exp(),
            _ => 0.0,
        };
        let short = |z: Zone| faded(z, 0.4);
        let long = |z: Zone| faded(z, 1.3);
        let new_poke = feel.poked.is_some_and(|(.., ago)| ago < self.seen);
        self.seen = feel.poked.map_or(f32::MAX, |(.., ago)| ago);
        let flick = new_poke
            && feel
                .poked
                .is_some_and(|(z, ..)| matches!(z, Zone::Tail | Zone::Bottom | Zone::Back));
        let mood = |m: Mood| if feel.mood == m { 1.0 } else { 0.0 };
        let attend_hz = if self.soft_gaze && feel.gaze.is_none() {
            1.0
        } else {
            2.5
        };
        let attend = self
            .attend
            .step(feel.gaze.map_or(0.0, |_| 1.0), 0.0, attend_hz, 1.0, dt)
            .clamp(0.0, 1.0);
        let (gy, gx) = feel.gaze.unwrap_or((0.0, 0.0));
        // Soft: slower after a finger, slower still back once it has gone,
        // and damped so it never swings past.
        let (hz, damping) = match (self.soft_gaze, feel.gaze.is_some()) {
            (false, _) => (2.6, 0.9),
            (true, true) => (1.5, 1.0),
            (true, false) => (0.9, 1.0),
        };
        let gaze = (
            self.gaze.0.step(gy, 0.0, hz, damping, dt),
            self.gaze.1.step(gx, 0.0, hz, damping, dt),
        );
        let laugh = self.laugh.step(mood(Mood::Laugh), 0.0, 1.5, 0.75, dt);
        let melt = self.melt.step(mood(Mood::FedUp), 0.0, 1.1, 0.9, dt);
        let sulk = self.sulk.step(mood(Mood::Sulk), 0.0, 1.0, 1.0, dt);
        let content = self.content.step(mood(Mood::Content), 0.0, 1.2, 1.0, dt);
        let rub = self
            .rub
            .step(held(Zone::Belly), 0.0, 1.3, 0.8, dt)
            .clamp(0.0, 1.2);
        // Looking back at a hand behind it, unless it is a spot it loves:
        // then it enjoys it facing you, eyes shut.
        let loved = feel.fond == Some(Fondness::Loves);
        let back_goal = if loved {
            0.0
        } else {
            held(Zone::Tail)
                .max(held(Zone::Bottom))
                .max(held(Zone::Back))
                .max(long(Zone::Tail))
                .max(long(Zone::Bottom))
                .max(long(Zone::Back))
        };
        let back = self.back.step(back_goal, 0.0, 2.2, 0.9, dt);
        let head = self.head.step(held(Zone::Head), 0.0, 2.0, 0.8, dt);
        let chin = self.chin.step(held(Zone::Chin), 0.0, 2.0, 0.8, dt);
        let side = if gy < 0.0 { -1.0 } else { 1.0 };
        let cheek = self.cheek.step(
            side * held(Zone::Cheek).max(short(Zone::Cheek)),
            0.0,
            2.2,
            0.8,
            dt,
        );
        let flinch = short(Zone::Head);
        // A paw (or wing) it does not like touched stays tucked in.
        let paw = match (feel.held, feel.poked) {
            _ if feel.fond == Some(Fondness::Dislikes) => None,
            // A bird's wing lifts as a paw does.
            (Some((Zone::Paw | Zone::Wing, bone, secs)), _) => bone.map(|b| (b, ramp(secs))),
            (None, Some((z @ (Zone::Paw | Zone::Wing), bone, _))) => bone.map(|b| (b, short(z))),
            _ => None,
        };
        let scratch = self
            .scratch
            .step(feel.scratch, 0.0, 3.5, 0.9, dt)
            .clamp(0.0, 1.0);
        // A spot it loves, held: bliss after a moment. One it does not
        // like: shy at once, and for a while after the hand goes.
        let love_held = match (feel.held, feel.fond) {
            (Some((.., secs)), Some(Fondness::Loves)) => {
                chonkgene_rig::math::smoothstep(0.3, 0.9, secs)
            }
            _ => 0.0,
        };
        let bliss = self
            .bliss
            .step(love_held, 0.0, 2.0, 0.9, dt)
            .clamp(0.0, 1.0);
        let shy_goal = match (feel.fond, feel.held, feel.poked) {
            (Some(Fondness::Dislikes), Some(_), _) => 1.0,
            (Some(Fondness::Dislikes), None, Some((.., ago))) => (1.0 - ago / 1.2).max(0.0),
            _ => 0.0,
        };
        let shy = self.shy.step(shy_goal, 0.0, 3.0, 0.9, dt).clamp(0.0, 1.0);
        // A good scratch under the chin or on the belly, or a finger rubbing
        // about anywhere, sets a hind leg going; so does a loved bottom held
        // (the Shiba scratched at the base of its tail).
        let loved_bottom = if feel.fond == Some(Fondness::Loves) {
            held(Zone::Bottom).max(held(Zone::Back))
        } else {
            0.0
        };
        let kick = held(Zone::Chin)
            .max(held(Zone::Belly))
            .max(loved_bottom)
            .max(scratch)
            * (0.5 + 0.5 * (TAU * 0.5 * self.t).sin().abs());
        // A quick poke anywhere: a start, over in about half a second.
        let startle = feel
            .poked
            .filter(|&(.., ago)| ago < 0.8)
            .map_or(0.0, |(.., ago)| (1.0 - ago / 0.8).powi(2));
        Levels {
            attend,
            laugh: laugh.clamp(0.0, 1.2),
            melt: melt.clamp(0.0, 1.0),
            sulk: sulk.clamp(0.0, 1.0),
            content: content.clamp(0.0, 1.0),
            rub,
            back: back.clamp(0.0, 1.0),
            gaze: (gaze.0 * attend, gaze.1 * attend),
            head: head.clamp(0.0, 1.0),
            chin: chin.clamp(0.0, 1.0) + 0.5 * short(Zone::Chin),
            cheek: cheek.clamp(-1.0, 1.0),
            flinch,
            paw,
            flick,
            kick,
            scratch,
            bliss,
            shy,
            trap: feel.trap.clamp(0.0, 1.0),
            startle,
            t: self.t,
        }
    }
}

/// A damped spring: `x` chases `target`, shaken by `push` (an acceleration).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Spring {
    pub x: f32,
    pub v: f32,
}

impl Spring {
    pub fn step(&mut self, target: f32, push: f32, hz: f32, damping: f32, dt: f32) -> f32 {
        let w = std::f32::consts::TAU * hz;
        let a = -w * w * (self.x - target) - 2.0 * damping * w * self.v + push;
        self.v += a * dt;
        self.x += self.v * dt;
        self.x
    }
}

/// Steps a second at full walking pace.
const STEP_HZ: f32 = 2.0;
/// Leg swing either way, degrees: with legs 0.45 long and 62% of each step
/// on the floor, a planted foot slides back at 1.0 world units a second,
/// the pace the pet's window moves (60 px/s at 100%).
const SWING: f32 = 20.0;
/// How long the legs are the swing is set for (floor to hip pivot, world
/// units; the Shiba's): shorter legs swing wider to keep pace with the
/// window, longer ones narrower.
const LEG_REF: f32 = 0.59;
const DUTY: f32 = 0.62;
const LIFT: f32 = 0.07;
/// How quickly the walk gets going or stops at the usual pace, full pace a
/// second: as quickly as the window does (`chonkgene_core::motion`), so a
/// planted paw stays put while the pet slows down.
pub(crate) const SPEED_UP: f32 = 1.0 / chonkgene_core::motion::WALK_RAMP;
/// Still slowing down faster than this (0 … 1), the pet keeps facing the way
/// it goes rather than turning to you.
pub(crate) const STILL_GOING: f32 = 0.3;
/// How quickly the pet turns to face where it goes (spring hertz), at rest
/// and walking: walking, the window already moves the new way, so the
/// turn must be quick or the pet seems to slide backwards.
pub(crate) const TURN_HZ: (f32, f32) = (1.3, 2.8);
/// The widest a leg swings, degrees, however fast the pet goes.
pub(crate) const MAX_SWING: f32 = 45.0;

/// Where a snack is at `t` seconds into eating it: how far the head is
/// down for a bite (0 … 1), and the chewing after it (−1 … 1). Each bite
/// goes down to the snack, nibbles at it a moment (the head bobbing a
/// little), comes up and chews.
pub(crate) fn bite(t: f32) -> (f32, f32) {
    use chonkgene_core::anim::BITE_SECS;
    use std::f32::consts::TAU;
    let b = (t / BITE_SECS).fract();
    let ease = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    if b < 0.5 {
        let nibble = 0.12 * (TAU * 7.0 * t).sin().abs();
        (ease(b / 0.2) - nibble * ease((b - 0.15) / 0.1), 0.0)
    } else {
        let chew = (TAU * 2.5 * (b - 0.5) * BITE_SECS).sin();
        (1.0 - ease((b - 0.5) / 0.15), chew * ease((b - 0.55) / 0.1))
    }
}

/// A bird's quick peck, `hz` a second: straight down, then back up more
/// slowly (0 up … 1 down).
pub(crate) fn peck(t: f32, hz: f32) -> f32 {
    let p = (t * hz).fract();
    if p < 0.3 {
        (std::f32::consts::FRAC_PI_2 * p / 0.3).sin()
    } else {
        let x = (p - 0.3) / 0.7;
        1.0 - x * x * (3.0 - 2.0 * x)
    }
}

/// Splits a pace (1 usual) into how much quicker and how much longer the
/// steps get: both grow, so zoomies neither blur into a flicker of legs
/// nor stride absurdly; their product is about the pace, for the sine of
/// the swing grows a little slower than the angle.
pub(crate) fn tempo_split(tempo: f32) -> (f32, f32) {
    (tempo.powf(0.6), tempo.powf(0.4))
}

/// Where each leg is in the step cycle (a walk, one foot at a time), and
/// whether it is a front leg.
const LEGS: [(&str, f32, bool); 4] = [
    ("leg_bl", 0.0, false),
    ("leg_fl", 0.25, true),
    ("leg_br", 0.5, false),
    ("leg_fr", 0.75, true),
];
/// Where each leg (in [`LEGS`]' order) is in the step cycle running: a
/// gallop, as a dog or a cat runs: the hind paws land a beat apart, then
/// the front paws the same way half a step on (a phase further on lands
/// sooner: hind right, hind left, front right, front left). Placed against the
/// walk so that no leg moves its phase by more than a third of a step, and
/// the mix of the two never brings the legs of one side together (a pace,
/// 同手同腳: Roger's twentieth pass, item 4).
const GALLOP: [f32; 4] = [0.075, 0.575, 0.175, 0.675];
/// How much of each step a foot is on the floor, galloping (walking it is
/// [`DUTY`]): less, so there are moments with all four off it.
const RUN_DUTY: f32 = 0.45;
/// Running, the walk turns into a gallop between these paces.
const RUN_PACES: (f32, f32) = (1.3, 2.0);

/// Where a leg is in its step, `run` (0 walking … 1 galloping) of the way
/// from its place in the walk to its place in the gallop, the short way
/// round.
fn leg_phase(ph: f32, walk: f32, gallop: f32, run: f32) -> f32 {
    let d = (gallop - walk + 0.5).rem_euclid(1.0) - 0.5;
    (ph + walk + run * d).rem_euclid(1.0)
}

/// Scoops a second, each front paw, digging.
const DIG_HZ: f32 = 2.6;
/// The chest's rise and fall, world units; twice a step cycle.
const BOB: f32 = 0.02;
const ROLL: f32 = 3.0;
const TWIST: f32 = 4.0;
/// How far the pet turns side-on while walking, and at rest (toward you).
pub(crate) const WALK_YAW: f32 = 72.0;
pub(crate) const REST_YAW: f32 = 28.0;
/// Sitting, it turns almost to face you, so the tails fanned out behind it
/// show on both sides of its head.
pub(crate) const SIT_YAW: f32 = 8.0;
/// Hugging another pet it turns this far toward it, degrees: face to face
/// across the desktop, still showing its face.
pub(crate) const HUG_YAW: f32 = 58.0;

/// Sitting: the body tips back this far (degrees) about the front paws,
/// which brings the bottom down onto the floor; the hind legs fold
/// forward under it.
const SIT_PITCH: f32 = 24.0;
/// Lying down, the body sinks this share of the legs' length: the belly
/// comes to rest on the floor.
const LIE_DROP: f32 = 0.42;
const SIT_HIND: f32 = -68.0;
/// Where the front paws stand, the point the body tips back about.
const FRONT_PAWS: V3 = v3(0.0, FLOOR, 0.5);
/// Sitting, several tails open behind the pet like a fan, so all of them
/// show: spread up to this far either side of straight up (degrees), and
/// leaning back this much. A single tail just keeps its place against the
/// body tipping back.
const SIT_FAN: f32 = 95.0;
const SIT_FAN_BACK: f32 = 0.35;
/// The fan leaves a gap this wide either side of straight up, where the
/// head and ears would hide a tail.
const SIT_FAN_GAP: f32 = 24.0;
/// Asleep, several tails fold forward over the pet's back like a blanket:
/// the whole bunch tips forward this far (degrees) at the root, spreading
/// a little side to side, and each segment past the root droops on this
/// much, so the tips settle over the back. They move as one bunch: their
/// roots share skin, and pulling them apart would stretch it into a sheet.
const BLANKET_TIP: f32 = 62.0;
const BLANKET_SPREAD: f32 = 14.0;
const BLANKET_DROOP: f32 = 16.0;
/// Curled up asleep, the chest turns this far (degrees) one way and the
/// hips twice as far back the other, the head further round; seen from
/// a little to the side.
const CURL: f32 = 34.0;
pub(crate) const CURL_YAW: f32 = 50.0;
/// Curled up, a single tail tips down this far (degrees, times how upright
/// it is held) and bends this far in all round toward the front.
const CURL_TAIL_DOWN: f32 = 70.0;
const CURL_TAIL_WRAP: f32 = 110.0;
/// Asleep on its back, the belly swells and sinks this much more with each
/// breath (world units along the belly bone).
/// Roger's seventeenth pass: still too small to see (was 0.11; with the
/// rest, 1.6 times the old swell; at 0.28 its top came to a point).
const BELLY_UP_BREATH: f32 = 0.24;
/// How much more a belly wobbles than it used to, when the body moves or
/// is poked, and how far it may go (Roger's seventeenth pass: more to see
/// on every pet but the fox, whose fur hides it anyway).
pub(crate) const JIGGLE: f32 = 1.6;
pub(crate) const JIGGLE_MOST: f32 = 0.18;
/// Dancing, how far a front paw goes out to the side, degrees (the first
/// front leg's way; the other's the opposite).
const DANCE_OUT: f32 = 45.0;
/// Lying down in any pose, how much more its belly swells with a breath.
const LYING_BREATH: f32 = 0.05;
/// Lying chin on paws, turned this far, so its length shows behind the head.
pub(crate) const CHIN_YAW: f32 = 30.0;
/// Rolled over (degrees about its length): on its back laughing or asleep,
/// for a belly rub nearly so, asleep on its side a quarter turn less.
const UP_ROLL: f32 = 160.0;
const ROLL_TUCK: f32 = 72.0;
const ROLL_LIFT: f32 = 0.3;
const RUB_ROLL: f32 = 145.0;
const SIDE_ROLL: f32 = 85.0;
/// Eating, the pet crouches over the snack: it sinks this share of its
/// legs' length.
const EAT_CROUCH: f32 = 0.3;
/// The bottom's waddle side to side at full pace, world units, and the
/// bounce it gets plopping down into a sit (and a smaller one getting up),
/// world units a second.
const BUTT_WADDLE: f32 = 0.05;
const BUTT_PLOP: f32 = 1.8;

/// What the body is doing, for the fidgets: walking, in a gesture, or a
/// pose held.
pub(crate) fn hold(going: bool, gesture: bool) -> Hold {
    if going {
        Hold::Off
    } else if gesture {
        Hold::Busy
    } else {
        Hold::Free
    }
}

/// The slow drift under every held pose (degrees): the head tips, turns
/// and nods a little and the body sways, each on its own slow beat, so it
/// never stands frozen between fidgets. Head (x, y, z), then body z.
pub(crate) fn drift(t: f32) -> (f32, f32, f32, f32) {
    use std::f32::consts::TAU;
    (
        4.0 * (TAU * 0.29 * t + 2.0).sin(),
        8.0 * (TAU * 0.13 * t + 1.0).sin(),
        7.0 * (TAU * 0.21 * t).sin(),
        2.5 * (TAU * 0.17 * t + 0.5).sin(),
    )
}

/// Tail swish: how far each segment bends against the one before, degrees,
/// standing and walking. The bend runs down the tail as a wave, each
/// segment `WAVE_LAG` radians behind the one before, so a tail sways like
/// seaweed in a current rather than swinging stiff from its root.
const SWISH_REST: f32 = 12.0;
const SWISH_WALK: f32 = 13.0;
const WAVE_LAG: f32 = 0.9;
/// A long tail's wave still spans this many radians from root to tip, so
/// a tail of many short segments ripples no more than one of a few.
const WAVE_SPAN: f32 = 7.5;
/// Swishes a second, standing and at full pace.
const SWISH_HZ: (f32, f32) = (0.6, 1.6);

/// A tail: its bones from the base out, with a sideways swish and an
/// up-and-down bounce spring for each, and where in the swish it starts.
struct Tail {
    bones: Vec<usize>,
    swish: Vec<Spring>,
    bounce: Vec<Spring>,
    /// The base's turn about the vertical: flung out by the body turning.
    fling: Spring,
    offset: f32,
    /// The turn (degrees, as [`M3::euler`]) that takes the root from where
    /// it rests to its place in the fan, sitting.
    sit_turn: V3,
    /// The turns that wrap it round the pet asleep: the root's, and each
    /// segment's after it (several tails only; see [`blanket`]).
    lie_turn: V3,
    lie_curl: V3,
    /// How the sideways swish is shared between turning about z (a tail
    /// held up, as the Shiba's) and about the vertical (one held out
    /// behind, as the Fold's), from which way its root points.
    swish_axes: (f32, f32),
}

/// How a species' tail shows it is pleased or at play (PLAN §8 "Roger's
/// tenth pass", item 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TailStyle {
    /// A dog's: wagging hard all the while.
    Wag,
    /// A cat's: swaying slowly, curling up at the tip and flicking.
    Curl,
    /// The fox's: wagging, each tail lifting in turn.
    Ripple,
}

/// A four-legged walk, worked out frame by frame.
pub struct Walk {
    phase: f32,
    /// 0 standing … 1 full pace.
    pub speed: f32,
    yaw: Spring,
    /// Where the tails are in their swish, cycles.
    swish: f32,
    tails: Vec<Tail>,
    belly: Spring,
    /// How much its belly wobbles (see [`JIGGLE`]).
    jiggle: f32,
    butt: Spring,
    butt_z: Spring,
    butt_x: Spring,
    chin: Spring,
    /// The chest's height the last two frames, and the pace last frame, for
    /// the shakes they give the soft parts.
    bob: [f32; 2],
    nod: [f32; 2],
    pace: f32,
    /// The turn's speed last frame: turning flings the tails the other way.
    yaw_v: f32,
    /// Asked to sit, and how far down it is (0 standing … 1 sitting).
    sitting: bool,
    /// Asked to lie down (asleep, or `awake`: 趴下, a still mouse), and how
    /// far down it is.
    lying: bool,
    awake: bool,
    lie: Spring,
    /// The head's extra turn, and where it is heading.
    look: Spring,
    look_goal: f32,
    sit: Spring,
    legs: [(usize, f32, bool); 4],
    gesture: Option<Gesture>,
    /// Its part in a hug, for [`Act::Hug`](chonkgene_core::anim::Act::Hug).
    pub(crate) hug: Hug,
    /// Sliding, gliding or flying (see [`Travel`]), eased.
    pub(crate) travel: Travel,
    travel_w: [Spring; 3],
    /// Walking speed against the usual (see [`Walk::set_pace`]).
    tempo: f32,
    /// Leg swing either way at full pace, degrees: [`SWING`] for the legs
    /// of [`LEG_REF`], wider for shorter ones.
    swing: f32,
    /// Floor to hip pivot, world units.
    leg_len: f32,
    bones: Bones,
    n: usize,
    /// Still on its way somewhere (walking, turning, sitting down, getting
    /// up, a gesture): it needs smooth frames.
    busy: bool,
    feel: Feel,
    moods: Moods,
    /// How it sleeps, and each pose's share (eased, so it can change pose
    /// in its sleep).
    sleep_pose: SleepPose,
    sleep_w: [Spring; 4],
    /// Rolled over about its length, degrees (on its back 180).
    roll: Spring,
    /// A dog: a good scratch sets a hind leg kicking.
    kicks: bool,
    /// A cat: scratched, its front paws knead.
    kneads: bool,
    /// Floor to the middle of the body, world units: what it rolls about.
    body_h: f32,
    /// Out: how far the renderer should settle the pet onto the floor
    /// (rolled over, melted), and flatten it (melted), this frame.
    ground: f32,
    flatten: f32,
    /// The little movements under every held pose, and this frame's.
    fidgets: Fidgets,
    fidget: Out,
    /// Ears laid back less on a Fold: its ears are folded already.
    ear_k: f32,
    /// The fox's bell swinging on its cord (degrees about x and z).
    bell: (Spring, Spring),
    /// −1 … 1 over a breath: the belly rises and falls with it.
    breath: f32,
    /// A weight shift's lean last frame: shifting wobbles the belly.
    shift: f32,
    tail_style: TailStyle,
    /// Eating last frame: a snack finished, it licks its nose.
    ate: bool,
}

struct Bones {
    hind_l: Option<usize>,
    hind_r: Option<usize>,
    body: usize,
    hips: usize,
    head: usize,
    belly: Option<usize>,
    butt: Option<usize>,
    chin: Option<usize>,
    ears: [Option<usize>; 2],
    bell: Option<usize>,
}

impl Walk {
    pub fn new(sk: &Skeleton, facing: f32) -> Result<Self, String> {
        let need = |n: &str| sk.bone(n).ok_or_else(|| format!("no bone {n:?}"));
        let mut legs = [(0, 0.0, false); 4];
        for (l, (name, offset, front)) in legs.iter_mut().zip(LEGS) {
            *l = (need(name)?, offset, front);
        }
        let leg_len = legs.iter().map(|l| sk.pivots[l.0].y - FLOOR).sum::<f32>() / 4.0;
        let swing = SWING * (LEG_REF / leg_len.max(0.2)).clamp(0.8, 2.0);
        // Several tails swish out of step, so they ripple rather than
        // sway as one.
        let chains = sk.tails();
        let turns = fan(sk, &chains);
        let blankets = blanket(sk, &chains);
        let tails = chains
            .into_iter()
            .zip(turns.into_iter().zip(blankets))
            .enumerate()
            .map(|(i, (bones, (sit_turn, (lie_turn, lie_curl))))| {
                let up = match bones[..] {
                    [a, b, ..] => (sk.pivots[b] - sk.pivots[a]).normalize().y.abs(),
                    _ => 1.0,
                };
                Tail {
                    swish: vec![Spring::default(); bones.len()],
                    bounce: vec![Spring::default(); bones.len()],
                    bones,
                    fling: Spring::default(),
                    offset: i as f32 * 0.37,
                    sit_turn,
                    lie_turn,
                    lie_curl,
                    swish_axes: (up, (1.0 - up * up).max(0.0).sqrt()),
                }
            })
            .collect();
        Ok(Self {
            phase: 0.0,
            speed: 0.0,
            yaw: Spring {
                x: facing * REST_YAW,
                v: 0.0,
            },
            swish: 0.0,
            tails,
            belly: Spring::default(),
            jiggle: 1.0,
            butt: Spring::default(),
            butt_z: Spring::default(),
            butt_x: Spring::default(),
            chin: Spring::default(),
            bob: [0.0; 2],
            nod: [0.0; 2],
            pace: 0.0,
            yaw_v: 0.0,
            sitting: false,
            lying: false,
            awake: false,
            lie: Spring::default(),
            look: Spring::default(),
            look_goal: 0.0,
            sit: Spring::default(),
            legs,
            gesture: None,
            hug: Hug::ByYou,
            travel: Travel::None,
            travel_w: Default::default(),
            tempo: 1.0,
            swing,
            leg_len,
            bones: Bones {
                hind_l: sk.bone("leg_bl"),
                hind_r: sk.bone("leg_br"),
                body: need("body")?,
                hips: need("hips")?,
                head: need("head")?,
                belly: sk.bone("belly"),
                butt: sk.bone("butt"),
                chin: sk.bone("chin"),
                ears: [sk.bone("ear_l"), sk.bone("ear_r")],
                bell: sk.bone("bell"),
            },
            n: sk.names.len(),
            busy: false,
            feel: Feel::default(),
            moods: Moods::default(),
            sleep_pose: SleepPose::Chin,
            sleep_w: [
                Spring { x: 1.0, v: 0.0 },
                Spring::default(),
                Spring::default(),
                Spring::default(),
            ],
            roll: Spring::default(),
            kicks: false,
            kneads: false,
            body_h: sk.bone("body").map_or(0.8, |b| sk.pivots[b].y - FLOOR),
            ground: 0.0,
            flatten: 0.0,
            fidgets: Fidgets::new("", 1),
            fidget: Out::default(),
            ear_k: 1.0,
            bell: (Spring::default(), Spring::default()),
            breath: 0.0,
            shift: 0.0,
            tail_style: TailStyle::Wag,
            ate: false,
        })
    }

    /// The species it is: dogs kick a hind leg when scratched; each has its
    /// own fidgets.
    pub fn set_species(&mut self, id: &str) {
        self.kicks = matches!(id, "shiba" | "fox");
        self.jiggle = if id == "fox" { 1.0 } else { JIGGLE };
        self.kneads = matches!(id, "fold" | "munchkin");
        self.fidgets = Fidgets::new(id, 1);
        self.ear_k = if id == "fold" { 0.45 } else { 1.0 };
        self.tail_style = match id {
            "fold" | "munchkin" => TailStyle::Curl,
            "fox" => TailStyle::Ripple,
            _ => TailStyle::Wag,
        };
    }

    /// Pleased or at play (`lively`), out of breath (`puffed`): set every
    /// frame (see [`Fidgets::set_mood`]).
    pub fn set_mood(&mut self, lively: bool, puffed: bool) {
        self.fidgets.set_mood(lively, puffed);
    }

    /// How lively it is (0 … 1) and a seed of its own, so two pets of a
    /// kind never fidget in step.
    pub fn set_fidgets(&mut self, species: &str, energy: f32, seed: u32) {
        self.fidgets = Fidgets::new(species, seed);
        self.fidgets.set_energy(energy);
    }

    /// A lick of the nose now.
    pub fn lick(&mut self) {
        self.fidgets.now(Kind::Lick);
    }

    /// Where it is in its breath, −1 … 1 (set every frame).
    pub fn breathe(&mut self, b: f32) {
        self.breath = b;
    }

    /// No fidgets (see [`Fidgets::stop`]).
    pub fn still(&mut self) {
        self.fidgets.stop();
    }

    /// The fidgets this frame (the face shows the tongue; quick ones need
    /// smooth frames).
    pub fn fidget(&self) -> &Out {
        &self.fidget
    }

    /// How it feels and where it is touched (set every frame).
    pub fn feel(&mut self, feel: Feel) {
        self.feel = feel;
    }

    /// The pose to sleep in (eased into, even mid-sleep).
    pub fn sleep_pose(&mut self, pose: SleepPose) {
        self.sleep_pose = pose;
    }

    /// How far to settle the pet onto the floor this frame (0 … 1), and
    /// how much to flatten it (see the fields).
    pub fn ground(&self) -> f32 {
        self.ground
    }

    pub fn flatten(&self) -> f32 {
        self.flatten
    }

    /// Jolt the soft parts (a poke in the belly), in world units a second:
    /// the belly most, the bottom and the chin wobbling along.
    pub fn poke(&mut self, kick: f32) {
        self.belly.v += kick * self.jiggle;
        self.butt.v += kick * 0.7;
        self.chin.v += kick * 0.6;
    }

    /// The idle gesture under way, if any (set every frame).
    pub fn gesture(&mut self, g: Option<Gesture>) {
        self.gesture = g;
    }

    /// Walk this much faster than usual (zoomies) or slower: the legs step
    /// quicker and longer so a planted paw still keeps pace.
    pub fn set_pace(&mut self, pace: f32) {
        self.tempo = pace.clamp(0.2, 5.0);
    }

    /// Sit down (or get up); walking gets it up first.
    pub fn sit(&mut self, on: bool) {
        self.sitting = on;
    }

    /// Lie down to sleep (or get up); walking gets it up first.
    pub fn lie(&mut self, on: bool) {
        self.lying = on;
        self.awake = false;
    }

    /// Lie down awake (or get up): head up, looking about, fidgeting as
    /// usual.
    pub fn rest(&mut self, on: bool) {
        self.lying = on;
        self.awake = on;
    }

    /// Turn the head this far (degrees, + to the pet's left) on top of
    /// what the walk does with it; it gets there smoothly.
    pub fn look(&mut self, deg: f32) {
        self.look_goal = deg;
    }

    /// How far down into a sit, 0 standing … 1 sitting.
    pub fn sat(&self) -> f32 {
        self.sit.x.clamp(0.0, 1.0)
    }

    /// The pet's turn about the vertical, degrees (0: facing you).
    pub fn yaw(&self) -> f32 {
        self.yaw.x
    }

    /// Still on its way somewhere: walking, turning, sitting down or
    /// getting up, or in a gesture.
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// Advance `dt` seconds, walking or standing, facing +1 (right) or −1
    /// (left); the joints for this frame.
    pub fn step(&mut self, dt: f32, walking: bool, facing: f32) -> Vec<Joint> {
        use std::f32::consts::{PI, TAU};
        let dt = dt.max(1e-4);
        let goal = if walking { 1.0 } else { 0.0 };
        let ramp = dt * SPEED_UP * self.tempo.max(0.2);
        self.speed += (goal - self.speed).clamp(-ramp, ramp);
        let s = self.speed;
        // Faster walking takes both quicker and longer steps.
        let (quicker, longer) = tempo_split(self.tempo);
        if s > 0.0 {
            self.phase = (self.phase + dt * STEP_HZ * (0.4 + 0.6 * s) * quicker).fract();
        }
        let going = walking || s > STILL_GOING;
        let lv = self.moods.step(&self.feel, dt);
        // Eating, it turns to you, so you see it eat; a snack finished, it
        // licks its nose.
        let eating = self
            .gesture
            .is_some_and(|g| g.act == chonkgene_core::anim::Act::Eat);
        if self.ate && !eating {
            self.fidgets.now(Kind::Lick);
        }
        self.ate = eating;
        // Fidgets go on under any held pose; in a gesture only the tail's;
        // walking, none.
        let f = self.fidgets.step(
            dt,
            hold(going, self.gesture.is_some()),
            self.lying && !self.awake,
        );
        self.fidget = f;
        // How it sleeps, eased from one pose to the next.
        let mut sw = [0.0; 4];
        for (k, w) in self.sleep_w.iter_mut().enumerate() {
            let goal = if SleepPose::ALL[k] == self.sleep_pose {
                1.0
            } else {
                0.0
            };
            sw[k] = w.step(goal, 0.0, 0.5, 1.0, dt).clamp(0.0, 1.0);
        }
        // Touched, it turns to face you; a tail or a bottom touched, it only
        // looks back over its shoulder (turning round would hide them).
        let touched = self.feel.gaze.is_some() && !self.feel.behind();
        // Several tails (the fox) would prop it up off its back like legs:
        // it laughs and takes a rub sitting up instead.
        let many_tails = self.tails.len() > 1;
        let rolled_over = !many_tails && (self.feel.mood == Mood::Laugh || lv.rub > 0.3);
        let sits_up = many_tails && (self.feel.mood == Mood::Laugh || lv.rub > 0.3);
        let hugging = self
            .gesture
            .is_some_and(|g| g.act == chonkgene_core::anim::Act::Hug && self.hug != Hug::ByYou);
        let yaw = self.yaw.step(
            facing
                * if going || self.travel != Travel::None {
                    WALK_YAW
                } else if hugging {
                    HUG_YAW
                } else if self.feel.mood == Mood::Sulk {
                    SULK_YAW
                } else if rolled_over
                    || (self.lying
                        && matches!(self.sleep_pose, SleepPose::Side | SleepPose::BellyUp))
                {
                    UP_YAW
                } else if touched {
                    ATTEND_YAW
                } else if self.lying && self.sleep_pose == SleepPose::Curl {
                    CURL_YAW
                } else if self.lying && self.sleep_pose == SleepPose::Chin {
                    CHIN_YAW
                } else if self.sitting || self.lying || eating {
                    SIT_YAW
                } else {
                    REST_YAW
                },
            0.0,
            if going {
                TURN_HZ.1
            } else if touched {
                ATTEND_HZ
            } else {
                TURN_HZ.0
            },
            1.0,
            dt,
        );
        let ph = self.phase;
        // How far into a sit, worked out first: the tails and the bottom
        // need it too.
        let was_sat = self.sit.x;
        // Walking, it gets up quickly: the window is already moving.
        let up_hz = if walking { 2.5 } else { 1.2 };
        // Sulking it sits with its back to you; laughing, melted or rolled
        // over it does not sit.
        let lying_about = self.feel.mood == Mood::FedUp || rolled_over;
        let goal = if (self.sitting || self.feel.mood == Mood::Sulk || sits_up)
            && !self.lying
            && !lying_about
            && !walking
        {
            1.0
        } else {
            0.0
        };
        let sit = self.sit.step(goal, 0.0, up_hz, 1.0, dt).clamp(0.0, 1.0);
        let goal = if self.lying && !walking { 1.0 } else { 0.0 };
        let lie = self
            .lie
            .step(goal, 0.0, if walking { 2.5 } else { 0.8 }, 1.0, dt)
            .clamp(0.0, 1.0);
        let mut j = vec![Joint::default(); self.n];
        let b = &self.bones;

        // Legs: on the floor, a straight sweep back (the foot stays put
        // while the body passes over it); off it, an eased swing forward
        // with the foot lifted.
        // Running (Roger's eighteenth pass: 奔跑), the walk turns into a
        // gallop (see [`GALLOP`]): the back flexing and stretching, the
        // whole body bouncing, the feet down for less of each step and
        // lifted higher.
        let run = chonkgene_rig::math::smoothstep(RUN_PACES.0, RUN_PACES.1, self.tempo) * s;
        let duty = DUTY + (RUN_DUTY - DUTY) * run;
        for (&(leg, offset, _), gallop) in self.legs.iter().zip(GALLOP) {
            let psi = leg_phase(ph, offset, gallop, run);
            let (fwd, lift) = if psi < duty {
                (1.0 - 2.0 * psi / duty, 0.0)
            } else {
                let t = (psi - duty) / (1.0 - duty);
                let e = t * t * (3.0 - 2.0 * t);
                (-1.0 + 2.0 * e, (PI * t).sin())
            };
            j[leg] = Joint {
                rot: v3(-(self.swing * longer).min(MAX_SWING) * s * fwd, 0.0, 0.0),
                shift: v3(0.0, LIFT * s * lift * (1.0 + 0.6 * run), 0.0),
            };
        }
        // The run's bounce and the back's flex keep time with the front
        // left paw: lowest as the front paws have landed.
        let gph = leg_phase(ph, LEGS[1].1, GALLOP[1], run);

        // The spine: the chest rises over each planted leg and rolls onto
        // it, the hips roll and twist the other way.
        let bob =
            -BOB * s * (2.0 * TAU * ph).cos() * (1.0 - run) - 3.5 * BOB * run * (TAU * gph).cos();
        let roll = ROLL * s * (TAU * ph).sin() * (1.0 - 0.6 * run);
        j[0] = Joint {
            rot: v3(9.0 * run * (TAU * gph).sin(), yaw, 0.0),
            shift: v3(0.0, bob, 0.0),
        };
        j[b.body] = Joint {
            rot: v3(0.0, -0.4 * TWIST * s * (TAU * ph).cos(), roll),
            shift: V3::ZERO,
        };
        j[b.hips] = Joint {
            rot: v3(0.0, TWIST * s * (TAU * ph).cos(), -2.0 * roll),
            shift: V3::ZERO,
        };
        // The head stays steadier than the body (a little nod, a beat
        // late) and turns toward you, more so at rest.
        let nod = 3.0 * s * (2.0 * TAU * ph - 0.9).sin();
        let look = self.look.step(self.look_goal, 0.0, 1.6, 1.0, dt);
        j[b.head] = Joint {
            rot: v3(
                nod + lv.gaze.1,
                -yaw * (0.45 - 0.3 * s) * (1.0 - lv.attend) + look + lv.gaze.0,
                -0.6 * roll,
            ),
            shift: V3::ZERO,
        };

        // Shakes for the soft parts: the chest's vertical acceleration, the
        // nod's, and speeding up or slowing down.
        let acc = (bob - 2.0 * self.bob[0] + self.bob[1]) / (dt * dt);
        self.bob = [bob, self.bob[0]];
        let nod_acc = (nod - 2.0 * self.nod[0] + self.nod[1]) / (dt * dt);
        self.nod = [nod, self.nod[0]];
        let pace_acc = (s - self.pace) / dt;
        self.pace = s;
        let yaw_acc = (self.yaw.v - self.yaw_v) / dt;
        self.yaw_v = self.yaw.v;

        // Tails: a wave runs from root to tip, faster and wider at pace;
        // the root bounces with the steps, and turning flings it out.
        // A burst of wagging: quicker and wider.
        // Asleep, the tails hardly stir; a snack sets them going.
        let treat = self
            .gesture
            .filter(|g| g.act == chonkgene_core::anim::Act::Eat)
            .map_or(0.0, |g| g.e);
        // Pleased (a wiggle, a hop, a spin), the tail goes like mad.
        let joy = self
            .gesture
            .filter(|g| {
                use chonkgene_core::anim::Act;
                matches!(g.act, Act::Wiggle | Act::Bounce | Act::Spin | Act::Dance)
            })
            .map_or(0.0, |g| g.e);
        // Pleased or at play, a dog's tail (and the fox's) wags all the
        // while; a cat's sways a little quicker and curls (see `lively`).
        let keen = match self.tail_style {
            TailStyle::Wag | TailStyle::Ripple => 0.85 * f.lively,
            TailStyle::Curl => 0.0,
        };
        let wag = f.get(Kind::Wag).map_or(0.0, |w| w.0).max(joy).max(keen);
        // Dancing (肉肉舞): the Shiba's tail goes round like a helicopter,
        // the fox fans its tails out like a peacock.
        let dance = self
            .gesture
            .filter(|g| g.act == chonkgene_core::anim::Act::Dance)
            .map(|g| (g.e, g.t));
        let curl = if self.tail_style == TailStyle::Curl {
            f.lively
        } else {
            0.0
        };
        self.swish = (self.swish
            + dt * (SWISH_HZ.0 + (SWISH_HZ.1 - SWISH_HZ.0) * s) * (1.0 + 2.5 * wag + 0.6 * curl))
            .fract();
        // Laughing it thrashes its tail; melted it lies limp; held, it goes
        // stiff in your hand.
        let held_tail = matches!(self.feel.held, Some((Zone::Tail, ..)));
        // Lying down the tail stirs less, unless it is lively.
        let amp = (SWISH_REST + (SWISH_WALK - SWISH_REST) * s)
            * (1.0 - 0.75 * lie * (1.0 - 0.7 * f.lively))
            * (1.0 + 0.8 * treat + 0.9 * lv.laugh)
            * (1.0 - 0.8 * lv.melt)
            * (1.0 + 1.6 * wag)
            * if held_tail { 0.25 } else { 1.0 };
        for tail in &mut self.tails {
            // A poke on the tail or the bottom: a flick.
            if lv.flick {
                tail.swish[0].v += 260.0;
                tail.bounce[0].v -= 160.0;
            }
            let fling = tail.fling.step(0.0, -0.5 * yaw_acc, 1.8, 0.35, dt);
            let at = TAU * (self.swish + tail.offset);
            // The bend is shared out over the segments, so a long tail (a
            // Shiba's seven-segment ring) bends no more in all than a short
            // one and keeps its shape.
            let amp = amp * 3.0 / tail.bones.len().max(3) as f32;
            for (k, &bone) in tail.bones.iter().enumerate() {
                let lag = WAVE_LAG.min(WAVE_SPAN / tail.bones.len() as f32) * k as f32;
                // Sideways, the travelling wave; up and down, a smaller one
                // a quarter behind, plus a bounce with the steps at the root.
                let sw_target = amp * (at - lag).sin();
                let mut bo_target = 0.4 * amp * (at - lag - 1.6).sin();
                if k == 0 {
                    bo_target += 5.0 * s * (2.0 * TAU * ph - 0.8).sin();
                }
                // Springs smooth the wave as the pace (and so its speed and
                // size) changes.
                let sw = tail.swish[k].step(sw_target, 0.0, 3.0, 0.5, dt);
                let bo = tail.bounce[k].step(bo_target, 0.0, 3.0, 0.5, dt);
                j[bone] = Joint {
                    rot: v3(
                        bo,
                        sw * tail.swish_axes.1 + if k == 0 { fling } else { 0.0 },
                        sw * tail.swish_axes.0,
                    ),
                    shift: V3::ZERO,
                };
                if k == 0 {
                    // Sulking, back to you, the tails stay together: a fan
                    // seen from behind stretches the skin between them.
                    let mut fan = sit * (1.0 - lv.sulk);
                    if let Some((e, t)) = dance {
                        match self.tail_style {
                            TailStyle::Ripple => fan = fan.max(e),
                            TailStyle::Wag => {
                                let w = TAU * 2.6 * t;
                                j[bone].rot.x += e * 28.0 * w.cos();
                                j[bone].rot.z += e * 28.0 * w.sin();
                            }
                            TailStyle::Curl => {}
                        }
                    }
                    j[bone].rot = j[bone].rot + tail.sit_turn * fan + tail.lie_turn * lie;
                } else {
                    j[bone].rot = j[bone].rot + tail.lie_curl * lie;
                }
            }
        }
        if let Some(i) = b.belly {
            // The belly rises and falls with each breath; lying down (on
            // its side, its front, curled up) the flank swells more, and
            // asleep on its back it is round and plain to see (Roger: like
            // breathing; his fifteenth pass: in every sleeping pose).
            let back = lie * chonkgene_rig::math::smoothstep(90.0, 150.0, self.roll.x);
            let amp = 0.05 + LYING_BREATH * lie + BELLY_UP_BREATH * back;
            let y = self
                .belly
                .step(-amp * self.breath, -acc * self.jiggle, 3.6, 0.16, dt);
            let jiggle_most = if self.jiggle > 1.0 { JIGGLE_MOST } else { 0.12 };
            let most = jiggle_most + LYING_BREATH * lie + BELLY_UP_BREATH * back;
            j[i].shift = v3(0.0, y.clamp(-most, most), 0.0);
        }
        if let Some(i) = b.butt {
            // Plopping down into a sit squashes the bottom, and it bounces;
            // getting up gives it a smaller one.
            if lv.flick {
                self.butt.v -= 0.8 * BUTT_PLOP;
            }
            if was_sat < 0.85 && sit >= 0.85 {
                self.butt.v -= BUTT_PLOP;
            } else if was_sat > 0.5 && sit <= 0.5 {
                self.butt.v += 0.5 * BUTT_PLOP;
            }
            let y = self.butt.step(0.0, -acc, 3.2, 0.16, dt);
            let z = self.butt_z.step(0.0, -0.6 * pace_acc, 3.0, 0.2, dt);
            // Walking, it waddles side to side a beat behind the hips.
            let x = self
                .butt_x
                .step(BUTT_WADDLE * s * (TAU * ph).sin(), 0.0, 2.6, 0.15, dt);
            j[i].shift = v3(
                x.clamp(-0.1, 0.1),
                y.clamp(-0.12, 0.12),
                z.clamp(-0.08, 0.08),
            );
        }
        if let Some(i) = b.chin {
            let y = self.chin.step(0.0, -acc - 0.02 * nod_acc, 4.0, 0.18, dt);
            j[i].shift = v3(0.0, y.clamp(-0.09, 0.09), 0.0);
        }

        // Sitting: the body tips back about the front paws until the
        // bottom is on the floor, so the paws stay where they stand (the
        // front legs lean back a little, turning only a third of the way
        // back to upright); the hind legs fold forward under the body, and
        // the head lifts to look up. Tipping about the paws (before the
        // turn, so the pet does not swing round them) is a tip about the
        // root plus a shift.
        if sit > 0.0 {
            let pitch = -SIT_PITCH * sit;
            j[0].rot.x += pitch;
            let d = FRONT_PAWS - ROOT_PIVOT;
            let tipped = d - M3::rot_x(pitch.to_radians()).apply(d);
            j[0].shift = j[0].shift + M3::rot_y(yaw.to_radians()).apply(tipped);
            for &(leg, _, front) in &self.legs {
                if front {
                    // Leaning back toward upright swings the paw down
                    // through the floor (by more on a longer leg): lift the
                    // leg by as much, so the paw stays on the floor.
                    let lean = 0.3 * SIT_PITCH * sit;
                    let tip = SIT_PITCH * sit;
                    let sink =
                        self.leg_len * ((tip - lean).to_radians().cos() - tip.to_radians().cos());
                    j[leg].rot.x += lean;
                    j[leg].shift.y += sink;
                } else {
                    j[leg].rot.x += SIT_HIND * sit;
                }
            }
            j[b.head].rot.x += 0.85 * SIT_PITCH * sit;
        }
        // Lying down (asleep): the body sinks until the belly rests on the
        // floor, the front legs reach forward along it, the hind legs fold
        // under, and the head drops onto the paws.
        if lie > 0.0 {
            j[0].shift.y -= LIE_DROP * self.leg_len * lie;
            for &(leg, _, front) in &self.legs {
                j[leg].rot.x += if front { -80.0 } else { SIT_HIND } * lie;
            }
            j[b.head].rot.x += 16.0 * lie;
            j[b.head].shift.y -= 0.06 * lie;
            let [chin, curl, side, up] = sw.map(|w| w * lie);
            // Chin down on its paws: the head sinks more than it tips, so
            // the face still shows.
            j[b.head].rot.x += 12.0 * chin;
            j[b.head].shift.y -= 0.17 * chin;
            // Curled round toward you: the chest turns one way, the hips
            // the other, the head round toward the tail and down on the
            // floor.
            let c = -facing;
            j[b.body].rot.y += c * CURL * curl;
            j[b.hips].rot.y -= c * 2.0 * CURL * curl;
            // The head turns your way and lies on its cheek, so the
            // sleeping face shows.
            j[b.head].rot.y += c * 2.0 * CURL * curl;
            j[b.head].rot.x += 6.0 * curl;
            j[b.head].rot.z += c * 26.0 * curl;
            j[b.head].shift.y -= 0.14 * curl;
            let one_tail = self.tails.len() == 1;
            for tail in &self.tails {
                if let Some(&root) = tail.bones.first() {
                    j[root].rot.y += c * 40.0 * curl;
                    // One tail lies down and wraps round toward the nose,
                    // as a cat's does (Roger: the Munchkin's raised tail,
                    // twisted with the hips, looked cut off).
                    if one_tail {
                        j[root].rot.x -= CURL_TAIL_DOWN * tail.swish_axes.0 * curl;
                        let wrap = CURL_TAIL_WRAP / (tail.bones.len() - 1).max(1) as f32;
                        for &bone in &tail.bones[1..] {
                            j[bone].rot.y -= c * wrap * curl;
                        }
                    }
                }
            }
            // On its side, legs out; on its back, paws folded in the air
            // and the head tipped back: the roll comes below.
            for &(leg, _, front) in &self.legs {
                j[leg].rot.x += if front { 62.0 } else { 48.0 } * side;
                j[leg].rot.x += if front { 22.0 } else { 36.0 } * up;
            }
            j[b.head].rot.x -= 14.0 * side + 40.0 * up;
            j[b.head].rot.z += facing * (16.0 * side + 50.0 * up);
            // Awake, the head stays up off the paws, looking about.
            if self.awake {
                j[b.head].rot.x -= 30.0 * lie;
                j[b.head].shift.y += 0.16 * lie;
            }
        }

        // Some gestures are done sitting (see `Act::seated`); the rest
        // standing.
        if let Some(g) = self
            .gesture
            .filter(|g| s < 0.5 && (sit < 0.5 || g.act.seated()))
        {
            use chonkgene_core::anim::Act;
            let e = g.e;
            match g.act {
                // A play bow: the front goes down, the bottom up, the head
                // stays up and forward.
                Act::Stretch => {
                    j[b.body].rot.x += 20.0 * e;
                    j[b.head].rot.x -= 24.0 * e;
                    for &(leg, _, front) in &self.legs {
                        if front {
                            j[leg].rot.x -= 20.0 * e;
                        }
                    }
                }
                // The head tips back for the yawn (the face opens its
                // mouth), and the chin shakes.
                Act::Yawn => {
                    j[b.head].rot.x -= 16.0 * e;
                    self.chin.v += 0.5 * e * dt;
                }
                // It crouches over the snack (legs folding, as lying down
                // but less), then for each bite the head goes down to it,
                // nibbles, and comes up chewing, the head rocking a little
                // side to side with the jaw.
                Act::Eat => {
                    let (dip, chew) = bite(g.t);
                    j[0].shift.y -= EAT_CROUCH * self.leg_len * e;
                    for &(leg, _, front) in &self.legs {
                        j[leg].rot.x += if front { -40.0 } else { 0.5 * SIT_HIND } * e;
                    }
                    j[b.body].rot.x += e * (4.0 + 6.0 * dip);
                    j[b.head].rot.x += e * (10.0 + 26.0 * dip - 4.0 * chew.abs());
                    j[b.head].rot.z += e * 4.0 * chew;
                }
                // Digging: nose down over the hole, bottom up, the front
                // paws scooping at the floor in turn (each reaches forward
                // lifted and drags back along the floor).
                Act::Dig => {
                    j[b.body].rot.x += 12.0 * e;
                    j[b.head].rot.x += 14.0 * e;
                    j[b.hips].rot.x -= 6.0 * e;
                    let mut k = 0.0;
                    for &(leg, _, front) in &self.legs {
                        if front {
                            let p = TAU * (DIG_HZ * g.t + 0.5 * k);
                            j[leg].rot.x -= e * (14.0 + 40.0 * p.cos());
                            j[leg].shift.y += e * 0.12 * p.sin().max(0.0);
                            k += 1.0;
                        }
                    }
                }
                // Scratching behind the ear: sitting, a hind leg swings
                // out to the side and up toward the head and drums away (a
                // chubby leg cannot reach the ear, but the drumming reads);
                // the head leans into it, the body tips toward that side.
                Act::Scratch => {
                    let side = facing;
                    let drum = (TAU * 7.0 * g.t).sin();
                    if let Some(leg) = if side > 0.0 { b.hind_r } else { b.hind_l } {
                        j[leg].rot.x -= e * (20.0 + 8.0 * drum);
                        j[leg].rot.z -= side * e * (62.0 + 14.0 * drum);
                        j[leg].shift.y += e * 0.14;
                    }
                    j[b.body].rot.z -= side * 8.0 * e;
                    j[b.head].rot.z -= side * 24.0 * e;
                    j[b.head].rot.y -= side * 10.0 * e;
                }
                // Washing: sitting, a front paw comes up to the mouth to be
                // licked (little nods), then wipes over the face (the head
                // turning into it).
                Act::Groom => {
                    let wipe = (g.t / 1.5 - 1.0).clamp(0.0, 1.0);
                    let lick = (TAU * 3.0 * g.t).sin();
                    let swipe = (TAU * 1.6 * g.t).sin();
                    if let Some(&(leg, ..)) = self.legs.iter().find(|l| l.1 == 0.25) {
                        j[leg].rot.x -= e * (95.0 + 12.0 * wipe * swipe);
                        j[leg].rot.z -= e * 16.0;
                        j[leg].shift.y += e * 0.2;
                    }
                    j[b.head].rot.x += e * (24.0 * (1.0 - wipe) + 6.0 * lick * (1.0 - wipe));
                    j[b.head].rot.y -= e * wipe * (10.0 + 8.0 * swipe);
                    j[b.head].rot.z -= e * wipe * 8.0;
                }
                // A shake from head to tail: the body rolls quickly about
                // its length, the head a beat ahead, dying away at the end.
                Act::Shake => {
                    let fade = 1.0 - (g.t / 1.2).clamp(0.0, 1.0) * 0.6;
                    let w = TAU * 5.0 * g.t;
                    j[b.body].rot.z += e * fade * 16.0 * w.sin();
                    j[b.head].rot.z += e * fade * 10.0 * (w + 0.8).sin();
                    j[b.head].rot.y += e * fade * 12.0 * (w + 0.8).sin();
                    j[b.hips].rot.z -= e * fade * 22.0 * (w - 0.9).sin();
                    self.butt_x.v += e * fade * 1.5 * (w - 0.9).cos() * dt * 30.0;
                    self.chin.v += e * fade * 0.8 * w.cos() * dt * 30.0;
                }
                // Pleased: the front dips, the bottom wiggles side to side
                // (the hips swinging round, the bottom flung after them),
                // the head bobbing along a beat behind.
                Act::Wiggle => {
                    let w = TAU * 3.2 * g.t;
                    j[b.body].rot.x += 7.0 * e;
                    j[b.head].rot.x -= 10.0 * e;
                    j[b.hips].rot.y += e * 20.0 * w.sin();
                    j[b.hips].rot.z += e * 9.0 * w.sin();
                    j[b.body].rot.y -= e * 6.0 * w.sin();
                    j[b.head].rot.z += e * 8.0 * (w - 1.2).sin();
                    self.butt_x.v += e * 2.2 * w.cos() * dt * 30.0;
                }
                // Two little hops on the spot (the animator lifts it): it
                // crouches for each, nose up, and tucks its legs in the air.
                Act::Bounce => {
                    let w = TAU * g.t / 0.62;
                    let crouch = (-w.cos()).max(0.0);
                    j[0].shift.y -= e * 0.1 * self.leg_len * crouch;
                    j[b.head].rot.x -= e * 12.0;
                    for &(leg, _, front) in &self.legs {
                        j[leg].rot.x += if front { -14.0 } else { 14.0 } * e * (1.0 - crouch);
                    }
                }
                // A turn right round on the spot, the legs stepping it round.
                Act::Spin => {
                    let secs = chonkgene_core::anim::gesture_secs(Act::Spin).unwrap_or(1.4);
                    let u = (g.t / secs).clamp(0.0, 1.0);
                    j[0].rot.y += facing * 360.0 * u * u * (3.0 - 2.0 * u);
                    let mut k = 0.0;
                    for &(leg, offset, _) in &self.legs {
                        let p = TAU * (3.0 * g.t + offset + 0.5 * k);
                        j[leg].rot.x += e * 16.0 * p.sin();
                        j[leg].shift.y += e * 0.05 * p.cos().max(0.0);
                        k += 1.0;
                    }
                    j[b.head].rot.y += facing * 14.0 * e;
                }
                // Kneading: sitting, the front paws tread in turn, the
                // body rocking onto each, eyes half shut with pleasure.
                Act::Knead => {
                    let mut k = 0.0;
                    for &(leg, _, front) in &self.legs {
                        if front {
                            let p = TAU * 1.6 * g.t + PI * k;
                            let up = p.sin().max(0.0);
                            j[leg].rot.x -= e * (14.0 + 22.0 * up);
                            j[leg].shift.y += e * 0.08 * up;
                            k += 1.0;
                        }
                    }
                    j[b.body].rot.z += e * 3.0 * (TAU * 1.6 * g.t).sin();
                    j[b.head].rot.x += e * 6.0;
                }
                // 握手: a front paw held out to you, shaking up and down a
                // little as if in your hand; the head tips, pleased.
                Act::Paw => {
                    let shake = (TAU * 2.6 * g.t).sin();
                    if let Some(&(leg, ..)) = self.legs.iter().find(|l| l.1 == 0.25) {
                        j[leg].rot.x -= e * (62.0 + 8.0 * shake);
                        j[leg].rot.y += facing * 18.0 * e;
                        j[leg].shift.y += e * 0.06;
                    }
                    j[b.body].rot.z -= 3.0 * e;
                    j[b.head].rot.z += facing * 10.0 * e;
                }
                // Typing along: the front paws tap quickly in turn,
                // lying (paws out in front) or sitting.
                Act::Tap => {
                    let mut k = 0.0;
                    for &(leg, _, front) in &self.legs {
                        if front {
                            let p = TAU * 4.5 * g.t + PI * k;
                            let up = p.sin().max(0.0);
                            if lie > 0.5 {
                                j[leg].shift.y += e * 0.07 * up;
                            } else {
                                j[leg].rot.x -= e * (8.0 + 14.0 * up);
                                j[leg].shift.y += e * 0.05 * up;
                            }
                            k += 1.0;
                        }
                    }
                    j[b.head].rot.x += e * 4.0 * (TAU * 4.5 * g.t).sin().abs();
                }
                // The head tipped right over to one side, listening.
                Act::Tilt => {
                    let side = facing.signum();
                    j[b.head].rot.z += side * 28.0 * e;
                    j[b.head].rot.y += side * 6.0 * e;
                }
                // Nose down to the floor, sniffing (quick little nods).
                Act::Sniff => {
                    let sniff = (TAU * 6.0 * g.t).sin();
                    j[b.body].rot.x += 10.0 * e;
                    j[b.head].rot.x += e * (26.0 + 3.0 * sniff);
                    for &(leg, _, front) in &self.legs {
                        if front {
                            j[leg].rot.x -= 8.0 * e;
                        }
                    }
                }
                // Nose down to the ball, then a quick bump up and forward
                // that sends it rolling.
                Act::Nudge => {
                    let u = (g.t / 0.8).clamp(0.0, 1.0);
                    let down = (u / 0.45).min(1.0);
                    let bump = ((u - 0.45) / 0.2).clamp(0.0, 1.0)
                        * (1.0 - ((u - 0.65) / 0.35).clamp(0.0, 1.0));
                    j[b.body].rot.x += e * (12.0 * down - 8.0 * bump);
                    j[b.head].rot.x += e * (30.0 * down - 34.0 * bump);
                    let push = M3::rot_y(yaw.to_radians()).apply(v3(0.0, 0.0, 0.12 * e * bump));
                    j[0].shift = j[0].shift + push;
                    for &(leg, _, front) in &self.legs {
                        if front {
                            j[leg].rot.x -= 8.0 * e * down;
                        }
                    }
                }
                // A cheek rubbed along the floor: head down and turned
                // over, pushed one way and the other.
                Act::Rub => {
                    let rub = (TAU * 0.9 * g.t).sin();
                    j[b.body].rot.x += 12.0 * e;
                    j[b.body].rot.z += 8.0 * e * rub;
                    j[b.head].rot.x += 30.0 * e;
                    j[b.head].rot.z += e * (24.0 + 10.0 * rub) * facing;
                    j[b.head].rot.y += e * 12.0 * rub;
                    for &(leg, _, front) in &self.legs {
                        if front {
                            j[leg].rot.x -= 16.0 * e;
                        }
                    }
                }
                // Lying with a bone held in the front paws, gnawing at it
                // for real (Roger: chewing held still): the jaw works
                // quickly, the head swings slowly from side to side,
                // turning over to get the side teeth to it, and tugs back
                // now and then, the body rocking with the tug; the paws
                // shift their grip in turn.
                Act::Chew => {
                    let t = g.t;
                    let work = (TAU * 2.6 * t).sin();
                    let sway = (TAU * 0.45 * t).sin();
                    let tug = (TAU * 0.8 * t + 1.0).sin().max(0.0).powi(3);
                    j[b.head].rot.x += e * (22.0 + 5.0 * work - 16.0 * tug);
                    j[b.head].rot.y += e * 16.0 * sway;
                    j[b.head].rot.z += e * facing * (14.0 + 12.0 * sway);
                    j[b.head].shift.y -= 0.1 * e * lie;
                    j[b.body].rot.x -= e * 5.0 * tug;
                    j[b.body].rot.z += e * 3.0 * sway;
                    j[b.hips].rot.z -= e * 2.0 * sway;
                    if work > 0.8 {
                        self.chin.v -= 0.4 * e;
                    }
                    let mut k = 0.0;
                    for &(leg, _, front) in &self.legs {
                        if front {
                            let grip = (TAU * 0.6 * t + PI * k).sin();
                            j[leg].rot.x += e * (6.0 * grip - 8.0 * tug);
                            j[leg].rot.y += e * 10.0 * if k == 0.0 { 1.0 } else { -1.0 };
                            j[leg].shift.y += e * 0.04 * grip.max(0.0);
                            k += 1.0;
                        }
                    }
                }
                // A hug (PLAN §8, Roger's sixteenth pass), sitting.
                Act::Hug => {
                    let sway = (TAU * 0.55 * g.t).sin();
                    let squeeze = (TAU * 1.1 * g.t).sin().max(0.0);
                    let near = self.legs.iter().find(|l| l.1 == 0.25).map(|l| l.0);
                    match self.hug {
                        // Face to face with the other: leaning into it,
                        // the head tipped against its cheek, swaying
                        // together; in front, a paw over its shoulder.
                        Hug::Cuddle { front } => {
                            j[b.body].rot.x += e * 12.0;
                            j[b.body].rot.z += e * 5.0 * sway;
                            j[b.head].rot.x -= e * 8.0;
                            j[b.head].rot.z += facing * e * (16.0 + 3.0 * sway);
                            if front && let Some(leg) = near {
                                j[leg].rot.x -= e * (85.0 + 6.0 * squeeze);
                                j[leg].shift.y += e * 0.1;
                            }
                        }
                        // The fox up on its hind legs, both front paws
                        // round the other, squeezing, chin on its head.
                        Hug::Bear => {
                            j[b.body].rot.x -= e * 38.0;
                            j[0].shift.y += e * 0.12 * self.leg_len;
                            j[b.head].rot.x += e * 30.0;
                            j[b.body].rot.z += e * 4.0 * sway;
                            for &(leg, _, front) in &self.legs {
                                if front {
                                    j[leg].rot.x -= e * (70.0 + 10.0 * squeeze);
                                    j[leg].shift.y += e * 0.12;
                                }
                            }
                        }
                        // Squeezed in a bear hug: paws up, pushed back a
                        // little, hind legs paddling.
                        Hug::Squeezed => {
                            j[b.body].rot.x -= e * (8.0 + 4.0 * squeeze);
                            j[b.head].rot.z += facing * e * 10.0;
                            let mut k = 0.0;
                            for &(leg, _, front) in &self.legs {
                                if front {
                                    j[leg].rot.x -= e * 40.0;
                                } else {
                                    let kick = (TAU * 3.0 * g.t + PI * k).sin();
                                    j[leg].rot.x -= e * 12.0 * kick;
                                    k += 1.0;
                                }
                            }
                        }
                        // In your arms: rocked side to side, nuzzling,
                        // front paws up hugging you back; the fox wraps
                        // her tails round to the front, round you (傲嬌:
                        // she says no, her tails say yes).
                        Hug::ByYou => {
                            j[0].rot.z += e * 6.0 * sway;
                            j[b.head].rot.z += e * 10.0 * (TAU * 1.1 * g.t).sin();
                            j[b.head].rot.x -= e * 6.0;
                            for &(leg, _, front) in &self.legs {
                                if front {
                                    j[leg].rot.x -= e * (35.0 + 8.0 * squeeze);
                                }
                            }
                            if self.tails.len() > 1 {
                                let n = self.tails.len() as f32;
                                for (k, tail) in self.tails.iter().enumerate() {
                                    let side = (k as f32 + 0.5) / n * 2.0 - 1.0;
                                    if let Some(&root) = tail.bones.first() {
                                        j[root].rot.y += e * side * 85.0;
                                        j[root].rot.x -= e * (25.0 + 6.0 * squeeze);
                                    }
                                    for &seg in tail.bones.iter().skip(1) {
                                        j[seg].rot.y += e * side * 18.0;
                                    }
                                }
                            }
                        }
                    }
                }
                // The tails swung round hard at someone, the body
                // twisting into it, the bell knocked swinging.
                Act::Swish => {
                    let sweep = (TAU * 1.6 * g.t).sin();
                    for tail in &self.tails {
                        if let Some(&root) = tail.bones.first() {
                            j[root].rot.y += facing * e * 60.0 * sweep;
                            j[root].rot.x -= 18.0 * e;
                        }
                    }
                    j[b.hips].rot.y -= facing * e * 14.0 * sweep;
                    j[b.head].rot.y += facing * 16.0 * e;
                    self.bell.1.v += e * 40.0 * sweep * dt * 30.0;
                }
                // 肉肉舞 (see `dance_at`): the head bobbing to the beat
                // all through, and each move in turn.
                Act::Dance => {
                    use chonkgene_core::anim::{DanceMove, dance_at};
                    let d = dance_at(g.t);
                    let on_beat = (TAU * d.beat).cos();
                    j[b.head].rot.x += e * 9.0 * on_beat;
                    let fronts: Vec<usize> =
                        self.legs.iter().filter(|l| l.2).map(|l| l.0).collect();
                    match d.step {
                        // Bouncing: down into the beat, legs tucked up in
                        // the air between, nose up.
                        DanceMove::Bounce => {
                            let crouch = (0.5 + 0.5 * on_beat).powi(2);
                            j[0].shift.y -= e * 0.12 * self.leg_len * crouch;
                            j[b.head].rot.x -= e * 10.0;
                            for &(leg, _, front) in &self.legs {
                                j[leg].rot.x +=
                                    if front { -16.0 } else { 16.0 } * e * (1.0 - crouch);
                            }
                        }
                        // The bottom out to one side on each beat.
                        DanceMove::Wiggle => {
                            let w = PI * (d.count as f32 + d.beat);
                            j[b.body].rot.x += 7.0 * e;
                            j[b.hips].rot.y += e * 26.0 * w.sin();
                            j[b.hips].rot.z += e * 11.0 * w.sin();
                            j[b.body].rot.y -= e * 7.0 * w.sin();
                            j[b.head].rot.z += e * 10.0 * (w - 1.0).sin();
                            self.butt_x.v += e * 2.6 * w.cos() * dt * 30.0;
                        }
                        // A cat kneads the air with both front paws; the
                        // rest wave one front paw and then the other.
                        DanceMove::Paws => {
                            // Out to the side as well as up: facing you, a
                            // paw lifted only forward hardly shows.
                            let cat = self.tail_style == TailStyle::Curl;
                            for (k, &leg) in fronts.iter().enumerate() {
                                let out = if k == 0 { DANCE_OUT } else { -DANCE_OUT };
                                if cat {
                                    let p = TAU * 2.0 * g.t + PI * k as f32;
                                    let up = p.sin().max(0.0);
                                    j[leg].rot.x -= e * (50.0 + 25.0 * up);
                                    j[leg].rot.z += e * out * (0.4 + 0.3 * up);
                                    j[leg].shift.y += e * (0.08 + 0.06 * up);
                                } else if (d.count as usize + k).is_multiple_of(2) {
                                    let wave = (TAU * 2.0 * g.t).sin();
                                    j[leg].rot.x -= e * 60.0;
                                    j[leg].rot.z += e * out * (1.0 + 0.3 * wave);
                                    j[leg].shift.y += e * 0.06;
                                }
                            }
                            let lean = if d.count.is_multiple_of(2) { 1.0 } else { -1.0 };
                            j[b.body].rot.z += e * 5.0 * lean;
                            j[b.head].rot.z -= e * 8.0 * lean;
                        }
                        // Right round, the legs stepping it round.
                        DanceMove::Spin => {
                            let u = d.u;
                            j[0].rot.y += facing * 360.0 * u * u * (3.0 - 2.0 * u);
                            for (k, &(leg, offset, _)) in self.legs.iter().enumerate() {
                                let p = TAU * (4.0 * g.t + offset + 0.5 * k as f32);
                                j[leg].rot.x += e * 18.0 * p.sin();
                            }
                        }
                        // 「登登！」: a front paw high, chin up, leaning
                        // back, held.
                        DanceMove::Pose => {
                            let hold = chonkgene_rig::math::smoothstep(0.0, 0.25, d.u);
                            if let Some(&leg) = fronts.first() {
                                j[leg].rot.x -= e * hold * 80.0;
                                j[leg].rot.z += e * hold * 1.2 * DANCE_OUT;
                                j[leg].shift.y += e * hold * 0.1;
                            }
                            j[b.body].rot.x -= e * hold * 6.0;
                            j[b.body].rot.z -= facing * e * hold * 5.0;
                            j[b.head].rot.x -= e * hold * 16.0;
                            j[b.head].rot.z += facing * e * hold * 12.0;
                        }
                    }
                    // The fox's bell rings on the beat.
                    self.bell.1.v += e * 30.0 * (TAU * d.beat).sin() * dt * 30.0;
                }
                // A front paw up, waving.
                Act::Wave => {
                    let wiggle = (TAU * 2.2 * g.t).sin();
                    j[b.body].rot.z -= 4.0 * e;
                    if let Some(&(leg, ..)) = self.legs.iter().find(|l| l.1 == 0.25) {
                        j[leg].rot.x -= 75.0 * e;
                        j[leg].rot.z += 14.0 * e * wiggle;
                    }
                }
                _ => {}
            }
        }
        // Feelings and touches (PLAN §1.2), all facing you.
        self.react(&mut j, &lv, dt, facing, yaw, lie, &sw);
        self.fidgets(&mut j, &f, &lv, sit, lie, dt);
        self.lively(&mut j, &f, s, sit, facing, dt);
        self.busy = s > 0.0
            || self.gesture.is_some()
            || self.yaw.v.abs() > 1.0
            || self.look.v.abs() > 1.0
            || self.sit.v.abs() > 0.02
            || self.lie.v.abs() > 0.02
            || self.roll.v.abs() > 2.0
            || lv.attend > 0.01;
        j
    }

    /// The little movements on top of the pose (see [`crate::fidget`]):
    /// the ears twitch and listen, the tail tip flicks, the head sniffs and
    /// tilts, a paw shuffles or scratches, the weight shifts, the fox's
    /// bell swings. Legs stay put lying down, melted or rolled over.
    fn fidgets(&mut self, j: &mut [Joint], f: &Out, lv: &Levels, sit: f32, lie: f32, dt: f32) {
        use std::f32::consts::TAU;
        let b = &self.bones;
        let firm = (1.0 - lie) * (1.0 - lv.melt) * (1.0 - (self.roll.x / 90.0).clamp(0.0, 1.0));
        // Ears: each twitches on its own clock, both turn to listen, and
        // they lie back fed up or sulking, perk up pleased.
        let back = lv.melt.max(lv.sulk);
        let perk = lv.content.max(lv.head);
        for (k, ear) in b.ears.iter().enumerate() {
            let Some(e) = *ear else {
                continue;
            };
            // Out to its own side: the left ear (+x) turns about z the
            // negative way.
            let out = if k == 0 { -1.0 } else { 1.0 };
            let flick = f.ears[k].map_or(0.0, ear_flick);
            j[e].rot.z += self.ear_k * out * (34.0 * flick + 16.0 * back - 4.0 * perk);
            j[e].rot.x += self.ear_k * (-8.0 * flick.abs() - 30.0 * back + 6.0 * perk);
            j[e].rot.y += self.ear_k * 24.0 * f.swivel;
        }
        // The slow drift under it all (lying about, the body stays put).
        let (dx, dy, dz, body) = drift(f.t);
        j[b.head].rot.x += f.calm * dx;
        j[b.head].rot.y += f.calm * dy;
        j[b.head].rot.z += f.calm * dz;
        j[b.body].rot.z += f.calm * firm * body;
        let Some(run) = f.run else {
            self.swing_bell(j, 0.0, dt);
            self.shift = 0.0;
            return;
        };
        let (e, t, side) = (run.e(), run.t, run.side);
        let front = |side: f32| {
            // The left front leg (+x) comes second in the step cycle.
            let at = if side > 0.0 { 0.25 } else { 0.75 };
            self.legs.iter().find(|l| l.1 == at).map(|l| l.0)
        };
        // A head shake knocks the bell into a swing as it starts.
        let mut knock = 0.0;
        let mut shift = 0.0;
        match run.kind {
            // Wagging hard, the whole bottom wags along.
            Kind::Wag => j[b.hips].rot.y += 5.0 * e * (TAU * self.swish).sin(),
            Kind::TipFlick | Kind::Ripple => {
                // It glances back at the tail, the hips swaying with it.
                j[b.head].rot.y -= side * 9.0 * e;
                j[b.hips].rot.z += 3.0 * e * (TAU * 1.2 * t).sin();
                let n_tails = self.tails.len();
                for (i, tail) in self.tails.iter().enumerate() {
                    let n = tail.bones.len();
                    if run.kind == Kind::Ripple {
                        // One tail after another lifts and settles.
                        let lag = 0.6 * i as f32 / n_tails.max(1) as f32;
                        let x = ((t - lag) / (run.len - 0.6).max(0.3)).clamp(0.0, 1.0);
                        let bump = (std::f32::consts::PI * x).sin();
                        if let Some(&root) = tail.bones.first() {
                            j[root].rot.x -= 16.0 * bump;
                        }
                        continue;
                    }
                    // The last third curls and flicks.
                    for (k, &bone) in tail.bones.iter().enumerate() {
                        let w = ((k as f32 + 1.0) / n as f32 - 0.6).max(0.0) / 0.4;
                        j[bone].rot.x += w * e * 24.0 * (TAU * 2.4 * t).sin();
                        j[bone].rot.y += w * e * side * 14.0;
                    }
                }
            }
            Kind::Sniff => {
                j[b.head].rot.x -= e * (10.0 + 4.0 * (TAU * 7.0 * t).sin());
            }
            // Tipped over and wondering, never quite still.
            Kind::HeadTilt => {
                j[b.head].rot.z += e * (side * 20.0 + 5.0 * (TAU * 0.7 * t).sin());
            }
            Kind::PawScratch if sit > 0.5 => {
                // Sitting, a front paw comes up to the cheek or chest and
                // scratches; the head leans into it.
                if let Some(leg) = front(side) {
                    j[leg].rot.x -= firm * e * (78.0 + 10.0 * (TAU * 6.0 * t).sin());
                    j[leg].rot.z += firm * e * side * 14.0;
                    j[leg].shift.y += firm * e * 0.16;
                }
                j[b.head].rot.z -= side * 12.0 * e;
                j[b.head].rot.x += 8.0 * e;
            }
            Kind::PawShuffle | Kind::PawScratch | Kind::Dream => {
                // A paw lifted and set down (asleep, a dreaming twitch).
                if let Some(leg) = front(side) {
                    if run.kind == Kind::Dream {
                        j[leg].rot.x += e * 9.0 * (TAU * 5.0 * t).sin();
                    } else {
                        j[leg].rot.x -= firm * 30.0 * e;
                        j[leg].shift.y += firm * 0.08 * e;
                        j[b.body].rot.z -= side * 2.0 * e * firm;
                        // Lying about (melted, rolled over), the paw only
                        // twitches.
                        j[leg].rot.x += (1.0 - firm) * e * 9.0 * (TAU * 5.0 * t).sin();
                    }
                }
            }
            Kind::Knead | Kind::Paddle => {
                // Front paws in turn: kneading presses down, paddling swings.
                let hz = if run.kind == Kind::Knead { 1.4 } else { 2.4 };
                let mut k = 0.0;
                for &(leg, _, is_front) in &self.legs {
                    if !is_front {
                        continue;
                    }
                    let p = (TAU * hz * t + std::f32::consts::PI * k).sin();
                    if run.kind == Kind::Knead {
                        j[leg].rot.x -= firm * e * 16.0 * p.max(0.0);
                        j[leg].shift.y += firm * e * 0.05 * p.max(0.0);
                    } else {
                        j[leg].rot.x += firm * e * 14.0 * p;
                    }
                    j[leg].rot.x += (1.0 - firm) * e * 7.0 * p;
                    k += 1.0;
                }
            }
            Kind::WeightShift => {
                shift = side * e;
                // Lying about, the head lolls over instead.
                j[b.head].rot.z += (1.0 - firm) * side * 9.0 * e;
            }
            // Quick shallow breaths (unless it pants all the while anyway,
            // see `lively`).
            Kind::Pant => {
                let e = e * (1.0 - f.pant);
                let pant = (TAU * 3.2 * t).sin();
                j[0].shift.y += 0.01 * pant * e;
                j[b.head].rot.x -= e * (4.0 + 2.0 * pant);
                self.belly.v += 6.0 * pant * e * dt;
            }
            Kind::Lick => j[b.head].rot.x += 8.0 * e,
            // A blep comes with a slow, silly tilt of the head.
            Kind::Blep => {
                j[b.head].rot.z += e * (side * 8.0 + 6.0 * (TAU * 0.6 * t).sin());
                j[b.head].rot.x += 3.0 * e * (TAU * 0.45 * t).sin();
            }
            Kind::Jingle => {
                let w = (TAU * 6.0 * t).sin();
                j[b.head].rot.y += 10.0 * w * e;
                j[b.head].rot.z += 5.0 * w * e;
                if t <= dt {
                    knock = side;
                }
            }
            _ => {}
        }
        // Shifting its weight leans the body over and back; the belly
        // wobbles with the change.
        j[b.body].rot.z += 9.0 * shift * firm;
        j[b.hips].rot.z -= 6.0 * shift * firm;
        self.belly.v += 0.4 * (shift - self.shift).abs();
        self.shift = shift;
        self.swing_bell(j, knock, dt);
    }

    /// Pleased or at play, the motion never stops, gestures or not (PLAN
    /// §8 "Roger's tenth pass", items 3 and 6): a cat's tail curls up at
    /// the tip and flicks, the fox's tails lift one after another (a dog's
    /// wags, see the tails above), and the ears keep turning and twitching.
    /// Out of breath or lively, a dog pants: quick shallow breaths, chin up
    /// (the face puts its tongue out). Sitting facing you a cat's tail
    /// would hide behind it: it comes round to the side, where it shows.
    /// The head tips and nods to its own beat. `s` is the walking speed,
    /// `sit` how far into a sit.
    #[allow(clippy::too_many_arguments)]
    fn lively(&mut self, j: &mut [Joint], f: &Out, s: f32, sit: f32, facing: f32, dt: f32) {
        use std::f32::consts::TAU;
        let (l, t) = (f.lively, f.t);
        if l > 0.0 {
            let n_tails = self.tails.len();
            for (i, tail) in self.tails.iter().enumerate() {
                match self.tail_style {
                    TailStyle::Curl => {
                        let n = tail.bones.len() as f32;
                        if let Some(&root) = tail.bones.first() {
                            let swing = 55.0 + 15.0 * (TAU * 0.3 * t).sin();
                            j[root].rot.y += facing * l * sit * swing;
                        }
                        // Every few seconds the tip gives a quick flick.
                        let flick = (TAU * 0.35 * t).sin().max(0.0).powi(3);
                        for (k, &bone) in tail.bones.iter().enumerate() {
                            let w = (k as f32 + 1.0) / n;
                            let curl = 10.0 + 14.0 * (TAU * 0.9 * t - 0.6 * k as f32).sin();
                            j[bone].rot.x += l * w * curl;
                            j[bone].rot.y += l * w * 10.0 * (TAU * 0.45 * t + 1.0).sin();
                            if w > 0.6 {
                                j[bone].rot.x += l * flick * 18.0 * (TAU * 2.6 * t).sin();
                            }
                        }
                    }
                    TailStyle::Ripple => {
                        let lag = 0.8 * i as f32 / n_tails.max(1) as f32 * TAU;
                        if let Some(&root) = tail.bones.first() {
                            j[root].rot.x -= l * 12.0 * (TAU * 0.7 * t - lag).sin().max(0.0);
                        }
                    }
                    TailStyle::Wag => {}
                }
            }
            let head = self.bones.head;
            j[head].rot.z += l * 6.0 * (TAU * 0.55 * t + 0.4).sin();
            j[head].rot.x += l * 3.0 * (TAU * 1.1 * t).sin();
            for (k, ear) in self.bones.ears.iter().enumerate() {
                let Some(e) = *ear else {
                    continue;
                };
                let out = if k == 0 { -1.0 } else { 1.0 };
                let k = k as f32;
                j[e].rot.z += self.ear_k * out * l * 7.0 * (TAU * 1.3 * t + 1.7 * k).sin();
                j[e].rot.x += self.ear_k * l * 5.0 * (TAU * 0.9 * t + k).sin();
            }
        }
        if f.pant > 0.0 {
            let p = f.pant;
            let pant = (TAU * 3.2 * t).sin();
            // Walking, the steps already shake it: only the head and belly.
            j[0].shift.y += 0.01 * pant * p * (1.0 - s);
            j[self.bones.head].rot.x -= p * (3.0 + 2.0 * pant);
            self.belly.v += 6.0 * pant * p * dt;
        }
    }

    /// The fox's bell hangs from its cord: it keeps hanging down however
    /// the body tips, swinging on a while after each change, trails back a
    /// little walking, and a shake of the head (`knock`: which way) knocks
    /// it into a swing.
    fn swing_bell(&mut self, j: &mut [Joint], knock: f32, dt: f32) {
        let Some(bell) = self.bones.bell else {
            return;
        };
        let body = &j[self.bones.body];
        let lean = (body.rot.x + j[0].rot.x, body.rot.z + j[0].rot.z);
        if knock != 0.0 {
            self.bell.1.v += 160.0 * knock;
            self.bell.0.v += 60.0;
        }
        let x = self
            .bell
            .0
            .step(-lean.0, -900.0 * self.speed, 1.7, 0.12, dt);
        let z = self.bell.1.step(-lean.1, 0.0, 1.7, 0.12, dt);
        j[bell].rot.x += x.clamp(-35.0, 35.0);
        j[bell].rot.z += z.clamp(-35.0, 35.0);
    }

    /// The whole body's answer to a feeling or a touch: rolled over
    /// laughing (or for a belly rub, or asleep on its back), melted flat,
    /// its back turned, a paw given, the head leaning into a hand, looking
    /// back at a tail pulled.
    #[allow(clippy::too_many_arguments)]
    fn react(
        &mut self,
        j: &mut [Joint],
        lv: &Levels,
        dt: f32,
        facing: f32,
        yaw: f32,
        lie: f32,
        sw: &[f32; 4],
    ) {
        use std::f32::consts::TAU;
        self.ground = 0.0;
        self.flatten = 0.0;
        let t = lv.t;
        // Rolling over about its length, belly toward you; on its side to
        // sleep, a quarter of the way.
        let many_tails = self.tails.len() > 1;
        let awake_roll = if many_tails {
            0.0
        } else {
            (UP_ROLL * lv.laugh).max(RUB_ROLL * lv.rub.min(1.0))
        };
        let goal = awake_roll.max(lie * (SIDE_ROLL * sw[2] + UP_ROLL * sw[3]));
        let roll = self.roll.step(goal, 0.0, 1.2, 0.85, dt).max(0.0);
        let b = &self.bones;
        if roll > 0.5 {
            let up = chonkgene_rig::math::smoothstep(90.0, 150.0, roll);
            let giggle = 9.0 * lv.laugh * (TAU * 2.2 * t).sin();
            let phi = -facing * roll + giggle;
            j[0].rot.z += phi;
            // About the middle of the body, not the floor under it.
            let d = v3(0.0, self.body_h, 0.0);
            let turned = d - M3::rot_z(phi.to_radians()).apply(d);
            j[0].shift = j[0].shift + M3::rot_y(yaw.to_radians()).apply(turned);
            // Awake on its back: paws folded up, pedalling with laughter
            // or flopping about for a rub; the head tipped back to look at
            // you upside down.
            let awake = up * (1.0 - lie);
            for (k, &(leg, _, front)) in self.legs.iter().enumerate() {
                let pedal =
                    (18.0 * lv.laugh + 6.0 * lv.rub) * (TAU * 2.4 * t + k as f32 * 1.57).sin();
                j[leg].rot.x += awake * (if front { -55.0 } else { -35.0 } + pedal);
            }
            // Rolled over side-on, you are off to its side: the head turns
            // to you (undoing the turn it keeps at rest).
            let over = awake.max(up * lie);
            j[b.head].rot.y += (facing * 65.0 + 0.45 * yaw) * over;
            j[b.head].rot.x += ROLL_TUCK * over;
            j[b.head].shift.y -= ROLL_LIFT * over;
            j[b.head].rot.z += facing * 45.0 * awake;
            self.ground = (roll / 80.0).min(1.0);
        }
        // Sitting up laughing (several tails): rocking side to side, head
        // thrown back, front paws pedalling; a rub: leaning back into it.
        if many_tails {
            let l = lv.laugh.min(1.0);
            j[0].rot.z += 7.0 * l * (TAU * 2.2 * t).sin();
            j[b.head].rot.x -= 14.0 * l;
            for (k, &(leg, _, front)) in self.legs.iter().enumerate() {
                if front {
                    j[leg].rot.x -= l * (30.0 + 15.0 * (TAU * 2.4 * t + k as f32 * 1.57).sin());
                }
            }
            j[b.body].rot.x -= 10.0 * lv.rub.min(1.0);
        }
        // Getting about (Roger's eighteenth pass): sliding on its belly
        // after a run, flat out, front paws reaching forward, hind legs
        // trailing, chin up, ears back; gliding spread out like a flying
        // squirrel; under a balloon or a bamboo-copter, legs dangling and
        // paddling.
        let goals = self.travel.weights();
        let mut tw = [0.0f32; 3];
        for (k, w) in self.travel_w.iter_mut().enumerate() {
            tw[k] = w.step(goals[k], 0.0, 3.0, 1.0, dt).clamp(0.0, 1.0);
        }
        let [slide, glide, fly] = tw;
        if slide > 0.0 {
            j[0].shift.y -= 1.05 * LIE_DROP * self.leg_len * slide;
            for &(leg, offset, front) in &self.legs {
                j[leg].rot.x += if front { -85.0 } else { 75.0 } * slide;
                let out = if offset < 0.5 { 1.0 } else { -1.0 };
                j[leg].rot.z += out * 12.0 * slide;
            }
            j[b.head].rot.x -= 18.0 * slide;
            for ear in b.ears.into_iter().flatten() {
                j[ear].rot.x += 35.0 * slide * self.ear_k;
            }
            self.ground = self.ground.max(slide);
            self.flatten = self.flatten.max(0.08 * slide);
        }
        if glide > 0.0 {
            let sway = (TAU * 0.6 * t).sin();
            // Up off the bottom of its picture: the legs reach out below.
            j[0].shift.y += 0.28 * glide;
            j[b.body].rot.z += 4.0 * glide * sway;
            for &(leg, offset, front) in &self.legs {
                let out = if offset < 0.5 { 1.0 } else { -1.0 };
                j[leg].rot.z += out * 55.0 * glide;
                j[leg].rot.x += if front { -45.0 } else { 40.0 } * glide;
            }
            j[b.head].rot.x -= 10.0 * glide;
            for ear in b.ears.into_iter().flatten() {
                j[ear].rot.x += 40.0 * glide * self.ear_k;
            }
        }
        if fly > 0.0 {
            j[0].shift.y += 0.18 * fly;
            let mut k = 0.0;
            for &(leg, _, front) in &self.legs {
                let paddle = (TAU * 1.6 * t + std::f32::consts::PI * k).sin();
                j[leg].rot.x += fly * (if front { -10.0 } else { 15.0 } + 18.0 * paddle);
                k += 1.0;
            }
            j[b.body].rot.z += 5.0 * fly * (TAU * 0.5 * t).sin();
            j[b.head].rot.x -= 12.0 * fly;
        }
        // Melted flat (厭世): the belly on the floor, legs splayed out
        // front and back, chin down, flattened.
        let m = lv.melt;
        if m > 0.0 {
            j[0].shift.y -= 1.05 * LIE_DROP * self.leg_len * m;
            for &(leg, offset, front) in &self.legs {
                j[leg].rot.x += if front { -75.0 } else { 55.0 } * m;
                // The left legs (+x) come first in the step cycle.
                let out = if offset < 0.5 { 1.0 } else { -1.0 };
                j[leg].rot.z += out * 25.0 * m;
            }
            j[b.head].rot.x += 26.0 * m;
            j[b.head].shift.y -= 0.08 * m;
            self.ground = self.ground.max(m);
            self.flatten = 0.1 * m;
        }
        // Back turned (sulking), head down, glancing back now and then.
        if lv.sulk > 0.0 {
            let glance = (TAU * t / 3.5).sin().max(0.0).powi(4);
            // Back to you, its bottom comes nearest, which the camera
            // looking down a little draws lower: move it back a step.
            j[0].shift.z -= SULK_BACK * lv.sulk;
            j[b.head].rot.x += 14.0 * lv.sulk * (1.0 - glance);
            j[b.head].rot.y -= facing * 55.0 * lv.sulk * glance;
        }
        // Pleased: a slow tilt of the head.
        j[b.head].rot.z += 8.0 * lv.content * (TAU * 0.4 * t).sin();
        // A hand on its head: it pushes up into it, nuzzling side to side;
        // a scratch under the chin: chin up; on a cheek: leaning into it.
        j[b.head].rot.x += 10.0 * lv.head - 26.0 * lv.chin + 14.0 * lv.flinch;
        j[b.head].rot.z += 9.0 * lv.head * (TAU * 1.1 * t).sin()
            + 5.0 * lv.chin * (TAU * 0.8 * t).sin()
            + 12.0 * lv.cheek;
        // Looking back at a tail pulled or a bottom poked, over the
        // shoulder on your side, the body bending a little with it.
        j[b.head].rot.y -= facing * 65.0 * lv.back;
        j[b.body].rot.y -= facing * 12.0 * lv.back;
        // A paw held: given, shaking along with the hand; poked: lifted.
        if let Some((bone, up)) = lv.paw {
            let leg = self
                .legs
                .iter()
                .find(|l| l.0 == bone)
                .or_else(|| self.legs.iter().find(|l| l.1 == 0.25))
                .map(|l| l.0);
            if let Some(leg) = leg {
                j[leg].rot.x -= up * (55.0 + 8.0 * (TAU * 2.0 * t).sin());
                j[leg].shift.y += 0.06 * up;
            }
        }
        // Scratched: it leans into the finger, the head tipped against it
        // and the chin up; a cat treads with its front paws.
        if lv.scratch > 0.0 {
            let toward = if lv.gaze.0 < 0.0 { -1.0 } else { 1.0 };
            let sway = (TAU * 0.7 * t).sin();
            j[b.body].rot.z -= toward * lv.scratch * (7.0 + 2.0 * sway);
            j[b.head].rot.z -= toward * lv.scratch * (16.0 + 4.0 * sway);
            j[b.head].rot.x -= 10.0 * lv.scratch;
            if self.kneads {
                let mut k = 0.0;
                for &(leg, _, front) in &self.legs {
                    if front {
                        let tread = (TAU * 1.5 * t + std::f32::consts::PI * k).sin().max(0.0);
                        j[leg].rot.x -= lv.scratch * 24.0 * tread;
                        j[leg].shift.y += lv.scratch * 0.05 * tread;
                        k += 1.0;
                    }
                }
            }
        }
        // Poked: a start, the head and body jerking back from it, ears up.
        if lv.startle > 0.0 {
            j[b.body].rot.x -= 8.0 * lv.startle;
            j[b.head].rot.x -= 14.0 * lv.startle;
            for ear in b.ears.into_iter().flatten() {
                j[ear].rot.x -= 18.0 * lv.startle * self.ear_k;
            }
        }
        // A loved spot: leaning into the hand as into a scratch, a cat's
        // front paws treading.
        if lv.bliss > lv.scratch {
            let lean = lv.bliss - lv.scratch;
            let toward = if lv.gaze.0 < 0.0 { -1.0 } else { 1.0 };
            let sway = (TAU * 0.5 * t).sin();
            j[b.body].rot.z -= toward * lean * (5.0 + 2.0 * sway);
            j[b.head].rot.z -= toward * lean * (12.0 + 4.0 * sway);
            if self.kneads {
                let mut k = 0.0;
                for &(leg, _, front) in &self.legs {
                    if front {
                        let tread = (TAU * 1.3 * t + std::f32::consts::PI * k).sin().max(0.0);
                        j[leg].rot.x -= lean * 20.0 * tread;
                        j[leg].shift.y += lean * 0.04 * tread;
                        k += 1.0;
                    }
                }
            }
        }
        // A spot it does not like: ears back, the head turned away from
        // the hand, the body leaning off it.
        if lv.shy > 0.0 {
            let toward = if lv.gaze.0 < 0.0 { -1.0 } else { 1.0 };
            for ear in b.ears.into_iter().flatten() {
                j[ear].rot.x += 30.0 * lv.shy * self.ear_k;
            }
            j[b.head].rot.y += toward * 28.0 * lv.shy;
            j[b.head].rot.x += 8.0 * lv.shy;
            j[b.body].rot.z += toward * 6.0 * lv.shy;
        }
        // The belly trap: on its back, the front paws clutch the hand and
        // both hind legs kick away at it, fast.
        if lv.trap > 0.0 {
            let mut k = 0.0;
            for &(leg, _, front) in &self.legs {
                if front {
                    j[leg].rot.x -= lv.trap * 70.0;
                } else {
                    let kick = (TAU * 5.0 * t + std::f32::consts::PI * k).sin();
                    j[leg].rot.x -= lv.trap * (50.0 + 45.0 * kick);
                    j[leg].shift.y += 0.07 * lv.trap * kick.max(0.0);
                    k += 1.0;
                }
            }
            // Wriggling about with it, curled round the hand.
            j[0].rot.z += lv.trap * 9.0 * (TAU * 2.5 * t).sin();
            j[b.head].rot.x += lv.trap * 18.0;
        }
        // A dog scratched just right: a hind leg kicks away.
        if self.kicks && lv.kick > 0.0 {
            let leg = if facing > 0.0 { b.hind_r } else { b.hind_l };
            if let Some(leg) = leg {
                j[leg].rot.x -= lv.kick * (20.0 + 22.0 * (TAU * 6.0 * t).sin());
                j[leg].shift.y += 0.05 * lv.kick;
            }
        }
    }
}

/// How each tail folds over the pet asleep (see [`BLANKET_TIP`]): its
/// root's turn, and the droop each segment after it adds, both as
/// [`M3::euler`] degrees. One tail stays put.
fn blanket(sk: &Skeleton, chains: &[Vec<usize>]) -> Vec<(V3, V3)> {
    if chains.len() < 2 {
        return vec![(V3::ZERO, V3::ZERO); chains.len()];
    }
    chains
        .iter()
        .map(|c| {
            // Leaning the way it already leans, a little further.
            let lean = match c[..] {
                [a, b, ..] => (sk.pivots[b] - sk.pivots[a]).normalize().x,
                _ => 0.0,
            };
            (
                v3(BLANKET_TIP, 0.0, -BLANKET_SPREAD * lean.clamp(-1.0, 1.0)),
                v3(BLANKET_DROOP, 0.0, 0.0),
            )
        })
        .collect()
}

/// Where each tail's root turns to when the pet sits (see [`SIT_FAN`]),
/// degrees as [`M3::euler`], in the hips' frame. Several tails take places
/// in a fan in the order they lie left to right, so none cross; one tail
/// only rises against the body tipping back.
fn fan(sk: &Skeleton, chains: &[Vec<usize>]) -> Vec<V3> {
    let dirs: Vec<V3> = chains
        .iter()
        .map(|c| match c[..] {
            [a, b, ..] => (sk.pivots[b] - sk.pivots[a]).normalize(),
            _ => v3(0.0, 1.0, 0.0),
        })
        .collect();
    if chains.len() < 2 {
        return vec![v3(SIT_PITCH, 0.0, 0.0); chains.len()];
    }
    let mut order: Vec<usize> = (0..chains.len()).collect();
    order.sort_by(|&a, &b| {
        dirs[a]
            .x
            .atan2(dirs[a].y)
            .total_cmp(&dirs[b].x.atan2(dirs[b].y))
    });
    let mut turns = vec![V3::ZERO; chains.len()];
    // Evenly along the two arcs either side of the gap.
    let arc = SIT_FAN - SIT_FAN_GAP;
    for (slot, &t) in order.iter().enumerate() {
        let along = 2.0 * arc * slot as f32 / (chains.len() - 1) as f32;
        let across = if along <= arc {
            -SIT_FAN + along
        } else {
            SIT_FAN_GAP + along - arc
        };
        let (s, c) = across.to_radians().sin_cos();
        // Where it should point with the body tipped back, in the body's
        // own frame: tipped forward by as much.
        let world = v3(s, c, -SIT_FAN_BACK).normalize();
        let want = M3::rot_x(SIT_PITCH.to_radians()).apply(world);
        turns[t] = aim(dirs[t], want);
    }
    turns
}

fn cross(a: V3, b: V3) -> V3 {
    v3(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// The turn (degrees, as [`M3::euler`]) that points `from` along `to`:
/// the shortest turn between them (Rodrigues), read back as Euler angles.
fn aim(from: V3, to: V3) -> V3 {
    let (from, to) = (from.normalize(), to.normalize());
    let axis = cross(from, to);
    let (sin, cos) = (axis.length(), from.dot(to));
    let k = if sin > 1e-6 {
        axis / sin
    } else if cos > 0.0 {
        return V3::ZERO;
    } else {
        // Opposite ways: half a turn about any axis square to `from`.
        let other = if from.x.abs() < 0.9 {
            v3(1.0, 0.0, 0.0)
        } else {
            v3(0.0, 1.0, 0.0)
        };
        cross(from, other).normalize()
    };
    rodrigues(k, sin, cos)
}

fn rodrigues(k: V3, sin: f32, cos: f32) -> V3 {
    let kk = |i: usize, j: usize| [k.x, k.y, k.z][i] * [k.x, k.y, k.z][j];
    let skew = [[0.0, -k.z, k.y], [k.z, 0.0, -k.x], [-k.y, k.x, 0.0]];
    let m = |i: usize, j: usize| {
        let id = if i == j { 1.0 } else { 0.0 };
        id * cos + sin * skew[i][j] + (1.0 - cos) * kk(i, j)
    };
    // M3::euler is Ry · Rx · Rz: m12 = −sin x, (m10, m11) ∝ (sin z, cos z),
    // (m02, m22) ∝ (sin y, cos y).
    let x = (-m(1, 2)).clamp(-1.0, 1.0).asin();
    let z = m(1, 0).atan2(m(1, 1));
    let y = m(0, 2).atan2(m(2, 2));
    v3(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_and_running_legs_of_a_side_swing_apart() {
        // The legs' swing, recorded at a walk, half into a gallop and a full
        // one: the two legs of a side never swing together (a pace would
        // correlate near +1).
        fn corr(a: &[f32], b: &[f32]) -> f32 {
            let n = a.len() as f32;
            let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
            let cov: f32 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
            let va: f32 = a.iter().map(|x| (x - ma).powi(2)).sum();
            let vb: f32 = b.iter().map(|y| (y - mb).powi(2)).sum();
            cov / (va * vb).sqrt()
        }
        for id in QUAD_SPECIES {
            let (_, sk) = quad(id).unwrap();
            for pace in [1.0, 1.5, 1.6, 1.8, 1.9, 2.0, 2.1, 2.2, 3.0] {
                let mut w = Walk::new(&sk, 1.0).unwrap();
                w.set_pace(pace);
                let mut swing = [vec![], vec![], vec![], vec![]];
                for k in 0..300 {
                    let j = w.step(1.0 / 60.0, true, 1.0);
                    if k >= 180 {
                        for (l, &(leg, ..)) in w.legs.iter().enumerate() {
                            swing[l].push(j[leg].rot.x);
                        }
                    }
                }
                for (front, hind) in [(1, 0), (3, 2)] {
                    let c = corr(&swing[front], &swing[hind]);
                    assert!(
                        c < 0.4,
                        "{id} at pace {pace}: legs {front} and {hind} correlate {c}"
                    );
                }
            }
        }
    }

    #[test]
    fn running_never_moves_the_legs_of_a_side_together() {
        // The legs of a side a quarter of a step apart walking, half a step
        // galloping, and in between at every mix: never together (a pace).
        let ahead = |a: usize, b: usize, run: f32| {
            (leg_phase(0.0, LEGS[a].1, GALLOP[a], run) - leg_phase(0.0, LEGS[b].1, GALLOP[b], run))
                .rem_euclid(1.0)
        };
        for k in 0..=20 {
            let run = k as f32 / 20.0;
            for (front, hind) in [(1, 0), (3, 2)] {
                let gap = ahead(front, hind, run);
                assert!(
                    (0.249..=0.501).contains(&gap),
                    "run {run}: front {front} {gap} ahead of hind {hind}"
                );
            }
        }
        // Galloping: the hind paws a beat apart, then the front paws the
        // same way half a step on.
        assert!((ahead(2, 0, 1.0) - 0.1).abs() < 1e-4);
        assert!((ahead(1, 0, 1.0) - 0.5).abs() < 1e-4);
        assert!((ahead(3, 1, 1.0) - 0.1).abs() < 1e-4);
    }

    #[test]
    fn aim_turns_any_direction_onto_any_other() {
        // Straight back onto sideways is the case two angles cannot reach.
        let dirs = [
            v3(0.0, 0.0, -1.0),
            v3(1.0, 0.0, 0.0),
            v3(0.0, 1.0, 0.0),
            v3(0.16, -0.01, -0.99),
            v3(-0.86, 0.24, -0.46),
            v3(0.7, 0.62, 0.36),
        ];
        for from in dirs {
            for to in dirs.iter().map(|d| v3(d.z, d.x, -d.y)).chain(dirs) {
                let got = M3::euler(aim(from, to)).apply(from.normalize());
                let miss = (got - to.normalize()).length();
                assert!(miss < 1e-3, "{from:?} → {to:?} landed {got:?}");
            }
        }
    }
}
