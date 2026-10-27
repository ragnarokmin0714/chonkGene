//! Pre-rendered poses, made off the UI thread (PLAN §5).
//!
//! Ray-marching a pose takes tens of milliseconds, far too long for a frame,
//! so each pet renders its handful of poses once, on a few background
//! threads, and every frame after that is a cheap 2D blit. A new coat, body
//! value or size starts a new generation; until it lands the old sprites
//! keep showing, so nothing flickers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};

use chonkgene_rig::{Layer, Look, Model, PoseId, Scene};
use tiny_skia::{IntSize, Pixmap};

use crate::raymarch::{self, Sprite};

/// One pre-rendered pose and face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub pose: PoseId,
    pub look: Look,
}

const fn key(pose: PoseId, look: Look) -> Key {
    Key { pose, look }
}

/// Every sprite a pet needs, most urgent first: it appears once the first
/// one lands.
pub const KEYS: [Key; 18] = [
    key(PoseId::Sit, Look::Open),
    key(PoseId::Sit, Look::Shut),
    key(PoseId::WalkA, Look::Open),
    key(PoseId::WalkB, Look::Open),
    key(PoseId::Sit, Look::Half),
    key(PoseId::Sit, Look::Happy),
    key(PoseId::BellyUp, Look::Laugh),
    key(PoseId::Melt, Look::FedUp),
    key(PoseId::Melt, Look::Shut),
    key(PoseId::Loaf, Look::Asleep),
    key(PoseId::Sulk, Look::Pout),
    key(PoseId::WalkA, Look::Shut),
    key(PoseId::WalkB, Look::Shut),
    key(PoseId::LookLeft, Look::Open),
    key(PoseId::LookRight, Look::Open),
    key(PoseId::Stretch, Look::Happy),
    key(PoseId::Yawn, Look::Yawn),
    key(PoseId::Wave, Look::Happy),
];

/// What a background job renders: a pose and face without the tail, or the
/// tail alone in a pose (frames wag it, PLAN §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Slot {
    Body(Key),
    Tail(PoseId),
}

/// Every slot, most urgent first: each pose's tail right after the pose's
/// first sprite, and only for models with a tail.
fn slots(tailed: bool) -> Vec<Slot> {
    let mut out = Vec::new();
    for key in KEYS {
        out.push(Slot::Body(key));
        if tailed && !out.contains(&Slot::Tail(key.pose)) {
            out.push(Slot::Tail(key.pose));
        }
    }
    out
}

/// What to render: everything that changes a sprite's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variant {
    pub coat: usize,
    pub chonk: f32,
    pub scale: f32,
}

type Job = Box<dyn FnOnce() + Send>;

/// The shared background renderers.
fn farm() -> &'static Mutex<Sender<Job>> {
    static FARM: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    FARM.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        // Leave a core for the UI and the rest of the desktop.
        let n = std::thread::available_parallelism()
            .map_or(1, |n| n.get().saturating_sub(1))
            .clamp(1, 3);
        for i in 0..n {
            let rx = Arc::clone(&rx);
            let spawned = std::thread::Builder::new()
                .name(format!("chonkgene-sprites-{i}"))
                .spawn(move || {
                    loop {
                        let job = match rx.lock() {
                            Ok(rx) => rx.recv(),
                            Err(_) => return,
                        };
                        match job {
                            Ok(job) => job(),
                            Err(_) => return,
                        }
                    }
                });
            if spawned.is_err() {
                break;
            }
        }
        Mutex::new(tx)
    })
}

pub struct Ready {
    pub sprite: Sprite,
    /// The sprite cropped to what is drawn, and where the crop sits: frames
    /// filter only these pixels.
    pub pixmap: Pixmap,
    pub offset: (f32, f32),
}

pub struct Sprites {
    model: Arc<Model>,
    look: Variant,
    /// Bumped for every new look; jobs from older ones skip their work.
    generation: Arc<AtomicU64>,
    done: HashMap<Slot, Ready>,
    /// Slots of the current generation still on their way.
    waiting: usize,
    tx: Sender<(u64, Slot, Sprite)>,
    rx: Receiver<(u64, Slot, Sprite)>,
}

impl Sprites {
    pub fn new(model: Arc<Model>, look: Variant) -> Self {
        let (tx, rx) = channel();
        let mut s = Self {
            model,
            look,
            generation: Arc::new(AtomicU64::new(0)),
            done: HashMap::new(),
            waiting: 0,
            tx,
            rx,
        };
        s.request_all();
        s
    }

    pub fn variant(&self) -> Variant {
        self.look
    }

    /// Re-render everything for a new look; old sprites show until replaced.
    pub fn set_variant(&mut self, look: Variant) {
        if look != self.look {
            self.look = look;
            self.request_all();
        }
    }

    fn request_all(&mut self) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let slots = slots(self.model.bone("tail").is_some());
        self.waiting = slots.len();
        let Ok(farm) = farm().lock() else {
            return;
        };
        for slot in slots {
            let (model, look, tx) = (Arc::clone(&self.model), self.look, self.tx.clone());
            let current = Arc::clone(&self.generation);
            let job: Job = Box::new(move || {
                if current.load(Ordering::SeqCst) != generation {
                    return;
                }
                let sprite = match slot {
                    Slot::Body(key) => render_one(&model, look, key, 1),
                    Slot::Tail(pose) => render_tail(&model, look, pose, 1),
                };
                let _ = tx.send((generation, slot, sprite));
            });
            if farm.send(job).is_err() {
                return;
            }
        }
    }

    /// Take in finished sprites. Returns true if any arrived.
    pub fn poll(&mut self) -> bool {
        let mut any = false;
        let current = self.generation.load(Ordering::SeqCst);
        while let Ok((generation, slot, sprite)) = self.rx.try_recv() {
            if generation != current {
                continue;
            }
            self.waiting = self.waiting.saturating_sub(1);
            if let Some(ready) = ready(sprite) {
                self.done.insert(slot, ready);
                any = true;
            }
        }
        any
    }

    /// Block until the current generation is complete (tests, benchmarks).
    pub fn finish(&mut self) {
        let current = self.generation.load(Ordering::SeqCst);
        while self.waiting > 0 {
            let Ok((generation, slot, sprite)) = self.rx.recv() else {
                return;
            };
            if generation != current {
                continue;
            }
            self.waiting -= 1;
            if let Some(ready) = ready(sprite) {
                self.done.insert(slot, ready);
            }
        }
    }

    /// The sprite for `key`, or the closest one already made.
    pub fn best(&self, key: Key) -> Option<(Key, &Ready)> {
        let fallbacks = [
            key,
            Key {
                look: Look::Shut,
                ..key
            },
            Key {
                look: Look::Open,
                ..key
            },
            KEYS[0],
        ];
        fallbacks
            .into_iter()
            .find_map(|k| self.done.get(&Slot::Body(k)).map(|r| (k, r)))
            .or_else(|| {
                self.done.iter().find_map(|(s, r)| match s {
                    Slot::Body(k) => Some((*k, r)),
                    Slot::Tail(_) => None,
                })
            })
    }

    pub fn get(&self, key: Key) -> Option<&Ready> {
        self.done.get(&Slot::Body(key))
    }

    /// The tail drawn on its own for `pose`, if the pet has one and it is in.
    pub fn tail(&self, pose: PoseId) -> Option<&Ready> {
        self.done.get(&Slot::Tail(pose))
    }
}

impl Drop for Sprites {
    fn drop(&mut self) {
        // Queued jobs for this pet see a generation that never comes and skip.
        self.generation.store(u64::MAX, Ordering::SeqCst);
    }
}

/// Render one sprite now, on this thread plus `threads - 1` helpers: the
/// pet without its tail, which frames draw separately (see [`render_tail`]).
pub fn render_one(model: &Model, look: Variant, key: Key, threads: usize) -> Sprite {
    render_layer(model, look, key, Layer::Body, threads)
}

/// Render just the tail in `pose`.
pub fn render_tail(model: &Model, look: Variant, pose: PoseId, threads: usize) -> Sprite {
    let key = Key {
        pose,
        look: Look::Open,
    };
    render_layer(model, look, key, Layer::Tail, threads)
}

/// Render one layer of a pose: [`Layer::All`] is the whole pet in one image
/// (screenshots and the look tool).
pub fn render_layer(
    model: &Model,
    look: Variant,
    key: Key,
    layer: Layer,
    threads: usize,
) -> Sprite {
    let scene = Scene::new(model, look.coat, look.chonk, key.pose, key.look, layer);
    raymarch::render(&scene, look.scale, threads)
}

fn ready(sprite: Sprite) -> Option<Ready> {
    let (w, h) = (sprite.width as usize, sprite.height as usize);
    let alpha = |x: usize, y: usize| sprite.rgba[(y * w + x) * 4 + 3];
    let rows: Vec<usize> = (0..h)
        .filter(|&y| (0..w).any(|x| alpha(x, y) > 0))
        .collect();
    let cols: Vec<usize> = (0..w)
        .filter(|&x| (0..h).any(|y| alpha(x, y) > 0))
        .collect();
    let (&y0, &y1, &x0, &x1) = (rows.first()?, rows.last()?, cols.first()?, cols.last()?);
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut data = Vec::with_capacity(cw * ch * 4);
    for y in y0..=y1 {
        data.extend_from_slice(&sprite.rgba[(y * w + x0) * 4..(y * w + x1 + 1) * 4]);
    }
    let pixmap = Pixmap::from_vec(data, IntSize::from_wh(cw as u32, ch as u32)?)?;
    // The crop holds every visible pixel; the full image is dead weight.
    let sprite = Sprite {
        rgba: Vec::new(),
        ..sprite
    };
    Some(Ready {
        sprite,
        pixmap,
        offset: (x0 as f32, y0 as f32),
    })
}
