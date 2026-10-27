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
//! Frames that come round again (breathing in place, wagging at rest, a
//! steady walk's beats) are kept and copied, so an idle pet hardly touches
//! the GPU (PLAN §5). Frames are drawn without waiting: each one is picked
//! up on a later call, so the UI thread never stalls on the GPU, and a busy
//! GPU skips frames instead of falling behind.

use std::sync::Arc;

use chonkgene_core::anim::{Act, BEATS, Expression, PoseParams};
use chonkgene_rig::math::{smoothstep, v3};
use chonkgene_rig::{Layer, Look, Model, Pose, PoseId, Scene};
use tiny_skia::{Pixmap, Transform};

use crate::gpu::{Canvas, Frame, Gpu};
use crate::raymarch::{self, Camera};
use crate::sprites::{Key, Variant};
use crate::{key_for, sleep_overlay};

/// Breathing at rest is quantised to this many steps and drawn in 2D over
/// a cached frame (it only stretches the pet by 1.5%), so breathing never
/// needs the GPU.
const BREATH_STEPS: u8 = 6;
/// Cached frames kept at most, oldest dropped first: a walk's beats on both
/// feet, or resting poses with a blink and a wag. Each keeps only the box
/// around its pixels.
const MAX_CACHED: usize = 3 * BEATS as usize + 4;
/// A wagging tail is snapped to this many degrees, so a wag at rest is a
/// few cached frames too.
const WAG_STEP: f32 = 3.0;

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
    /// At rest: the pose, the way round, the tail (in wag steps).
    Still(PoseId, bool, i16),
    /// A beat of a steady walk: the step, the way round, the beat.
    Walk(bool, bool, u8),
}

type CacheKey = (Repeat, Look);

/// A cached frame: only the box around its pixels, at `at`.
struct Kept {
    key: CacheKey,
    at: (usize, usize),
    size: (usize, usize),
    data: Vec<u8>,
}

/// What a frame drew: enough to tell whether the next one differs, and to
/// hit-test it.
#[derive(Clone, Debug, PartialEq)]
struct Drawn {
    pose: Pose,
    look: Look,
    frame: Frame,
}

/// A frame on its way from the GPU.
struct Pending {
    seq: u64,
    drawn: Drawn,
    key: Option<CacheKey>,
}

pub struct GpuRenderer {
    gpu: &'static Gpu,
    model: Arc<Model>,
    variant: Variant,
    canvas: Canvas,
    /// The frame shown, before 2D breathing and overlays.
    base: Pixmap,
    /// What is shown: `base`, breathed.
    pixmap: Pixmap,
    /// The breath drawn in 2D over `base` (0.0 when the frame breathes in
    /// 3D), the one the wanted picture asks for, and the one shown.
    breath: f32,
    want_breath: f32,
    breathed: f32,
    /// Where frames from the GPU land before they are shown.
    arrival: Pixmap,
    /// The current aim, and the blend into it: the pose it started from,
    /// when, and for how long, on the animation clock.
    aim: Option<Aim>,
    from: Option<(Pose, f32, f32)>,
    /// The last pose before turning round and wagging: where a new blend
    /// starts from.
    unturned: Option<Pose>,
    /// The picture wanted now, and whether it still has to go to the GPU.
    wanted: Option<(Drawn, Option<CacheKey>)>,
    unsent: bool,
    pending: Option<Pending>,
    /// Pictures are numbered as they are wanted; an older one arriving
    /// after a newer one was shown is not shown.
    seq: u64,
    shown_seq: u64,
    shown: Option<Drawn>,
    cache: Vec<Kept>,
    changed: bool,
    tail_bone: Option<usize>,
    /// Frames sent to the GPU so far (the rest came from the cache).
    sent: u64,
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
            base: Pixmap::new(w, h)?,
            pixmap: Pixmap::new(w, h)?,
            breath: 0.0,
            want_breath: 0.0,
            breathed: 0.0,
            arrival: Pixmap::new(w, h)?,
            aim: None,
            from: None,
            unturned: None,
            wanted: None,
            unsent: false,
            pending: None,
            seq: 0,
            shown_seq: 0,
            shown: None,
            cache: Vec::new(),
            changed: true,
            sent: 0,
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
        self.wanted = None;
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    /// How many frames have gone to the GPU (the rest came from the cache).
    pub fn frames_sent(&self) -> u64 {
        self.sent
    }

    /// GPU seconds its frames took, as measured by the GPU (0 if it cannot).
    pub fn gpu_seconds(&self) -> f64 {
        self.canvas.gpu_seconds()
    }

    /// Blending between poses, or a frame still on its way: frames should
    /// keep coming even if the animation is otherwise at rest.
    pub fn busy(&self) -> bool {
        self.from.is_some() || self.pending.is_some() || self.unsent
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
        let mut pose = pose;
        let mut breath = 0.0;
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
                    && frame.lift == 0.0 =>
            {
                // At rest: the tail snapped to its wag step, no breath
                // (drawn in 2D below, from its step).
                let wag = (params.tail / WAG_STEP).round() as i16;
                if let Some(m) = self.tail_bone.and_then(|b| pose.bones.get_mut(b)) {
                    m.rot.y += f32::from(wag) * WAG_STEP - params.tail;
                }
                breath = f32::from(step) / f32::from(BREATH_STEPS - 1) * 2.0 - 1.0;
                frame.breath = 0.0;
                frame.squash = 0.0;
                frame.lean = 0.0;
                Some(Repeat::Still(id, mirrored, wag))
            }
            _ => None,
        };
        let drawn = Drawn {
            pose,
            look: key.look,
            frame,
        };
        let key = repeat.map(|r| (r, key.look));
        self.changed = false;
        self.collect(false);
        if self.wanted.as_ref().map(|(d, _)| d) != Some(&drawn) {
            self.seq += 1;
            if let Some(k) = key
                && let Some(i) = self.cache.iter().position(|c| c.key == k)
            {
                paste(&self.cache[i], &mut self.base);
                self.shown = Some(drawn.clone());
                self.shown_seq = self.seq;
                self.unsent = false;
                self.changed = true;
            } else {
                self.unsent = true;
            }
            self.wanted = Some((drawn, key));
        }
        if self.unsent && self.pending.is_none() {
            self.send(false);
        }
        self.want_breath = breath;
        self.sync_breath();
        if self.changed || self.breath != self.breathed {
            self.present();
            self.changed = true;
        }
        &self.pixmap
    }

    /// The breath goes with the picture that shows: the wanted one's once
    /// it is up (a 3D frame breathes by itself).
    fn sync_breath(&mut self) {
        if self.shown.as_ref() == self.wanted.as_ref().map(|(d, _)| d) {
            self.breath = self.want_breath;
        }
    }

    /// Show `base`, breathed in 2D: stretched up about the floor line.
    fn present(&mut self) {
        if self.breath == 0.0 {
            self.pixmap.data_mut().copy_from_slice(self.base.data());
        } else {
            let fy = self.base.height() as f32 - raymarch::FLOOR_PX * self.variant.scale;
            let ts = Transform::from_translate(0.0, fy)
                .pre_scale(1.0, 1.0 + 0.015 * self.breath)
                .pre_translate(0.0, -fy);
            let paint = tiny_skia::PixmapPaint {
                quality: tiny_skia::FilterQuality::Bilinear,
                ..tiny_skia::PixmapPaint::default()
            };
            self.pixmap.fill(tiny_skia::Color::TRANSPARENT);
            self.pixmap
                .draw_pixmap(0, 0, self.base.as_ref(), &paint, ts, None);
        }
        self.breathed = self.breath;
    }

    /// Send the wanted picture to the GPU, if the budget allows (`force`:
    /// regardless).
    fn send(&mut self, force: bool) {
        let Some((drawn, key)) = self.wanted.clone() else {
            return;
        };
        let v = self.variant;
        let scene = Scene::posed(
            &self.model,
            v.coat,
            v.chonk,
            &drawn.pose,
            drawn.look,
            Layer::All,
        );
        let sent = if force {
            self.gpu
                .submit(&mut self.canvas, &scene, &drawn.frame, true)
                .map(|()| true)
        } else {
            self.gpu
                .try_submit(&mut self.canvas, &scene, &drawn.frame, true)
        };
        match sent {
            // Over the GPU budget: try again on a later call (the frame
            // rate drops instead of the GPU share rising).
            Ok(false) => {}
            Ok(true) => {
                self.sent += 1;
                self.pending = Some(Pending {
                    seq: self.seq,
                    drawn,
                    key,
                });
                self.unsent = false;
            }
            Err(e) => eprintln!("GPU frame failed: {e}"),
        }
    }

    /// Take in a frame that has arrived (`wait`: wait for it): keep it if
    /// it comes round again, show it unless something newer is shown.
    fn collect(&mut self, wait: bool) {
        if self.pending.is_none() {
            return;
        }
        match self.gpu.collect(&mut self.canvas, &mut self.arrival, wait) {
            Ok(false) => {}
            Ok(true) => {
                let Some(p) = self.pending.take() else {
                    return;
                };
                if let Some(k) = p.key {
                    if self.cache.len() >= MAX_CACHED {
                        self.cache.remove(0);
                    }
                    self.cache.push(keep(k, &self.arrival));
                }
                if p.seq > self.shown_seq {
                    self.base.data_mut().copy_from_slice(self.arrival.data());
                    self.shown = Some(p.drawn);
                    self.shown_seq = p.seq;
                    self.changed = true;
                }
            }
            Err(e) => {
                eprintln!("GPU frame failed: {e}");
                self.pending = None;
            }
        }
    }

    /// Wait until the wanted picture is shown (tests, screenshots).
    pub fn finish(&mut self) {
        self.collect(true);
        if self.unsent {
            self.send(true);
            self.collect(true);
        }
        self.sync_breath();
        self.present();
    }

    pub fn render_asleep(&mut self, params: &PoseParams, phase: f32) -> &Pixmap {
        self.render(params, Expression::Asleep);
        // The bubble and the z move every frame, over the pet as shown.
        self.present();
        self.changed = true;
        if let Some(d) = &self.shown {
            let v = self.variant;
            let scene = Scene::posed(&self.model, v.coat, v.chonk, &d.pose, d.look, Layer::All);
            let cam = Camera::new(v.scale);
            let at = cam.project(scene.nose() + v3(0.0, d.frame.lift / raymarch::PPU, 0.0));
            sleep_overlay(&mut self.pixmap, at, Transform::identity(), v.scale, phase);
        }
        &self.pixmap
    }

    /// The zone under pixel `(x, y)` of the frame shown: one ray on the CPU
    /// through the same pose and frame.
    pub fn hit(&self, x: f32, y: f32) -> Option<&'static str> {
        let d = self.shown.as_ref()?;
        let v = self.variant;
        let scene = Scene::posed(&self.model, v.coat, v.chonk, &d.pose, d.look, Layer::All);
        raymarch::zone_at(&scene, v.scale, &d.frame, x, y)
    }

    /// The top of the head in the frame shown: the highest solid pixels in
    /// the middle of the picture.
    pub fn anchor(&self) -> (f32, f32) {
        let pm = &self.base;
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

/// Keep the box around the pixels of `pm`.
fn keep(key: CacheKey, pm: &Pixmap) -> Kept {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let data = pm.data();
    let alpha = |x: usize, y: usize| data[(y * w + x) * 4 + 3] != 0;
    let rows: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| alpha(x, y))).collect();
    let cols: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| alpha(x, y))).collect();
    let (Some(&y0), Some(&y1), Some(&x0), Some(&x1)) =
        (rows.first(), rows.last(), cols.first(), cols.last())
    else {
        return Kept {
            key,
            at: (0, 0),
            size: (0, 0),
            data: Vec::new(),
        };
    };
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut out = Vec::with_capacity(cw * ch * 4);
    for y in y0..=y1 {
        out.extend_from_slice(&data[(y * w + x0) * 4..(y * w + x1 + 1) * 4]);
    }
    Kept {
        key,
        at: (x0, y0),
        size: (cw, ch),
        data: out,
    }
}

/// Show a kept frame in `pm`.
fn paste(k: &Kept, pm: &mut Pixmap) {
    let w = pm.width() as usize;
    let data = pm.data_mut();
    data.fill(0);
    let (cw, ch) = k.size;
    for y in 0..ch {
        let at = ((k.at.1 + y) * w + k.at.0) * 4;
        data[at..at + cw * 4].copy_from_slice(&k.data[y * cw * 4..(y + 1) * cw * 4]);
    }
}
