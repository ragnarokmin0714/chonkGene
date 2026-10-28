//! Ray-marches a posed pet into a sprite on the CPU (PLAN §3.1).
//!
//! The light, the soft two-tone shading, the fuzzy rim and the contact
//! shadow are the look Roger picked from the 3D spike (spikes/blob3d). Rays
//! are orthographic, so a sprite maps to world space with one scale: anchors
//! and hit-testing need no projection maths.

use chonkgene_rig::math::{V3, clamp01, smoothstep, v3};
use std::f32::consts::FRAC_1_PI;

use chonkgene_rig::{FLOOR, Layer, Scene, ZONES};

/// Canvas in world units, and pixels per unit at 100%.
const UNITS: (f32, f32) = (3.9, 3.45);
pub(crate) const PPU: f32 = 58.0;
/// Pixels below the floor line at 100%, for the contact shadow.
pub const FLOOR_PX: f32 = 8.0;
/// Camera tilt: looking slightly down at the pet.
const TILT_DEG: f32 = 9.0;

/// A rendered pose: premultiplied RGBA plus what each pixel would poke.
#[derive(Clone, Debug)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// 0 for nothing, else 1 + an index into [`ZONES`].
    pub zones: Vec<u8>,
    /// Top of the head, in pixels: where speech bubbles point.
    pub top: (f32, f32),
    /// Beside the nose, in pixels: where the snot bubble hangs.
    pub nose: (f32, f32),
    /// Where the tail joins the body, in pixels, and whether the tail is in
    /// front of the body; none for a pet without a tail bone.
    pub tail: Option<((f32, f32), bool)>,
}

impl Sprite {
    pub fn zone_at(&self, x: f32, y: f32) -> Option<&'static str> {
        if x < 0.0 || y < 0.0 || x >= self.width as f32 || y >= self.height as f32 {
            return None;
        }
        let z = self.zones[y as usize * self.width as usize + x as usize];
        (z > 0).then(|| ZONES[usize::from(z - 1)])
    }
}

/// Pixel size of a pet at `scale` (1.0 = 100%).
pub fn pixel_size(scale: f32) -> (u32, u32) {
    (
        (UNITS.0 * PPU * scale).ceil() as u32,
        (UNITS.1 * PPU * scale).ceil() as u32,
    )
}

pub(crate) struct Camera {
    pub w: u32,
    pub h: u32,
    pub ppu: f32,
    pub floor_px: f32,
    pub right: V3,
    pub up: V3,
    pub fwd: V3,
    /// The floor point under the pet; it sits on the floor line.
    pub anchor: V3,
}

impl Camera {
    pub fn new(scale: f32) -> Self {
        let (w, h) = pixel_size(scale);
        let (s, c) = TILT_DEG.to_radians().sin_cos();
        Self {
            w,
            h,
            ppu: PPU * scale,
            floor_px: FLOOR_PX * scale,
            right: v3(1.0, 0.0, 0.0),
            up: v3(0.0, c, -s),
            fwd: v3(0.0, -s, -c),
            anchor: v3(0.0, FLOOR, 0.0),
        }
    }

    /// Ray origin through pixel coordinates `(x, y)` (not centres).
    fn origin(&self, x: f32, y: f32) -> V3 {
        let u = (x - self.w as f32 / 2.0) / self.ppu;
        let v = (self.h as f32 - self.floor_px - y) / self.ppu;
        self.anchor + self.right * u + self.up * v - self.fwd * 20.0
    }

    pub fn project(&self, p: V3) -> (f32, f32) {
        let rel = p - self.anchor;
        (
            rel.dot(self.right) * self.ppu + self.w as f32 / 2.0,
            self.h as f32 - self.floor_px - rel.dot(self.up) * self.ppu,
        )
    }
}

/// One sample: premultiplied colour and alpha, plus the hit depth and zone.
#[derive(Clone, Copy, Default)]
struct Sample {
    rgba: [f32; 4],
    t: f32,
    zone: u8,
}

fn trace(scene: &Scene, o: V3, d: V3) -> Option<f32> {
    let oc = o - scene.center;
    let b = oc.dot(d);
    let disc = b * b - (oc.dot(oc) - scene.radius * scene.radius);
    if disc <= 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let (mut t, end) = ((-b - sq).max(0.0), -b + sq);
    for _ in 0..160 {
        let dist = scene.distance(o + d * t);
        if dist < 4e-4 {
            return Some(t);
        }
        t += dist * 0.9;
        if t > end {
            return None;
        }
    }
    None
}

/// GLSL's fract: always in 0..1, also for negative numbers (Rust's
/// `f32::fract` keeps the sign, which mirrors the noise across x = 0).
fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn hash3(p: V3) -> f32 {
    let q = v3(
        fract(p.x * FRAC_1_PI + 0.1),
        fract(p.y * FRAC_1_PI + 0.2),
        fract(p.z * FRAC_1_PI + 0.3),
    ) * 17.0;
    fract(q.x * q.y * q.z * (q.x + q.y + q.z))
}

fn vnoise(p: V3) -> f32 {
    let i = v3(p.x.floor(), p.y.floor(), p.z.floor());
    let f = p - i;
    let w = f.mul_elem(f).mul_elem(v3(3.0, 3.0, 3.0) - f * 2.0);
    let h = |x: f32, y: f32, z: f32| hash3(i + v3(x, y, z));
    let mx = |a: f32, b: f32, t: f32| a + (b - a) * t;
    mx(
        mx(
            mx(h(0., 0., 0.), h(1., 0., 0.), w.x),
            mx(h(0., 1., 0.), h(1., 1., 0.), w.x),
            w.y,
        ),
        mx(
            mx(h(0., 0., 1.), h(1., 0., 1.), w.x),
            mx(h(0., 1., 1.), h(1., 1., 1.), w.x),
            w.y,
        ),
        w.z,
    )
}

pub(crate) fn occlusion(scene: &Scene, p: V3, n: V3) -> f32 {
    let (mut occ, mut sca) = (0.0, 1.0);
    for i in 0..5 {
        let h = 0.02 + 0.14 * i as f32 / 4.0;
        occ += (h - scene.distance(p + n * h)) * sca;
        sca *= 0.85;
    }
    clamp01(1.0 - 2.2 * occ)
}

fn shade(scene: &Scene, cam: &Camera, x: f32, y: f32) -> Sample {
    let (o, d) = (cam.origin(x, y), cam.fwd);
    let Some(t) = trace(scene, o, d) else {
        // A tail drawn on its own casts no shadow: the body's covers it.
        if scene.layer() == Layer::Tail {
            return Sample::default();
        }
        // Missed: a soft contact shadow where the ray meets the floor.
        let tf = (FLOOR - o.y) / d.y;
        let g = o + d * tf;
        if (g - scene.center).length() - scene.radius > 0.6 {
            return Sample::default();
        }
        let a = 0.20 * (1.0 - smoothstep(0.0, 0.55, scene.distance(g)));
        let s = v3(0.25, 0.18, 0.22) * a;
        return Sample {
            rgba: [s.x, s.y, s.z, a],
            t: -1.0,
            zone: 0,
        };
    };
    let p = o + d * t;
    let surf = scene.surface(p, 1.0 / cam.ppu);
    // Fuzz a few pixels across at any size: finer would alias into a grid.
    let f = cam.ppu * 0.3;
    let fz = v3(
        vnoise(p * f),
        vnoise(p * f + V3::ONE * 7.1),
        vnoise(p * f + V3::ONE * 13.7),
    ) - V3::ONE * 0.5;
    let n = (scene.normal(p) + fz * (0.14 * surf.fuzz)).normalize();
    let alb = surf.color * (0.97 + 0.06 * surf.fuzz * vnoise(p * (f * 1.7)));

    let l = v3(-0.55, 0.85, 0.65).normalize();
    let v = -d;
    let wrap = clamp01((n.dot(l) + 0.35) / 1.35);
    let band = smoothstep(0.28, 0.66, wrap);
    let ao = occlusion(scene, p, n);
    let tone = v3(0.76, 0.64, 0.66).lerp(v3(1.04, 1.0, 0.97), band);
    let mut col = alb.mul_elem(tone) * (0.70 + 0.30 * ao);
    // Bounce light from below and a soft fuzzy rim: the plush look.
    col = col + alb.mul_elem(v3(1.0, 0.85, 0.75)) * (0.10 * clamp01(-n.y));
    let fres = (1.0 - n.dot(v).max(0.0)).powf(2.5);
    col = col + v3(1.0, 0.96, 0.92).lerp(alb, 0.3) * (fres * 0.28 * (0.5 + 0.5 * ao));
    let h = (l + v).normalize();
    col = col + V3::ONE * (surf.gloss * 0.55 * n.dot(h).max(0.0).powf(40.0));
    Sample {
        rgba: [clamp01(col.x), clamp01(col.y), clamp01(col.z), 1.0],
        t,
        zone: surf.zone + 1,
    }
}

/// Sub-pixel offsets for edge pixels: a rotated grid around the centre.
const SUBS: [(f32, f32); 4] = [
    (-0.125, -0.375),
    (0.375, -0.125),
    (0.125, 0.375),
    (-0.375, 0.125),
];

/// Render `scene` at `scale` using up to `threads` threads.
pub fn render(scene: &Scene, scale: f32, threads: usize) -> Sprite {
    let cam = Camera::new(scale);
    let (w, h) = (cam.w as usize, cam.h as usize);
    let threads = threads.clamp(1, h);
    let rows = h.div_ceil(threads);

    // Pass 1: one ray through every pixel centre.
    let mut centre = vec![Sample::default(); w * h];
    std::thread::scope(|s| {
        for (band, out) in centre.chunks_mut(rows * w).enumerate() {
            let cam = &cam;
            s.spawn(move || {
                for (i, px) in out.iter_mut().enumerate() {
                    let (x, y) = (i % w, band * rows + i / w);
                    *px = shade(scene, cam, x as f32 + 0.5, y as f32 + 0.5);
                }
            });
        }
    });

    // Pass 2: more rays only where the silhouette or the depth jumps.
    let edge = |x: usize, y: usize| {
        let c = &centre[y * w + x];
        let hit = c.t >= 0.0;
        [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)]
            .iter()
            .any(|(dx, dy)| {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    return false;
                }
                let n = &centre[ny as usize * w + nx as usize];
                (n.t >= 0.0) != hit || (hit && (n.t - c.t).abs() > 0.15)
            })
    };
    let mut rgba = vec![0u8; w * h * 4];
    std::thread::scope(|s| {
        for (band, out) in rgba.chunks_mut(rows * w * 4).enumerate() {
            let (cam, centre, edge) = (&cam, &centre, &edge);
            s.spawn(move || {
                for (i, px) in out.chunks_mut(4).enumerate() {
                    let (x, y) = (i % w, band * rows + i / w);
                    let mut acc = centre[y * w + x].rgba;
                    if edge(x, y) {
                        for (ox, oy) in SUBS {
                            let sm = shade(scene, cam, x as f32 + 0.5 + ox, y as f32 + 0.5 + oy);
                            for (a, v) in acc.iter_mut().zip(sm.rgba) {
                                *a += v;
                            }
                        }
                        acc = acc.map(|v| v / 5.0);
                    }
                    // Samples are premultiplied (a hit is opaque), so they average.
                    let a = acc[3];
                    for c in 0..3 {
                        px[c] = (acc[c].min(a) * 255.0).round() as u8;
                    }
                    px[3] = (a * 255.0).round() as u8;
                }
            });
        }
    });
    let zones = centre.iter().map(|s| s.zone).collect();
    let top = top_of(&rgba, w, h);
    Sprite {
        width: cam.w,
        height: cam.h,
        rgba,
        zones,
        top,
        nose: cam.project(scene.nose()),
        tail: scene.tail().map(|(at, front)| (cam.project(at), front)),
    }
}

/// The zone the ray through pixel `(x, y)` hits, with the frame's squash,
/// lean and lift applied: hit-testing for frames drawn on the GPU (one ray
/// on the CPU costs microseconds).
pub fn zone_at(
    scene: &Scene,
    scale: f32,
    frame: &crate::gpu::Frame,
    x: f32,
    y: f32,
) -> Option<&'static str> {
    let cam = Camera::new(scale);
    let (o, d) = (cam.origin(x, y), cam.fwd);
    let pet = |p: V3| frame.pet_space(p, cam.anchor, cam.fwd);
    let k = frame.stretch();
    // The bounding sphere, moved by the frame (as for pet.wgsl).
    let (center, radius) = frame.bound(scene.center, scene.radius, cam.anchor, cam.fwd);
    let oc = o - center;
    let b = oc.dot(d);
    let disc = b * b - (oc.dot(oc) - radius * radius);
    if disc <= 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let (mut t, end) = ((-b - sq).max(0.0), -b + sq);
    for _ in 0..160 {
        let p = o + d * t;
        let dist = scene.distance(pet(p)) * k;
        if dist < 4e-4 {
            let zone = scene.surface(pet(p), 1.0 / cam.ppu).zone;
            return ZONES.get(usize::from(zone)).copied();
        }
        t += dist * 0.9;
        if t > end {
            return None;
        }
    }
    None
}

/// The highest solid pixels in the middle of the sprite.
fn top_of(rgba: &[u8], w: usize, h: usize) -> (f32, f32) {
    let (x0, x1) = (w / 5, w - w / 5);
    for y in 0..h {
        let xs: Vec<usize> = (x0..x1)
            .filter(|x| rgba[(y * w + x) * 4 + 3] > 200)
            .collect();
        if !xs.is_empty() {
            return ((xs[0] + xs[xs.len() - 1]) as f32 / 2.0, y as f32);
        }
    }
    (w as f32 / 2.0, 0.0)
}
