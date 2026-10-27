//! Frames ray-marched on the GPU, every one in the pose the animation has
//! blended to (PLAN §3.1).
//!
//! What the sprite path fakes in 2D happens in 3D here:
//! - moving between poses (sitting down into a loaf, glancing aside) blends
//!   every bone over a moment instead of cutting;
//! - turning round blends the pose into its mirror image, so the pet turns
//!   through facing you;
//! - walking swings the legs from one step pose to the other;
//! - squash, lean and hops move the pet in 3D while the contact shadow stays
//!   on the floor;
//! - the tail wags on its bone, a waving paw waves.
//!
//! Frames that come round again (breathing in place, a steady walk's beats)
//! are kept and copied, so an idle pet hardly touches the GPU (PLAN §5).

use std::sync::Arc;

use chonkgene_core::anim::{Act, BEATS, Expression, PoseParams};
use chonkgene_rig::math::{smoothstep, v3};
use chonkgene_rig::{Layer, Look, Model, Pose, PoseId, Scene};
use tiny_skia::{Color, Pixmap, Transform};

use crate::gpu::{Canvas, Frame, Gpu};
use crate::raymarch::{self, Camera};
use crate::sprites::{Key, Variant};
use crate::{key_for, sleep_overlay};

/// Breathing in place is quantised to this many steps (as on the sprite
/// path), so idle frames are a few cached images.
const BREATH_STEPS: u8 = 6;
/// Cached frames kept at most: a walk's beats on both feet, with a blink.
const MAX_CACHED: usize = 4 * BEATS as usize;

/// Where the body is heading: a pose, or walking (whose legs swing between
/// the two step poses without counting as a change).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Aim {
    Pose(PoseId),
    Walk,
}

impl Aim {
    fn lying(self) -> bool {
        matches!(
            self,
            Aim::Pose(PoseId::BellyUp | PoseId::Melt | PoseId::Loaf | PoseId::Sulk)
        )
    }
}

/// Seconds to blend from one aim to the next.
fn blend_secs(from: Aim, to: Aim) -> f32 {
    if from.lying() || to.lying() {
        0.45
    } else if from == Aim::Walk || to == Aim::Walk {
        0.22
    } else {
        0.3
    }
}

/// A frame that comes round again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Repeat {
    /// Breathing in place: the pose, the way round, the breath step.
    Breath(PoseId, bool, u8),
    /// A beat of a steady walk: the step, the way round, the beat.
    Walk(bool, bool, u8),
}

/// What a frame drew: enough to tell whether the next one differs, and to
/// hit-test it.
#[derive(Clone, Debug, PartialEq)]
struct Drawn {
    pose: Pose,
    look: Look,
    frame: Frame,
}

pub struct GpuRenderer {
    gpu: &'static Gpu,
    model: Arc<Model>,
    variant: Variant,
    canvas: Canvas,
    pixmap: Pixmap,
    /// The current aim, and the blend into it: the pose it started from
    /// and when, on the animation clock.
    aim: Option<Aim>,
    from: Option<(Pose, f32, f32)>,
    /// The last pose before turning round and wagging: where a new blend
    /// starts from.
    unturned: Option<Pose>,
    last: Option<Drawn>,
    cache: Vec<((Repeat, Look), Pixmap)>,
    changed: bool,
    tail_bone: Option<usize>,
}

impl GpuRenderer {
    pub fn new(
        gpu: &'static Gpu,
        model: Arc<Model>,
        scale: f32,
        chonk: f32,
        coat: usize,
    ) -> Option<Self> {
        let canvas = gpu.canvas(scale);
        let (w, h) = canvas.size();
        Some(Self {
            gpu,
            tail_bone: model.bone("tail"),
            model,
            variant: Variant { coat, chonk, scale },
            canvas,
            pixmap: Pixmap::new(w, h)?,
            aim: None,
            from: None,
            unturned: None,
            last: None,
            cache: Vec::new(),
            changed: true,
        })
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    pub fn set_chonk(&mut self, chonk: f32) {
        if (chonk - self.variant.chonk).abs() > 1e-3 {
            self.variant.chonk = chonk;
            self.restyle();
        }
    }

    pub fn set_coat(&mut self, coat: usize) {
        if coat != self.variant.coat {
            self.variant.coat = coat;
            self.restyle();
        }
    }

    fn restyle(&mut self) {
        self.cache.clear();
        self.last = None;
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    /// Still blending between poses at the time of the last frame?
    pub fn blending(&self) -> bool {
        self.from.is_some()
    }

    /// The pose the body is heading for, before turning round.
    fn target(&self, key: Key, params: &PoseParams) -> (Aim, Pose) {
        let walking = matches!(params.act, Act::StepA | Act::StepB)
            && matches!(key.pose, PoseId::WalkA | PoseId::WalkB);
        if walking {
            // Legs swing from one step pose to the other and back over the
            // two-step cycle.
            let s = (1.0 - (params.stride * std::f32::consts::PI).cos()) / 2.0;
            let pose = self
                .model
                .pose(PoseId::WalkB)
                .blend(self.model.pose(PoseId::WalkA), s);
            return (Aim::Walk, pose);
        }
        let mut pose = self.model.pose(key.pose).clone();
        if key.pose == PoseId::Wave {
            // The raised paw (or wing) waves.
            let wave = 22.0 * (params.clock * std::f32::consts::TAU * 2.4).sin();
            for name in ["front_legs", "wings"] {
                if let Some(twin) = self
                    .model
                    .bone(name)
                    .and_then(|b| pose.twins.get_mut(b))
                    .and_then(Option::as_mut)
                {
                    twin.rot.z += wave;
                }
            }
        }
        (Aim::Pose(key.pose), pose)
    }

    /// The pose to draw now: blended from where the body was into the aim,
    /// turned round as far as `facing` says, the tail wagged.
    fn pose(&mut self, key: Key, params: &PoseParams) -> (Aim, Pose) {
        let (aim, target) = self.target(key, params);
        let t = params.clock;
        if self.aim != Some(aim) {
            // Blend from wherever the body was; the first frame just starts.
            self.from = match (self.aim, self.unturned.take()) {
                (Some(prev), Some(start)) => Some((start, t, blend_secs(prev, aim))),
                _ => None,
            };
            self.aim = Some(aim);
        }
        let mut pose = match &self.from {
            Some((start, since, secs)) => {
                let k = smoothstep(0.0, 1.0, (t - since) / secs);
                if k >= 1.0 {
                    self.from = None;
                    target
                } else {
                    start.blend(&target, k)
                }
            }
            None => target,
        };
        self.unturned = Some(pose.clone());
        // Turning round: the pose blends into its own mirror image.
        let f = params.facing.clamp(-1.0, 1.0);
        if f < 1.0 {
            pose = pose.blend(&pose.mirrored(), (1.0 - f) / 2.0);
        }
        if let Some(b) = self.tail_bone
            && params.tail != 0.0
            && let Some(m) = pose.bones.get_mut(b)
        {
            m.rot.y += params.tail;
        }
        (aim, pose)
    }

    pub fn render(&mut self, params: &PoseParams, expression: Expression) -> &Pixmap {
        let key = key_for(expression, params);
        let (aim, pose) = self.pose(key, params);
        let turned = params.facing.abs() == 1.0;
        let mirrored = params.facing < 0.0;
        let step = ((params.breath.clamp(-1.0, 1.0) + 1.0) / 2.0 * f32::from(BREATH_STEPS - 1))
            .round() as u8;
        let mut frame = Frame {
            squash: 0.35 * params.squash + 0.15 * params.belly,
            breath: params.breath,
            lean: params.sway,
            lift: -params.bob,
        };
        let settled = self.from.is_none() && turned;
        let repeat = match (aim, params.beat) {
            (Aim::Walk, Some(beat)) if settled => {
                Some(Repeat::Walk(params.act == Act::StepA, mirrored, beat))
            }
            (Aim::Pose(id), _)
                if settled
                    && id != PoseId::Wave
                    && frame.squash.abs() < 0.004
                    && frame.lean.abs() < 0.05
                    && frame.lift == 0.0
                    && params.tail == 0.0 =>
            {
                // Breathing in place: snap the breath to its step.
                frame.breath = f32::from(step) / f32::from(BREATH_STEPS - 1) * 2.0 - 1.0;
                frame.squash = 0.0;
                frame.lean = 0.0;
                Some(Repeat::Breath(id, mirrored, step))
            }
            _ => None,
        };
        let drawn = Drawn {
            pose,
            look: key.look,
            frame,
        };
        if self.last.as_ref() == Some(&drawn) {
            self.changed = false;
            return &self.pixmap;
        }
        self.changed = true;
        if let Some(r) = repeat
            && let Some((_, img)) = self.cache.iter().find(|(k, _)| *k == (r, key.look))
        {
            self.pixmap.data_mut().copy_from_slice(img.data());
            self.last = Some(drawn);
            return &self.pixmap;
        }
        let v = self.variant;
        let scene = Scene::posed(
            &self.model,
            v.coat,
            v.chonk,
            &drawn.pose,
            drawn.look,
            Layer::All,
        );
        if let Err(e) = self.gpu.render(
            &mut self.canvas,
            &scene,
            &drawn.frame,
            true,
            &mut self.pixmap,
        ) {
            eprintln!("GPU frame failed: {e}");
            self.pixmap.fill(Color::TRANSPARENT);
        }
        if let Some(r) = repeat {
            // Keep only frames that may come round again: breathing in
            // the same pose, or the same walk.
            self.cache.retain(|((k, _), _)| match (k, r) {
                (Repeat::Breath(p, m, _), Repeat::Breath(q, n, _)) => *p == q && *m == n,
                (Repeat::Walk(_, m, _), Repeat::Walk(_, n, _)) => *m == n,
                _ => false,
            });
            if self.cache.len() >= MAX_CACHED {
                self.cache.remove(0);
            }
            self.cache.push(((r, key.look), self.pixmap.clone()));
        }
        self.last = Some(drawn);
        &self.pixmap
    }

    pub fn render_asleep(&mut self, params: &PoseParams, phase: f32) -> &Pixmap {
        // The bubble and the z move every frame over the pet: draw it afresh
        // (from the cache, when it only breathes).
        self.last = None;
        self.render(params, Expression::Asleep);
        if let Some(d) = &self.last {
            let v = self.variant;
            let scene = Scene::posed(&self.model, v.coat, v.chonk, &d.pose, d.look, Layer::All);
            let cam = Camera::new(v.scale);
            let nose = scene.nose();
            let at = cam.project(nose + v3(0.0, d.frame.lift / raymarch::PPU, 0.0));
            sleep_overlay(&mut self.pixmap, at, Transform::identity(), v.scale, phase);
        }
        &self.pixmap
    }

    /// The zone under pixel `(x, y)` of the last frame: one ray on the CPU
    /// through the same pose and frame.
    pub fn hit(&self, x: f32, y: f32) -> Option<&'static str> {
        let d = self.last.as_ref()?;
        let v = self.variant;
        let scene = Scene::posed(&self.model, v.coat, v.chonk, &d.pose, d.look, Layer::All);
        raymarch::zone_at(&scene, v.scale, &d.frame, x, y)
    }

    /// The top of the head in the last frame: the highest solid pixels in
    /// the middle of the picture.
    pub fn anchor(&self) -> (f32, f32) {
        let pm = &self.pixmap;
        let (w, h) = (pm.width() as usize, pm.height() as usize);
        let (x0, x1) = (w / 5, w - w / 5);
        let data = pm.data();
        for y in 0..h {
            let solid: Vec<usize> = (x0..x1)
                .filter(|x| data[(y * w + x) * 4 + 3] > 200)
                .collect();
            if let (Some(a), Some(b)) = (solid.first(), solid.last()) {
                return ((a + b) as f32 / 2.0, y as f32);
            }
        }
        (w as f32 / 2.0, 0.0)
    }
}
