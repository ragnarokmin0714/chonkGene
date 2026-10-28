//! Skinned-mesh spike: a pet's signed distance field baked once into a
//! triangle mesh (surface nets), every vertex carrying its coat colour,
//! occlusion, where it sits on the face, and up to four bone weights. Posing
//! then only moves vertices, so a frame costs a raster pass, not a ray march.

use chonkgene_rig::math::{V3, smoothstep, v3};
use chonkgene_rig::sdf::Shape;
use chonkgene_rig::{Layer, Look, Model, Pose, Scene};

use crate::raymarch::occlusion;
use crate::skin::Skeleton;

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
    /// Position in the mouth part's frame; w: 1.0 on its surface.
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
    let vertices = points
        .iter()
        .map(|&p0| {
            let mut p = p0;
            for _ in 0..2 {
                p = p - scene.normal(p) * scene.distance(p);
            }
            let nrm = scene.normal(p);
            let s = scene.coat_at(p);
            let (bones, weights) = weigh(&parts, skeleton, p);
            Vertex {
                pos: [p.x, p.y, p.z, occlusion(&scene, p, nrm)],
                normal: [nrm.x, nrm.y, nrm.z, s.gloss],
                color: [s.color.x, s.color.y, s.color.z, s.fuzz],
                eye: on_face(eyes, p, 0.05),
                mouth: on_face(mouth, p, 0.04),
                bones,
                weights,
            }
        })
        .collect();
    Ok(Mesh {
        vertices,
        indices,
        eye_r: radii(eyes),
        mouth_r: radii(mouth),
    })
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
    for &(from, to, near, far) in &sk.tips {
        let f = smoothstep(near, far, (p - sk.pivots[from]).length());
        let moved = w[from] * f;
        w[from] -= moved;
        w[to] += moved;
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
