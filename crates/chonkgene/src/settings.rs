//! The settings window (PLAN §3.2): its own process, on Slint. It reads the
//! saves, and sends every change to the pet process, which owns the files.
//! If the pets are not running, it writes the files itself.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use chonkgene_core::anim::Animator;
use chonkgene_core::mind::XP_PER_LEVEL;
use chonkgene_core::species::Species;
use chonkgene_render::sprites::{Variant, render_one};
use chonkgene_render::{KEYS, Renderer};
use chonkgene_rig::{Model, builtin};
use slint::{
    Color, ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, Timer,
    TimerMode, VecModel,
};

use crate::ipc::{self, Msg};
use crate::lines::{MAX_NAME, MAX_YOU};
use crate::store::{self, DataDir, PetSave, Pets, Settings};

slint::include_modules!();

const PREVIEW_FPS: u64 = 10;
const SEND_EVERY: Duration = Duration::from_millis(150);

fn coat_label(coat: &str) -> &'static str {
    match coat {
        "red" => "赤柴",
        "black" => "黑色",
        "sesame" => "胡麻",
        "white" => "白色",
        "milktea" => "奶茶",
        "orange" => "橘色",
        "grey" => "灰色",
        "bluegrey" => "藍灰",
        "cream" => "奶油",
        "silver" => "銀虎斑",
        "brown" => "褐色",
        "snowy" => "雪白",
        "yellow" => "黃色",
        "silkie" => "烏骨",
        _ => "其他",
    }
}

fn blurb(s: Species) -> &'static str {
    match s {
        Species::Shiba => "固執又愛笑，被戳會翻肚。",
        Species::Munchkin => "腿短但很努力，常常跳不上去。",
        Species::Fold => "高冷系，戳多了會厭世。",
        Species::Owl => "白天打瞌睡，晚上最有精神。",
        Species::Chick => "好奇又急性子，什麼都想啄一下。",
        Species::Duck => "慢半拍，喜歡跟在別人後面走。",
    }
}

fn tint(s: Species) -> Color {
    let (r, g, b) = match s {
        Species::Shiba => (0xFF, 0xE7, 0xD3),
        Species::Munchkin => (0xF7, 0xEB, 0xDD),
        Species::Fold => (0xE6, 0xE9, 0xF5),
        Species::Owl => (0xF1, 0xE6, 0xD8),
        Species::Chick => (0xFF, 0xF3, 0xC9),
        Species::Duck => (0xE1, 0xF0, 0xF5),
    };
    Color::from_rgb_u8(r, g, b)
}

/// What the pet says in the preview at each body snap point (mockup, PLAN §1.9).
fn body_line(s: Species, chonk: f32) -> &'static str {
    let stop = if chonk < 0.17 {
        0
    } else if chonk < 0.5 {
        1
    } else if chonk < 0.83 {
        2
    } else {
        3
    };
    let lines: [&str; 4] = match s {
        Species::Fold => [
            "……還不錯。",
            "……剛好。",
            "……你什麼都沒看到。",
            "……我只是毛多。",
        ],
        Species::Duck => [
            "……輕飄飄，呱。",
            "剛剛好，呱。",
            "……變圓了？呱。",
            "……麻糬，呱。",
        ],
        Species::Chick => [
            "啾！好輕！",
            "啾～剛剛好",
            "啾？肚子變大了？",
            "啾！我是麻糬！",
        ],
        Species::Owl => [
            "咕，身輕如燕。",
            "咕咕，剛剛好。",
            "咕？好像變圓了",
            "咕……麻糬咕。",
        ],
        Species::Munchkin => ["好輕喔！", "剛剛好～", "肚子……變大了？", "我變成麻糬了……"],
        Species::Shiba => [
            "身輕如燕！",
            "剛剛好的肉肉～",
            "肚子好像變大了？",
            "我是一顆麻糬。",
        ],
    };
    lines[stop]
}

fn color(c: [u8; 4]) -> Color {
    Color::from_argb_u8(c[3], c[0], c[1], c[2])
}

fn image(pm: &tiny_skia::Pixmap) -> Image {
    let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(pm.data(), pm.width(), pm.height());
    Image::from_rgba8_premultiplied(buf)
}

/// A small sitting portrait for the adoption list, rendered now: six of
/// them take well under 0.1 s on all cores.
fn portrait(model: &Model) -> Image {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let variant = Variant {
        coat: 0,
        chonk: 0.33,
        scale: 0.6,
    };
    let s = render_one(model, variant, KEYS[0], threads);
    tiny_skia::IntSize::from_wh(s.width, s.height)
        .and_then(|size| tiny_skia::Pixmap::from_vec(s.rgba, size))
        .map(|pm| image(&pm))
        .unwrap_or_default()
}

struct Kind {
    model: Arc<Model>,
}

struct Preview {
    pet: u64,
    renderer: Renderer,
    anim: Animator,
}

struct State {
    data: DataDir,
    settings: Settings,
    pets: Pets,
    kinds: HashMap<Species, Kind>,
    selected: Option<u64>,
    adopt_species: Option<Species>,
    adopt_coat: String,
    reachable: bool,
    pet_dirty: bool,
    settings_dirty: bool,
    preview: Option<Preview>,
}

impl State {
    fn pet(&self) -> Option<&PetSave> {
        self.pets.pets.iter().find(|p| Some(p.id) == self.selected)
    }

    fn pet_mut(&mut self) -> Option<&mut PetSave> {
        let id = self.selected;
        self.pets.pets.iter_mut().find(|p| Some(p.id) == id)
    }

    fn coats(&self, s: Species) -> Vec<(String, [u8; 4], [u8; 4])> {
        self.kinds[&s]
            .model
            .coats
            .iter()
            .map(|c| {
                let (a, b) = c.swatch();
                (c.name.clone(), a, b)
            })
            .collect()
    }

    /// Send a message, or apply it to the files if the pets are not running.
    fn send(&mut self, msg: Msg) {
        if self.reachable && ipc::send(&msg).is_ok() {
            return;
        }
        self.reachable = false;
        match msg {
            Msg::Settings(s) => {
                let _ = store::save(&self.data.settings(), &s);
            }
            Msg::Pet(p) => {
                if let Some(old) = self.pets.pets.iter_mut().find(|o| o.id == p.id) {
                    *old = p;
                }
                let _ = store::save(&self.data.pets(), &self.pets);
            }
            Msg::Adopt { species, coat } => {
                if let Some(s) = Species::from_id(&species) {
                    let id = self.pets.next_id;
                    self.pets.next_id += 1;
                    self.pets
                        .pets
                        .push(PetSave::adopt(id, s, &coat, &mut fastrand::Rng::new()));
                    let _ = store::save(&self.data.pets(), &self.pets);
                }
            }
            Msg::Release(id) => {
                self.pets.pets.retain(|p| p.id != id);
                let _ = store::save(&self.data.pets(), &self.pets);
            }
            _ => {}
        }
    }

    fn send_settings(&mut self) {
        let s = self.settings.clone();
        self.send(Msg::Settings(s));
    }
}

fn pet_rows(st: &State) -> ModelRc<PetRow> {
    let rows: Vec<PetRow> = st
        .pets
        .pets
        .iter()
        .filter_map(|p| {
            let s = p.species()?;
            let model = &st.kinds[&s].model;
            let (dot, ring) = model.coats[model.coat(&p.coat).unwrap_or(0)].swatch();
            Some(PetRow {
                id: p.id.to_string().into(),
                name: p.name.clone().into(),
                dot: color(dot),
                ring: color(ring),
                resting: !p.on_desktop,
            })
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn coat_rows(coats: Vec<(String, [u8; 4], [u8; 4])>) -> ModelRc<CoatRow> {
    let rows: Vec<CoatRow> = coats
        .into_iter()
        .map(|(id, main, light)| CoatRow {
            label: coat_label(&id).into(),
            id: id.into(),
            main: color(main),
            light: color(light),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// Push the selected pet's fields into the UI and rebuild its preview.
fn show_pet(ui: &SettingsWindow, st: &mut State) {
    ui.set_pets(pet_rows(st));
    let Some(p) = st.pet().cloned() else {
        ui.set_selected_pet(SharedString::new());
        st.preview = None;
        return;
    };
    let Some(species) = p.species() else {
        return;
    };
    ui.set_selected_pet(p.id.to_string().into());
    ui.set_species_label(species.label().into());
    ui.set_pet_name(p.name.clone().into());
    ui.set_pet_you(p.you.clone().into());
    ui.set_coats(coat_rows(st.coats(species)));
    ui.set_coat(p.coat.clone().into());
    ui.set_chonk(p.chonk);
    ui.set_size(p.size);
    ui.set_ticklish(p.ticklish);
    ui.set_patience(p.patience);
    ui.set_energy(p.energy);
    ui.set_on_desktop(p.on_desktop);
    ui.set_preview_tint(tint(species));
    ui.set_preview_line(body_line(species, p.chonk).into());
    let progress = p.xp * 100 / XP_PER_LEVEL;
    ui.set_level_text(format!("親密度 Lv.{}（{progress}%）", p.level).into());
    ui.set_confirm_release(false);
    let kind = &st.kinds[&species];
    let coat = kind.model.coat(&p.coat).unwrap_or(0);
    if let Some(renderer) = Renderer::new(Arc::clone(&kind.model), 1.0, p.chonk, coat) {
        st.preview = Some(Preview {
            pet: p.id,
            renderer,
            anim: Animator::new(p.id, p.chonk, species.waddle()),
        });
    }
}

fn tick_preview(ui: &SettingsWindow, st: &mut State) {
    let Some(pv) = &mut st.preview else {
        return;
    };
    pv.anim.step(1.0 / PREVIEW_FPS as f32);
    pv.renderer.poll();
    let frame = pv.renderer.render(&pv.anim.params(), pv.anim.expression);
    ui.set_preview(image(frame));
}

/// Apply an edit to the selected pet: update the preview and mark it for sending.
fn edit(ui: &SettingsWindow, st: &mut State, f: impl FnOnce(&mut PetSave)) {
    let Some(p) = st.pet_mut() else {
        return;
    };
    f(p);
    p.sanitise();
    let p = p.clone();
    st.pet_dirty = true;
    if let Some(species) = p.species() {
        ui.set_preview_line(body_line(species, p.chonk).into());
        let kind = &st.kinds[&species];
        if let Some(pv) = st.preview.as_mut().filter(|pv| pv.pet == p.id) {
            pv.renderer.set_chonk(p.chonk);
            if let Some(coat) = kind.model.coat(&p.coat) {
                pv.renderer.set_coat(coat);
            }
            pv.anim.set_chonk(p.chonk);
            pv.anim.poke(chonkgene_core::Zone::Belly);
        }
    }
    ui.set_pets(pet_rows(st));
}

/// Fill `ui` from the saves in `data` and wire up every callback. The timers
/// must live as long as the window.
fn setup(
    ui: &SettingsWindow,
    data: DataDir,
    pet: Option<u64>,
) -> Result<(Rc<RefCell<State>>, [Timer; 2]), String> {
    let settings = store::load::<Settings>(&data.settings())
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut pets = store::load::<Pets>(&data.pets())
        .ok()
        .flatten()
        .unwrap_or_default();
    pets.sanitise();
    let mut kinds = HashMap::new();
    for s in Species::ALL {
        let model = builtin::load(s.id()).map_err(|e| e.to_string())?;
        kinds.insert(
            s,
            Kind {
                model: Arc::new(model),
            },
        );
    }
    let selected = pet.or_else(|| pets.pets.first().map(|p| p.id));
    let st = Rc::new(RefCell::new(State {
        data,
        settings,
        pets,
        kinds,
        selected,
        adopt_species: None,
        adopt_coat: String::new(),
        reachable: true,
        pet_dirty: false,
        settings_dirty: false,
        preview: None,
    }));

    {
        let mut st = st.borrow_mut();
        let s = st.settings.clone();
        ui.set_sound(s.sound);
        ui.set_volume(f32::from(s.volume) / 100.0);
        ui.set_irrational(s.irrational);
        ui.set_squabble(i32::from(s.squabble.min(2)));
        ui.set_hide_fullscreen(s.hide_in_fullscreen);
        ui.set_start_login(s.start_at_login);
        ui.set_desktop_limit(s.on_desktop.min(i32::MAX as u32) as i32);
        let where_ = if st.data.portable {
            "可攜模式："
        } else {
            ""
        };
        ui.set_data_path(format!("{where_}{}", st.data.path.display()).into());
        ui.set_version(env!("CARGO_PKG_VERSION").into());
        let species: Vec<SpeciesRow> = Species::ALL
            .into_iter()
            .map(|s| {
                let kind = &st.kinds[&s];
                let portrait = portrait(&kind.model);
                SpeciesRow {
                    id: s.id().into(),
                    label: s.label().into(),
                    blurb: blurb(s).into(),
                    portrait,
                    tint: tint(s),
                }
            })
            .collect();
        ui.set_species(ModelRc::new(VecModel::from(species)));
        if st.pets.pets.is_empty() {
            ui.set_adopting(true);
        }
        show_pet(ui, &mut st);
    }

    macro_rules! on {
        ($setter:ident, |$ui:ident, $st:ident $(, $arg:ident : $ty:ty)*| $body:block) => {{
            let weak = ui.as_weak();
            let st = st.clone();
            ui.$setter(move |$($arg: $ty),*| {
                let Some($ui) = weak.upgrade() else { return };
                let mut guard = st.borrow_mut();
                let $st = &mut *guard;
                $body
            });
        }};
    }

    on!(on_select_pet, |ui, st, id: SharedString| {
        st.selected = id.parse().ok();
        ui.set_adopting(false);
        show_pet(&ui, st);
    });
    on!(on_start_adopting, |ui, st| {
        ui.set_adopting(true);
        ui.set_adopt_species(SharedString::new());
        st.adopt_species = None;
    });
    on!(on_pick_adopt_species, |ui, st, id: SharedString| {
        if let Some(s) = Species::from_id(&id) {
            st.adopt_species = Some(s);
            let coats = st.coats(s);
            st.adopt_coat = coats.first().map(|c| c.0.clone()).unwrap_or_default();
            ui.set_adopt_coats(coat_rows(coats));
            ui.set_adopt_species(id);
            ui.set_adopt_coat(st.adopt_coat.clone().into());
        }
    });
    on!(on_pick_adopt_coat, |ui, st, id: SharedString| {
        st.adopt_coat = id.to_string();
        ui.set_adopt_coat(id);
    });
    on!(on_adopt, |ui, st| {
        let Some(s) = st.adopt_species else { return };
        let coat = st.adopt_coat.clone();
        // The pet process saves the new pet; the send timer reads it back.
        st.send(Msg::Adopt {
            species: s.id().into(),
            coat,
        });
        ui.set_adopting(false);
        if !st.reachable {
            st.selected = st.pets.pets.last().map(|p| p.id);
            show_pet(&ui, st);
        }
    });
    on!(on_name_edited, |ui, st, t: SharedString| {
        let name: String = t.chars().take(MAX_NAME).collect();
        edit(&ui, st, |p| {
            if !name.trim().is_empty() {
                p.name = name.trim().to_string();
            }
        });
    });
    on!(on_you_edited, |ui, st, t: SharedString| {
        let you: String = t.chars().take(MAX_YOU).collect();
        edit(&ui, st, |p| {
            if !you.trim().is_empty() {
                p.you = you.trim().to_string();
            }
        });
    });
    on!(on_coat_picked, |ui, st, id: SharedString| {
        ui.set_coat(id.clone());
        edit(&ui, st, |p| p.coat = id.to_string());
    });
    on!(on_chonk_changed, |ui, st, v: f32| {
        edit(&ui, st, |p| p.chonk = v);
    });
    on!(on_size_changed, |ui, st, v: f32| {
        let v = (v * 10.0).round() / 10.0;
        ui.set_size(v);
        edit(&ui, st, |p| p.size = v);
    });
    on!(on_trait_changed, |ui, st, which: i32, v: f32| {
        match which {
            0 => ui.set_ticklish(v),
            1 => ui.set_patience(v),
            _ => ui.set_energy(v),
        }
        edit(&ui, st, |p| match which {
            0 => p.ticklish = v,
            1 => p.patience = v,
            _ => p.energy = v,
        });
    });
    on!(on_desktop_toggled, |ui, st, v: bool| {
        edit(&ui, st, |p| p.on_desktop = v);
    });
    on!(on_release, |ui, st| {
        let Some(id) = st.selected else { return };
        st.send(Msg::Release(id));
        st.pets.pets.retain(|p| p.id != id);
        st.selected = st.pets.pets.first().map(|p| p.id);
        if st.pets.pets.is_empty() {
            ui.set_adopting(true);
        }
        show_pet(&ui, st);
    });
    on!(on_sound_toggled, |_ui, st, v: bool| {
        st.settings.sound = v;
        st.send_settings();
    });
    on!(on_volume_changed, |_ui, st, v: f32| {
        // Dragging sends at most every SEND_EVERY, via the send timer.
        st.settings.volume = (v * 100.0).round().clamp(0.0, 100.0) as u8;
        st.settings_dirty = true;
    });
    on!(on_irrational_toggled, |_ui, st, v: bool| {
        st.settings.irrational = v;
        st.send_settings();
    });
    on!(on_squabble_picked, |_ui, st, i: i32| {
        st.settings.squabble = i.clamp(0, 2) as u8;
        st.send_settings();
    });
    on!(on_hide_fullscreen_toggled, |_ui, st, v: bool| {
        st.settings.hide_in_fullscreen = v;
        st.send_settings();
    });
    on!(on_start_login_toggled, |_ui, st, v: bool| {
        st.settings.start_at_login = v;
        st.send_settings();
    });
    on!(on_desktop_limit_changed, |ui, st, v: i32| {
        st.settings.on_desktop = v.max(1) as u32;
        ui.set_desktop_limit(v.max(1));
        st.send_settings();
    });
    on!(on_open_folder, |_ui, st| {
        open_folder(&st.data.path);
    });
    on!(on_remove_traces, |ui, st| {
        st.settings.start_at_login = false;
        ui.set_start_login(false);
        st.send_settings();
        st.send(Msg::RemoveTraces);
        ui.set_notice(
            format!(
                "已移除開機自動啟動。要完全移除，請結束肉肉賴桌，再刪除這個資料夾：{}",
                st.data.path.display()
            )
            .into(),
        );
    });

    // Preview animation, debounced sending, and noticing when the pets are gone.
    let preview_timer = Timer::default();
    {
        let weak = ui.as_weak();
        let st = st.clone();
        preview_timer.start(
            TimerMode::Repeated,
            Duration::from_millis(1000 / PREVIEW_FPS),
            move || {
                if let Some(ui) = weak.upgrade() {
                    tick_preview(&ui, &mut st.borrow_mut());
                }
            },
        );
    }
    let send_timer = Timer::default();
    {
        let weak = ui.as_weak();
        let st = st.clone();
        let mut last_count = usize::MAX;
        send_timer.start(TimerMode::Repeated, SEND_EVERY, move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = st.borrow_mut();
            if st.settings_dirty {
                st.settings_dirty = false;
                st.send_settings();
            }
            if st.pet_dirty {
                st.pet_dirty = false;
                if let Some(p) = st.pet().cloned() {
                    st.send(Msg::Pet(p));
                }
            }
            if !st.reachable {
                ui.set_notice("肉肉賴桌目前沒有在執行。改動已經存起來，下次開啟時套用。".into());
            }
            // Pick up pets the pet process just adopted.
            if st.reachable
                && let Ok(Some(mut fresh)) = store::load::<Pets>(&st.data.pets())
            {
                fresh.sanitise();
                if fresh.pets.len() != st.pets.pets.len() && fresh.pets.len() != last_count {
                    last_count = fresh.pets.len();
                    let newest = fresh.pets.last().map(|p| p.id);
                    let grew = fresh.pets.len() > st.pets.pets.len();
                    st.pets = fresh;
                    if grew {
                        st.selected = newest;
                    }
                    show_pet(&ui, &mut st);
                }
            }
        });
    }

    Ok((st, [preview_timer, send_timer]))
}

pub fn run(data: Option<PathBuf>, pet: Option<u64>) -> Result<(), String> {
    let data = DataDir::locate(data).map_err(|e| format!("cannot find the data folder: {e}"))?;
    let ui = SettingsWindow::new().map_err(|e| e.to_string())?;
    let (st, _timers) = setup(&ui, data, pet)?;
    let _repaints = repaint_when_shown(&ui);
    ui.run().map_err(|e| e.to_string())?;
    let mut st = st.borrow_mut();
    if st.settings_dirty {
        st.send_settings();
    }
    if st.pet_dirty
        && let Some(p) = st.pet().cloned()
    {
        st.send(Msg::Pet(p));
    }
    Ok(())
}

/// Repaint the whole window a few times just after it opens. On Windows the
/// software renderer only redraws what changed, and the first frame can be
/// drawn before the window is on screen and lost, leaving it black until
/// something scrolled. (A resize or a restore after minimising already gets
/// a full redraw from Slint.)
fn repaint_when_shown(ui: &SettingsWindow) -> [Timer; 3] {
    [100, 600, 1500].map(|ms| {
        let ui = ui.as_weak();
        let t = Timer::default();
        t.start(
            TimerMode::SingleShot,
            Duration::from_millis(ms),
            move || {
                if let Some(ui) = ui.upgrade() {
                    ui.set_repaint(ui.get_repaint().wrapping_add(1));
                }
            },
        );
        t
    })
}

fn open_folder(path: &std::path::Path) {
    #[cfg(windows)]
    let cmd = "explorer";
    #[cfg(not(windows))]
    let cmd = "xdg-open";
    if let Err(e) = std::process::Command::new(cmd).arg(path).spawn() {
        eprintln!("cannot open {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{Platform, WindowAdapter};

    struct Headless(Rc<MinimalSoftwareWindow>);

    impl Platform for Headless {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
            Ok(self.0.clone())
        }
    }

    /// Bumping `repaint` redraws the whole window, even when the renderer
    /// otherwise redraws only what changed (see `repaint_when_shown`).
    #[test]
    fn a_repaint_redraws_the_whole_window() {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
        let (w, h) = (880u32, 620u32);
        window.set_size(slint::PhysicalSize::new(w, h));
        let dir = std::env::temp_dir().join(format!("chonkgene-repaint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = DataDir::locate(Some(dir.clone())).unwrap();
        let ui = SettingsWindow::new().unwrap();
        let (_st, _timers) = setup(&ui, data, None).unwrap();
        ui.show().unwrap();
        let mut buf = vec![slint::Rgb8Pixel::default(); (w * h) as usize];
        let mut draw = || {
            let mut size = None;
            window.request_redraw();
            window.draw_if_needed(|renderer| {
                let region = renderer.render(&mut buf, w as usize);
                size = Some(region.bounding_box_size());
            });
            size.map_or((0, 0), |s| (s.width, s.height))
        };
        assert_eq!(draw(), (w, h), "the first frame is whole");
        let (rw, rh) = draw();
        assert!(rw * rh < w * h / 2, "nothing changed: {rw}×{rh}");
        ui.set_repaint(ui.get_repaint() + 1);
        assert_eq!(draw(), (w, h), "a repaint redraws everything");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Renders every settings tab to PNG without a display:
    /// `cargo test -p chonkgene -- --ignored settings_screenshots`
    /// writes target/screens/settings-*.png.
    #[test]
    #[ignore = "writes screenshots; run by hand"]
    fn settings_screenshots() {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
        let (w, h) = (880u32, 620u32);
        window.set_size(slint::PhysicalSize::new(w, h));

        let dir = std::env::temp_dir().join(format!("chonkgene-shots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = DataDir::locate(Some(dir.clone())).unwrap();
        let mut rng = fastrand::Rng::with_seed(4);
        let mut pets = Pets::default();
        for (i, (s, coat)) in [
            (Species::Shiba, "red"),
            (Species::Fold, "bluegrey"),
            (Species::Duck, "white"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut p = PetSave::adopt(i as u64 + 1, s, coat, &mut rng);
            p.level = 3;
            p.xp = 60;
            pets.pets.push(p);
        }
        pets.next_id = 4;
        store::save(&data.pets(), &pets).unwrap();

        let ui = SettingsWindow::new().unwrap();
        let (_st, _timers) = setup(&ui, data, Some(1)).unwrap();
        ui.show().unwrap();
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/screens");
        std::fs::create_dir_all(&out).unwrap();
        for (name, tab, adopting) in [
            ("pets", 0, false),
            ("adopt", 0, true),
            ("play", 1, false),
            ("quiet", 2, false),
            ("general", 3, false),
        ] {
            ui.set_tab(tab);
            ui.set_adopting(adopting);
            if adopting {
                ui.invoke_pick_adopt_species("chick".into());
            }
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(600));
            slint::platform::update_timers_and_animations();
            window.request_redraw();
            let mut buf = vec![slint::Rgb8Pixel::default(); (w * h) as usize];
            window.draw_if_needed(|renderer| {
                renderer.render(&mut buf, w as usize);
            });
            let mut pm = tiny_skia::Pixmap::new(w, h).unwrap();
            let (dst, _) = pm.data_mut().as_chunks_mut::<4>();
            for (px, d) in buf.iter().zip(dst) {
                *d = [px.r, px.g, px.b, 255];
            }
            pm.save_png(out.join(format!("settings-{name}.png")))
                .unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
