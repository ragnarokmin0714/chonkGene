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

fn tapered(uv: vec2<f32>, a: vec2<f32>, b: vec2<f32>, r1: f32, r2: f32) -> f32 {
    let pa = uv - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - (r1 + (r2 - r1) * h);
}

// Where the fox's forehead mark stands, and her crest when casting.
const CREST_AT: vec2<f32> = vec2<f32>(0.0, 0.45);

// A leaf from `a` to `b`, `w` wide at its middle, pointed at both ends, as
// face.rs `leaf`.
fn leaf(uv: vec2<f32>, a: vec2<f32>, b: vec2<f32>, w: f32) -> f32 {
    let pa = uv - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    let r = w * pow(sin(3.14159265 * h), 0.7) + 0.003;
    return length(pa - ba * h) - r;
}

// The fox's marks at rest, as face.rs `fox_marks`: three leaves fanned up
// on the forehead, two long strokes back along each cheek. x: coverage;
// y: how far out toward the ears (the strokes pale toward their ends).
fn fox_marks(uv: vec2<f32>, aa: f32) -> vec2<f32> {
    var d = leaf(uv, vec2<f32>(0.0, 0.45), vec2<f32>(0.0, 0.71), 0.05);
    for (var k = 0; k < 2; k++) {
        let side = select(-1.0, 1.0, k == 1);
        d = min(d, leaf(uv, vec2<f32>(0.03 * side, 0.44), vec2<f32>(0.15 * side, 0.62), 0.038));
        d = min(d, tapered(uv, vec2<f32>(0.5 * side, -0.2), vec2<f32>(0.88 * side, -0.1), 0.04, 0.005));
        d = min(d, tapered(uv, vec2<f32>(0.52 * side, -0.31), vec2<f32>(0.86 * side, -0.25), 0.034, 0.005));
    }
    return vec2<f32>(ink(d, aa), ss(0.55, 0.86, abs(uv.x)));
}

// The seal in the fox's human form, as face.rs `seal_marks`, placed from
// the eyes (`eye`: their middle x, y and radii): a small crest low between
// the brows, a leaf over the inner half of each eye for a brow, two short
// stripes on each cheek under the eye. Coverage.
fn seal_marks(uv: vec2<f32>, aa: f32, eye: vec4<f32>) -> f32 {
    let ex = eye.x;
    let ey = eye.y;
    let rx = eye.z;
    let ry = eye.w;
    let foot = ey + 0.55 * ry;
    var d = tapered(uv, vec2<f32>(0.0, foot + 0.04), vec2<f32>(0.0, foot + 0.2), 0.026, 0.003);
    for (var k = 0; k < 2; k++) {
        let side = select(-1.0, 1.0, k == 1);
        d = min(d, leaf(uv, vec2<f32>(0.012 * side, foot), vec2<f32>(0.085 * side, foot + 0.17), 0.024));
        let b0 = vec2<f32>((ex - 1.45 * rx) * side, ey + 1.32 * ry);
        let b1 = vec2<f32>((ex + 0.2 * rx) * side, ey + 1.93 * ry);
        d = min(d, leaf(uv, b0, b1, 0.042));
        for (var i = 0; i < 2; i++) {
            let fi = f32(i);
            let outer = vec2<f32>((ex + (0.42 - 0.08 * fi) * rx) * side, ey - (1.31 + 0.3 * fi) * ry);
            let inner = vec2<f32>((ex - (0.58 - 0.12 * fi) * rx) * side, ey - (1.66 + 0.3 * fi) * ry);
            d = min(d, tapered(uv, outer, inner, 0.034 - 0.006 * fi, 0.005));
        }
    }
    return ink(d, aa);
}

// A big chibi eye, as face.rs `chibi_eye`.
fn chibi_eye(c0: vec3<f32>, uv: vec2<f32>, ec: vec2<f32>, r: vec2<f32>, side: f32, iris: vec3<f32>, ink_c: vec3<f32>, aa: f32) -> vec3<f32> {
    let rx = r.x;
    let ry = r.y;
    let white = vec3<f32>(1.0);
    let open = ell(uv, ec, r);
    var c = mix(c0, vec3<f32>(0.98, 0.97, 0.99), ink(open, aa));
    let ic = vec2<f32>(ec.x, ec.y - 0.04 * ry);
    let ir = vec2<f32>(0.8 * rx, 0.95 * ry);
    let di = max(ell(uv, ic, ir), open);
    let t = ss(-ir.y, ir.y, uv.y - ic.y);
    let shade = mix(mix(iris, white, 0.4), iris * 0.45, t);
    c = mix(c, shade, ink(di, aa));
    let rim = max(abs(ell(uv, ic, ir)) * ir.x - 0.012, open);
    c = mix(c, iris * 0.35, ink(rim, aa));
    c = mix(c, mix(iris * 0.18, ink_c, 0.5), ink(max(ell(uv, ic, vec2<f32>(0.46 * rx, 0.52 * ry)), open), aa));
    let top = ss(ec.y - 0.35 * ry, ec.y + 0.7 * ry, uv.y);
    let lash = max(abs(open) * rx - (0.004 + 0.03 * top), ec.y - 0.2 * ry - uv.y);
    let flick = tapered(
        uv,
        vec2<f32>(ec.x + 0.8 * rx * side, ec.y + 0.45 * ry),
        vec2<f32>(ec.x + (rx + 0.1) * side, ec.y + 0.75 * ry),
        0.03,
        0.004,
    );
    c = mix(c, ink_c, ink(min(lash, flick), aa));
    let hl = vec2<f32>(ec.x + 0.3 * rx, ec.y + 0.32 * ry);
    c = mix(c, white, ink(length(uv - hl) - 0.24 * rx, aa));
    let hl2 = vec2<f32>(ec.x - 0.28 * rx, ec.y - 0.38 * ry);
    return mix(c, white, 0.85 * ink(length(uv - hl2) - 0.11 * rx, aa));
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
const BLEP: i32 = 9;
const PANT: i32 = 10;

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
        if (look == OPEN || look == BLEP) && u.style.x > 0.5 {
            c = chibi_eye(c, uv, ec, vec2<f32>(rx, ry), side, iris_c, ink_c, aa);
        } else if look == OPEN || look == BLEP {
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
        } else if look == HAPPY || look == LAUGH || look == PANT {
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
    if u.spots.w >= 0.0 {
        let mark = u.palette[u32(u.spots.w)].rgb;
        let g = u.fwd.w;
        // Casting, the rest marks give way to the seal (咒印) as it grows.
        if u.style.y > 0.5 {
            c = mix(c, mark, seal_marks(uv, aa, u.eye) * (1.0 - ss(0.2, 0.7, g)));
        } else {
            let m0 = fox_marks(uv, aa);
            c = mix(c, mix(mark, white, 0.35 * m0.y), m0.x * (1.0 - ss(0.2, 0.7, g)));
        }
        if g > 0.0 {
            let m = spell_marks(uv, aa, g);
            c = mix(c, mark * 0.85, m.x * min(2.0 * g, 1.0));
        }
    }
    return c;
}

// The fox casting (u.fwd.w, 0 … 1; mesh.wgsl only): her seal (咒印, after
// Roger's 紅髮版配件 picture): on the forehead a crest drawing itself up from
// its foot, a tall flame drop with a round hollow in it and a curling flame
// wing either side (a trident, a lotus seen from the front); a leaf at each
// brow; three stacked stripes along each cheek, drawn out toward the ears;
// a ring round each eye.
// x: how much of `uv` they cover; y: a soft halo round them.
fn spell_marks(uv: vec2<f32>, aa: f32, g: f32) -> vec2<f32> {
    let rise = ss(0.0, 0.6, g);
    let top = CREST_AT + vec2<f32>(0.0, 0.36 * rise);
    var d = tapered(uv, CREST_AT, top, 0.065 * rise, 0.004);
    // The hollow in the drop, cut out once it has risen past it.
    d = max(d, -(length(uv - vec2<f32>(0.0, 0.53)) - 0.026 * ss(0.3, 0.7, g)));
    let wing = ss(0.25, 0.8, g);
    for (var k = 0; k < 2; k++) {
        let side = select(-1.0, 1.0, k == 1);
        // A wing: up and out from the crest's foot, curling back in at its
        // tip (two leaves meeting at an elbow).
        let w0 = vec2<f32>(0.035 * side, 0.43);
        let w1 = vec2<f32>(0.17 * side, 0.57);
        let w2 = vec2<f32>(0.12 * side, 0.72);
        d = min(d, leaf(uv, w0, mix(w0, w1, wing), 0.04 * wing + 0.001));
        d = min(d, tapered(uv, w1, mix(w1, w2, ss(0.5, 0.9, g)), 0.022 * wing, 0.003));
        // The brow leaves.
        let bl = ss(0.45, 1.0, g);
        let b0 = vec2<f32>(0.2 * side, 0.33);
        d = min(d, leaf(uv, b0, mix(b0, vec2<f32>(0.4 * side, 0.4), bl), 0.045 * bl + 0.001));
        for (var i = 0; i < 3; i++) {
            let fi = f32(i);
            let a = vec2<f32>((0.5 + 0.01 * fi) * side, -0.16 - 0.085 * fi);
            let b = vec2<f32>((0.93 - 0.05 * fi) * side, -0.04 - 0.07 * fi);
            d = min(d, tapered(uv, a, mix(a, b, g), 0.036, 0.006));
        }
        // A ring round the eye (the picture's full eyeliner).
        let ec = vec2<f32>(u.eye.x * side, u.eye.y);
        let ring = length((uv - ec) / vec2<f32>(u.eye.z * 1.22, u.eye.w * 1.18)) - 1.0;
        d = min(d, abs(ring) * u.eye.z - 0.01 * ss(0.5, 1.0, g));
    }
    return vec2<f32>(ink(d, aa), ink(d - 0.04, aa * 6.0));
}

// Light the casting fox gives off, over the lit colour `lit`: the forehead
// flame blazing with a halo, the irises lit up round dark pupils, the seal
// lines shining.
fn spell_glow(q: vec3<f32>, r: vec3<f32>, lit: vec3<f32>, px: f32) -> vec3<f32> {
    let g = u.fwd.w;
    let n = normalize(q / r);
    if g <= 0.0 || n.z < 0.15 || u.spots.w < 0.0 {
        return lit;
    }
    let uv = n.xy;
    let aa = px / r.y;
    let white = vec3<f32>(1.0);
    let mark = u.palette[u32(u.spots.w)].rgb;
    let hot = vec3<f32>(1.0, 0.42, 0.36);
    var c = lit;
    var eye_c = mix(hot, white, 0.35);
    var pupil = 0.55;
    if u.iris.x >= 0.0 {
        eye_c = mix(u.palette[u32(u.iris.x)].rgb, white, 0.45);
        pupil = u.iris.y;
    }
    for (var k = 0; k < 2; k++) {
        let side = select(-1.0, 1.0, k == 1);
        let ec = vec2<f32>(u.eye.x * side, u.eye.y);
        let er = vec2<f32>(u.eye.z, u.eye.w);
        let ring = ink(ell(uv, ec, er), aa) * (1.0 - ink(ell(uv, ec, er * pupil), aa));
        let halo = ink(ell(uv, ec, er * vec2<f32>(1.6, 1.5)), aa * 10.0);
        c = mix(c, eye_c, g * (0.2 * halo + 0.75 * ring));
    }
    let m = spell_marks(uv, aa, g);
    // A soft glow round the seal, not over it: the seal itself stays the
    // mark's deep colour (Roger's picture).
    c = mix(c, mix(hot, white, 0.4), g * 0.22 * m.y * (1.0 - m.x));
    // The crest blazing, a halo round it.
    let halo = ink(ell(uv, CREST_AT + vec2<f32>(0.0, 0.16), vec2<f32>(0.24, 0.3)), aa * 12.0);
    c = mix(c, mix(hot, white, 0.5), g * 0.2 * halo);
    return clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
}

// A tongue from `a` down to `b`, `r` wide, as face.rs `tongue`.
fn tongue(c0: vec3<f32>, uv: vec2<f32>, a: vec2<f32>, b: vec2<f32>, r: f32, aa: f32) -> vec3<f32> {
    let t = segment(uv, a, b, r);
    let c = mix(c0, vec3<f32>(1.0, 0.52, 0.56), ink(t, aa));
    let groove = max(segment(uv, vec2<f32>(a.x, a.y - 0.02), vec2<f32>(b.x, b.y + 0.6 * r), 0.012), t);
    return mix(c, vec3<f32>(0.86, 0.36, 0.42), ink(groove, aa));
}

// A child's mouth at rest, as face.rs `chibi_smile`: a wide, shallow closed
// smile, a blush of pink lip under it, no line up to the nose.
fn chibi_smile(c0: vec3<f32>, uv: vec2<f32>, ink_c: vec3<f32>, aa: f32) -> vec3<f32> {
    let lip = ell(uv, vec2<f32>(0.0, -0.32), vec2<f32>(0.34, 0.16));
    let c = mix(c0, vec3<f32>(0.96, 0.62, 0.62), 0.3 * ink(lip, aa * 6.0));
    let line = max(max(arc(uv, vec2<f32>(0.0, 1.0), 1.12, 0.05), uv.y - 0.1), abs(uv.x) - 0.44);
    return mix(c, mix(ink_c, vec3<f32>(0.75, 0.35, 0.35), 0.4), ink(line, aa));
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
    } else if look == PANT {
        let m = max(length(vec2<f32>(uv.x / 1.1, (uv.y + 0.22) / 0.8)) - 0.26, uv.y + 0.12);
        c = mix(c, vec3<f32>(0.55, 0.18, 0.22), ink(m, aa));
        c = tongue(c, uv, vec2<f32>(0.0, -0.28), vec2<f32>(0.0, -0.64), 0.17, aa);
    } else if u.style.x > 0.5 && look != BLEP {
        return chibi_smile(c, uv, ink_c, aa);
    } else {
        for (var k = 0; k < 2; k++) {
            let s = select(-1.0, 1.0, k == 1);
            let d = max(arc(uv, vec2<f32>(0.15 * s, -0.10), 0.15, 0.022), uv.y + 0.10);
            c = mix(c, ink_c, ink(d, aa));
        }
        if look == BLEP {
            c = tongue(c, uv, vec2<f32>(0.0, -0.2), vec2<f32>(0.0, -0.44), 0.13, aa);
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
