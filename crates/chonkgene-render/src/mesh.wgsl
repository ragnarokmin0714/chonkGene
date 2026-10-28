// Draws a baked, skinned pet mesh (mesh.rs): every vertex moves with up to
// four bones, then the same plush shading as pet.wgsl lights it, with the
// occlusion baked at rest and the face painted per pixel (paint.wgsl).
// A soft shadow on the floor is drawn first, under the pet.

struct Uniforms {
    // As pet.wgsl, so paint.wgsl reads the face the same way.
    cam: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    fwd: vec4<f32>,
    anchor: vec4<f32>,
    bound: vec4<f32>,
    face: vec4<f32>,
    eye: vec4<f32>,
    iris: vec4<f32>,
    brows: vec4<f32>,
    spots: vec4<f32>,
    blush: vec4<f32>,
    frame: vec4<f32>,
    palette: array<vec4<f32>, 16>,
    // Radii of the parts the eyes and mouth are painted on; eye_r.w: the
    // pet's turn about the vertical, radians.
    eye_r: vec4<f32>,
    mouth_r: vec4<f32>,
    // The shadow: centre x and z on the floor, radii across and along the pet.
    shadow: vec4<f32>,
    // Three rows of [m | t] per bone.
    bones: array<vec4<f32>, 192>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

const FLOOR: f32 = -1.1;

fn clamp01(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

fn ss(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp01((x - e0) / (e1 - e0));
    return t * t * (3.0 - 2.0 * t);
}

// World point → clip space: the camera looks along `fwd`, pixels are
// `ppu` world units apart, and the floor under the pet sits `cam.w` pixels
// above the bottom.
fn project(p: vec3<f32>) -> vec4<f32> {
    let rel = p - u.anchor.xyz;
    let x = dot(rel, u.right.xyz) * u.cam.z + u.cam.x / 2.0;
    let y = u.cam.y - u.cam.w - dot(rel, u.up.xyz) * u.cam.z;
    let depth = 0.5 + 0.08 * dot(rel, u.fwd.xyz);
    return vec4<f32>(x / u.cam.x * 2.0 - 1.0, 1.0 - y / u.cam.y * 2.0, depth, 1.0);
}

struct In {
    @location(0) pos: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) eye: vec4<f32>,
    @location(4) mouth: vec4<f32>,
    @location(5) bones: vec4<u32>,
    @location(6) weights: vec4<f32>,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    // Rest position, for fur noise that sticks to the pet.
    @location(1) rest: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) eye: vec4<f32>,
    @location(4) mouth: vec4<f32>,
    // x: occlusion, y: gloss.
    @location(5) shade: vec2<f32>,
}

fn bone_point(b: u32, p: vec3<f32>) -> vec3<f32> {
    let r0 = u.bones[b * 3u];
    let r1 = u.bones[b * 3u + 1u];
    let r2 = u.bones[b * 3u + 2u];
    return vec3<f32>(dot(r0.xyz, p) + r0.w, dot(r1.xyz, p) + r1.w, dot(r2.xyz, p) + r2.w);
}

fn bone_dir(b: u32, d: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(u.bones[b * 3u].xyz, d), dot(u.bones[b * 3u + 1u].xyz, d), dot(u.bones[b * 3u + 2u].xyz, d));
}

@vertex
fn vs_pet(v: In) -> Out {
    var p = vec3<f32>(0.0);
    var n = vec3<f32>(0.0);
    for (var k = 0; k < 4; k++) {
        let w = v.weights[k];
        if w > 0.0 {
            p += bone_point(v.bones[k], v.pos.xyz) * w;
            n += bone_dir(v.bones[k], v.normal.xyz) * w;
        }
    }
    var o: Out;
    o.clip = project(p);
    o.normal = n;
    o.rest = v.pos.xyz;
    o.color = v.color;
    o.eye = v.eye;
    o.mouth = v.mouth;
    o.shade = vec2<f32>(v.pos.w, v.normal.w);
    return o;
}

@fragment
fn fs_pet(i: Out) -> @location(0) vec4<f32> {
    let ppu = u.cam.z;
    let px = 1.0 / ppu;
    var color = i.color.rgb;
    let fuzz = i.color.w;
    if i.eye.w > 0.5 {
        color = paint_eyes(i.eye.xyz, u.eye_r.xyz, color, px);
    }
    if i.mouth.w > 0.5 {
        color = paint_mouth(i.mouth.xyz, u.mouth_r.xyz, color, px);
    }
    let pp = i.rest;
    let f = ppu * 0.3;
    let fz = vec3<f32>(vnoise(pp * f), vnoise(pp * f + vec3<f32>(7.1)), vnoise(pp * f + vec3<f32>(13.7)))
        - vec3<f32>(0.5);
    let n = normalize(normalize(i.normal) + fz * (0.14 * fuzz));
    let alb = color * (0.97 + 0.06 * fuzz * vnoise(pp * (f * 1.7)));
    let l = normalize(vec3<f32>(-0.55, 0.85, 0.65));
    let v = -u.fwd.xyz;
    let wrap = clamp01((dot(n, l) + 0.35) / 1.35);
    let band = ss(0.28, 0.66, wrap);
    let ao = i.shade.x;
    let tone = mix(vec3<f32>(0.76, 0.64, 0.66), vec3<f32>(1.04, 1.0, 0.97), band);
    var col = alb * tone * (0.70 + 0.30 * ao);
    col += alb * vec3<f32>(1.0, 0.85, 0.75) * (0.10 * clamp01(-n.y));
    let fres = pow(1.0 - max(dot(n, v), 0.0), 2.5);
    col += mix(vec3<f32>(1.0, 0.96, 0.92), alb, 0.3) * (fres * 0.28 * (0.5 + 0.5 * ao));
    let hv = normalize(l + v);
    col += vec3<f32>(i.shade.y * 0.55 * pow(max(dot(n, hv), 0.0), 40.0));
    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

struct ShadowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) at: vec2<f32>,
}

// A quad on the floor under the pet, turned with it.
@vertex
fn vs_shadow(@builtin(vertex_index) k: u32) -> ShadowOut {
    let c = vec2<f32>(select(-1.0, 1.0, (k & 1u) == 1u), select(-1.0, 1.0, k >= 2u));
    let r = c * u.shadow.zw * 1.6;
    let yaw = u.eye_r.w;
    let s = sin(yaw);
    let co = cos(yaw);
    let world = vec3<f32>(u.shadow.x + co * r.x + s * r.y, FLOOR, u.shadow.y - s * r.x + co * r.y);
    var o: ShadowOut;
    o.clip = project(world);
    o.clip.z = 0.999;
    o.at = c * 1.6;
    return o;
}

@fragment
fn fs_shadow(i: ShadowOut) -> @location(0) vec4<f32> {
    let a = 0.20 * (1.0 - ss(0.45, 1.25, length(i.at)));
    return vec4<f32>(vec3<f32>(0.25, 0.18, 0.22) * a, a);
}
