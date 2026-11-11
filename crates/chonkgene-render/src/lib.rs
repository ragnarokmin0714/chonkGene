//! Draws chonkGene pets (docs/PLAN.md §3.1).
//!
//! Two ways to draw the same 3D pets: with a real GPU, every frame is the
//! pet ray-marched in whatever pose its animation has blended to
//! (`gpu_frames`); without one, frames are cut from a few pre-rendered
//! sprites and warped in 2D (`sprite_frames`). Pets with a four-legged
//! model are drawn instead as skinned meshes on the GPU (`mesh_frames`).
//! [`Renderer`] picks one.

pub mod bubble;
pub mod cursor;
pub mod fidget;
pub mod gpu;
pub mod gpu_check;
mod gpu_frames;
pub mod menu;
pub mod mesh;
mod mesh_frames;
pub mod mesh_gpu;
pub mod props;
pub mod raymarch;
pub mod skin;
mod sprite_frames;
pub mod sprites;
pub mod text;
pub mod toy;
pub mod waddle;

use std::sync::{Arc, OnceLock};

use chonkgene_core::anim::{Act, Expression, PoseParams};
use chonkgene_rig::{Look, Model, PoseId};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Pixmap, Stroke, Transform};

use crate::gpu::Gpu;
pub use crate::gpu_frames::GpuRenderer;
pub use crate::mesh_frames::{FrameStats, MeshRenderer, pixel_size as mesh_pixel_size};
pub use crate::sprite_frames::{SpriteRenderer, frame_transform};
pub use crate::sprites::{KEYS, Key, Variant};

/// The shared GPU, if real-time frames are worth it here: a real GPU (an
/// integrated one counts), not the CPU pretending to be one.
/// `CHONKGENE_RENDERER=sprites` or `=gpu` overrides the choice (`gpu`
/// also takes a software GPU, for testing).
pub fn shared_gpu() -> Option<&'static Gpu> {
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    GPU.get_or_init(|| {
        let choice = std::env::var("CHONKGENE_RENDERER").unwrap_or_default();
        if choice == "sprites" {
            return None;
        }
        match Gpu::new() {
            Ok(gpu) if gpu.is_software() && choice != "gpu" => {
                eprintln!("{} is a software GPU: drawing from sprites", gpu.name());
                None
            }
            Ok(gpu) => {
                eprintln!("drawing on {}", gpu.name());
                Some(gpu)
            }
            Err(e) => {
                eprintln!("{e}: drawing from sprites");
                None
            }
        }
    })
    .as_ref()
}

/// Draws one pet, on the GPU when there is a real one.
pub enum Renderer {
    Sprites(Box<SpriteRenderer>),
    Gpu(Box<GpuRenderer>),
    /// On all fours, as a skinned mesh (a species with a four-legged
    /// model, on a real GPU).
    Mesh(Box<MeshRenderer>),
}

impl Renderer {
    /// Pixel size of an upright pet drawn at `scale` (1.0 = 100%); one on
    /// all fours is wider (see [`Renderer::size`]).
    pub fn pixel_size(scale: f32) -> (u32, u32) {
        raymarch::pixel_size(scale)
    }

    /// The best renderer this machine has: on a real GPU, the pet on all
    /// fours if it has a four-legged model (`quad`), else ray-marched;
    /// without one, sprites of `model`.
    pub fn new(
        model: Arc<Model>,
        quad: Option<Arc<Model>>,
        scale: f32,
        chonk: f32,
        coat: usize,
    ) -> Option<Self> {
        match (shared_gpu(), quad) {
            (Some(gpu), Some(quad)) => MeshRenderer::new(gpu, quad, scale, chonk, coat)
                .map(|r| Self::Mesh(Box::new(r)))
                .or_else(|| Self::gpu(gpu, model, scale, chonk, coat)),
            (Some(gpu), None) => Self::gpu(gpu, model, scale, chonk, coat),
            (None, _) => Self::sprites(model, scale, chonk, coat),
        }
    }

    /// The pet's window size, pixels.
    pub fn size(&self) -> (u32, u32) {
        let pm = self.pixmap();
        (pm.width(), pm.height())
    }

    /// Frames cut from pre-rendered sprites; they render in the background
    /// and frames are empty until the first lands.
    pub fn sprites(model: Arc<Model>, scale: f32, chonk: f32, coat: usize) -> Option<Self> {
        SpriteRenderer::new(model, scale, chonk, coat).map(|r| Self::Sprites(Box::new(r)))
    }

    /// Every frame ray-marched on `gpu`.
    pub fn gpu(
        gpu: &'static Gpu,
        model: Arc<Model>,
        scale: f32,
        chonk: f32,
        coat: usize,
    ) -> Option<Self> {
        GpuRenderer::new(gpu, model, scale, chonk, coat).map(|r| Self::Gpu(Box::new(r)))
    }

    pub fn is_gpu(&self) -> bool {
        matches!(self, Self::Gpu(_) | Self::Mesh(_))
    }

    pub fn pixmap(&self) -> &Pixmap {
        match self {
            Self::Sprites(r) => r.pixmap(),
            Self::Gpu(r) => r.pixmap(),
            Self::Mesh(r) => r.pixmap(),
        }
    }

    pub fn set_chonk(&mut self, chonk: f32) {
        match self {
            Self::Sprites(r) => r.set_chonk(chonk),
            Self::Gpu(r) => r.set_chonk(chonk),
            Self::Mesh(r) => r.set_chonk(chonk),
        }
    }

    /// The fidget that began since this was last asked, if any (a mesh
    /// pet's: the app gives some a sound).
    pub fn take_fidget_start(&mut self) -> Option<fidget::Kind> {
        match self {
            Self::Mesh(r) => r.take_fidget_start(),
            _ => None,
        }
    }

    /// How lively the pet is (0 … 1) and a seed of its own: a mesh pet's
    /// fidgets (PLAN §8 "Roger's seventh pass") go by them.
    pub fn set_fidgets(&mut self, energy: f32, seed: u32) {
        if let Self::Mesh(r) = self {
            r.set_fidgets(energy, seed);
        }
    }

    pub fn set_coat(&mut self, coat: usize) {
        match self {
            Self::Sprites(r) => r.set_coat(coat),
            Self::Gpu(r) => r.set_coat(coat),
            Self::Mesh(r) => r.set_coat(coat),
        }
    }

    /// Take in sprites finished in the background. True if any arrived, so
    /// the caller can redraw.
    pub fn poll(&mut self) -> bool {
        match self {
            Self::Sprites(r) => r.poll(),
            Self::Gpu(_) => false,
            Self::Mesh(r) => r.poll(),
        }
    }

    /// Wait for every sprite, or for the GPU frame on its way (tests,
    /// benchmarks, screenshots).
    pub fn finish(&mut self) {
        match self {
            Self::Sprites(r) => r.finish(),
            Self::Gpu(r) => r.finish(),
            Self::Mesh(r) => r.finish(),
        }
    }

    /// Can the pet be seen yet?
    pub fn ready(&self) -> bool {
        match self {
            Self::Sprites(r) => r.ready(),
            Self::Gpu(_) => true,
            Self::Mesh(r) => r.ready(),
        }
    }

    /// Did the last `render` change the picture? When it did not, there is
    /// nothing to show again (PLAN §5: no upload when the pose is unchanged).
    /// How its frames went since this was last asked (mesh pets).
    pub fn take_stats(&mut self) -> Option<FrameStats> {
        match self {
            Self::Mesh(r) => Some(r.take_stats()),
            _ => None,
        }
    }

    /// Why the next picture has not come yet, for the log (mesh pets).
    pub fn waiting(&self) -> String {
        match self {
            Self::Mesh(r) => r.waiting(),
            _ => "no".into(),
        }
    }

    pub fn changed(&self) -> bool {
        match self {
            Self::Sprites(r) => r.changed(),
            Self::Gpu(r) => r.changed(),
            Self::Mesh(r) => r.changed(),
        }
    }

    /// Should frames keep coming even if the animation is otherwise at
    /// rest: a blend between poses, or a GPU frame on its way? (Mesh pets
    /// always: tails sway and bottoms settle all the time.)
    pub fn busy(&self) -> bool {
        match self {
            Self::Sprites(_) => false,
            Self::Gpu(r) => r.busy(),
            Self::Mesh(_) => true,
        }
    }

    /// Frames a second the pet needs even if its animation is otherwise
    /// at rest: 30 for a blend between poses or a GPU frame on its way;
    /// a mesh pet's tails sway and bottoms settle all the time.
    pub fn min_fps(&self) -> u32 {
        match self {
            Self::Sprites(_) => 0,
            Self::Gpu(r) => {
                if r.busy() {
                    30
                } else {
                    0
                }
            }
            Self::Mesh(r) => r.min_fps(),
        }
    }

    pub fn render(&mut self, params: &PoseParams, expression: Expression) -> &Pixmap {
        match self {
            Self::Sprites(r) => r.render(params, expression),
            Self::Gpu(r) => r.render(params, expression),
            Self::Mesh(r) => r.render(params, expression),
        }
    }

    /// Render a sleeping pet: the frame plus a snot bubble that swells with
    /// each breath and a drifting 「z」. `phase` is seconds asleep.
    pub fn render_asleep(&mut self, params: &PoseParams, phase: f32) -> &Pixmap {
        match self {
            Self::Sprites(r) => r.render_asleep(params, phase),
            Self::Gpu(r) => r.render_asleep(params, phase),
            Self::Mesh(r) => r.render_asleep(params, phase),
        }
    }

    /// The hit zone under pixel `(x, y)` of the last frame.
    pub fn hit(&self, x: f32, y: f32) -> Option<&'static str> {
        match self {
            Self::Sprites(r) => r.hit(x, y),
            Self::Gpu(r) => r.hit(x, y),
            Self::Mesh(r) => r.hit(x, y),
        }
    }

    /// Where something carried in the mouth goes, in pixmap pixels: under
    /// the nose of a mesh pet; for the rest, a guess from the top of the
    /// head and the way it faces.
    pub fn mouth(&self, facing: f32) -> (f32, f32) {
        if let Self::Mesh(r) = self
            && let Some(at) = r.mouth()
        {
            return at;
        }
        let (x, y) = self.anchor();
        let (w, h) = self.size();
        (x + facing.signum() * 0.16 * w as f32, y + 0.32 * h as f32)
    }

    /// How far the head is tipped over, radians (+ clockwise on the
    /// screen); 0 where frames are pictures cut beforehand.
    pub fn head_roll(&self) -> f32 {
        match self {
            Self::Mesh(r) => r.head_roll(),
            _ => 0.0,
        }
    }

    /// The top of the head, in pixmap pixels.
    pub fn anchor(&self) -> (f32, f32) {
        match self {
            Self::Sprites(r) => r.anchor(),
            Self::Gpu(r) => r.anchor(),
            Self::Mesh(r) => r.anchor(),
        }
    }

    /// Where a speech bubble points, in pixmap pixels: over the top of the
    /// head, or over the top of the whole picture where that is higher
    /// (rolled over or lying, the head is low down and the bubble would
    /// cover the pet).
    pub fn bubble_at(&self) -> (f32, f32) {
        let (x, y) = self.anchor();
        (x, y.min(top_row(self.pixmap())))
    }
}

/// The sprite for an expression and what the body is doing. Strong
/// feelings take over the whole body; otherwise steps and gestures do.
pub fn key_for(expression: Expression, params: &PoseParams) -> Key {
    let eyes = if params.eyes_open > 0.66 {
        Look::Open
    } else if params.eyes_open > 0.25 {
        Look::Half
    } else {
        Look::Shut
    };
    let gesture = |eyes| match params.act {
        Act::Rest => (PoseId::Sit, eyes),
        Act::StepA => (PoseId::WalkA, eyes),
        Act::StepB => (PoseId::WalkB, eyes),
        Act::LookLeft => (PoseId::LookLeft, eyes),
        Act::LookRight => (PoseId::LookRight, eyes),
        Act::Stretch => (PoseId::Stretch, Look::Happy),
        Act::Yawn => (PoseId::Yawn, Look::Yawn),
        Act::Wave => (PoseId::Wave, Look::Happy),
        // Sprites have no eating pose: a happy sit (meshes bite and chew).
        Act::Eat => (PoseId::Sit, Look::Happy),
        // Nor poses for the little things done at rest: the nearest one
        // (the meshes do them properly).
        Act::Dig | Act::Peck => (PoseId::Stretch, Look::Happy),
        Act::Scratch | Act::Groom => (PoseId::Sit, Look::Happy),
        Act::Shake => (PoseId::Sit, Look::Shut),
        Act::Preen => (PoseId::LookLeft, Look::Happy),
        Act::Flap => (PoseId::Wave, Look::Happy),
        Act::Wiggle | Act::Bounce | Act::Spin | Act::Dance => (PoseId::Stretch, Look::Happy),
        Act::Knead | Act::Tap | Act::Chew => (PoseId::Sit, Look::Happy),
        Act::Paw => (PoseId::Wave, eyes),
        Act::Tilt => (PoseId::LookLeft, eyes),
        Act::Sniff | Act::Rub | Act::Nudge => (PoseId::Stretch, eyes),
        Act::Bliss => (PoseId::BellyUp, Look::Happy),
        Act::Swish => (PoseId::LookLeft, Look::Happy),
    };
    let (pose, look) = match expression {
        Expression::Neutral => gesture(eyes),
        // Smiling ^ ^ at rest; open-eyed in a gesture, blinking too.
        Expression::Happy => match params.act {
            Act::Rest => (PoseId::Sit, Look::Happy),
            _ => gesture(if params.blink { Look::Shut } else { Look::Open }),
        },
        Expression::Giggle => (PoseId::BellyUp, Look::Laugh),
        Expression::FedUp => (
            PoseId::Melt,
            if eyes == Look::Shut {
                Look::Shut
            } else {
                Look::FedUp
            },
        ),
        Expression::Sulk => (PoseId::Sulk, Look::Pout),
        Expression::Asleep => (PoseId::Loaf, Look::Asleep),
    };
    Key { pose, look }
}

/// Draw the snot bubble beside the nose and a drifting 「z」 over a sleeping
/// pet. `nose` is mapped into `pm` by `ts`; `s` is the pet's scale.
/// A round cookie on the floor at `at` (under its middle, pixels), `eaten` of it
/// gone (0 … 1): it shrinks and loses a bite out of its top with each bite.
fn snack_overlay(pm: &mut Pixmap, at: (f32, f32), s: f32, eaten: f32) {
    let r = (9.0 * s * (1.0 - 0.7 * eaten.clamp(0.0, 1.0))).max(0.5);
    // Resting on the floor: the rim's bottom on `at`.
    let (x, y) = (at.0, at.1 - 1.25 * r);
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    let mut fill = |pm: &mut Pixmap, cx: f32, cy: f32, rr: f32, c: [u8; 4]| {
        if let Some(p) = tiny_skia::PathBuilder::from_circle(cx, cy, rr) {
            paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
            pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
    };
    // A flattened disc: a darker rim under a golden top.
    fill(pm, x, y + 0.25 * r, r, [0xC9, 0x84, 0x4A, 255]);
    fill(pm, x, y, r, [0xEC, 0xB0, 0x6A, 255]);
    for (dx, dy) in [(-0.35, -0.2), (0.3, 0.1), (-0.05, 0.4)] {
        fill(
            pm,
            x + dx * r,
            y + dy * r,
            0.16 * r,
            [0x6B, 0x41, 0x2C, 255],
        );
    }
    if eaten > 0.05 {
        // The bite: a scoop out of the top edge, cleared to transparent.
        if let Some(p) = tiny_skia::PathBuilder::from_circle(x + 0.45 * r, y - 0.75 * r, 0.55 * r) {
            let clear = Paint {
                anti_alias: true,
                blend_mode: tiny_skia::BlendMode::Clear,
                ..Paint::default()
            };
            pm.fill_path(&p, &clear, FillRule::Winding, Transform::identity(), None);
        }
    }
}

/// A hole being dug at `at` (pixels, on the floor), `t` seconds in: a
/// little mound of earth growing beside it, and clods flicked out behind
/// the pet (to `-side` on screen, the way it faces being `side`).
fn dirt_overlay(pm: &mut Pixmap, at: (f32, f32), s: f32, t: f32, side: f32) {
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    let grow = (t / 1.5).clamp(0.0, 1.0);
    // The mound: a squat dome of earth, a darker hollow in front of it.
    let (x, y) = (at.0 - side * 4.0 * s, at.1);
    let r = (4.0 + 5.0 * grow) * s;
    if let Some(p) = tiny_skia::Rect::from_xywh(x - r, y - 0.55 * r, 2.0 * r, 1.1 * r)
        .and_then(tiny_skia::PathBuilder::from_oval)
    {
        paint.set_color_rgba8(0x9A, 0x6B, 0x45, 255);
        pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
    }
    if let Some(p) = tiny_skia::Rect::from_xywh(x - 0.6 * r, y - 0.5 * r, 1.2 * r, 0.45 * r)
        .and_then(tiny_skia::PathBuilder::from_oval)
    {
        paint.set_color_rgba8(0xB8, 0x86, 0x5B, 255);
        pm.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
    }
    // Clods: each flies out in an arc, a third of a second apart.
    for k in 0..3 {
        let p = (t * 2.6 + k as f32 / 3.0).fract();
        let cx = x - side * (6.0 + 26.0 * p) * s;
        let cy = y - (22.0 * p * (1.0 - p) * 4.0 - 3.0 * p) * s;
        if let Some(c) = tiny_skia::PathBuilder::from_circle(cx, cy, (1.8 - 0.6 * p) * s) {
            paint.set_color_rgba8(0x8A, 0x5E, 0x3B, (255.0 * (1.0 - p * p)) as u8);
            pm.fill_path(&c, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
}

fn sleep_overlay(pm: &mut Pixmap, nose: (f32, f32), ts: Transform, s: f32, phase: f32) {
    let (nx, ny) = nose;
    let tau = std::f32::consts::TAU;

    // Snot bubble beside the nose, swelling with the slow sleeping breath.
    let swell = 0.5 + 0.5 * (phase * tau / 5.1).sin();
    if let Some(bubble) =
        tiny_skia::PathBuilder::from_circle(nx + 3.0 * s * swell, ny, (2.5 + 5.5 * swell) * s)
    {
        let mut fill = Paint::default();
        fill.set_color_rgba8(0xD6, 0xEE, 0xFA, 220);
        fill.anti_alias = true;
        pm.fill_path(&bubble, &fill, FillRule::Winding, ts, None);
        let mut edge = fill.clone();
        edge.set_color_rgba8(0xA9, 0xD6, 0xEE, 255);
        let stroke = Stroke {
            width: 1.2 * s,
            ..Stroke::default()
        };
        pm.stroke_path(&bubble, &edge, &stroke, ts, None);
    }

    // A 「z」 drifting up and fading every 2.4 s.
    let cycle = (phase / 2.4).fract();
    // Placed in sprite space but drawn upright, so a mirrored pet's z
    // still reads as a z.
    let mut at = [tiny_skia::Point::from_xy(
        nx + (24.0 + 6.0 * cycle) * s,
        ny - (48.0 + 14.0 * cycle) * s,
    )];
    ts.map_points(&mut at);
    let z = 8.0 * s;
    let (zx, zy) = (at[0].x - z / 2.0, at[0].y);
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(zx, zy);
    pb.line_to(zx + z, zy);
    pb.line_to(zx, zy + z);
    pb.line_to(zx + z, zy + z);
    if let Some(path) = pb.finish() {
        let mut ink = Paint::default();
        let alpha = (255.0 * (1.0 - cycle) * (cycle * 4.0).min(1.0)) as u8;
        ink.set_color_rgba8(0x8E, 0x7C, 0x6E, alpha);
        ink.anti_alias = true;
        let stroke = Stroke {
            width: 2.4 * s,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        pm.stroke_path(&path, &ink, &stroke, Transform::identity(), None);
    }
}

/// The first row from the top with anything drawn on it (the height if
/// none).
fn top_row(pm: &Pixmap) -> f32 {
    let w = pm.width() as usize;
    pm.pixels()
        .chunks(w.max(1))
        .position(|row| row.iter().any(|p| p.alpha() > 0))
        .unwrap_or(pm.height() as usize) as f32
}
