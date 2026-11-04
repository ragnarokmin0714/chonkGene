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

use chonkgene_core::Zone;
use chonkgene_core::anim::{
    Act, BITE_SECS, EAT_SECS, Expression, HOLD_SECS, PoseParams, Touch, gesture_secs,
};
use chonkgene_rig::math::{M3, V3, v3};
use chonkgene_rig::{FLOOR, Layer, Look, Model, Pose, Scene, ZONES};
use tiny_skia::{Pixmap, Transform};

use crate::fidget::{Kind, Tongue};
use crate::gpu::Gpu;
use crate::mesh::{Mesh, bake};
use crate::mesh_gpu::MeshCanvas;
use crate::raymarch::PPU;
use crate::skin::{Affine, Feel, Gesture, Mood, Skeleton, SleepPose, Spring, bite};
use crate::waddle::Mover;
use crate::{dirt_overlay, key_for, sleep_overlay, snack_overlay};

/// The canvas, world units: wider than an upright pet's, for a body lying
/// along the floor. A model may ask for more (`canvas` in its file: the
/// fox's fan of tails).
const UNITS: (f32, f32) = (4.5, 3.3);
/// The longest step the walk takes at once, seconds: its springs stay
/// steady however slowly frames come.
const MAX_STEP: f32 = 1.0 / 60.0;
/// Standing still doing nothing this long (seconds), a pet sits down.
const IDLE_SIT: f32 = 0.8;
/// Asleep, a pet may change how it lies after this many seconds (and up
/// to as many again).
const SLEEP_SHIFT: f32 = 45.0;
/// How the head follows a finger: degrees of turn per world unit the
/// finger is beside the head, of tilt per unit below it, and the most.
const GAZE_TURN: (f32, f32) = (28.0, 40.0);
const GAZE_TILT: (f32, f32) = (22.0, 28.0);

/// Pixel size of `model`'s picture at `scale` (1.0 = 100%).
pub fn pixel_size(model: &Model, scale: f32) -> (u32, u32) {
    let (w, h) = model.canvas.unwrap_or(UNITS);
    (
        (w * PPU * scale).ceil() as u32,
        (h * PPU * scale).ceil() as u32,
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
    /// The vertex under the finger, and the pixel it was found for.
    pressed: Option<(usize, (f32, f32))>,
    /// How deep the finger has pressed in (world units, springing back out
    /// past zero when it lets go), and the dent sent with the next frame.
    dent: Spring,
    dent_r: f32,
    press: Option<(V3, f32, f32)>,
    /// Asleep: the pose, and when (the animator's clock) it may change.
    sleep: Option<(SleepPose, f32)>,
    /// For picking sleep poses; or the one pose to sleep in (tests, looks).
    seed: u32,
    sleep_in: Option<SleepPose>,
    /// The fidget under way last frame, and one that has begun since the
    /// app last asked.
    fidget_was: Option<Kind>,
    fidget_began: Option<Kind>,
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
        let mut walk = Mover::new(&sk, 1.0, &model).ok()?;
        let seed = 0x9E37_79B9 ^ (coat as u32).wrapping_mul(0x85EB_CA6B);
        walk.set_fidgets(&model.species, 0.5, seed);
        let (w, h) = pixel_size(&model, scale);
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
            pressed: None,
            dent: Spring::default(),
            dent_r: 0.3,
            press: None,
            sleep: None,
            seed,
            sleep_in: None,
            fidget_was: None,
            fidget_began: None,
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
    /// tails sway, bellies settle and fidgets go on all the time, slowly at
    /// rest; turning, sitting down, getting up, gestures and the quick
    /// fidgets (an ear flick, a sniff) move fast and need smooth frames.
    pub fn min_fps(&self) -> u32 {
        if self.walk.busy() || self.dent.x.abs() > 0.002 || self.walk.fidget().quick() {
            30
        } else {
            15
        }
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

    /// The fidget that began since this was last asked, if any.
    pub fn take_fidget_start(&mut self) -> Option<Kind> {
        self.fidget_began.take()
    }

    /// How lively the pet is (0 … 1) and a seed of its own, for its
    /// fidgets.
    pub fn set_fidgets(&mut self, energy: f32, seed: u32) {
        self.walk.set_fidgets(&self.model.species, energy, seed);
    }

    pub fn render(&mut self, params: &PoseParams, expression: Expression) -> &Pixmap {
        let mut look = key_for(expression, params).look;
        if matches!(expression, Expression::Neutral | Expression::Happy) {
            look = touched_look(&params.touch).unwrap_or(look);
            // The tongue out (a blep, a lick, panting), unless it blinks.
            look = match (self.walk.fidget().tongue(), look) {
                (Some(Tongue::Blep), Look::Open) => Look::Blep,
                (Some(Tongue::Pant), Look::Open | Look::Happy) => Look::Pant,
                _ => look,
            };
        }
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
        let digging = (params.act == Act::Dig)
            .then_some(self.gesture)
            .flatten()
            .map(|(_, since)| (params.clock - since).max(0.0));
        if let Some(t) = digging {
            // Earth piling up under the paws, clods flying.
            self.pixmap.data_mut().copy_from_slice(self.base.data());
            if let Some(at) = self.snack_spot() {
                let side = if params.facing < 0.0 { -1.0 } else { 1.0 };
                dirt_overlay(&mut self.pixmap, at, self.scale, t, side);
            }
            self.changed = true;
        } else if let Some(t) = eating {
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
        // with a feeling on its face, it sits; some gestures are done
        // sitting (a yawn, a scratch, washing, preening: they sit it down
        // at once), the rest standing (a stretch, a wave, a snack, digging).
        let asleep = expression == Expression::Asleep;
        let standing = self.gesture.is_some_and(|(act, _)| !act.seated());
        let seated = self.gesture.is_some_and(|(act, _)| act.seated());
        if walking || standing || asleep {
            self.rest_since = None;
        } else {
            self.rest_since.get_or_insert(params.clock);
        }
        let rested = self
            .rest_since
            .is_some_and(|since| params.clock - since > IDLE_SIT);
        self.walk.lie(asleep && !walking);
        // Laughing, fed up and sulking are poses of their own (see
        // `Feel`); pleased, it sits.
        self.walk.sit(
            !walking
                && !asleep
                && !standing
                && (rested || seated || expression == Expression::Happy),
        );
        self.sleep_pose(asleep, params.clock);
        let feel = self.feel(params, expression);
        self.walk.feel(feel);
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
        self.walk.breathe(params.breath);
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
        let now = self.walk.fidget().run.map(|r| r.kind);
        if now.is_some() && now != self.fidget_was {
            self.fidget_began = now;
        }
        self.fidget_was = now;
        self.bones = self.sk.pose(&joints);
        self.settle(params);
        self.dent(params, dt);
        self.look = look;
        self.changed = false;
        // A walking pet's picture must keep up with its window, which moves
        // every frame (slowing to a stop too): its frames skip the budget,
        // and if the last one is still on its way it is waited for rather
        // than skipped (a skipped frame shows the legs standing still while
        // the window slides on).
        let keep_up = walking || self.walk.speed() > 0.0;
        let mut arrived = self.collect(false);
        let pending = |r: &Self| r.live.as_ref().is_some_and(|l| l.canvas.pending());
        if keep_up && pending(self) {
            arrived |= self.collect(true);
        }
        if self.live.is_some() && !pending(self) {
            self.send(keep_up);
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
        live.canvas.press = self.press;
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
        let i = self.nearest(x, y)?;
        ZONES.get(usize::from(live.mesh.zones[i])).copied()
    }

    /// The vertex drawn nearest the camera within a couple of pixels of
    /// pixel `(x, y)` of the picture shown, if the pet is there.
    fn nearest(&self, x: f32, y: f32) -> Option<usize> {
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
        let mut best: Option<(f32, usize)> = None;
        for i in 0..live.mesh.vertices.len() {
            let p = self.skinned(&live.mesh, i);
            let (px, py) = live.canvas.project(p);
            if (px - x).abs() > reach || (py - y).abs() > reach {
                continue;
            }
            let depth = p.dot(fwd);
            if best.is_none_or(|(d, _)| depth < d) {
                best = Some((depth, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// How the pet feels and is touched, for the walk: the face's feeling
    /// as a pose, and where the finger is (it turns to you and looks).
    fn feel(&mut self, params: &PoseParams, expression: Expression) -> Feel {
        let mood = match expression {
            Expression::Giggle => Mood::Laugh,
            Expression::FedUp => Mood::FedUp,
            Expression::Sulk => Mood::Sulk,
            Expression::Happy => Mood::Content,
            Expression::Neutral | Expression::Asleep => Mood::Calm,
        };
        let touch = &params.touch;
        // The vertex under the finger, found again when the finger moves.
        match touch.at() {
            Some(at) if self.pressed.is_none_or(|(_, was)| was != at) => {
                if let Some(i) = self.nearest(at.0, at.1) {
                    self.pressed = Some((i, at));
                }
            }
            _ => {}
        }
        let bone = self.pressed.and_then(|(i, _)| {
            let live = self.live.as_ref()?;
            Some(live.mesh.vertices[i].bones[0] as usize)
        });
        // Asleep or sulking it does not turn to you.
        let gaze = (touch.attending() && mood != Mood::Sulk && expression != Expression::Asleep)
            .then(|| touch.at())
            .flatten()
            .map(|at| {
                let head = self.project_on(self.model.part("head"), V3::ZERO);
                let unit = PPU * self.scale;
                let (dx, dy) = ((at.0 - head.0) / unit, (at.1 - head.1) / unit);
                (
                    (dx * GAZE_TURN.0).clamp(-GAZE_TURN.1, GAZE_TURN.1),
                    (dy * GAZE_TILT.0).clamp(-GAZE_TILT.1, GAZE_TILT.1),
                )
            });
        Feel {
            mood,
            gaze,
            held: touch.held.map(|(z, _, secs)| (z, bone, secs)),
            poked: touch.last.map(|(z, _, ago)| (z, bone, ago)),
        }
    }

    /// How deep a finger has the flesh pressed in now, world units
    /// (negative springing back out; 0 when nothing presses).
    pub fn pressed_in(&self) -> f32 {
        self.press.map_or(0.0, |(_, depth, _)| depth)
    }

    /// Always sleep in `pose` (tests, looking at the poses); none: pick as
    /// the species likes.
    pub fn sleep_in(&mut self, pose: Option<SleepPose>) {
        self.sleep_in = pose;
        self.sleep = None;
    }

    /// Asleep, pick how to lie (by the species' liking), and now and then
    /// another; awake, forget it.
    fn sleep_pose(&mut self, asleep: bool, clock: f32) {
        if !asleep {
            self.sleep = None;
            return;
        }
        if let Some(pose) = self.sleep_in {
            self.walk.sleep_pose(pose);
            return;
        }
        let due = self.sleep.is_none_or(|(_, until)| clock >= until);
        if !due {
            return;
        }
        let weights = SleepPose::weights(&self.model.species);
        let mut roll = |n: u32| {
            // xorshift
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 17;
            self.seed ^= self.seed << 5;
            self.seed % n.max(1)
        };
        let total: u32 = weights.iter().sum();
        let mut pick = roll(total);
        let mut pose = SleepPose::Chin;
        for (k, &w) in weights.iter().enumerate() {
            if pick < w {
                pose = SleepPose::ALL[k];
                break;
            }
            pick -= w;
        }
        let wait = SLEEP_SHIFT * (1.0 + roll(1000) as f32 / 1000.0);
        self.sleep = Some((pose, clock + wait));
        self.walk.sleep_pose(pose);
    }

    /// The whole body's squash (a finger pressing, a landing, a giggle,
    /// melting) and breathing, its lean (held swinging, dizzy), and, rolled
    /// over or melted, settling it onto the floor: one move after the
    /// skeleton's, for every bone.
    fn settle(&mut self, params: &PoseParams) {
        let sq =
            (0.35 * params.squash + 0.15 * params.belly + self.walk.flatten()).clamp(-0.3, 0.3);
        let breath = 0.02 * params.breath;
        let wide = 1.0 + 0.5 * sq;
        let feet = v3(0.0, FLOOR, crate::skin::ROOT_PIVOT.z);
        let squash = Affine::about(M3::diag(v3(wide, (1.0 - sq) * (1.0 + breath), wide)), feet);
        let lean = Affine::about(M3::rot_z(params.sway.to_radians()), feet);
        let mut world = lean.then(&squash);
        let g = self.walk.ground();
        if g > 0.0
            && let Some(live) = &self.live
        {
            // The lowest of the skin (a sample of it) comes to the floor,
            // and the pet to the middle of its picture (rolling over about
            // its length carries it off to one side).
            let (mut low, mut left, mut right) = (f32::MAX, f32::MAX, f32::MIN);
            for v in live.mesh.vertices.iter().step_by(5) {
                let rest = v3(v.pos[0], v.pos[1], v.pos[2]);
                let mut p = V3::ZERO;
                for (b, w) in v.bones.iter().zip(v.weights) {
                    if w > 0.0 {
                        p = p + self.bones[*b as usize].apply(rest) * w;
                    }
                }
                let p = world.apply(p);
                low = low.min(p.y);
                (left, right) = (left.min(p.x), right.max(p.x));
            }
            if low < f32::MAX {
                world.t.y += (FLOOR - low) * g;
                world.t.x -= 0.5 * (left + right) * g;
            }
        }
        for b in &mut self.bones {
            *b = world.then(b);
        }
    }

    /// A finger pressing in: the dent deepens while it is down (chubbier
    /// pets give more) and springs back past flat when it lets go, so the
    /// flesh wobbles out and settles.
    fn dent(&mut self, params: &PoseParams, dt: f32) {
        let (goal, radius) = match params.touch.held {
            Some((zone, ..)) => {
                let (d, r) = dent_for(zone);
                (d * (0.8 + 0.4 * self.chonk), r)
            }
            None => (0.0, self.dent_r),
        };
        self.dent_r = radius;
        let mut x = 0.0;
        let steps = (dt / MAX_STEP).ceil().max(1.0);
        for _ in 0..steps as usize {
            x = self.dent.step(goal, 0.0, 5.5, 0.3, dt / steps);
        }
        let resting = x.abs() < 0.002 && self.dent.v.abs() < 0.02;
        if resting && params.touch.held.is_none() {
            self.dent = Spring::default();
            self.press = None;
            if params.touch.last.is_none() {
                self.pressed = None;
            }
            return;
        }
        self.press = self.pressed.and_then(|(i, _)| {
            let live = self.live.as_ref()?;
            let v = &live.mesh.vertices[i];
            let rest = v3(v.pos[0], v.pos[1], v.pos[2]);
            let mut p = V3::ZERO;
            for (b, w) in v.bones.iter().zip(v.weights) {
                if w > 0.0 {
                    p = p + self.bones[*b as usize].apply(rest) * w;
                }
            }
            Some((p, x, radius))
        });
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

/// How deep a finger presses in on each zone (world units) and how wide
/// the dent is: the belly and bottom are the softest.
fn dent_for(zone: Zone) -> (f32, f32) {
    match zone {
        Zone::Belly => (0.13, 0.32),
        Zone::Bottom => (0.11, 0.3),
        Zone::Cheek => (0.09, 0.2),
        Zone::Chin => (0.07, 0.2),
        Zone::Head => (0.06, 0.24),
        Zone::Paw => (0.05, 0.16),
        Zone::Tail => (0.06, 0.14),
    }
}

/// The face a touch puts on: content ^ ^ for a hand held on its head, chin
/// or cheek (or a long belly rub), a squint for a poke on the head.
fn touched_look(touch: &Touch) -> Option<Look> {
    if let Some((zone, _, secs)) = touch.held {
        let nice = match zone {
            Zone::Head | Zone::Chin | Zone::Cheek => secs > 0.25,
            Zone::Belly => secs > HOLD_SECS,
            _ => false,
        };
        return nice.then_some(Look::Happy);
    }
    match touch.last {
        Some((Zone::Head, _, ago)) if ago < 0.35 => Some(Look::Shut),
        _ => None,
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
