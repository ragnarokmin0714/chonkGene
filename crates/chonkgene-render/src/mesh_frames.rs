//! Pets on all fours drawn as skinned meshes on the GPU (mesh.rs, skin.rs,
//! mesh_gpu.rs): every frame is the baked mesh in a pose worked out that
//! moment, so tails sway, bottoms wobble and turns happen in 3D without a
//! single cached picture.
//!
//! The animator's params drive the walk: stepping walks, any strong feeling
//! sits the pet down (its face shows the feeling), a poke jolts the belly,
//! a hop lifts the whole pet, glances turn the head.

use std::sync::Arc;

use chonkgene_core::anim::{Act, Expression, PoseParams};
use chonkgene_rig::math::{V3, v3};
use chonkgene_rig::{Layer, Look, Model, Pose, Scene, ZONES};
use tiny_skia::{Pixmap, Transform};

use crate::gpu::Gpu;
use crate::mesh::{Mesh, bake};
use crate::mesh_gpu::MeshCanvas;
use crate::raymarch::PPU;
use crate::skin::{Affine, Skeleton, Walk};
use crate::{key_for, sleep_overlay};

/// The canvas, world units: wider than an upright pet's, for a body lying
/// along the floor and a fox's fan of tails.
const UNITS: (f32, f32) = (4.5, 3.3);
/// Grid cells baking the mesh, world units.
const CELL: f32 = 0.025;
/// The longest step the walk takes at once, seconds: its springs stay
/// steady however slowly frames come.
const MAX_STEP: f32 = 1.0 / 60.0;

/// Pixel size of a pet on all fours at `scale` (1.0 = 100%).
pub fn pixel_size(scale: f32) -> (u32, u32) {
    (
        (UNITS.0 * PPU * scale).ceil() as u32,
        (UNITS.1 * PPU * scale).ceil() as u32,
    )
}

pub struct MeshRenderer {
    gpu: &'static Gpu,
    model: Arc<Model>,
    sk: Skeleton,
    scale: f32,
    chonk: f32,
    coat: usize,
    mesh: Mesh,
    canvas: MeshCanvas,
    pixmap: Pixmap,
    walk: Walk,
    bones: Vec<Affine>,
    /// The animator's clock at the last frame, and its belly jiggle then.
    clock: Option<f32>,
    belly: f32,
    changed: bool,
}

impl MeshRenderer {
    /// Bake `model` (a four-legged one, see [`crate::skin::quad`]) and get
    /// ready to draw it; none if it has no legs to walk on or no pixmap.
    pub fn new(
        gpu: &'static Gpu,
        model: Arc<Model>,
        scale: f32,
        chonk: f32,
        coat: usize,
    ) -> Option<Self> {
        let sk = Skeleton::new(&model);
        let walk = Walk::new(&sk, 1.0).ok()?;
        let mesh = bake(&model, &sk, coat, chonk, CELL).ok()?;
        let (w, h) = pixel_size(scale);
        let canvas = MeshCanvas::new(gpu, &mesh, w, h, scale);
        let bones = sk.pose(&[]);
        Some(Self {
            gpu,
            model,
            sk,
            scale,
            chonk,
            coat,
            mesh,
            canvas,
            pixmap: Pixmap::new(w, h)?,
            walk,
            bones,
            clock: None,
            belly: 0.0,
            changed: true,
        })
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    /// Bake again for a new body value or coat: the mesh carries both.
    fn rebake(&mut self) {
        match bake(&self.model, &self.sk, self.coat, self.chonk, CELL) {
            Ok(mesh) => {
                let (w, h) = self.canvas.size();
                self.canvas = MeshCanvas::new(self.gpu, &mesh, w, h, self.scale);
                self.mesh = mesh;
                self.changed = true;
            }
            Err(e) => eprintln!("baking the mesh failed: {e}"),
        }
    }

    pub fn set_chonk(&mut self, chonk: f32) {
        if chonk != self.chonk {
            self.chonk = chonk;
            self.rebake();
        }
    }

    pub fn set_coat(&mut self, coat: usize) {
        if coat != self.coat {
            self.coat = coat;
            self.rebake();
        }
    }

    pub fn changed(&self) -> bool {
        self.changed
    }

    pub fn render(&mut self, params: &PoseParams, expression: Expression) -> &Pixmap {
        let look = key_for(expression, params).look;
        self.draw(params, expression, look);
        &self.pixmap
    }

    pub fn render_asleep(&mut self, params: &PoseParams, phase: f32) -> &Pixmap {
        self.draw(params, Expression::Asleep, Look::Asleep);
        let nose = self.project_on(self.model.part("nose"), V3::ZERO);
        sleep_overlay(
            &mut self.pixmap,
            nose,
            Transform::identity(),
            self.scale,
            phase,
        );
        &self.pixmap
    }

    fn draw(&mut self, params: &PoseParams, expression: Expression, look: Look) {
        let walking = matches!(params.act, Act::StepA | Act::StepB);
        let facing = if params.facing < 0.0 { -1.0 } else { 1.0 };
        // Feelings sit it down; the face shows which.
        self.walk.sit(!walking && expression != Expression::Neutral);
        self.walk.look(match params.act {
            Act::LookLeft => 35.0,
            Act::LookRight => -35.0,
            _ => 0.0,
        });
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
        let scene = Scene::posed(
            &self.model,
            self.coat,
            self.chonk,
            &Pose::default(),
            look,
            Layer::All,
        );
        if let Err(e) = self.canvas.draw(
            self.gpu,
            &scene,
            &self.bones,
            self.walk.yaw(),
            &mut self.pixmap,
        ) {
            eprintln!("drawing the mesh failed: {e}");
        }
        self.changed = true;
    }

    /// Where a point given in part `part`'s rest frame (offset `at` from
    /// its origin) is now, on the canvas.
    fn project_on(&self, part: Option<usize>, at: V3) -> (f32, f32) {
        let Some(part) = part.map(|i| &self.model.parts[i]) else {
            return self.anchor();
        };
        let p = self.bones[part.bone].apply(part.at + at);
        self.canvas.project(p)
    }

    /// Where vertex `i` is now, in the world.
    fn skinned(&self, i: usize) -> V3 {
        let v = &self.mesh.vertices[i];
        let rest = v3(v.pos[0], v.pos[1], v.pos[2]);
        let mut p = V3::ZERO;
        for (b, w) in v.bones.iter().zip(v.weights) {
            if w > 0.0 {
                p = p + self.bones[*b as usize].apply(rest) * w;
            }
        }
        p
    }

    /// The hit zone under pixel `(x, y)`: of the vertices drawn within a
    /// couple of pixels of it, the one nearest the camera.
    pub fn hit(&self, x: f32, y: f32) -> Option<&'static str> {
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
        for i in 0..self.mesh.vertices.len() {
            let p = self.skinned(i);
            let (px, py) = self.canvas.project(p);
            if (px - x).abs() > reach || (py - y).abs() > reach {
                continue;
            }
            let depth = p.dot(fwd);
            if best.is_none_or(|(d, _)| depth < d) {
                best = Some((depth, self.mesh.zones[i]));
            }
        }
        best.and_then(|(_, z)| ZONES.get(usize::from(z)).copied())
    }

    /// Into the screen, the way the camera looks.
    fn canvas_forward(&self) -> V3 {
        let (s, c) = crate::raymarch::TILT_DEG.to_radians().sin_cos();
        v3(0.0, -s, -c)
    }

    /// Where a speech bubble points: the top of the head.
    pub fn anchor(&self) -> (f32, f32) {
        self.canvas.project(self.skinned(self.mesh.crown))
    }
}
