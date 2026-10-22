//! Golden snapshots (PLAN §7): the Shiba at each body snap point and coat.
//! `UPDATE_GOLDEN=1 cargo test -p chonkgene-render` rewrites them.

use std::path::PathBuf;

use chonkgene_core::anim::PoseParams;
use chonkgene_render::{BoneMap, Renderer};
use chonkgene_rig::{Rig, builtin};
use tiny_skia::Pixmap;

/// Largest per-channel difference that still counts as equal; tiny-skia's
/// SIMD paths differ slightly between x86-64 and aarch64.
const CHANNEL_TOLERANCE: u8 = 3;
/// Share of pixels allowed beyond the tolerance.
const PIXEL_TOLERANCE: f64 = 0.002;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Pixels where any channel differs by more than the tolerance.
fn differing(a: &Pixmap, b: &Pixmap) -> usize {
    let (a, _) = a.data().as_chunks::<4>();
    let (b, _) = b.data().as_chunks::<4>();
    a.iter()
        .zip(b)
        .filter(|(p, q)| {
            p.iter()
                .zip(q.iter())
                .any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOLERANCE)
        })
        .count()
}

fn compare(name: &str, actual: &Pixmap) {
    let path = golden_dir().join(format!("{name}.png"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        actual.save_png(&path).expect("write golden");
        return;
    }
    let expected = Pixmap::load_png(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (run with UPDATE_GOLDEN=1 to create it)",
            path.display()
        )
    });
    assert_eq!(
        (expected.width(), expected.height()),
        (actual.width(), actual.height()),
        "{name}: size changed"
    );
    let off = differing(&expected, actual);
    let share = off as f64 / f64::from(actual.width() * actual.height());
    assert!(
        share <= PIXEL_TOLERANCE,
        "{name}: {off} pixels differ ({:.2}%)",
        share * 100.0
    );
}

fn render(chonk: f32, coat: &str, params: PoseParams) -> Pixmap {
    let rig = Rig::load(builtin::SHIBA).unwrap();
    let mut r = Renderer::new(&rig, 1.0, chonk, rig.coat(coat).unwrap()).unwrap();
    let mut pose = rig.rest_pose();
    BoneMap::new(&rig).apply(&params, &mut pose);
    r.render(&rig, &pose).clone()
}

#[test]
fn shiba_at_every_body_snap_point() {
    for (label, chonk) in [
        ("standard", 0.0),
        ("chubby", 0.33),
        ("round", 0.66),
        ("mochi", 1.0),
    ] {
        compare(
            &format!("shiba-red-{label}"),
            &render(chonk, "red", PoseParams::default()),
        );
    }
}

#[test]
fn shiba_in_every_coat() {
    for coat in ["black", "sesame", "white"] {
        compare(
            &format!("shiba-{coat}-chubby"),
            &render(0.33, coat, PoseParams::default()),
        );
    }
}

#[test]
fn shiba_squashed_mid_poke_and_blinking() {
    let params = PoseParams {
        squash: 0.3,
        belly: 0.3,
        eyes_open: 0.12,
        tail: 8.0,
        ..PoseParams::default()
    };
    compare("shiba-red-poked", &render(0.33, "red", params));
}

/// The face layer is used from the second identical frame on; it must look
/// the same as drawing every part.
#[test]
fn cached_frames_match_direct_drawing() {
    let rig = Rig::load(builtin::SHIBA).unwrap();
    let mut r = Renderer::new(&rig, 1.0, 0.33, 0).unwrap();
    let pose = rig.rest_pose();
    let direct = r.render(&rig, &pose).clone();
    r.render(&rig, &pose);
    let cached = r.render(&rig, &pose).clone();
    let off = differing(&direct, &cached);
    assert_eq!(off, 0, "{off} pixels differ");
}

#[test]
fn speech_bubbles() {
    let mut face = chonkgene_render::text::Face::bubble();
    for (name, line) in [
        ("bubble-laugh", "哈哈哈哈！不行了啦～"),
        ("bubble-sigh", "……唉。"),
    ] {
        let pm = chonkgene_render::bubble::bubble(&mut face, line, 1.0).unwrap();
        compare(name, &pm);
    }
}

#[test]
fn every_species_at_its_default_coat() {
    for id in ["munchkin", "fold", "owl", "chick", "duck"] {
        let rig = Rig::load(builtin::for_species(id).unwrap()).unwrap();
        for (label, chonk) in [("chubby", 0.33), ("mochi", 1.0)] {
            let mut r = Renderer::new(&rig, 1.0, chonk, 0).unwrap();
            let pm = r.render(&rig, &rig.rest_pose()).clone();
            compare(&format!("{id}-{label}"), &pm);
        }
    }
}

#[test]
fn pet_menu() {
    use chonkgene_render::menu::{self, Icon, Item, Model};
    let item = |label: &str, hint: Option<&str>, icon, separated| Item {
        label: label.into(),
        hint: hint.map(Into::into),
        icon,
        separated,
    };
    let model = Model {
        name: "阿柴".into(),
        species: "柴犬".into(),
        level: 3,
        progress: 0.6,
        status: Some("有起床氣".into()),
        grumpy: true,
        avatar: ([0xE8, 0x95, 0x5A, 255], [0xFF, 0xF3, 0xE2, 255]),
        items: vec![
            item("摸摸頭", None, Icon::Heart, false),
            item("餵零食", Some("還有 2 份"), Icon::Snack, false),
            item("改名字…", None, Icon::Pencil, false),
            item("幫牠換造型…", None, Icon::Palette, false),
            item("讓牠去睡覺", None, Icon::Moon, false),
            item("全部收起來", Some("Ctrl+Alt+P"), Icon::Hide, true),
            item("設定…", None, Icon::Sliders, false),
            item("結束肉肉賴桌", None, Icon::Exit, false),
        ],
    };
    let mut face = chonkgene_render::text::Face::ui();
    let drawn = menu::draw(&mut face, &model, Some(1), 1.0).unwrap();
    assert_eq!(drawn.rows.len(), 8);
    let r = drawn.rows[4];
    assert_eq!(menu::row_at(&drawn, r.x() + 5.0, r.y() + 5.0), Some(4));
    compare("menu", &drawn.pixmap);
}
