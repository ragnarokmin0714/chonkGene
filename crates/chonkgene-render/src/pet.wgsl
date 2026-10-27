// Ray-marches a posed pet on the GPU: the same field, paint, face and plush
// shading as raymarch.rs, scene.rs and face.rs on the CPU, so the two agree
// to within rounding. Keep them in step.
//
// On top of the pose, a frame squashes, leans and lifts the whole pet in 3D
// about the point under it; the floor and its contact shadow stay put, so a
// hop leaves the shadow behind on the floor, smaller and fainter.

struct Part {
    // Rows of the world → part matrix; w: distance factor, blend, radius.
    a0: vec4<f32>,
    a1: vec4<f32>,
    a2: vec4<f32>,
    // Offset of the matrix; w: shape kind (0 ellipsoid, 1 capsule, 2 torus, 3 cone).
    b: vec4<f32>,
    // Bounding sphere centre; w: gloss.
    center: vec4<f32>,
    // Shape parameters (see `shape_distance`).
    shape: vec4<f32>,
    // x: fuzz, y: colour slot, z: first mark, w: mark count.
    paint: vec4<f32>,
}

struct Mark {
    // xyz: centre, w: softness.
    at: vec4<f32>,
    // xyz: radii, w: colour slot.
    radii: vec4<f32>,
    // x: also mirrored across x = 0 (1.0).
    flags: vec4<f32>,
}

struct Uniforms {
    // Canvas width and height in pixels, pixels per world unit, pixels below the floor.
    cam: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    fwd: vec4<f32>,
    // xyz: the floor point under the pet; w: number of parts.
    anchor: vec4<f32>,
    // Sphere around the pet once the frame has moved it: centre, radius.
    bound: vec4<f32>,
    // Eye part, mouth part (−1 for none), look, ink slot.
    face: vec4<f32>,
    // Eye centre (x, y) and radii in face space.
    eye: vec4<f32>,
    // Iris slot (−1 for none), pupil share, whiskers (1.0), shadow (1.0).
    iris: vec4<f32>,
    brows: vec4<f32>,
    // Brow slot (−1 for none), blush slot.
    spots: vec4<f32>,
    blush: vec4<f32>,
    // Squash (> 0 flattens), breath (−1…1), lean (radians), lift (world units).
    frame: vec4<f32>,
    palette: array<vec4<f32>, 16>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> parts: array<Part>;
@group(0) @binding(2) var<storage, read> marks: array<Mark>;

const FLOOR: f32 = -1.1;
const BIG: f32 = 3.0e38;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

// ---- small maths --------------------------------------------------------------

fn clamp01(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

// Hermite step that also works with the edges reversed (as math.rs's does).
fn ss(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp01((x - e0) / (e1 - e0));
    return t * t * (3.0 - 2.0 * t);
}

fn blend_weight(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return select(0.0, 1.0, a <= b);
    }
    return clamp01(0.5 + 0.5 * (b - a) / k);
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = blend_weight(a, b, k);
    return b + (a - b) * h - k * h * (1.0 - h);
}

// ---- shapes ---------------------------------------------------------------------

fn ellipsoid(p: vec3<f32>, r: vec3<f32>) -> f32 {
    let k0 = length(p / r);
    let k1 = length(p / (r * r));
    if k1 == 0.0 {
        return -min(r.x, min(r.y, r.z));
    }
    return k0 * (k0 - 1.0) / k1;
}

fn round_cone(p: vec3<f32>, r1: f32, r2: f32, h: f32) -> f32 {
    let qx = length(p.xz);
    let qy = p.y;
    let b = (r1 - r2) / h;
    let a = sqrt(max(1.0 - b * b, 0.0));
    let k = -b * qx + a * qy;
    if k < 0.0 {
        return length(vec2<f32>(qx, qy)) - r1;
    }
    if k > a * h {
        return length(vec2<f32>(qx, qy - h)) - r2;
    }
    return a * qx + b * qy - r1;
}

fn shape_distance(kind: f32, s: vec4<f32>, p: vec3<f32>) -> f32 {
    if kind < 0.5 {
        return ellipsoid(p, s.xyz);
    }
    if kind < 1.5 {
        let h = clamp(dot(p, s.xyz) / dot(s.xyz, s.xyz), 0.0, 1.0);
        return length(p - s.xyz * h) - s.w;
    }
    if kind < 2.5 {
        let qx = length(p.xz) - s.x;
        return length(vec2<f32>(qx, p.y)) - s.y;
    }
    return round_cone(vec3<f32>(p.x, p.y, p.z * s.w), s.x, s.y, s.z) / s.w;
}

fn local(i: u32, p: vec3<f32>) -> vec3<f32> {
    let pt = parts[i];
    return vec3<f32>(dot(pt.a0.xyz, p), dot(pt.a1.xyz, p), dot(pt.a2.xyz, p)) + pt.b.xyz;
}

// ---- the frame: squash, lean and lift about the point under the pet --------------

// World → posed-pet space, and how much it stretches distances.
fn to_pet(p: vec3<f32>) -> vec3<f32> {
    let squash = u.frame.x;
    let breath = u.frame.y;
    let lean = u.frame.z;
    // Undo the lift, then the lean about the camera's line of sight through
    // the floor point, then the squash about the floor point.
    var q = p - u.anchor.xyz - vec3<f32>(0.0, u.frame.w, 0.0);
    let axis = u.fwd.xyz;
    let c = cos(-lean);
    let s = sin(-lean);
    q = q * c + cross(axis, q) * s + axis * dot(axis, q) * (1.0 - c);
    let sx = 1.0 + squash;
    let sy = (1.0 - squash) * (1.0 + 0.015 * breath);
    q = vec3<f32>(q.x / sx, q.y / sy, q.z / sx);
    return q + u.anchor.xyz;
}

fn stretch() -> f32 {
    let squash = u.frame.x;
    return min(1.0 + squash, (1.0 - squash) * (1.0 + 0.015 * u.frame.y));
}

// ---- the field ----------------------------------------------------------------------

fn field(p: vec3<f32>) -> f32 {
    var d = BIG;
    let n = u32(u.anchor.w);
    for (var i = 0u; i < n; i++) {
        let pt = parts[i];
        let blend = pt.a1.w;
        if length(p - pt.center.xyz) - pt.a2.w >= d + blend {
            continue;
        }
        let dp = shape_distance(pt.b.w, pt.shape, local(i, p)) * pt.a0.w;
        if d == BIG {
            d = dp;
        } else {
            d = smin(d, dp, blend);
        }
    }
    return d;
}

fn distance_at(p: vec3<f32>) -> f32 {
    return field(to_pet(p)) * stretch();
}

fn normal_at(p: vec3<f32>) -> vec3<f32> {
    let e = 0.0015;
    let k0 = vec3<f32>(1.0, -1.0, -1.0);
    let k1 = vec3<f32>(-1.0, -1.0, 1.0);
    let k2 = vec3<f32>(-1.0, 1.0, -1.0);
    let k3 = vec3<f32>(1.0, 1.0, 1.0);
    return normalize(
        k0 * distance_at(p + k0 * e) + k1 * distance_at(p + k1 * e) + k2 * distance_at(p + k2 * e)
            + k3 * distance_at(p + k3 * e),
    );
}

// ---- paint and the face ---------------------------------------------------------------

fn mark_cover(q: vec3<f32>, at: vec3<f32>, radii: vec3<f32>, soft: f32) -> f32 {
    return 1.0 - ss(1.0 - soft, 1.0, length((q - at) / radii));
}

fn part_color(i: u32, q: vec3<f32>) -> vec3<f32> {
    let pt = parts[i];
    var c = u.palette[u32(pt.paint.y)].rgb;
    let first = u32(pt.paint.z);
    let count = u32(pt.paint.w);
    for (var m = first; m < first + count; m++) {
        let mk = marks[m];
        var cover = mark_cover(q, mk.at.xyz, mk.radii.xyz, mk.at.w);
        if mk.flags.x > 0.5 {
            cover = max(cover, mark_cover(q, mk.at.xyz * vec3<f32>(-1.0, 1.0, 1.0), mk.radii.xyz, mk.at.w));
        }
        if cover > 0.0 {
            c = mix(c, u.palette[u32(mk.radii.w)].rgb, cover);
        }
    }
    return c;
}

fn ink(d: f32, aa: f32) -> f32 {
    return 1.0 - ss(-aa, aa, d);
}

fn ell(uv: vec2<f32>, c: vec2<f32>, r: vec2<f32>) -> f32 {
    return length((uv - c) / r) - 1.0;
}

fn arc(uv: vec2<f32>, c: vec2<f32>, r: f32, w: f32) -> f32 {
    return abs(length(uv - c) - r) - w;
}

fn segment(uv: vec2<f32>, a: vec2<f32>, b: vec2<f32>, w: f32) -> f32 {
    let pa = uv - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - w;
}

// Looks, as numbered in `look_index`.
const OPEN: i32 = 0;
const HALF: i32 = 1;
const SHUT: i32 = 2;
const HAPPY: i32 = 3;
const LAUGH: i32 = 4;
const FED_UP: i32 = 5;
const POUT: i32 = 6;
const ASLEEP: i32 = 7;
const YAWN: i32 = 8;

fn paint_eyes(q: vec3<f32>, r: vec3<f32>, base: vec3<f32>, px: f32) -> vec3<f32> {
    let n = normalize(q / r);
    if n.z < 0.15 {
        return base;
    }
    let uv = n.xy;
    let ry_part = r.y;
    let aa = px / ry_part;
    let ink_c = u.palette[u32(u.face.w)].rgb;
    let white = vec3<f32>(1.0);
    let look = i32(u.face.z);
    var c = base;
    for (var k = 0; k < 2; k++) {
        let side = select(-1.0, 1.0, k == 1);
        let ec = vec2<f32>(u.eye.x * side, u.eye.y);
        let rx = u.eye.z;
        let ry = u.eye.w;
        if u.spots.x >= 0.0 {
            let d = ell(uv, vec2<f32>(u.brows.x * side, u.brows.y), u.brows.zw);
            c = mix(c, u.palette[u32(u.spots.x)].rgb, ink(d, aa));
        }
        let db = ell(uv, vec2<f32>(u.blush.x * side, u.blush.y), u.blush.zw);
        c = mix(c, u.palette[u32(u.spots.y)].rgb, 0.55 * ink(db, aa * 8.0));
        if u.iris.z > 0.5 {
            for (var i = 0; i < 3; i++) {
                let y = -0.31 + 0.06 * f32(i);
                let a = vec2<f32>(0.40 * side, y);
                let b = vec2<f32>(0.66 * side, y + 0.05 * (f32(i) - 1.0));
                c = mix(c, ink_c, 0.45 * ink(segment(uv, a, b, 0.007), aa));
            }
        }
        // Stroke width scales with the eye (face.rs shadows the part's ry).
        let w = 0.17 * ry;
        var iris_c = ink_c;
        var pupil = 1.0;
        if u.iris.x >= 0.0 {
            iris_c = u.palette[u32(u.iris.x)].rgb;
            pupil = u.iris.y;
        }
        if look == OPEN {
            c = mix(c, iris_c, ink(ell(uv, ec, vec2<f32>(rx, ry)), aa));
            if pupil < 1.0 {
                c = mix(c, ink_c, ink(ell(uv, ec, vec2<f32>(rx * pupil, ry * pupil)), aa));
            }
            let hl = vec2<f32>(ec.x - 0.30 * rx, ec.y + 0.36 * ry);
            c = mix(c, white, ink(length(uv - hl) - 0.38 * rx, aa));
            let hl2 = vec2<f32>(ec.x + 0.35 * rx, ec.y - 0.36 * ry);
            c = mix(c, white, ink(length(uv - hl2) - 0.17 * rx, aa));
        } else if look == HALF || look == FED_UP {
            let lid = ec.y + 0.05 * ry;
            let d = max(ell(uv, ec, vec2<f32>(rx, ry)), uv.y - lid);
            c = mix(c, iris_c, ink(d, aa));
            if pupil < 1.0 {
                let dp = max(ell(uv, ec, vec2<f32>(rx * pupil, ry * pupil)), uv.y - lid);
                c = mix(c, ink_c, ink(dp, aa));
            }
            let hl = vec2<f32>(ec.x - 0.25 * rx, ec.y - 0.35 * ry);
            c = mix(c, white, ink(length(uv - hl) - 0.16 * rx, aa));
            let l = max(abs(uv.y - lid) - 0.13 * ry, abs(uv.x - ec.x) - 1.05 * rx);
            c = mix(c, ink_c, ink(l, aa));
        } else if look == SHUT || look == ASLEEP {
            let d = max(arc(uv, vec2<f32>(ec.x, ec.y + 0.5 * ry), 0.75 * rx, w), uv.y - (ec.y + 0.2 * ry));
            c = mix(c, ink_c, ink(d, aa));
        } else if look == HAPPY || look == LAUGH {
            let d = max(arc(uv, vec2<f32>(ec.x, ec.y - 0.4 * ry), 0.75 * rx, w), -(uv.y - (ec.y - 0.25 * ry)));
            c = mix(c, ink_c, ink(d, aa));
        } else {
            // Pout and yawn: > < squeezed shut.
            let tip = vec2<f32>(ec.x - 0.45 * rx * side, ec.y);
            let a = vec2<f32>(ec.x + 0.55 * rx * side, ec.y + 0.5 * ry);
            let b = vec2<f32>(ec.x + 0.55 * rx * side, ec.y - 0.5 * ry);
            let d = min(segment(uv, a, tip, w), segment(uv, b, tip, w));
            c = mix(c, ink_c, ink(d, aa));
        }
    }
    return c;
}

fn paint_mouth(q: vec3<f32>, r: vec3<f32>, base: vec3<f32>, px: f32) -> vec3<f32> {
    let n = normalize(q / r);
    if n.z < 0.3 {
        return base;
    }
    let uv = n.xy;
    let ink_c = u.palette[u32(u.face.w)].rgb;
    let aa = px / r.y;
    let look = i32(u.face.z);
    var c = base;
    if look == LAUGH {
        let m = max(length(vec2<f32>(uv.x, (uv.y + 0.18) / 0.85)) - 0.30, uv.y + 0.18);
        c = mix(c, vec3<f32>(0.55, 0.18, 0.22), ink(m, aa));
        let t = max(length(uv - vec2<f32>(0.0, -0.48)) - 0.16, m);
        c = mix(c, vec3<f32>(1.0, 0.52, 0.56), ink(t, aa));
    } else if look == YAWN {
        let m = length(vec2<f32>(uv.x / 0.7, uv.y + 0.3)) - 0.3;
        c = mix(c, vec3<f32>(0.50, 0.16, 0.20), ink(m, aa));
        let t = max(length(uv - vec2<f32>(0.0, -0.58)) - 0.15, m);
        c = mix(c, vec3<f32>(1.0, 0.52, 0.56), ink(t, aa));
    } else if look == FED_UP {
        let d = max(abs(uv.y + 0.22) - 0.022, abs(uv.x) - 0.13);
        c = mix(c, ink_c, ink(d, aa));
    } else if look == POUT {
        let d = max(arc(uv, vec2<f32>(0.0, -0.32), 0.10, 0.022), -(uv.y + 0.30));
        c = mix(c, ink_c, ink(d, aa));
    } else {
        for (var k = 0; k < 2; k++) {
            let s = select(-1.0, 1.0, k == 1);
            let d = max(arc(uv, vec2<f32>(0.15 * s, -0.10), 0.15, 0.022), uv.y + 0.10);
            c = mix(c, ink_c, ink(d, aa));
        }
    }
    let ridge = max(abs(uv.x) - 0.022, max(uv.y + 0.10, -uv.y - 0.02));
    return mix(c, ink_c, ink(ridge, aa));
}

struct Surf {
    color: vec3<f32>,
    gloss: f32,
    fuzz: f32,
}

fn surface(p: vec3<f32>, px: f32) -> Surf {
    var d = BIG;
    var color = vec3<f32>(0.0);
    var gloss = 0.0;
    var fuzz = 0.0;
    let n = u32(u.anchor.w);
    for (var i = 0u; i < n; i++) {
        let pt = parts[i];
        let blend = pt.a1.w;
        if length(p - pt.center.xyz) - pt.a2.w >= d + blend {
            continue;
        }
        let q = local(i, p);
        let dp = shape_distance(pt.b.w, pt.shape, q) * pt.a0.w;
        let c = part_color(i, q);
        if d == BIG {
            d = dp;
            color = c;
            gloss = pt.center.w;
            fuzz = pt.paint.x;
        } else {
            let h = blend_weight(d, dp, blend);
            color = mix(c, color, h);
            gloss = mix(pt.center.w, gloss, h);
            fuzz = mix(pt.paint.x, fuzz, h);
            d = smin(d, dp, blend);
        }
    }
    if u.face.x >= 0.0 {
        let fi = u32(u.face.x);
        let fq = local(fi, p);
        let fp = parts[fi];
        if abs(shape_distance(fp.b.w, fp.shape, fq)) < 0.05 {
            color = paint_eyes(fq, fp.shape.xyz, color, px);
        }
    }
    if u.face.y >= 0.0 {
        let mi = u32(u.face.y);
        let mq = local(mi, p);
        let mp = parts[mi];
        if abs(shape_distance(mp.b.w, mp.shape, mq)) < 0.04 {
            color = paint_mouth(mq, mp.shape.xyz, color, px);
        }
    }
    return Surf(color, gloss, fuzz);
}

// ---- shading ------------------------------------------------------------------------

fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * 0.31830988618 + vec3<f32>(0.1, 0.2, 0.3)) * 17.0;
    return fract(q.x * q.y * q.z * (q.x + q.y + q.z));
}

fn vnoise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let w = f * f * (vec3<f32>(3.0) - f * 2.0);
    let a = mix(hash3(i), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), w.x);
    let b = mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), w.x);
    let c = mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), w.x);
    let e = mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), w.x);
    return mix(mix(a, b, w.y), mix(c, e, w.y), w.z);
}

fn occlusion(p: vec3<f32>, n: vec3<f32>) -> f32 {
    var occ = 0.0;
    var sca = 1.0;
    for (var i = 0; i < 5; i++) {
        let h = 0.02 + 0.14 * f32(i) / 4.0;
        occ += (h - distance_at(p + n * h)) * sca;
        sca *= 0.85;
    }
    return clamp01(1.0 - 2.2 * occ);
}

// Where a ray first meets the pet, or −1.
fn trace(o: vec3<f32>, d: vec3<f32>) -> f32 {
    // The bounding sphere, already moved by the frame.
    let center = u.bound.xyz;
    let radius = u.bound.w;
    let oc = o - center;
    let b = dot(oc, d);
    let disc = b * b - (dot(oc, oc) - radius * radius);
    if disc <= 0.0 {
        return -1.0;
    }
    let sq = sqrt(disc);
    var t = max(-b - sq, 0.0);
    let end = -b + sq;
    for (var i = 0; i < 160; i++) {
        let dist = distance_at(o + d * t);
        if dist < 4.0e-4 {
            return t;
        }
        t += dist * 0.9;
        if t > end {
            return -1.0;
        }
    }
    return -1.0;
}

// One premultiplied sample, and where the ray hit (−1 for a miss).
struct Sample {
    rgba: vec4<f32>,
    t: f32,
}

// One sample through canvas point (x, y).
fn shade(x: f32, y: f32) -> Sample {
    let w = u.cam.x;
    let h = u.cam.y;
    let ppu = u.cam.z;
    let uu = (x - w / 2.0) / ppu;
    let vv = (h - u.cam.w - y) / ppu;
    let d = u.fwd.xyz;
    let o = u.anchor.xyz + u.right.xyz * uu + u.up.xyz * vv - d * 20.0;
    let t = trace(o, d);
    if t < 0.0 {
        if u.iris.w < 0.5 {
            return Sample(vec4<f32>(0.0), -1.0);
        }
        // A soft contact shadow where the ray meets the floor.
        let tf = (FLOOR - o.y) / d.y;
        let g = o + d * tf;
        if length(g - u.bound.xyz) - u.bound.w > 0.6 + u.frame.w {
            return Sample(vec4<f32>(0.0), -1.0);
        }
        let a = 0.20 * (1.0 - ss(0.0, 0.55, distance_at(g)));
        return Sample(vec4<f32>(vec3<f32>(0.25, 0.18, 0.22) * a, a), -1.0);
    }
    let p = o + d * t;
    let pp = to_pet(p);
    let surf = surface(pp, 1.0 / ppu);
    let f = ppu * 0.3;
    let fz = vec3<f32>(vnoise(pp * f), vnoise(pp * f + vec3<f32>(7.1)), vnoise(pp * f + vec3<f32>(13.7)))
        - vec3<f32>(0.5);
    let n = normalize(normal_at(p) + fz * (0.14 * surf.fuzz));
    let alb = surf.color * (0.97 + 0.06 * surf.fuzz * vnoise(pp * (f * 1.7)));
    let l = normalize(vec3<f32>(-0.55, 0.85, 0.65));
    let v = -d;
    let wrap = clamp01((dot(n, l) + 0.35) / 1.35);
    let band = ss(0.28, 0.66, wrap);
    let ao = occlusion(p, n);
    let tone = mix(vec3<f32>(0.76, 0.64, 0.66), vec3<f32>(1.04, 1.0, 0.97), band);
    var col = alb * tone * (0.70 + 0.30 * ao);
    col += alb * vec3<f32>(1.0, 0.85, 0.75) * (0.10 * clamp01(-n.y));
    let fres = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    col += mix(vec3<f32>(1.0, 0.96, 0.92), alb, 0.3) * (fres * 0.28 * (0.5 + 0.5 * ao));
    let hv = normalize(l + v);
    col += vec3<f32>(surf.gloss * 0.55 * pow(max(dot(n, hv), 0.0), 40.0));
    return Sample(vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0), t);
}

// ---- two passes, as on the CPU ---------------------------------------------------------
//
// Pass 1 traces one ray through every pixel centre and keeps where it hit.
// Pass 2 adds four more rays only where the silhouette or the depth jumps
// between neighbours: a few percent of the pixels.

@group(0) @binding(3) var centre_color: texture_2d<f32>;
@group(0) @binding(4) var centre_depth: texture_2d<f32>;

struct Centre {
    @location(0) color: vec4<f32>,
    @location(1) depth: f32,
}

@fragment
fn fs_centre(@builtin(position) pos: vec4<f32>) -> Centre {
    let s = shade(pos.x, pos.y);
    return Centre(s.rgba, s.t);
}

// Sub-pixel offsets for edge pixels: a rotated grid around the centre.
const SUBS = array<vec2<f32>, 4>(
    vec2<f32>(-0.125, -0.375),
    vec2<f32>(0.375, -0.125),
    vec2<f32>(0.125, 0.375),
    vec2<f32>(-0.375, 0.125),
);

const NEIGHBOURS = array<vec2<i32>, 4>(
    vec2<i32>(-1, 0),
    vec2<i32>(1, 0),
    vec2<i32>(0, -1),
    vec2<i32>(0, 1),
);

@fragment
fn fs_edges(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let at = vec2<i32>(pos.xy);
    let size = vec2<i32>(textureDimensions(centre_depth));
    let c = textureLoad(centre_color, at, 0);
    let t = textureLoad(centre_depth, at, 0).r;
    let hit = t >= 0.0;
    var edge = false;
    for (var i = 0; i < 4; i++) {
        let n = at + NEIGHBOURS[i];
        if any(n < vec2<i32>(0)) || any(n >= size) {
            continue;
        }
        let nt = textureLoad(centre_depth, n, 0).r;
        if (nt >= 0.0) != hit || (hit && abs(nt - t) > 0.15) {
            edge = true;
        }
    }
    if !edge {
        return c;
    }
    var acc = c;
    for (var i = 0; i < 4; i++) {
        acc += shade(pos.x + SUBS[i].x, pos.y + SUBS[i].y).rgba;
    }
    acc /= 5.0;
    return vec4<f32>(min(acc.rgb, vec3<f32>(acc.a)), acc.a);
}
