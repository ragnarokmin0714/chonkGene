//! Layer cache for runs of parts that share a bone (the face).
//!
//! A run is drawn into its own pixmap once and blitted while its bone keeps
//! the same rotation and scale; only translation may change. To keep the
//! blit crisp, that bone and its children are snapped to whole pixels on
//! frames where the cache is used, so the eyes stay put relative to the face.
//!
//! A bone whose shape changes every frame (the breathing body) would only
//! waste work, so a layer is built only once the same rotation and scale has
//! been seen on two frames in a row.

use chonkgene_rig::Rig;
use tiny_skia::{FillRule, Paint, Path, Pixmap, PixmapRef, Rect, Transform};

/// Runs shorter than this are drawn directly.
const MIN_RUN: usize = 3;
/// Rotation/scale entries closer than this count as unchanged.
const EPSILON: f32 = 1e-4;

#[derive(Clone, Copy, Debug)]
pub struct Run {
    pub start: usize,
    pub end: usize,
    pub bone: usize,
}

pub struct Layer {
    pixmap: Pixmap,
    /// Top-left of the layer relative to the bone's translation, in pixels.
    origin: (f32, f32),
}

impl Layer {
    pub fn pixmap(&self) -> PixmapRef<'_> {
        self.pixmap.as_ref()
    }

    pub fn position(&self, ts: Transform) -> (i32, i32) {
        (
            (ts.tx + self.origin.0).round() as i32,
            (ts.ty + self.origin.1).round() as i32,
        )
    }
}

#[derive(Clone, Copy, Default)]
struct BoneState {
    key: Option<[f32; 4]>,
    seen: Option<[f32; 4]>,
    active: bool,
}

pub struct Cache {
    runs: Vec<Run>,
    run_at: Vec<Option<usize>>,
    layers: Vec<Option<Layer>>,
    bones: Vec<BoneState>,
    /// Bones that own a run, each with its descendants.
    families: Vec<(usize, Vec<usize>)>,
}

fn linear(ts: Transform) -> [f32; 4] {
    [ts.sx, ts.kx, ts.ky, ts.sy]
}

fn same(a: Option<[f32; 4]>, b: [f32; 4]) -> bool {
    a.is_some_and(|a| a.iter().zip(b).all(|(x, y)| (x - y).abs() < EPSILON))
}

impl Cache {
    pub fn new(rig: &Rig) -> Self {
        let parts = &rig.data().parts;
        let mut runs = Vec::new();
        let mut run_at = vec![None; parts.len()];
        let mut start = 0;
        while start < parts.len() {
            let bone = parts[start].bone;
            let end = (start..parts.len())
                .find(|&i| parts[i].bone != bone)
                .unwrap_or(parts.len());
            if end - start >= MIN_RUN {
                run_at[start] = Some(runs.len());
                runs.push(Run {
                    start,
                    end,
                    bone: usize::from(bone),
                });
            }
            start = end;
        }

        let bones = &rig.data().bones;
        let mut owners: Vec<usize> = runs.iter().map(|r| r.bone).collect();
        owners.sort_unstable();
        owners.dedup();
        let families = owners
            .into_iter()
            .map(|b| {
                // Parents come first, so one pass finds every descendant.
                let mut family = vec![b];
                for (i, bone) in bones.iter().enumerate().skip(b + 1) {
                    if bone
                        .parent
                        .is_some_and(|p| family.contains(&usize::from(p)))
                    {
                        family.push(i);
                    }
                }
                (b, family)
            })
            .collect();

        Self {
            layers: runs.iter().map(|_| None).collect(),
            runs,
            run_at,
            bones: vec![BoneState::default(); bones.len()],
            families,
        }
    }

    pub fn invalidate(&mut self) {
        self.layers.iter_mut().for_each(|l| *l = None);
        self.bones
            .iter_mut()
            .for_each(|b| *b = BoneState::default());
    }

    pub fn run_at(&self, part: usize) -> Option<Run> {
        self.run_at[part].map(|r| self.runs[r])
    }

    /// Decide per bone whether its layers are used this frame, and snap the
    /// bones that use them to whole pixels.
    pub fn prepare(&mut self, _rig: &Rig, view: Transform, scale: f32, world: &mut [Transform]) {
        for (bone, family) in &self.families {
            let ts = view.pre_concat(world[*bone]);
            let lin = linear(ts);
            let state = &mut self.bones[*bone];
            if same(state.key, lin) {
                state.active = true;
            } else if same(state.seen, lin) {
                state.key = Some(lin);
                state.active = true;
                for (layer, run) in self.layers.iter_mut().zip(&self.runs) {
                    if run.bone == *bone {
                        *layer = None;
                    }
                }
            } else {
                state.seen = Some(lin);
                state.key = None;
                state.active = false;
            }
            if state.active {
                let shift = Transform::from_translate(
                    (ts.tx.round() - ts.tx) / scale,
                    (ts.ty.round() - ts.ty) / scale,
                );
                for &b in family {
                    world[b] = shift.pre_concat(world[b]);
                }
            }
        }
    }

    /// The layer for the run starting at `part`, built if needed; `None`
    /// means draw the parts directly this frame.
    pub fn layer(
        &mut self,
        part: usize,
        ts: Transform,
        rig: &Rig,
        fills: &[Option<Path>],
        paints: &[Paint<'static>],
    ) -> Option<&Layer> {
        let r = self.run_at[part]?;
        let run = self.runs[r];
        if !self.bones[run.bone].active {
            return None;
        }
        if self.layers[r].is_none() {
            self.layers[r] = build(run, ts, rig, fills, paints);
        }
        self.layers[r].as_ref()
    }
}

fn build(
    run: Run,
    ts: Transform,
    rig: &Rig,
    fills: &[Option<Path>],
    paints: &[Paint<'static>],
) -> Option<Layer> {
    let lin = Transform::from_row(ts.sx, ts.ky, ts.kx, ts.sy, 0.0, 0.0);
    let mut bounds: Option<Rect> = None;
    for path in fills[run.start..run.end].iter().flatten() {
        let b = path.clone().transform(lin)?.bounds();
        bounds = Some(match bounds {
            Some(a) => Rect::from_ltrb(
                a.left().min(b.left()),
                a.top().min(b.top()),
                a.right().max(b.right()),
                a.bottom().max(b.bottom()),
            )?,
            None => b,
        });
    }
    let b = bounds?;
    // One pixel of room for anti-aliasing.
    let (x0, y0) = (b.left().floor() - 1.0, b.top().floor() - 1.0);
    let w = (b.right().ceil() + 1.0 - x0) as u32;
    let h = (b.bottom().ceil() + 1.0 - y0) as u32;
    let mut pixmap = Pixmap::new(w, h)?;
    let local = lin.post_translate(-x0, -y0);
    for (part, path) in rig.data().parts[run.start..run.end]
        .iter()
        .zip(&fills[run.start..run.end])
    {
        if let Some(path) = path {
            pixmap.fill_path(
                path,
                &paints[usize::from(part.slot)],
                FillRule::Winding,
                local,
                None,
            );
        }
    }
    Some(Layer {
        pixmap,
        origin: (x0, y0),
    })
}
