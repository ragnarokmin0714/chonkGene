// Shared by pet.wgsl (ray-marched) and mesh.wgsl (skinned mesh): the face
// painted in a part's own frame, as face.rs on the CPU, and the fur noise.
// Each shader's own `u` carries the face uniforms these read, under the same
// names.

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
        if u.spots.z >= 0.0 {
            // Eyeliner: a flick out of the outer corner, under the eye
            // (face.rs `liner`).
            let la = vec2<f32>(ec.x + 0.85 * rx * side, ec.y + 0.1 * ry);
            let lb = vec2<f32>(ec.x + (rx + 0.13) * side, ec.y + 0.55 * ry + 0.05);
            c = mix(c, u.palette[u32(u.spots.z)].rgb, ink(segment(uv, la, lb, 0.018), aa));
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

// ---- fur noise -----------------------------------------------------------------------

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
