//! Build tasks for chonkGene.
//!
//! `cargo xtask art [--check]` compiles every `assets/species/<id>/` (rig.ron,
//! standard.svg, mochi.svg) into `crates/chonkgene-rig/compiled/<id>.rig`
//! (PLAN §3.3). `--check` only reports stale output, for CI.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chonkgene_rig::{Bone, Coat, CompiledRig, FORMAT, Part, Rig, RigSource, Verb, Zone};
use usvg::tiny_skia_path::PathSegment;

const USAGE: &str = "usage: cargo xtask art [--check]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("art") => art(args[1..].iter().any(|a| a == "--check")),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace")
        .to_path_buf()
}

fn art(check: bool) -> Result<(), String> {
    let root = workspace_root();
    let out_dir = root.join("crates/chonkgene-rig/compiled");
    let mut dirs: Vec<PathBuf> = fs::read_dir(root.join("assets/species"))
        .map_err(|e| format!("cannot list assets/species: {e}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut stale = Vec::new();
    for dir in dirs {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let rig = compile(&dir).map_err(|e| format!("{name}: {e}"))?;
        // Same checks the app runs when it loads the rig.
        Rig::from_compiled(rig.clone()).map_err(|e| format!("{name}: {e}"))?;
        let bytes = postcard::to_stdvec(&rig).map_err(|e| format!("{name}: {e}"))?;
        let out = out_dir.join(format!("{}.rig", rig.species));
        if fs::read(&out).ok().as_deref() == Some(bytes.as_slice()) {
            println!("{name}: up to date");
        } else if check {
            stale.push(out.display().to_string());
        } else {
            fs::write(&out, &bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
            println!("{name}: wrote {} ({} bytes)", out.display(), bytes.len());
        }
    }
    if stale.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "stale compiled rigs: {}\nrun `cargo xtask art`",
            stale.join(", ")
        ))
    }
}

fn compile(dir: &Path) -> Result<CompiledRig, String> {
    let text = fs::read_to_string(dir.join("rig.ron")).map_err(|e| format!("rig.ron: {e}"))?;
    let src: RigSource = ron::from_str(&text).map_err(|e| format!("rig.ron: {e}"))?;
    let standard = load_svg(&dir.join("standard.svg"))?;
    let mochi = load_svg(&dir.join("mochi.svg"))?;

    let index = |kind: &str, names: &[&str], name: &str| -> Result<u16, String> {
        names
            .iter()
            .position(|n| *n == name)
            .map(|i| i as u16)
            .ok_or_else(|| format!("unknown {kind} {name}"))
    };
    let slot_names: Vec<&str> = src.slots.iter().map(String::as_str).collect();
    let bone_names: Vec<&str> = src.bones.iter().map(|b| b.name.as_str()).collect();
    let part_names: Vec<&str> = src.parts.iter().map(|p| p.id.as_str()).collect();
    for (kind, names) in [
        ("slot", &slot_names),
        ("bone", &bone_names),
        ("part", &part_names),
    ] {
        if let Some(dup) = names
            .iter()
            .enumerate()
            .find(|(i, n)| names[..*i].contains(n))
        {
            return Err(format!("duplicate {kind} {}", dup.1));
        }
    }

    let mut bones = Vec::new();
    for (i, b) in src.bones.iter().enumerate() {
        let parent = match &b.parent {
            Some(p) => {
                let p = index("bone", &bone_names, p)?;
                if usize::from(p) >= i {
                    return Err(format!("bone {} must come after its parent", b.name));
                }
                Some(p)
            }
            None => None,
        };
        bones.push(Bone {
            name: b.name.clone(),
            parent,
            pivot: b.pivot,
        });
    }

    let mut coats = Vec::new();
    for c in &src.coats {
        if let Some(extra) = c.colors.keys().find(|k| !slot_names.contains(&k.as_str())) {
            return Err(format!("coat {}: unknown slot {extra}", c.name));
        }
        let colors = src
            .slots
            .iter()
            .map(|slot| {
                let hex = c
                    .colors
                    .get(slot)
                    .ok_or_else(|| format!("coat {}: no colour for {slot}", c.name))?;
                parse_color(hex).ok_or_else(|| format!("coat {}: bad colour {hex}", c.name))
            })
            .collect::<Result<_, _>>()?;
        coats.push(Coat {
            name: c.name.clone(),
            colors,
        });
    }

    let mut parts = Vec::new();
    for p in &src.parts {
        let (verbs, standard_pts, stroke) =
            extract(&standard, &p.id).map_err(|e| format!("standard.svg: {e}"))?;
        let (mochi_verbs, mochi_pts, _) =
            extract(&mochi, &p.id).map_err(|e| format!("mochi.svg: {e}"))?;
        if verbs != mochi_verbs {
            return Err(format!(
                "part {}: standard.svg and mochi.svg differ in structure",
                p.id
            ));
        }
        parts.push(Part {
            id: p.id.clone(),
            bone: index("bone", &bone_names, &p.bone)?,
            slot: index("slot", &slot_names, &p.slot)?,
            stroke,
            verbs,
            standard: standard_pts,
            mochi: mochi_pts,
        });
    }

    let zones = src
        .zones
        .iter()
        .map(|z| {
            let parts = z
                .parts
                .iter()
                .map(|p| index("part", &part_names, p))
                .collect::<Result<_, _>>()?;
            Ok(Zone {
                name: z.name.clone(),
                parts,
            })
        })
        .collect::<Result<_, String>>()?;

    Ok(CompiledRig {
        format: FORMAT,
        species: src.species,
        size: src.size,
        ground_y: src.ground_y,
        slots: src.slots,
        coats,
        bones,
        parts,
        zones,
        anchors: src.anchors.into_iter().collect(),
    })
}

fn load_svg(path: &Path) -> Result<usvg::Tree, String> {
    let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    usvg::Tree::from_data(&data, &usvg::Options::default())
        .map_err(|e| format!("{}: {e}", path.display()))
}

type Points = Vec<(f32, f32)>;

/// Verbs and absolute points of the element `id`; stroke-only shapes are lines.
fn extract(tree: &usvg::Tree, id: &str) -> Result<(Vec<Verb>, Points, Option<f32>), String> {
    let node = tree
        .node_by_id(id)
        .ok_or_else(|| format!("no element with id {id}"))?;
    let usvg::Node::Path(path) = node else {
        return Err(format!("{id} is not a shape"));
    };
    let data = path
        .data()
        .clone()
        .transform(path.abs_transform())
        .ok_or_else(|| format!("{id}: cannot apply its transform"))?;
    let stroke = match path.fill() {
        Some(_) => None,
        None => Some(
            path.stroke()
                .ok_or_else(|| format!("{id} has neither fill nor stroke"))?
                .width()
                .get(),
        ),
    };
    let mut verbs = Vec::new();
    let mut pts = Vec::new();
    for seg in data.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                verbs.push(Verb::Move);
                pts.push((p.x, p.y));
            }
            PathSegment::LineTo(p) => {
                verbs.push(Verb::Line);
                pts.push((p.x, p.y));
            }
            PathSegment::QuadTo(c, p) => {
                verbs.push(Verb::Quad);
                pts.extend([(c.x, c.y), (p.x, p.y)]);
            }
            PathSegment::CubicTo(c1, c2, p) => {
                verbs.push(Verb::Cubic);
                pts.extend([(c1.x, c1.y), (c2.x, c2.y), (p.x, p.y)]);
            }
            PathSegment::Close => verbs.push(Verb::Close),
        }
    }
    Ok((verbs, pts, stroke))
}

/// `#RRGGBB` or `#RRGGBBAA`.
fn parse_color(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}
