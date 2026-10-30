//! Draws chonkGene pets (docs/PLAN.md §3.1).
//!
//! Two ways to draw the same 3D pets: with a real GPU, every frame is the
//! pet ray-marched in whatever pose its animation has blended to
//! (`gpu_frames`); without one, frames are cut from a few pre-rendered
//! sprites and warped in 2D (`sprite_frames`). Pets with a four-legged
//! model are drawn instead as skinned meshes on the GPU (`mesh_frames`).
//! [`Renderer`] picks one.

pub mod bubble;
pub mod gpu;
pub mod gpu_check;
mod gpu_frames;
pub mod menu;
pub mod mesh;
mod mesh_frames;
pub mod mesh_gpu;
pub mod raymarch;
pub mod skin;
mod sprite_frames;
pub mod sprites;
pub mod text;

use std::sync::{Arc, OnceLock};

use chonkgene_core::anim::{Act, Expression, PoseParams};
use chonkgene_rig::{Look, Model, PoseId};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Pixmap, Stroke, Transform};

use crate::gpu::Gpu;
pub use crate::gpu_frames::GpuRenderer;
pub use crate::mesh_frames::MeshRenderer;
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
    pub fn changed(&self) -> bool {
        match self {
            Self::Sprites(r) => r.changed(),
            Self::Gpu(r) => r.changed(),
            Self::Mesh(r) => r.changed(),
        }
    }

    /// Should frames keep coming (30 fps) even if the animation is
    /// otherwise at rest: a blend between poses, or a GPU frame on its way?
    pub fn busy(&self) -> bool {
        match self {
            Self::Sprites(_) => false,
            Self::Gpu(r) => r.busy(),
            // Tails sway and bottoms settle all the time.
            Self::Mesh(_) => true,
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

    /// Where a speech bubble points, in pixmap pixels: the top of the head.
    pub fn anchor(&self) -> (f32, f32) {
        match self {
            Self::Sprites(r) => r.anchor(),
            Self::Gpu(r) => r.anchor(),
            Self::Mesh(r) => r.anchor(),
        }
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
    };
    let (pose, look) = match expression {
        Expression::Neutral => gesture(eyes),
        Expression::Happy => match params.act {
            Act::Rest => (PoseId::Sit, Look::Happy),
            _ => gesture(Look::Open),
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
