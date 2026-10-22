//! Draws a rig into a pixmap with tiny-skia (docs/PLAN.md §3.1).
//!
//! What keeps a frame cheap (PLAN §5):
//! - paths are rebuilt only when the body value changes, and line parts are
//!   turned into filled outlines at the same time;
//! - runs of parts on one bone (the face) are drawn once into a layer and
//!   re-blitted while that bone only moves, never turns or scales.

pub mod bubble;
mod layer;
mod pose;
pub mod text;

use chonkgene_rig::{Pose, Rig, Shape};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, Path, Pixmap, PixmapPaint, Stroke, Transform,
};

use crate::layer::Cache;
pub use pose::BoneMap;

/// Room around the design box for squash and sway, in design units:
/// left/right, then top. The feet sit on the bottom edge.
const MARGIN: (f32, f32) = (10.0, 8.0);

pub struct Renderer {
    pixmap: Pixmap,
    scale: f32,
    shape: Shape,
    /// Every part as a filled outline (strokes already expanded).
    fills: Vec<Option<Path>>,
    coat: usize,
    paints: Vec<Paint<'static>>,
    world: Vec<Transform>,
    cache: Cache,
}

impl Renderer {
    /// Pixel size of a pet drawn at `scale` (1.0 = 100%).
    pub fn pixel_size(rig: &Rig, scale: f32) -> (u32, u32) {
        let (w, h) = rig.size();
        (
            ((w + 2.0 * MARGIN.0) * scale).ceil() as u32,
            ((h + MARGIN.1) * scale).ceil() as u32,
        )
    }

    pub fn new(rig: &Rig, scale: f32, chonk: f32, coat: usize) -> Option<Self> {
        let (w, h) = Self::pixel_size(rig, scale);
        let shape = rig.shape(chonk);
        let mut r = Self {
            pixmap: Pixmap::new(w, h)?,
            scale,
            fills: outlines(rig, &shape, scale),
            shape,
            coat: usize::MAX,
            paints: Vec::new(),
            world: Vec::new(),
            cache: Cache::new(rig),
        };
        r.set_coat(rig, coat);
        Some(r)
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    /// Design space → pixmap pixels.
    pub fn view(&self) -> Transform {
        Transform::from_scale(self.scale, self.scale).pre_translate(MARGIN.0, MARGIN.1)
    }

    pub fn set_chonk(&mut self, rig: &Rig, chonk: f32) {
        if (chonk.clamp(0.0, 1.0) - self.shape.chonk).abs() > 1e-3 {
            self.shape = rig.shape(chonk);
            self.fills = outlines(rig, &self.shape, self.scale);
            self.cache.invalidate();
        }
    }

    pub fn set_coat(&mut self, rig: &Rig, coat: usize) {
        if coat == self.coat {
            return;
        }
        let Some(colors) = rig.data().coats.get(coat).map(|c| &c.colors) else {
            return;
        };
        self.paints = colors
            .iter()
            .map(|c| {
                let mut p = Paint::default();
                p.set_color_rgba8(c[0], c[1], c[2], c[3]);
                p.anti_alias = true;
                p
            })
            .collect();
        self.coat = coat;
        self.cache.invalidate();
    }

    pub fn render(&mut self, rig: &Rig, pose: &Pose) -> &Pixmap {
        self.pixmap.fill(Color::TRANSPARENT);
        rig.world_into(pose, &mut self.world);
        let view = self.view();
        self.cache.prepare(rig, view, self.scale, &mut self.world);

        let parts = &rig.data().parts;
        let mut i = 0;
        while i < parts.len() {
            if let Some(run) = self.cache.run_at(i) {
                let ts = view.pre_concat(self.world[run.bone]);
                if let Some(layer) = self.cache.layer(i, ts, rig, &self.fills, &self.paints) {
                    let (x, y) = layer.position(ts);
                    self.pixmap.draw_pixmap(
                        x,
                        y,
                        layer.pixmap(),
                        &PixmapPaint::default(),
                        Transform::identity(),
                        None,
                    );
                    i = run.end;
                    continue;
                }
            }
            let part = &parts[i];
            if let Some(path) = &self.fills[i] {
                let ts = view.pre_concat(self.world[usize::from(part.bone)]);
                self.pixmap.fill_path(
                    path,
                    &self.paints[usize::from(part.slot)],
                    FillRule::Winding,
                    ts,
                    None,
                );
            }
            i += 1;
        }
        &self.pixmap
    }

    /// The hit zone under pixel `(x, y)` of the last rendered frame.
    pub fn hit<'r>(&self, rig: &'r Rig, x: f32, y: f32) -> Option<&'r str> {
        let inv = self.view().invert()?;
        let mut p = [tiny_skia::Point::from_xy(x, y)];
        inv.map_points(&mut p);
        rig.hit(&self.shape, &self.world, (p[0].x, p[0].y))
    }
}

/// Filled outline of every part; line parts are stroked once here instead of
/// every frame.
fn outlines(rig: &Rig, shape: &Shape, scale: f32) -> Vec<Option<Path>> {
    rig.data()
        .parts
        .iter()
        .zip(&shape.paths)
        .map(|(part, path)| {
            let path = path.as_ref()?;
            match part.stroke {
                None => Some(path.clone()),
                Some(width) => path.stroke(
                    &Stroke {
                        width,
                        line_cap: LineCap::Round,
                        line_join: LineJoin::Round,
                        ..Stroke::default()
                    },
                    // Flatten finely enough for the largest scale we draw at.
                    scale * 2.0,
                ),
            }
        })
        .collect()
}
