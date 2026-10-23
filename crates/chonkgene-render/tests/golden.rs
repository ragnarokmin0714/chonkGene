//! Golden snapshots (PLAN §7): the Shiba at each body snap point, coat, pose
//! and face; every species; bubbles and the menu.
//! `UPDATE_GOLDEN=1 cargo test -p chonkgene-render` rewrites them.

use std::path::PathBuf;

use std::sync::Arc;

use chonkgene_core::anim::{Expression, PoseParams};
use chonkgene_render::sprites::{Variant, render_one};
use chonkgene_render::{KEYS, Key, Renderer};
use chonkgene_rig::{Look, PoseId, builtin};
use tiny_skia::{IntSize, Pixmap};

/// Largest per-channel difference that still counts as equal: tiny-skia's
/// SIMD paths and the platform's sin/cos differ slightly between x86-64 and
/// aarch64.
const CHANNEL_TOLERANCE: u8 = 3;
/// Share of pixels allowed beyond the tolerance; a ray grazing an edge may
/// land on either side of it.
const PIXEL_TOLERANCE: f64 = 0.005;

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

/// One sprite, rendered now on every core.
fn sprite(id: &str, chonk: f32, coat: &str, key: Key) -> Pixmap {
    let model = builtin::load(id).unwrap();
    let coat = model.coat(coat).unwrap();
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let s = render_one(
        &model,
        Variant {
            coat,
            chonk,
            scale: 1.0,
        },
        key,
        threads,
    );
    Pixmap::from_vec(s.rgba, IntSize::from_wh(s.width, s.height).unwrap()).unwrap()
}

fn shiba() -> Renderer {
    let model = Arc::new(builtin::load("shiba").unwrap());
    let mut r = Renderer::new(model, 1.0, 0.33, 0).unwrap();
    r.finish();
    r
}

const SIT: Key = Key {
    pose: PoseId::Sit,
    look: Look::Open,
};

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
            &sprite("shiba", chonk, "red", SIT),
        );
    }
}

#[test]
fn shiba_in_every_coat() {
    for coat in ["black", "sesame", "white"] {
        compare(
            &format!("shiba-{coat}-chubby"),
            &sprite("shiba", 0.33, coat, SIT),
        );
    }
}

#[test]
fn shiba_in_every_pose_and_face() {
    for key in KEYS {
        let name = format!("shiba-{}-{:?}", key.pose.name(), key.look).to_lowercase();
        compare(&name, &sprite("shiba", 0.33, "red", key));
    }
}

#[test]
fn shiba_squashed_mid_poke() {
    let params = PoseParams {
        squash: 0.3,
        belly: 0.3,
        ..PoseParams::default()
    };
    compare(
        "shiba-red-poked",
        &shiba().render(&params, Expression::Neutral).clone(),
    );
}

/// Idle pets mostly look the same from frame to frame; those frames must
/// not be drawn or shown again (PLAN §5), but any visible change must be.
#[test]
fn unchanged_frames_are_not_redrawn() {
    let mut r = shiba();
    let still = PoseParams::default();
    r.render(&still, Expression::Neutral);
    assert!(r.changed());
    r.render(&still, Expression::Neutral);
    assert!(!r.changed(), "same pose twice");
    let breath = PoseParams {
        breath: 1.0,
        ..still
    };
    r.render(&breath, Expression::Neutral);
    assert!(r.changed(), "a breath step");
    r.render(&breath, Expression::Happy);
    assert!(r.changed(), "a new face");
    let poked = PoseParams {
        squash: 0.3,
        ..breath
    };
    r.render(&poked, Expression::Happy);
    assert!(r.changed(), "a squash");
    // Asleep, the snot bubble and the z always move.
    r.render_asleep(&still, 1.0);
    r.render_asleep(&still, 1.0);
    assert!(r.changed());
}

/// Hit-testing reads the zone the ray hit, through the frame's squash and
/// mirroring, so a poke lands where the pixels are.
#[test]
fn pokes_land_on_the_zone_under_the_cursor() {
    let mut r = shiba();
    for facing in [1.0, -1.0] {
        let params = PoseParams {
            facing,
            ..PoseParams::default()
        };
        let frame = r.render(&params, Expression::Neutral).clone();
        let (ax, ay) = r.anchor();
        assert_eq!(r.hit(ax, ay + 6.0), Some("head"), "facing {facing}");
        assert_eq!(r.hit(1.0, 1.0), None);
        // The belly is a front patch of the body, below the face.
        let belly: Vec<(u32, u32)> = (0..frame.height())
            .flat_map(|y| (0..frame.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| r.hit(x as f32, y as f32) == Some("belly"))
            .collect();
        assert!(belly.len() > 200, "belly is only {} px", belly.len());
        assert!(belly.iter().all(|&(_, y)| y as f32 > ay + 40.0));
        for zone in ["cheek", "paw", "bottom"] {
            assert!(
                (0..frame.height())
                    .any(|y| (0..frame.width()).any(|x| r.hit(x as f32, y as f32) == Some(zone))),
                "no {zone} zone"
            );
        }
    }
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
        let coat = builtin::load(id).unwrap().coats[0].name.clone();
        for (label, chonk) in [("chubby", 0.33), ("mochi", 1.0)] {
            compare(&format!("{id}-{label}"), &sprite(id, chonk, &coat, SIT));
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

#[test]
fn shiba_asleep() {
    let params = PoseParams {
        eyes_open: 0.06,
        breath: 1.0,
        ..PoseParams::default()
    };
    compare("shiba-asleep", &shiba().render_asleep(&params, 1.6).clone());
}
