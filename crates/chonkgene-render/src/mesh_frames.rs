//! Pets on all fours drawn as skinned meshes on the GPU (mesh.rs, skin.rs,
//! mesh_gpu.rs): every frame is the baked mesh in a pose worked out that
//! moment, so tails sway, bottoms wobble and turns happen in 3D without a
//! single cached picture.
//!
//! The animator's params drive the walk: stepping walks, any strong feeling
//! sits the pet down (its face shows the feeling), a poke jolts the belly,
//! a hop lifts the whole pet, glances turn the head.
//!
//! The UI thread never waits on the GPU or the CPU for a pet: the mesh is
//! baked on its own thread (the pet shows once it lands), frames are sent
//! to the GPU without waiting and picked up on a later call (one frame
//! late), and a frame the GPU budget cannot pay for waits, so a busy GPU
//! drops frames instead of its share rising.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use chonkgene_core::anim::{Act, BITE_SECS, EAT_SECS, Expression, PoseParams, gesture_secs};
use chonkgene_rig::math::{V3, v3};
use chonkgene_rig::{FLOOR, Layer, Look, Model, Pose, Scene, ZONES};
use tiny_skia::{Pixmap, Transform};

use crate::gpu::Gpu;
use crate::mesh::{Mesh, bake};
use crate::mesh_gpu::MeshCanvas;
use crate::raymarch::PPU;
use crate::skin::{Affine, Gesture, Skeleton, bite};
use crate::waddle::Mover;
use crate::{key_for, sleep_overlay, snack_overlay};

/// The canvas, world units: wider than an upright pet's, for a body lying
/// along the floor and a fox's fan of tails.
const UNITS: (f32, f32) = (4.5, 3.3);
/// The longest step the walk takes at once, seconds: its springs stay
/// steady however slowly frames come.
const MAX_STEP: f32 = 1.0 / 60.0;
/// Standing still doing nothing this long (seconds), a pet sits down.
const IDLE_SIT: f32 = 1.5;

/// Pixel size of a pet on all fours at `scale` (1.0 = 100%).
pub fn pixel_size(scale: f32) -> (u32, u32) {
    (
        (UNITS.0 * PPU * scale).ceil() as u32,
        (UNITS.1 * PPU * scale).ceil() as u32,
    )
}

/// The baked mesh, on the GPU.
struct Live {
    mesh: Mesh,
    canvas: MeshCanvas,
}

pub struct MeshRenderer {
    gpu: &'static Gpu,
    model: Arc<Model>,
    sk: Skeleton,
    scale: f32,
    chonk: f32,
    coat: usize,
    /// None until the first bake has landed.
    live: Option<Live>,
    /// A bake on its own thread, and whether the body or coat changed
    /// since it started (so it needs another).
    baking: Option<Receiver<Result<Mesh, String>>>,
    rebake: bool,
    /// What shows: the picture that last arrived, and the sleep overlay.
    pixmap: Pixmap,
    /// The picture that last arrived from the GPU.
    base: Pixmap,
    walk: Mover,
    /// The pose being worked out now, the one sent to the GPU, and the one
    /// in the picture shown (hits and the bubble go by that).
    bones: Vec<Affine>,
    sent_bones: Vec<Affine>,
    shown_bones: Vec<Affine>,
    look: Look,
    /// The wanted frame has not gone to the GPU yet (it was busy, or over
    /// its budget).
    unsent: bool,
    /// The animator's clock at the last frame, and its belly jiggle then.
    clock: Option<f32>,
    belly: f32,
    /// The idle gesture under way and the animator's clock when it began.
    gesture: Option<(Act, f32)>,
    /// Since when (the animator's clock) it has stood still, doing nothing.
    rest_since: Option<f32>,
    changed: bool,
}

impl MeshRenderer {
    /// Start baking `model` (a four-legged one, see [`crate::skin::quad`])
    /// on its own thread and get ready to draw it; none if it has no legs
    /// to walk on or no pixmap. The pet shows once the bake lands
    /// ([`MeshRenderer::poll`]).
    pub fn new(
        gpu: &'static Gpu,
        model: Arc<Model>,
        scale: f32,
        chonk: f32,
        coat: usize,
    ) -> Option<Self> {
        let sk = Skeleton::new(&model);
        let walk = Mover::new(&sk, 1.0, &model).ok()?;
        let (w, h) = pixel_size(scale);
        let bones = sk.pose(&[]);
        let baking = Some(spawn_bake(&model, &sk, coat, chonk));
        Some(Self {
            gpu,
            model,
            sk,
            scale,
            chonk,
            coat,
            live: None,
            baking,
            rebake: false,
            pixmap: Pixmap::new(w, h)?,
            base: Pixmap::new(w, h)?,
            walk,
            sent_bones: bones.clone(),
            shown_bones: bones.clone(),
            bones,
            look: Look::Open,
            unsent: false,
            clock: None,
            belly: 0.0,
            gesture: None,
            rest_since: None,
            changed: false,
        })
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    /// Can the pet be seen yet: has the bake landed?
    pub fn ready(&self) -> bool {
        self.live.is_some()
    }

    /// Take in a bake that has finished. True if one landed, so the caller
    /// can redraw.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.baking else {
            return false;
        };
        match rx.try_recv() {
            Ok(baked) => {
                self.baking = None;
                self.install(baked)
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.baking = None;
                eprintln!("baking the mesh failed: the thread died");
                false
            }
        }
    }

    /// Put a baked mesh on the GPU, replacing the old one (the frame on
    /// its way with it), and bake again if the body or coat has moved on.
    fn install(&mut self, baked: Result<Mesh, String>) -> bool {
        let landed = match baked {
            Ok(mesh) => {
                let (w, h) = (self.base.width(), self.base.height());
                let canvas = MeshCanvas::new(self.gpu, &mesh, w, h, self.scale);
                self.live = Some(Live { mesh, canvas });
                self.unsent = true;
                self.changed = true;
                true
            }
            Err(e) => {
                eprintln!("baking the mesh failed: {e}");
                false
            }
        };
        if std::mem::take(&mut self.rebake) {
            self.baking = Some(spawn_bake(&self.model, &self.sk, self.coat, self.chonk));
        }
        landed
    }

    /// Bake again for a new body value or coat: the mesh carries both. The
    /// old mesh keeps drawing until the new one lands.
    fn request_bake(&mut self) {
        if self.baking.is_some() {
            self.rebake = true;
        } else {
            self.baking = Some(spawn_bake(&self.model, &self.sk, self.coat, self.chonk));
        }
    }

    pub fn set_chonk(&mut self, chonk: f32) {
        if chonk != self.chonk {
            self.chonk = chonk;
            self.request_bake();
        }
    }

    pub fn set_coat(&mut self, coat: usize) {
        if coat != self.coat {
            self.coat = coat;
            self.request_bake();
        }
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    /// Frames a second this pet needs however still its animation is:
    /// tails sway and bellies settle all the time, slowly at rest.
    pub fn min_fps(&self) -> u32 {
        if self.walk.speed() > 0.0 { 30 } else { 15 }
    }

    /// Wait for the bake and for the frame on its way (tests, screenshots),
    /// sending the wanted one first if it has not gone yet.
    pub fn finish(&mut self) {
        if let Some(rx) = self.baking.take()
            && let Ok(baked) = rx.recv()
        {
            self.install(baked);
        }
        if self.live.as_ref().is_some_and(|l| !l.canvas.pending()) && self.unsent {
            self.send(true);
        }
        self.collect(true);
        self.pixmap.data_mut().copy_from_slice(self.base.data());
    }

    pub fn render(&mut self, params: &PoseParams, expression: Expression) -> &Pixmap {
        let mut look = key_for(expression, params).look;
        let eating = (params.act == Act::Eat)
            .then_some(self.gesture)
            .flatten()
            .map(|(_, since)| (params.clock - since).max(0.0));
        if let Some(t) = eating {
            // Mouth open for each bite and, quickly, while chewing.
            let (dip, chew) = bite(t);
            if dip > 0.6 || chew > 0.3 {
                look = Look::Yawn;
            }
        }
        let arrived = self.draw(params, expression, look);
        if let Some(t) = eating {
            // The snack on the floor before it, a bite smaller each bite.
            self.pixmap.data_mut().copy_from_slice(self.base.data());
            let eaten = (t / BITE_SECS).floor() / (EAT_SECS / BITE_SECS);
            if let Some(at) = self.snack_spot() {
                snack_overlay(&mut self.pixmap, at, self.scale, eaten);
            }
            self.changed = true;
        } else if arrived {
            self.pixmap.data_mut().copy_from_slice(self.base.data());
        }
        &self.pixmap
    }

    /// Where a snack lies: on the floor just under the nose (or beak
    /// or bill) of the pose shown, pixels.
    fn snack_spot(&self) -> Option<(f32, f32)> {
        let live = self.live.as_ref()?;
        let part = ["nose", "beak", "bill"]
            .iter()
            .find_map(|n| self.model.part(n))
            .map(|i| &self.model.parts[i])?;
        let nose = self.shown_bones[part.bone].apply(part.at);
        let (x, y) = live.canvas.project(v3(nose.x, FLOOR, nose.z));
        // The floor under a nose held toward you lies below the canvas's
        // floor line (the camera looks down a little): keep it in view.
        let bottom = self.pixmap.height() as f32 - 2.0 * self.scale;
        Some((x, y.min(bottom)))
    }

    pub fn render_asleep(&mut self, params: &PoseParams, phase: f32) -> &Pixmap {
        self.draw(params, Expression::Asleep, Look::Asleep);
        // The bubble and the z move every frame, over the picture shown.
        self.pixmap.data_mut().copy_from_slice(self.base.data());
        // The bubble hangs off the nose, or the beak or bill of a bird.
        let nose = ["nose", "beak", "bill"]
            .iter()
            .find_map(|n| self.model.part(n));
        let nose = self.project_on(nose, V3::ZERO);
        sleep_overlay(
            &mut self.pixmap,
            nose,
            Transform::identity(),
            self.scale,
            phase,
        );
        self.changed = true;
        &self.pixmap
    }

    /// Work out the pose for this moment and send it to the GPU if it is
    /// free and the budget allows; take in a picture that has arrived.
    /// True if a new picture is in `base`.
    fn draw(&mut self, params: &PoseParams, expression: Expression, look: Look) -> bool {
        let walking = matches!(params.act, Act::StepA | Act::StepB);
        let facing = if params.facing < 0.0 { -1.0 } else { 1.0 };
        // Idle gestures: stretch, yawn, wave and eating, eased in and out
        // over their length.
        if self.gesture.map(|g| g.0) != Some(params.act) {
            self.gesture = gesture_secs(params.act).map(|_| (params.act, params.clock));
        }
        // How it holds itself: asleep it lies down; resting a moment, or
        // with a feeling on its face, it sits; a gesture is done standing.
        let asleep = expression == Expression::Asleep;
        if walking || self.gesture.is_some() || asleep {
            self.rest_since = None;
        } else {
            self.rest_since.get_or_insert(params.clock);
        }
        let rested = self
            .rest_since
            .is_some_and(|since| params.clock - since > IDLE_SIT);
        self.walk.lie(asleep && !walking);
        self.walk.sit(
            !walking
                && !asleep
                && self.gesture.is_none()
                && (rested || expression != Expression::Neutral),
        );
        self.walk.look(match params.act {
            Act::LookLeft => 35.0,
            Act::LookRight => -35.0,
            _ => 0.0,
        });
        self.walk.gesture(self.gesture.and_then(|(act, since)| {
            let secs = gesture_secs(act)?;
            let t = (params.clock - since).max(0.0);
            let e = (std::f32::consts::PI * (t / secs).min(1.0)).sin().sqrt();
            Some(Gesture { act, e, t })
        }));
        self.walk.set_pace(params.pace);
        // A poke jiggles the animator's belly: jolt the mesh's.
        if params.belly > 0.15 && self.belly <= 0.15 {
            self.walk.poke(2.0);
        }
        self.belly = params.belly;
        let dt = self
            .clock
            .map_or(MAX_STEP, |c| (params.clock - c).clamp(0.0, 0.25));
        self.clock = Some(params.clock);
        let steps = (dt / MAX_STEP).ceil().max(1.0);
        let mut joints = Vec::new();
        for _ in 0..steps as usize {
            joints = self.walk.step(dt / steps, walking, facing);
        }
        // Hops: `bob` is design pixels, negative up.
        if let Some(root) = joints.first_mut() {
            root.shift.y += (-params.bob).max(0.0) / PPU;
        }
        self.bones = self.sk.pose(&joints);
        self.look = look;
        self.changed = false;
        // A walking pet's picture must keep up with its window, which moves
        // every frame: its frames skip the budget, and if the last one is
        // still on its way it is waited for rather than skipped (a skipped
        // frame shows the legs standing still while the window slides on).
        let mut arrived = self.collect(false);
        let pending = |r: &Self| r.live.as_ref().is_some_and(|l| l.canvas.pending());
        if walking && pending(self) {
            arrived |= self.collect(true);
        }
        if self.live.is_some() && !pending(self) {
            self.send(walking);
        }
        arrived
    }

    /// Send the wanted frame to the GPU (`force`: whatever the budget
    /// says). The canvas must be free.
    fn send(&mut self, force: bool) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let scene = Scene::posed(
            &self.model,
            self.coat,
            self.chonk,
            &Pose::default(),
            self.look,
            Layer::All,
        );
        let yaw = self.walk.yaw();
        let sent = if force {
            live.canvas
                .submit(self.gpu, &scene, &self.bones, yaw)
                .map(|()| true)
        } else {
            live.canvas.try_submit(self.gpu, &scene, &self.bones, yaw)
        };
        match sent {
            Ok(true) => {
                self.sent_bones.clone_from(&self.bones);
                self.unsent = false;
            }
            // Over the budget: try again on a later call, with a newer
            // pose (the frame rate drops instead of the GPU share rising).
            Ok(false) => self.unsent = true,
            Err(e) => eprintln!("drawing the mesh failed: {e}"),
        }
    }

    /// Take in the frame on its way if it has arrived (`wait`: wait for
    /// it): it becomes the picture shown. True if one did.
    fn collect(&mut self, wait: bool) -> bool {
        let Some(live) = self.live.as_mut() else {
            return false;
        };
        match live.canvas.collect(self.gpu, &mut self.base, wait) {
            Ok(true) => {
                self.shown_bones.clone_from(&self.sent_bones);
                self.changed = true;
                true
            }
            Ok(false) => false,
            Err(e) => {
                eprintln!("drawing the mesh failed: {e}");
                false
            }
        }
    }

    /// Where a point given in part `part`'s rest frame (offset `at` from
    /// its origin) is now, on the canvas.
    fn project_on(&self, part: Option<usize>, at: V3) -> (f32, f32) {
        let Some(part) = part.map(|i| &self.model.parts[i]) else {
            return self.anchor();
        };
        let Some(live) = &self.live else {
            return self.anchor();
        };
        let p = self.shown_bones[part.bone].apply(part.at + at);
        live.canvas.project(p)
    }

    /// Where vertex `i` of the mesh shown is now, in the world.
    fn skinned(&self, mesh: &Mesh, i: usize) -> V3 {
        let v = &mesh.vertices[i];
        let rest = v3(v.pos[0], v.pos[1], v.pos[2]);
        let mut p = V3::ZERO;
        for (b, w) in v.bones.iter().zip(v.weights) {
            if w > 0.0 {
                p = p + self.shown_bones[*b as usize].apply(rest) * w;
            }
        }
        p
    }

    /// The hit zone under pixel `(x, y)`: of the vertices drawn within a
    /// couple of pixels of it, the one nearest the camera.
    pub fn hit(&self, x: f32, y: f32) -> Option<&'static str> {
        let live = self.live.as_ref()?;
        let (w, h) = (self.pixmap.width() as f32, self.pixmap.height() as f32);
        if !(0.0..w).contains(&x) || !(0.0..h).contains(&y) {
            return None;
        }
        let alpha = self
            .pixmap
            .pixel(x as u32, y as u32)
            .map_or(0, |p| p.alpha());
        if alpha < 128 {
            return None;
        }
        let reach = 2.5 * self.scale;
        let fwd = self.canvas_forward();
        let mut best: Option<(f32, u8)> = None;
        for i in 0..live.mesh.vertices.len() {
            let p = self.skinned(&live.mesh, i);
            let (px, py) = live.canvas.project(p);
            if (px - x).abs() > reach || (py - y).abs() > reach {
                continue;
            }
            let depth = p.dot(fwd);
            if best.is_none_or(|(d, _)| depth < d) {
                best = Some((depth, live.mesh.zones[i]));
            }
        }
        best.and_then(|(_, z)| ZONES.get(usize::from(z)).copied())
    }

    /// Into the screen, the way the camera looks.
    fn canvas_forward(&self) -> V3 {
        let (s, c) = crate::raymarch::TILT_DEG.to_radians().sin_cos();
        v3(0.0, -s, -c)
    }

    /// Where a speech bubble points: the top of the head (the middle of
    /// the canvas, top, until the bake has landed).
    pub fn anchor(&self) -> (f32, f32) {
        match &self.live {
            Some(l) => l.canvas.project(self.skinned(&l.mesh, l.mesh.crown)),
            None => (self.pixmap.width() as f32 / 2.0, 0.0),
        }
    }
}

/// Bake on a thread of its own: the fox takes most of a second.
fn spawn_bake(
    model: &Arc<Model>,
    sk: &Skeleton,
    coat: usize,
    chonk: f32,
) -> Receiver<Result<Mesh, String>> {
    let (tx, rx) = channel();
    let (model, sk) = (Arc::clone(model), sk.clone());
    std::thread::spawn(move || {
        // The receiver is gone if the pet was; nobody to tell then.
        let _ = tx.send(bake(&model, &sk, coat, chonk, model.bake_cell));
    });
    rx
}
