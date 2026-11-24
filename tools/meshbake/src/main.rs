//! Simplifies a mesh written by a Blender script (the text format
//! `chonkgene_render::mesh::import` reads) to what the app draws: a pet a
//! couple of hundred pixels tall needs nowhere near the triangles the
//! scripts make for close-up checks.
//!
//! ```text
//! cargo run --release --manifest-path tools/meshbake/Cargo.toml -- <in.mesh> <out.mesh> [error]
//! ```
//!
//! `error` is how far (model units) the surface may move; the default,
//! 0.003, is about a fifth of a pixel at the usual size (58 pixels a unit).
//! Vertices are only taken away, never made, so the ones left keep their
//! colours, bones and woven-border marks as the script wrote them. Where
//! two colours (or zones, borders, shine, or the bone a vertex mostly
//! follows) meet, the vertices stay where they are, so no colour bleeds
//! into the next; the normal, the colour mix, the shading and the place on
//! a woven border weigh in on what may go.
use std::collections::HashMap;
use std::fmt::Write as _;

use meshopt::{SimplifyOptions, SimplifyVertex, VertexDataAdapter};

struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    /// The colour slots, mix, gloss and fuzz as written.
    a: String,
    b: String,
    t: f32,
    gloss: String,
    fuzz: String,
    ao: f32,
    /// Bone names and weights as written.
    bones: Vec<(String, f32)>,
    zone: String,
    /// x, y, half and slot of a woven border.
    woven: Option<(f32, f32, f32, String)>,
}

impl Vertex {
    /// What may not change from one vertex to the next without a line
    /// between them staying put.
    fn kind(&self) -> String {
        let main = self
            .bones
            .iter()
            .max_by(|x, y| x.1.total_cmp(&y.1))
            .map(|b| b.0.as_str())
            .unwrap_or("");
        let woven = match &self.woven {
            Some((_, _, half, slot)) => format!("{half} {slot}"),
            None => String::new(),
        };
        format!(
            "{} {} {} {} {} {} {}",
            self.a, self.b, self.gloss, self.fuzz, self.zone, main, woven
        )
    }
}

fn parse(src: &str) -> Result<(Vec<Vertex>, Vec<u32>), String> {
    let mut verts: Vec<Vertex> = Vec::new();
    let mut idx = Vec::new();
    let mut zone = "back".to_string();
    for (n, line) in src.lines().enumerate() {
        let bad = || format!("line {}: {line:?}", n + 1);
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| -> Result<f32, String> {
            f.get(i).and_then(|s| s.parse().ok()).ok_or_else(bad)
        };
        match f.first().copied() {
            Some("v") => {
                if f.len() < 15 || !(f.len() - 13).is_multiple_of(2) {
                    return Err(bad());
                }
                let mut bones = Vec::new();
                for k in (13..f.len()).step_by(2) {
                    bones.push((f[k].to_string(), num(k + 1)?));
                }
                verts.push(Vertex {
                    pos: [num(1)?, num(2)?, num(3)?],
                    normal: [num(4)?, num(5)?, num(6)?],
                    a: f[7].to_string(),
                    b: f[8].to_string(),
                    t: num(9)?,
                    gloss: f[10].to_string(),
                    fuzz: f[11].to_string(),
                    ao: num(12)?,
                    bones,
                    zone: zone.clone(),
                    woven: None,
                });
            }
            Some("f") => {
                for k in 1..4 {
                    let i: u32 = f.get(k).and_then(|s| s.parse().ok()).ok_or_else(bad)?;
                    idx.push(i);
                }
            }
            Some("z") => zone = f.get(1).ok_or_else(bad)?.to_string(),
            Some("w") => {
                let v = verts.last_mut().ok_or_else(bad)?;
                v.woven = Some((
                    num(1)?,
                    num(2)?,
                    num(3)?,
                    f.get(4).ok_or_else(bad)?.to_string(),
                ));
            }
            Some(_) => return Err(bad()),
            None => {}
        }
    }
    if let Some(i) = idx.iter().find(|&&i| i as usize >= verts.len()) {
        return Err(format!("triangle with vertex {i} of {}", verts.len()));
    }
    Ok((verts, idx))
}

fn write(verts: &[Vertex], idx: &[u32]) -> String {
    let mut out = String::new();
    let mut zone = "";
    for v in verts {
        if v.zone != zone {
            let _ = writeln!(out, "z {}", v.zone);
            zone = &v.zone;
        }
        let [x, y, z] = v.pos;
        let [nx, ny, nz] = v.normal;
        let _ = write!(
            out,
            "v {x:.4} {y:.4} {z:.4} {nx:.3} {ny:.3} {nz:.3} {} {} {:.3} {} {} {:.3}",
            v.a, v.b, v.t, v.gloss, v.fuzz, v.ao
        );
        for (name, w) in &v.bones {
            let _ = write!(out, " {name} {w:.3}");
        }
        out.push('\n');
        if let Some((wx, wy, half, slot)) = &v.woven {
            let _ = writeln!(out, "w {wx:.4} {wy:.3} {half:.4} {slot}");
        }
    }
    for t in idx.chunks(3) {
        let _ = writeln!(out, "f {} {} {}", t[0], t[1], t[2]);
    }
    out
}

/// The triangles of `idx` over fewer of `verts`, the surface moving no
/// more than `error`.
fn simplify(verts: &[Vertex], idx: &[u32], error: f32) -> Vec<u32> {
    let pos: Vec<f32> = verts.iter().flat_map(|v| v.pos).collect();
    let adapter = VertexDataAdapter::new(bytemuck_f32(&pos), 12, 0).expect("positions");
    // What weighs in, besides where the surface is: the normal, the colour
    // mix, the shading, the place across a woven border (along it, where
    // the pattern repeats, it changes evenly, so long triangles keep it).
    const ATTRS: usize = 7;
    let mut attrs = Vec::with_capacity(verts.len() * ATTRS);
    for v in verts {
        let (wx, wy) = v.woven.as_ref().map(|w| (w.0, w.1)).unwrap_or((0.0, 0.0));
        attrs.extend_from_slice(&[v.normal[0], v.normal[1], v.normal[2], v.t, v.ao, wx, wy]);
    }
    // The weights turn each into model units against `error`: a normal
    // turning 0.15, the mix or the shading changing 0.1, the place across
    // a border 0.2 count as much as the surface moving `error` (0.003).
    let w = |change: f32| error / change;
    let weights = [w(0.15), w(0.15), w(0.15), w(0.1), w(0.1), w(20.0), w(0.2)];
    // Where kinds meet, the vertices stay.
    let kinds: Vec<String> = verts.iter().map(Vertex::kind).collect();
    let mut lock = vec![SimplifyVertex::None; verts.len()];
    for t in idx.chunks(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if kinds[a] != kinds[b] || kinds[b] != kinds[c] {
            for i in [a, b, c] {
                lock[i] = SimplifyVertex::Lock;
            }
        }
    }
    let locked = lock.iter().filter(|l| **l == SimplifyVertex::Lock).count();
    eprintln!(
        "{locked} of {} vertices locked where kinds meet",
        verts.len()
    );
    meshopt::simplify_with_attributes_and_locks(
        idx,
        &adapter,
        &attrs,
        &weights,
        ATTRS,
        &lock,
        0,
        error,
        SimplifyOptions::ErrorAbsolute,
        None,
    )
}

fn bytemuck_f32(v: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding and any byte pattern is a valid u8.
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast::<u8>(), std::mem::size_of_val(v)) }
}

/// Only the vertices still used, in the order the triangles (cache
/// ordered) first use them.
fn compact(verts: Vec<Vertex>, idx: &[u32]) -> (Vec<Vertex>, Vec<u32>) {
    let idx = meshopt::optimize_vertex_cache(idx, verts.len());
    let mut new_of: HashMap<u32, u32> = HashMap::new();
    let mut order = Vec::new();
    let out: Vec<u32> = idx
        .iter()
        .map(|&i| {
            *new_of.entry(i).or_insert_with(|| {
                order.push(i as usize);
                (order.len() - 1) as u32
            })
        })
        .collect();
    let mut slots: Vec<Option<Vertex>> = verts.into_iter().map(Some).collect();
    let kept = order
        .iter()
        .map(|&i| slots[i].take().expect("each once"))
        .collect();
    (kept, out)
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, output) = match (args.first(), args.get(1)) {
        (Some(i), Some(o)) => (i, o),
        _ => return Err("meshbake <in.mesh> <out.mesh> [error]".into()),
    };
    let error: f32 = match args.get(2) {
        Some(e) => e.parse().map_err(|_| format!("not a number: {e}"))?,
        None => 0.003,
    };
    let src = std::fs::read_to_string(input).map_err(|e| format!("{input}: {e}"))?;
    let (verts, idx) = parse(&src)?;
    let (n_v, n_t) = (verts.len(), idx.len() / 3);
    let simple = simplify(&verts, &idx, error);
    let (verts, idx) = compact(verts, &simple);
    std::fs::write(output, write(&verts, &idx)).map_err(|e| format!("{output}: {e}"))?;
    println!(
        "{output}: {} vertices, {} triangles (from {n_v}, {n_t}: {:.0}%)",
        verts.len(),
        idx.len() / 3,
        100.0 * idx.len() as f32 / 3.0 / n_t as f32
    );
    Ok(())
}
