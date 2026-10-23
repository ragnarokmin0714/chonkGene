// A round 3D Shiba, ray-marched from signed distance fields.
//
// Every part is a soft primitive (ellipsoid, capsule, torus) blended with a
// smooth union, so the pet reads as one squishy bean instead of glued shapes.
// The face is painted on the head (eyes, brows, blush) and the muzzle (mouth),
// the way cute 3D games do it, so expressions stay cheap to swap.

struct U {
    res: vec4<f32>,   // width, height, yaw, legs (0 sit, 1 paws up, 2 tucked)
    shape: vec4<f32>, // fat, squash, expression, hat
    pose: vec4<f32>,  // pitch, roll, lift, squish (+ squashes, - stretches)
    head: vec4<f32>,  // head offset xyz, world x shift
    base: vec4<f32>,  // coat
    cream: vec4<f32>, // urajiro (cheeks, chest, paws)
    dark: vec4<f32>,  // nose, eyes
    ear: vec4<f32>,   // inner ear
}
@group(0) @binding(0) var<uniform> u: U;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

// ---- primitives -------------------------------------------------------------

fn sd_ellipsoid(p: vec3<f32>, r: vec3<f32>) -> f32 {
    let k0 = length(p / r);
    let k1 = length(p / (r * r));
    return k0 * (k0 - 1.0) / k1;
}

fn sd_capsule(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - r;
}

fn sd_torus(p: vec3<f32>, t: vec2<f32>) -> f32 {
    let q = vec2<f32>(length(p.xz) - t.x, p.y);
    return length(q) - t.y;
}

// Rounded cone along +y: radius r1 at the base, r2 at height h.
fn sd_round_cone(p: vec3<f32>, r1: f32, r2: f32, h: f32) -> f32 {
    let q = vec2<f32>(length(p.xz), p.y);
    let b = (r1 - r2) / h;
    let a = sqrt(1.0 - b * b);
    let k = dot(q, vec2<f32>(-b, a));
    if k < 0.0 { return length(q) - r1; }
    if k > a * h { return length(q - vec2<f32>(0.0, h)) - r2; }
    return dot(q, vec2<f32>(a, b)) - r1;
}

fn rot_x(a: f32) -> mat3x3<f32> {
    let c = cos(a); let s = sin(a);
    return mat3x3<f32>(1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c);
}
fn rot_y(a: f32) -> mat3x3<f32> {
    let c = cos(a); let s = sin(a);
    return mat3x3<f32>(c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c);
}
fn rot_z(a: f32) -> mat3x3<f32> {
    let c = cos(a); let s = sin(a);
    return mat3x3<f32>(c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0);
}

// A surface sample: distance, colour, gloss.
struct S { d: f32, c: vec3<f32>, g: f32 }

// Smooth union that blends colour across the seam too.
fn su(a: S, b: S, k: f32) -> S {
    let h = clamp(0.5 + 0.5 * (b.d - a.d) / k, 0.0, 1.0);
    return S(mix(b.d, a.d, h) - k * h * (1.0 - h), mix(b.c, a.c, h), mix(b.g, a.g, h));
}

// ---- the pet ------------------------------------------------------------------

fn head_c() -> vec3<f32> { return vec3<f32>(0.0, 0.50, 0.08) + u.head.xyz; }
const HEAD_R = vec3<f32>(1.02, 0.84, 0.88);
fn muzzle_c() -> vec3<f32> { return head_c() + vec3<f32>(0.0, -0.24, 0.66); }
const MUZZLE_R = vec3<f32>(0.36, 0.25, 0.26);

// The body always sits on the ground, however squashed.
fn body_r() -> vec3<f32> { return vec3<f32>(0.84 * u.shape.x, 0.62 * u.shape.y, 0.76 * u.shape.x); }
fn body_c() -> vec3<f32> { return vec3<f32>(0.0, -1.10 + body_r().y, 0.0); }

// World space -> pet space: undo lift, yaw, then the pose's pitch and roll.
fn to_pet(p: vec3<f32>) -> vec3<f32> {
    var q = p - vec3<f32>(u.head.w, u.pose.z, 0.0);
    q = rot_y(-u.res.z) * q;
    q = rot_x(-u.pose.x) * q;
    q = rot_z(-u.pose.y) * q;
    // Squish about the ground: shorter and wider, volume roughly kept.
    let sy = 1.0 - u.pose.w;
    let sxz = 1.0 / sqrt(sy);
    q = vec3<f32>(q.x / sxz, (q.y + 1.10) / sy - 1.10, q.z / sxz);
    return q;
}

// Where the coat turns cream: lower face, chest and belly, paws.
fn coat(p: vec3<f32>) -> vec3<f32> {
    let q = p - head_c();
    let face = smoothstep(0.06, -0.06, q.y + 0.20 - 0.30 * q.x * q.x) * smoothstep(0.15, 0.45, q.z);
    let b = p - body_c();
    let belly = smoothstep(1.0, 0.85, length(vec2<f32>(b.x / (0.42 * u.shape.x), (b.y - 0.05) / (0.62 * u.shape.y))))
        * smoothstep(0.05, 0.35, b.z);
    let paws = smoothstep(-0.86, -0.94, p.y);
    let m = clamp(max(max(face, belly), paws), 0.0, 1.0);
    return mix(u.base.rgb, u.cream.rgb, m);
}

fn ear(p: vec3<f32>, side: f32) -> S {
    var q = p - (head_c() + vec3<f32>(0.52 * side, 0.56, -0.10));
    q = rot_z(0.38 * side) * rot_x(0.18) * q;
    // Flatten front to back; dividing the distance keeps the field conservative.
    let d = sd_round_cone(q * vec3<f32>(1.0, 1.0, 2.0), 0.24, 0.06, 0.40) / 2.0;
    let w = mix(0.17, 0.04, clamp(q.y / 0.38, 0.0, 1.0));
    let inner = smoothstep(0.0, 0.03, q.z) * smoothstep(w, w * 0.7, abs(q.x)) * smoothstep(0.06, 0.12, q.y);
    return S(d, mix(u.base.rgb, u.ear.rgb, inner), 0.0);
}

fn legs(p: vec3<f32>) -> S {
    let fat = u.shape.x;
    let r = 0.17 * sqrt(fat);
    let m = u.res.w;
    var d = 1e3;
    if m < 0.5 {
        // Sitting: front legs straight down, haunches and feet in front.
        for (var i = 0; i < 2; i++) {
            let s = f32(i) * 2.0 - 1.0;
            d = min(d, sd_capsule(p, vec3<f32>(0.30 * s, -0.62, 0.52), vec3<f32>(0.31 * s, -0.98, 0.58), r));
            d = min(d, sd_ellipsoid(p - vec3<f32>(0.62 * fat * s, -0.84, 0.0), vec3<f32>(0.30, 0.27, 0.34)));
            d = min(d, sd_ellipsoid(p - vec3<f32>(0.62 * fat * s, -1.02, 0.40), vec3<f32>(0.17, 0.11, 0.24)));
        }
    } else if m < 1.5 {
        // On its back: all four paws up in the air (pet +z is up after the pitch).
        for (var i = 0; i < 2; i++) {
            let s = f32(i) * 2.0 - 1.0;
            d = min(d, sd_capsule(p, vec3<f32>(0.32 * s, -0.30, 0.50), vec3<f32>(0.36 * s, -0.22, 0.74), r * 0.9));
            d = min(d, sd_capsule(p, vec3<f32>(0.48 * fat * s, -0.86, 0.42), vec3<f32>(0.52 * fat * s, -0.82, 0.66), r));
        }
    } else {
        // Tucked under a loaf: just the front paw tips peek out.
        for (var i = 0; i < 2; i++) {
            let s = f32(i) * 2.0 - 1.0;
            d = min(d, sd_ellipsoid(p - vec3<f32>(0.30 * s, -1.03, 0.55 * u.shape.x), vec3<f32>(0.17, 0.09, 0.24)));
        }
    }
    return S(d, coat(p), 0.0);
}

fn tail(p: vec3<f32>) -> S {
    var q = p - vec3<f32>(0.30, -0.05, -0.80);
    q = rot_z(1.35) * rot_y(0.5) * q;
    let d = sd_torus(q, vec2<f32>(0.22, 0.13));
    let inner = smoothstep(0.24, 0.18, length(q.xz));
    return S(d, mix(u.base.rgb, u.cream.rgb, inner), 0.0);
}

fn hat(p: vec3<f32>) -> S {
    var q = p - (head_c() + vec3<f32>(0.20, 0.66, -0.05));
    q = rot_z(-0.32) * q;
    let cone = sd_round_cone(q, 0.27, 0.035, 0.58);
    let pom = length(q - vec3<f32>(0.0, 0.64, 0.0)) - 0.10;
    let stripe = step(0.5, fract(q.y * 3.2 + atan2(q.x, q.z) * 0.32));
    let cone_c = mix(vec3<f32>(1.0, 0.70, 0.78), vec3<f32>(0.62, 0.86, 0.98), stripe);
    if pom < cone { return S(pom, vec3<f32>(1.0, 0.93, 0.62), 0.0); }
    return S(cone, cone_c, 0.15);
}

fn scene(pw: vec3<f32>) -> S {
    let p = to_pet(pw);
    // Body: a pear, wide at the bottom.
    var s = S(sd_ellipsoid(p - body_c(), body_r()), coat(p), 0.0);
    s = su(s, S(sd_ellipsoid(p - head_c(), HEAD_R), coat(p), 0.0), 0.20);
    s = su(s, S(sd_ellipsoid(p - muzzle_c(), MUZZLE_R), coat(p), 0.0), 0.16);
    s = su(s, ear(p, -1.0), 0.10);
    s = su(s, ear(p, 1.0), 0.10);
    s = su(s, legs(p), 0.12);
    s = su(s, tail(p), 0.06);
    let nose = S(sd_ellipsoid(p - (muzzle_c() + vec3<f32>(0.0, 0.10, 0.25)), vec3<f32>(0.11, 0.075, 0.08)),
                 u.dark.rgb, 1.0);
    s = su(s, nose, 0.03);
    if u.shape.w > 0.5 { s = su(s, hat(p), 0.02); }
    // to_pet stretched space by up to 1/min(scale); shrink the step to match.
    let sy = 1.0 - u.pose.w;
    s.d *= min(sy, 1.0 / sqrt(sy));
    return s;
}

fn dist(p: vec3<f32>) -> f32 { return scene(p).d; }

// ---- the face ------------------------------------------------------------------

fn ell(uv: vec2<f32>, c: vec2<f32>, r: vec2<f32>) -> f32 { return length((uv - c) / r) - 1.0; }
// Coverage from a distance in face units, anti-aliased over ~one pixel.
fn ink(d: f32, aa: f32) -> f32 { return 1.0 - smoothstep(-aa, aa, d); }
fn arc(uv: vec2<f32>, c: vec2<f32>, r: f32, w: f32) -> f32 { return abs(length(uv - c) - r) - w; }

fn face(p: vec3<f32>, col: vec3<f32>, aa: f32) -> vec3<f32> {
    var c = col;
    let q = (p - head_c()) / HEAD_R;
    let n = normalize(q);
    if n.z < 0.15 { return c; }
    let uv = n.xy;
    let e = u.shape.z;
    for (var i = 0; i < 2; i++) {
        let s = f32(i) * 2.0 - 1.0;
        let ec = vec2<f32>(0.36 * s, 0.02);
        // Maro brows: the two cream dots every Shiba has.
        c = mix(c, u.cream.rgb, ink(ell(uv, vec2<f32>(0.28 * s, 0.31), vec2<f32>(0.075, 0.048)), aa));
        // Blush.
        c = mix(c, vec3<f32>(1.0, 0.55, 0.58), 0.55 * ink(ell(uv, vec2<f32>(0.53 * s, -0.16), vec2<f32>(0.14, 0.085)), aa * 6.0));
        if e < 0.5 {
            // Open: big glossy eyes with two highlights.
            c = mix(c, u.dark.rgb, ink(ell(uv, ec, vec2<f32>(0.10, 0.125)), aa));
            c = mix(c, vec3<f32>(1.0), ink(length(uv - ec - vec2<f32>(-0.03, 0.045)) - 0.038, aa));
            c = mix(c, vec3<f32>(1.0), ink(length(uv - ec - vec2<f32>(0.035, -0.045)) - 0.017, aa));
        } else if e < 1.5 {
            // Laughing: ^ ^
            let d = max(arc(uv, ec - vec2<f32>(0.0, 0.05), 0.075, 0.017), -(uv.y - (ec.y - 0.03)));
            c = mix(c, u.dark.rgb, ink(d, aa));
        } else if e < 2.5 {
            // Asleep: closed curves.
            let d = max(arc(uv, ec + vec2<f32>(0.0, 0.06), 0.075, 0.015), uv.y - (ec.y + 0.02));
            c = mix(c, u.dark.rgb, ink(d, aa));
        } else {
            // Fed up: half-lidded, the lid a flat line.
            let lid = ec.y + 0.005;
            let d = max(ell(uv, ec, vec2<f32>(0.085, 0.10)), uv.y - lid);
            c = mix(c, u.dark.rgb, ink(d, aa));
            c = mix(c, vec3<f32>(1.0), ink(length(uv - ec - vec2<f32>(-0.025, -0.035)) - 0.016, aa));
            let l = max(abs(uv.y - lid) - 0.013, abs(uv.x - ec.x) - 0.105);
            c = mix(c, u.dark.rgb * 1.4, ink(l, aa));
        }
    }
    return c;
}

fn mouth(p: vec3<f32>, col: vec3<f32>, aa: f32) -> vec3<f32> {
    var c = col;
    let n = normalize((p - muzzle_c()) / MUZZLE_R);
    if n.z < 0.3 { return c; }
    let uv = n.xy;
    let e = u.shape.z;
    if e > 0.5 && e < 1.5 {
        // Laughing: a wide open mouth with a tongue.
        let m = max(length((uv - vec2<f32>(0.0, -0.18)) / vec2<f32>(1.0, 0.85)) - 0.30, uv.y + 0.18);
        c = mix(c, vec3<f32>(0.55, 0.18, 0.22), ink(m, aa));
        let t = max(length(uv - vec2<f32>(0.0, -0.48)) - 0.16, m);
        c = mix(c, vec3<f32>(1.0, 0.52, 0.56), ink(t, aa));
    } else if e > 2.5 {
        // Fed up: a flat little line.
        c = mix(c, u.dark.rgb, ink(max(abs(uv.y + 0.22) - 0.022, abs(uv.x) - 0.13), aa));
    } else {
        // ω
        for (var i = 0; i < 2; i++) {
            let s = f32(i) * 2.0 - 1.0;
            let d = max(arc(uv, vec2<f32>(0.15 * s, -0.10), 0.15, 0.022), uv.y + 0.10);
            c = mix(c, u.dark.rgb, ink(d, aa));
        }
    }
    let ridge = max(abs(uv.x) - 0.022, max(uv.y + 0.10, -uv.y - 0.02));
    c = mix(c, u.dark.rgb, ink(ridge, aa));
    return c;
}

// ---- plush fuzz -------------------------------------------------------------

fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * 0.3183099 + vec3<f32>(0.1, 0.2, 0.3)) * 17.0;
    return fract(q.x * q.y * q.z * (q.x + q.y + q.z));
}
fn vnoise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    return mix(mix(mix(hash3(i), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), w.x),
                   mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), w.x), w.y),
               mix(mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), w.x),
                   mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), w.x), w.y), w.z);
}

// ---- lighting ----------------------------------------------------------------

fn normal(p: vec3<f32>) -> vec3<f32> {
    let e = vec2<f32>(1.0, -1.0) * 0.0015;
    return normalize(e.xyy * dist(p + e.xyy) + e.yyx * dist(p + e.yyx)
                   + e.yxy * dist(p + e.yxy) + e.xxx * dist(p + e.xxx));
}

fn occlusion(p: vec3<f32>, n: vec3<f32>) -> f32 {
    var occ = 0.0;
    var sca = 1.0;
    for (var i = 0; i < 5; i++) {
        let h = 0.02 + 0.14 * f32(i) / 4.0;
        occ += (h - dist(p + n * h)) * sca;
        sca *= 0.85;
    }
    return clamp(1.0 - 2.2 * occ, 0.0, 1.0);
}

const GROUND = -1.12;

@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let res = u.res.xy;
    let uv = (vec2<f32>(frag.x, res.y - frag.y) - 0.5 * res) / res.y;
    let ro = vec3<f32>(0.0, 1.4, 9.0);
    let ta = vec3<f32>(0.0, 0.40, 0.0);
    let ww = normalize(ta - ro);
    let uu = normalize(cross(ww, vec3<f32>(0.0, 1.0, 0.0)));
    let vv = cross(uu, ww);
    let focal = 2.3;
    let rd = normalize(uv.x * uu + uv.y * vv + focal * ww);
    // World units per pixel at the pet, for anti-aliasing the painted face.
    let px = length(ro) / focal / res.y;

    // March from the front of a bounding sphere.
    let bc = vec3<f32>(0.0, 0.2, 0.0);
    let oc = ro - bc;
    let b = dot(oc, rd);
    let disc = b * b - (dot(oc, oc) - 3.2 * 3.2);
    var t = 1e9;
    if disc > 0.0 {
        var tt = max(-b - sqrt(disc), 0.0);
        let tmax = -b + sqrt(disc);
        for (var i = 0; i < 160; i++) {
            let d = dist(ro + rd * tt);
            if d < 0.0004 * tt { t = tt; break; }
            tt += d * 0.85;
            if tt > tmax { break; }
        }
    }

    if t > 1e8 {
        // Missed: a soft contact shadow on the (invisible) ground.
        if rd.y < 0.0 {
            let tg = (GROUND - ro.y) / rd.y;
            let g = ro + rd * tg;
            let d = dist(g);
            let a = 0.20 * (1.0 - smoothstep(0.0, 0.55, d));
            return vec4<f32>(vec3<f32>(0.25, 0.18, 0.22) * a, a);
        }
        return vec4<f32>(0.0);
    }

    let p = ro + rd * t;
    let s = scene(p);
    let pp = to_pet(p);
    let fz = vec3<f32>(vnoise(pp * 38.0), vnoise(pp * 38.0 + 7.1), vnoise(pp * 38.0 + 13.7)) - 0.5;
    let n = normalize(normal(p) + 0.18 * fz * (1.0 - s.g));
    var alb = s.c * (0.97 + 0.06 * vnoise(pp * 90.0));
    let aa = px * 1.5;
    if abs(sd_ellipsoid(pp - head_c(), HEAD_R)) < 0.05 { alb = face(pp, alb, aa); }
    if abs(sd_ellipsoid(pp - muzzle_c(), MUZZLE_R)) < 0.04 { alb = mouth(pp, alb, aa * 2.0); }

    let l = normalize(vec3<f32>(-0.55, 0.85, 0.65));
    let v = -rd;
    let wrap = clamp((dot(n, l) + 0.35) / 1.35, 0.0, 1.0);
    let band = smoothstep(0.28, 0.66, wrap);
    let ao = occlusion(p, n);
    var col = alb * mix(vec3<f32>(0.76, 0.64, 0.66), vec3<f32>(1.04, 1.0, 0.97), band);
    col *= mix(0.70, 1.0, ao);
    // Bounce from below and a soft fuzzy rim: the "plush" look.
    col += alb * 0.10 * clamp(-n.y, 0.0, 1.0) * vec3<f32>(1.0, 0.85, 0.75);
    let fres = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    col += fres * 0.28 * mix(vec3<f32>(1.0, 0.96, 0.92), alb, 0.3) * (0.5 + 0.5 * ao);
    let h = normalize(l + v);
    col += s.g * 0.55 * pow(max(dot(n, h), 0.0), 40.0);
    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
