//! Skinned-mesh spike: a pet's signed distance field baked once into a
//! triangle mesh (surface nets), every vertex carrying its coat colour,
//! occlusion, where it sits on the face, and up to four bone weights. Posing
//! then only moves vertices, so a frame costs a raster pass, not a ray march.

use std::io::Read;

use chonkgene_rig::math::{V3, smoothstep, v3};
use chonkgene_rig::sdf::Shape;
use chonkgene_rig::{FLOOR, Layer, Look, Model, Pose, Scene, ZONES};

use crate::raymarch::occlusion;
use crate::skin::{ROOT_PIVOT, Skeleton};

/// One vertex as `mesh.wgsl` reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Rest position; w: ambient occlusion.
    pub pos: [f32; 4],
    /// Rest normal; w: gloss.
    pub normal: [f32; 4],
    /// Coat colour; w: fuzz.
    pub color: [f32; 4],
    /// Position in the eye part's frame; w: 1.0 on its surface.
    pub eye: [f32; 4],
    /// Position in the mouth part's frame; w: 1.0 on its surface, or on a
    /// tail of several (the fox's, for her flame tails, mesh.wgsl) minus
    /// how much it is the tail's and how far along it, 0 root … 1 tip
    /// (`-(1 + along)` fully on a tail).
    pub mouth: [f32; 4],
    pub bones: [u32; 4],
    pub weights: [f32; 4],
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Radii of the parts the eyes and mouth are painted on.
    pub eye_r: V3,
    pub mouth_r: V3,
    /// The shadow's radii across and along the body: what stands on the
    /// floor (legs, belly), a little wider.
    pub shadow: (f32, f32),
    /// Each vertex's hit zone (an index into [`chonkgene_rig::ZONES`]), for
    /// telling where a click lands; kept on the CPU only.
    pub zones: Vec<u8>,
    /// The vertex at the top of the head (ears and all), where a speech
    /// bubble points.
    pub crown: usize,
}

/// Bake `model` at rest (coat `coat`, body value `chonk`) with grid cells
/// `cell` world units across.
pub fn bake(
    model: &Model,
    skeleton: &Skeleton,
    coat: usize,
    chonk: f32,
    cell: f32,
) -> Result<Mesh, String> {
    let rest = Pose::default();
    let scene = Scene::posed(model, coat, chonk, &rest, Look::Open, Layer::All);
    let lo = scene.center - V3::ONE * (scene.radius + 2.0 * cell);
    let n = ((2.0 * (scene.radius + 2.0 * cell)) / cell).ceil() as usize + 1;
    let at = |i: usize, j: usize, k: usize| lo + v3(i as f32, j as f32, k as f32) * cell;
    let field = sample(&scene, n, &at);
    let idx = |i: usize, j: usize, k: usize| (k * n + j) * n + i;

    // One vertex per cell the surface passes through, where its edges
    // cross the surface on average, then pulled onto the surface.
    let mut cell_vertex = vec![u32::MAX; n * n * n];
    let mut points = Vec::new();
    const CORNERS: [(usize, usize, usize); 8] = [
        (0, 0, 0),
        (1, 0, 0),
        (0, 1, 0),
        (1, 1, 0),
        (0, 0, 1),
        (1, 0, 1),
        (0, 1, 1),
        (1, 1, 1),
    ];
    const EDGES: [(usize, usize); 12] = [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for k in 0..n - 1 {
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let d = CORNERS.map(|(a, b, c)| field[idx(i + a, j + b, k + c)]);
                let inside = d.iter().filter(|v| **v < 0.0).count();
                if inside == 0 || inside == 8 {
                    continue;
                }
                let (mut sum, mut count) = (V3::ZERO, 0.0);
                for (a, b) in EDGES {
                    if (d[a] < 0.0) == (d[b] < 0.0) {
                        continue;
                    }
                    let t = d[a] / (d[a] - d[b]);
                    let pa = CORNERS[a];
                    let pb = CORNERS[b];
                    let ca = at(i + pa.0, j + pa.1, k + pa.2);
                    let cb = at(i + pb.0, j + pb.1, k + pb.2);
                    sum = sum + ca.lerp(cb, t);
                    count += 1.0;
                }
                cell_vertex[idx(i, j, k)] = points.len() as u32;
                points.push(sum / count);
            }
        }
    }

    // A quad across every grid edge the surface crosses, joining the four
    // cells around it.
    let mut indices = Vec::new();
    for k in 1..n - 1 {
        for j in 1..n - 1 {
            for i in 1..n - 1 {
                let here = field[idx(i, j, k)] < 0.0;
                for axis in 0..3 {
                    let (di, dj, dk) = [(1, 0, 0), (0, 1, 0), (0, 0, 1)][axis];
                    if (field[idx(i + di, j + dj, k + dk)] < 0.0) == here {
                        continue;
                    }
                    // The four cells sharing this edge.
                    let cells = match axis {
                        0 => [(i, j - 1, k - 1), (i, j, k - 1), (i, j, k), (i, j - 1, k)],
                        1 => [(i - 1, j, k - 1), (i, j, k - 1), (i, j, k), (i - 1, j, k)],
                        _ => [(i - 1, j - 1, k), (i, j - 1, k), (i, j, k), (i - 1, j, k)],
                    };
                    let v = cells.map(|(a, b, c)| cell_vertex[idx(a, b, c)]);
                    if v.contains(&u32::MAX) {
                        continue;
                    }
                    indices.extend([v[0], v[1], v[2], v[0], v[2], v[3]]);
                }
            }
        }
    }

    let parts: Vec<_> = scene.parts().collect();
    let (eyes, mouth) = scene.face_parts();
    let radii = |i: Option<usize>| match i.map(|i| parts[i].shape) {
        Some(Shape::Ellipsoid(r)) => r,
        _ => V3::ONE,
    };
    let local = |i: usize, p: V3| parts[i].a.apply(p) + parts[i].b;
    let on_face = |i: Option<usize>, p: V3, within: f32| -> [f32; 4] {
        let Some(i) = i else {
            return [0.0; 4];
        };
        let q = local(i, p);
        let on = parts[i].shape.distance(q).abs() < within;
        [q.x, q.y, q.z, if on { 1.0 } else { 0.0 }]
    };
    let mut zones = Vec::with_capacity(points.len());
    // Under the chin is below the nose (or beak, or bill).
    let nose_y = parts
        .iter()
        .find(|pv| ["nose", "beak", "bill"].contains(&pv.part.name.as_str()))
        .map(|pv| pv.center.y);
    let along_tail = tail_places(skeleton);
    let vertices: Vec<Vertex> = points
        .iter()
        .map(|&p0| {
            let mut p = p0;
            for _ in 0..2 {
                p = p - scene.normal(p) * scene.distance(p);
            }
            let nrm = scene.normal(p);
            let s = scene.coat_at(p);
            let (bones, weights) = weigh(&parts, skeleton, p);
            zones.push(refine_zone(
                skeleton, s.zone, p, nrm, &bones, &weights, nose_y,
            ));
            Vertex {
                pos: [p.x, p.y, p.z, occlusion(&scene, p, nrm)],
                normal: [nrm.x, nrm.y, nrm.z, s.gloss],
                color: [s.color.x, s.color.y, s.color.z, s.fuzz],
                eye: on_face(eyes, p, 0.05),
                mouth: {
                    let mut m = on_face(mouth, p, 0.04);
                    let tail = tail_mark(&along_tail, &bones, &weights);
                    if tail < 0.0 && m[3] < 0.5 {
                        m[3] = tail;
                    }
                    m
                },
                bones,
                weights,
            }
        })
        .collect();
    let shadow = shadow_of(&vertices, &zones, skeleton);
    let head = chonkgene_rig::ZONES.iter().position(|z| *z == "head");
    let crown = (0..vertices.len())
        .filter(|&i| head.is_some_and(|h| usize::from(zones[i]) == h))
        .max_by(|&a, &b| vertices[a].pos[1].total_cmp(&vertices[b].pos[1]))
        .unwrap_or(0);
    let mut mesh = Mesh {
        shadow,
        zones,
        crown,
        vertices,
        indices,
        eye_r: radii(eyes),
        mouth_r: radii(mouth),
    };
    // What was made elsewhere for this look (the fox's human form: all
    // but her painted head).
    for m in model.meshes_on(coat).filter(|m| !m.src.is_empty()) {
        let mut text = String::new();
        flate2::read::GzDecoder::new(&m.src[..])
            .read_to_string(&mut text)
            .map_err(|e| format!("mesh {:?}: {e}", m.name))?;
        import(&mut mesh, model, skeleton, coat, "back", &text)
            .map_err(|e| format!("mesh {:?}: {e}", m.name))?;
    }
    Ok(mesh)
}

/// The shadow's radii across and along the body: what stands on the floor
/// (legs, belly, feet), a little wider. Tails, hair and a human form's arms
/// never stand on it, however low they hang (the fox's human form, made
/// chubbier with shorter legs, 2026-11-24: her outer tails came down past
/// her knees, her long hair past her hips, her hands, held out as she was
/// made, below the line her legs reach to).
fn shadow_of(vertices: &[Vertex], zones: &[u8], sk: &Skeleton) -> (f32, f32) {
    let hangs = ["tail", "head"].map(|name| ZONES.iter().position(|z| *z == name));
    let arm: Vec<bool> = sk
        .names
        .iter()
        .map(|n| n.starts_with("arm_") || n.starts_with("fore_"))
        .collect();
    let on_arm = |v: &Vertex| {
        let main = (0..4).max_by(|&a, &b| v.weights[a].total_cmp(&v.weights[b]));
        main.is_some_and(|k| arm.get(v.bones[k] as usize).copied().unwrap_or(false))
    };
    let reach = |axis: usize, centre: f32| {
        vertices
            .iter()
            .zip(zones)
            .filter(|(v, z)| {
                v.pos[1] < FLOOR + 0.5 && !hangs.contains(&Some(usize::from(**z))) && !on_arm(v)
            })
            .map(|(v, _)| (v.pos[axis] - centre).abs())
            .fold(0.3f32, f32::max)
            * 1.15
    };
    (reach(0, ROOT_PIVOT.x), reach(2, ROOT_PIVOT.z))
}

/// The field on an `n`³ grid, a slab of it per thread.
fn sample(scene: &Scene, n: usize, at: &(dyn Fn(usize, usize, usize) -> V3 + Sync)) -> Vec<f32> {
    let mut field = vec![0.0f32; n * n * n];
    let threads = std::thread::available_parallelism().map_or(4, |t| t.get());
    let per = n.div_ceil(threads);
    std::thread::scope(|s| {
        for (t, slab) in field.chunks_mut(per * n * n).enumerate() {
            s.spawn(move || {
                for (o, d) in slab.iter_mut().enumerate() {
                    let k = t * per + o / (n * n);
                    let j = (o / n) % n;
                    let i = o % n;
                    *d = scene.distance(at(i, j, k));
                }
            });
        }
    });
    field
}

/// Up to four bones and their weights for surface point `p`: each part
/// pulls toward its bone by how near it is, fading out across the seam's
/// blend, so a seam bends smoothly and nothing past it follows; tips and
/// jiggles then take their share.
/// The hit zones the model's parts cannot give: the tail (skin that moves
/// mostly with a `tail…` bone), a bird's wings (with a `wing…` bone),
/// under the chin (head skin that wobbles with the `chin` soft spot, below
/// the nose), and the body's skin away from the `butt` soft spot split
/// into back and belly. The rest keep their part's.
fn refine_zone(
    sk: &Skeleton,
    zone: u8,
    p: V3,
    normal: V3,
    bones: &[u32; 4],
    weights: &[f32; 4],
    nose_y: Option<f32>,
) -> u8 {
    let index = |name: &str| ZONES.iter().position(|z| *z == name).map(|i| i as u8);
    let name = |b: u32| sk.names[b as usize].as_str();
    let share = |on: &dyn Fn(&str) -> bool| -> f32 {
        bones
            .iter()
            .zip(weights)
            .filter(|(b, _)| on(name(**b)))
            .map(|(_, w)| *w)
            .sum()
    };
    // A long tail spreads its skin over several segments' bones; a bird's
    // short one blends into its bottom.
    let tail = |n: &str| n.starts_with("tail");
    if share(&tail) > 0.5 || (tail(name(bones[0])) && weights[0] > 0.35) {
        return index("tail").unwrap_or(zone);
    }
    if share(&|n| n.starts_with("wing")) > 0.5 {
        return index("wing").unwrap_or(zone);
    }
    let chin = share(&|n| n == "chin");
    let on_head = [index("head"), index("cheek")].contains(&Some(zone));
    if on_head && chin > 0.25 && nose_y.is_some_and(|y| p.y < y - 0.02) {
        return index("chin").unwrap_or(zone);
    }
    // The body (forward is +z, up +y): the rump, round the `butt` soft
    // spot, is the bottom; the rest of the body's skin is its back where
    // it faces up (a bird's, standing upright, wherever it does not face
    // forward or down) and its belly elsewhere. The
    // model's own belly patch stays the belly.
    // The rump: round the soft spot, or a four-legged pet's hindquarters
    // facing back.
    let rump =
        share(&|n| n == "butt") >= 0.2 || (share(&|n| n == "hips") > 0.5 && normal.z < -0.35);
    if zone != index("bottom").unwrap_or(u8::MAX) || rump {
        return zone;
    }
    let bird = sk.names.iter().any(|n| n.starts_with("wing"));
    let back = if bird {
        // Upright: the front and the underside are its belly, the rest
        // (sides, top, back) its back.
        normal.z <= 0.2 && normal.y > -0.3
    } else {
        normal.y > -0.15
    };
    index(if back { "back" } else { "belly" }).unwrap_or(zone)
}

/// Several tails (the fox's): each tail bone's place along its tail, the
/// middle of its segment, 0 root … 1 tip; `None` off the tails, and for
/// every bone of a pet with one tail.
fn tail_places(sk: &Skeleton) -> Vec<Option<f32>> {
    let mut places = vec![None; sk.names.len()];
    let chains = sk.tails();
    if chains.len() > 1 {
        for c in &chains {
            for (k, &b) in c.iter().enumerate() {
                places[b] = Some((k as f32 + 0.5) / c.len() as f32);
            }
        }
    }
    places
}

/// A vertex's mark for the shader's burning tails (mouth.w): minus how
/// much it is a tail's, and past −1 how far along it; 0 off the tails.
fn tail_mark(places: &[Option<f32>], bones: &[u32; 4], weights: &[f32; 4]) -> f32 {
    let (mut tail, mut along) = (0.0, 0.0);
    for (&b, &w) in bones.iter().zip(weights) {
        if let Some(a) = places.get(b as usize).copied().flatten() {
            tail += w;
            along += w * a;
        }
    }
    if tail > 0.0 {
        -(tail + if tail > 0.99 { along / tail } else { 0.0 })
    } else {
        0.0
    }
}

fn weigh(parts: &[chonkgene_rig::PartView], sk: &Skeleton, p: V3) -> ([u32; 4], [f32; 4]) {
    let mut w = vec![0.0f32; sk.names.len()];
    let d: Vec<f32> = parts
        .iter()
        .map(|pv| pv.shape.distance(pv.a.apply(p) + pv.b) * pv.k)
        .collect();
    let (near, dmin) = d
        .iter()
        .copied()
        .enumerate()
        .fold((0, f32::MAX), |a, (i, x)| if x < a.1 { (i, x) } else { a });
    for (i, (pv, &dp)) in parts.iter().zip(&d).enumerate() {
        // Softened over the blend of the seam this part makes with the
        // nearest one, which is the later part's (each blends into those
        // before it): a wide-blending rump reaches the chest it melts into,
        // not the front paws.
        let seam = parts[i.max(near)].blend.max(0.04);
        w[pv.part.bone] += 1.0 - smoothstep(0.0, seam, dp - dmin);
    }
    // A soft spot takes its share from the bone it hangs on only: a belly
    // wobbles the chest's skin, never the legs beside it.
    for &(bone, at, radius, share) in &sk.jiggles {
        // A soft lump: all of its share near the middle, fading out to the rim.
        let f = share * (1.0 - smoothstep(0.35 * radius, radius, (p - at).length()));
        if let Some(parent) = sk.parents[bone] {
            let moved = w[parent] * f;
            w[parent] -= moved;
            w[bone] += moved;
        }
    }
    let mut ranked: Vec<(usize, f32)> = w.into_iter().enumerate().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let top = &ranked[..4.min(ranked.len())];
    let sum: f32 = top.iter().map(|t| t.1).sum::<f32>().max(1e-6);
    let mut bones = [0u32; 4];
    let mut weights = [0.0f32; 4];
    for (k, &(b, x)) in top.iter().enumerate() {
        bones[k] = b as u32;
        weights[k] = x / sum;
    }
    (bones, weights)
}

/// Add a mesh made elsewhere (by a Blender script beside the model, e.g.
/// `assets/species/fox/hair_blend.py`: hair strands, sharp edges and
/// tapering tips that a blend of rounded shapes cannot make) to `mesh`,
/// in zone `zone`. `src` is text, one item a line:
///
/// - `v x y z  nx ny nz  slot slot t  gloss fuzz ao  bone weight …`: a
///   vertex at rest, its normal, its colour (the model's two colour slots
///   by name, mixed `t` of the way to the second), its shine, fuzz and
///   ambient occlusion, and up to four bones by name with their weights;
/// - `f a b c`: a triangle, the vertices counted from 0 in this file;
/// - `z zone`: the vertices after it are in touch zone `zone` (a figure
///   made whole in Blender: its hands, robe and ears answer as theirs);
/// - `w x y half slot`: the vertex before it is on a woven border, `x`
///   along the band and `y` across it (-1 … 1), both in half-widths,
///   the band `half` wide on each side, its pattern in colour `slot`
///   (mesh.wgsl `woven` draws the pattern, sharp at any size).
///
/// The colours come from `model`'s coat `coat`, so a dyed model dyes it.
/// Nothing here is painted (no face); vertices on a tail are marked to
/// burn as baked ones are. The crown and the shadow take it in.
pub fn import(
    mesh: &mut Mesh,
    model: &Model,
    sk: &Skeleton,
    coat: usize,
    zone: &str,
    src: &str,
) -> Result<(), String> {
    let zone_of = |zone: &str| {
        ZONES
            .iter()
            .position(|z| *z == zone)
            .map(|z| z as u8)
            .ok_or_else(|| format!("no zone {zone:?}"))
    };
    let mut zone = zone_of(zone)?;
    let colors = &model
        .coats
        .get(coat)
        .ok_or_else(|| format!("no coat {coat}"))?
        .colors;
    let slot = |name: &str| -> Result<V3, String> {
        let i = model
            .slots
            .iter()
            .position(|s| s == name)
            .ok_or_else(|| format!("no colour slot {name:?}"))?;
        let c = colors[i];
        Ok(v3(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0)
    };
    let places = tail_places(sk);
    let base = mesh.vertices.len() as u32;
    let mut count = 0u32;
    for (n, line) in src.lines().enumerate() {
        let bad = |what: &str| format!("line {}: {what}: {line:?}", n + 1);
        let mut it = line.split_whitespace();
        let num = |s: Option<&str>| -> Result<f32, String> {
            s.and_then(|s| s.parse().ok())
                .ok_or_else(|| bad("a number"))
        };
        match it.next() {
            Some("v") => {
                let mut f = [0.0f32; 6];
                for x in &mut f {
                    *x = num(it.next())?;
                }
                let a = slot(it.next().ok_or_else(|| bad("a colour"))?)?;
                let b = slot(it.next().ok_or_else(|| bad("a colour"))?)?;
                let t = num(it.next())?;
                let (gloss, fuzz, ao) = (num(it.next())?, num(it.next())?, num(it.next())?);
                let mut bones = [0u32; 4];
                let mut weights = [0.0f32; 4];
                let mut k = 0;
                while let Some(name) = it.next() {
                    if k == 4 {
                        return Err(bad("more than four bones"));
                    }
                    let b = sk.bone(name).ok_or_else(|| bad("no such bone"))?;
                    bones[k] = b as u32;
                    weights[k] = num(it.next())?;
                    k += 1;
                }
                let sum: f32 = weights.iter().sum();
                if k == 0 || sum <= 0.0 {
                    return Err(bad("no bone"));
                }
                for w in &mut weights {
                    *w /= sum;
                }
                let c = a.lerp(b, t);
                mesh.vertices.push(Vertex {
                    pos: [f[0], f[1], f[2], ao],
                    normal: [f[3], f[4], f[5], gloss],
                    color: [c.x, c.y, c.z, fuzz],
                    eye: [0.0; 4],
                    // Tails burn when she casts, as baked ones do.
                    mouth: [0.0, 0.0, 0.0, tail_mark(&places, &bones, &weights)],
                    bones,
                    weights,
                });
                mesh.zones.push(zone);
                count += 1;
            }
            Some("f") => {
                for _ in 0..3 {
                    let i: u32 = it
                        .next()
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| bad("a vertex"))?;
                    mesh.indices.push(base + i);
                }
            }
            Some("z") => zone = zone_of(it.next().ok_or_else(|| bad("a zone"))?)?,
            Some("w") => {
                let (x, y, half) = (num(it.next())?, num(it.next())?, num(it.next())?);
                let c = slot(it.next().ok_or_else(|| bad("a colour"))?)?;
                if count == 0 {
                    return Err(bad("a woven mark before any vertex"));
                }
                let v = mesh.vertices.last_mut().expect("a vertex");
                // eye.w -1 marks it woven (a face's is +1); mouth.w stays 0
                // (no mouth painted, no tail).
                v.eye = [x, y, half, -1.0];
                v.mouth = [c.x, c.y, c.z, 0.0];
            }
            Some(_) => return Err(bad("not a v, an f, a z or a w")),
            None => {}
        }
    }
    if let Some(&i) = mesh.indices[..].iter().find(|&&i| i >= base + count) {
        return Err(format!("a triangle names vertex {} of {count}", i - base));
    }
    mesh.shadow = shadow_of(&mesh.vertices, &mesh.zones, sk);
    // The crown (where bubbles hang from) may now be the top of the hair.
    let head = ZONES.iter().position(|z| *z == "head").map(|h| h as u8);
    if let Some(top) = (0..mesh.vertices.len())
        .filter(|&i| Some(mesh.zones[i]) == head)
        .max_by(|&a, &b| mesh.vertices[a].pos[1].total_cmp(&mesh.vertices[b].pos[1]))
    {
        mesh.crown = top;
    }
    Ok(())
}
