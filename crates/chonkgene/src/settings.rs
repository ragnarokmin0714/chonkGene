//! The settings window (PLAN §3.2): its own process, on Slint. It reads the
//! saves, and sends every change to the pet process, which owns the files.
//! If the pets are not running, it writes the files itself.

use chonkgene_core::i18n::{self, Lang, tr, trf};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

use crate::brand;
use crate::ipc::{self, Msg};
use crate::lines::{MAX_NAME, MAX_YOU};
use crate::store::{self, DataDir, PetSave, Pets, ROTATE_CHOICES, Room, Settings};

slint::include_modules!();

/// The preview's timer ticks this often; a frame is drawn when the pet
/// needs one, at [`PREVIEW_FPS`] or as fast as its renderer asks (up to
/// 60). It was a fixed 10 fps stepping a fixed tenth of a second, a frame
/// late (Roger's sixteenth pass, item 2: the motion lagged).
const PREVIEW_TICK: Duration = Duration::from_micros(16_667);
const PREVIEW_FPS: u32 = 30;
const SEND_EVERY: Duration = Duration::from_millis(150);
/// After we call a pet out or send one to rest, the pets' file is left
/// unread for who is out this long (the pet process saves it at once on
/// the message; this covers a busy moment).
const DESK_SETTLE: Duration = Duration::from_secs(2);

fn coat_label(coat: &str) -> &'static str {
    match coat {
        "red" => tr("赤柴"),
        "black" => tr("黑色"),
        "sesame" => tr("胡麻"),
        "white" => tr("白色"),
        "milktea" => tr("奶茶"),
        "orange" => tr("橘色"),
        "grey" => tr("灰色"),
        "bluegrey" => tr("藍灰"),
        "cream" => tr("奶油"),
        "silver" => tr("銀虎斑"),
        "brown" => tr("褐色"),
        "snowy" => tr("雪白"),
        "yellow" => tr("黃色"),
        "silkie" => tr("烏骨"),
        // The fox's (金色 and 夜色 showed as 其他 until 2026-11-16).
        "gold" => tr("金色"),
        "night" => tr("夜色"),
        "snow" => tr("雪狐"),
        _ => tr("其他"),
    }
}

fn blurb(s: Species) -> &'static str {
    match s {
        Species::Shiba => tr("固執又愛笑，被戳會翻肚。"),
        Species::Munchkin => tr("腿短但很努力，常常跳不上去。"),
        Species::Fold => tr("高冷系，戳多了會厭世。"),
        Species::Owl => tr("白天打瞌睡，晚上最有精神。"),
        Species::Chick => tr("好奇又急性子，什麼都想啄一下。"),
        Species::Duck => tr("慢半拍，喜歡跟在別人後面走。"),
        Species::Fox => tr("愛惡作劇的小狐仙，坐下會開屏。"),
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
        Species::Fox => (0xFF, 0xEC, 0xF0),
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
    let lines: [&'static str; 4] = match s {
        Species::Fold => [
            tr("……還不錯。"),
            tr("……剛好。"),
            tr("……你什麼都沒看到。"),
            tr("……我只是毛多。"),
        ],
        Species::Duck => [
            tr("……輕飄飄，呱。"),
            tr("剛剛好，呱。"),
            tr("……變圓了？呱。"),
            tr("……麻糬，呱。"),
        ],
        Species::Chick => [
            tr("啾！好輕！"),
            tr("啾～剛剛好"),
            tr("啾？肚子變大了？"),
            tr("啾！我是麻糬！"),
        ],
        Species::Owl => [
            tr("咕，身輕如燕。"),
            tr("咕咕，剛剛好。"),
            tr("咕？好像變圓了"),
            tr("咕……麻糬咕。"),
        ],
        Species::Munchkin => [
            tr("好輕喔！"),
            tr("剛剛好～"),
            tr("肚子……變大了？"),
            tr("我變成麻糬了……"),
        ],
        Species::Shiba => [
            tr("身輕如燕！"),
            tr("剛剛好的肉肉～"),
            tr("肚子好像變大了？"),
            tr("我是一顆麻糬。"),
        ],
        Species::Fox => [
            tr("輕飄飄，像一陣風～"),
            tr("剛剛好，嘻嘻"),
            tr("尾巴……變胖了？"),
            tr("九條尾巴的麻糬！"),
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

/// A small sitting portrait for the adoption list, in the coat at `coat`,
/// rendered now: six of them take well under 0.1 s on all cores.
fn portrait(model: &Model, coat: usize) -> Image {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let variant = Variant {
        coat,
        chonk: 0.33,
        scale: 0.6,
    };
    let s = render_one(model, variant, KEYS[0], threads);
    tiny_skia::IntSize::from_wh(s.width, s.height)
        .and_then(|size| tiny_skia::Pixmap::from_vec(s.rgba, size))
        .map(|pm| image(&pm))
        .unwrap_or_default()
}

/// Shows species `s` on its adoption card in the coat at `coat`.
fn set_portrait(ui: &SettingsWindow, st: &State, s: Species, coat: usize) {
    use slint::Model as _;
    let rows = ui.get_species();
    let Some(i) = Species::ALL.iter().position(|&k| k == s) else {
        return;
    };
    if let Some(mut row) = rows.row_data(i) {
        row.portrait = portrait(&st.kinds[&s].model, coat);
        rows.set_row_data(i, row);
    }
}

struct Kind {
    model: Arc<Model>,
    /// The species on all fours, if it has a four-legged model.
    quad: Option<Arc<Model>>,
}

struct Preview {
    pet: u64,
    renderer: Renderer,
    anim: Animator,
    /// When the last frame was drawn: the animator steps by the time
    /// really passed since.
    drawn: Instant,
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
    /// When we last moved a pet between 「出場中」 and 「在寵物窩休息」 (or
    /// along one, or made it 常駐): the pets' file is not read back for who
    /// is where until the pet process has saved it.
    desk_at: Instant,
    /// For drawing a name as it is dragged (`chip_image`), and those drawn
    /// (by name, coat, 常駐 and scale).
    face: chonkgene_render::text::Face,
    chips: HashMap<String, (Image, i32)>,
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
                // The limit may have sent pets to rest or called them out.
                let _ = store::save(&self.data.pets(), &self.pets);
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
                    let save = PetSave::adopt(id, s, &coat, &mut fastrand::Rng::new());
                    if self.pets.adopt(save, self.settings.on_desktop) == Room::Over {
                        self.settings.on_desktop = self.pets.out() as u32;
                        let _ = store::save(&self.data.settings(), &self.settings);
                    }
                    let _ = store::save(&self.data.pets(), &self.pets);
                }
            }
            // Already done to our list (`on_pet_dropped`, `on_pin_pet`),
            // the limit too.
            Msg::Roster(_) => {
                let _ = store::save(&self.data.settings(), &self.settings);
                let _ = store::save(&self.data.pets(), &self.pets);
            }
            Msg::Release(id) => {
                self.pets.pets.retain(|p| p.id != id);
                let _ = store::save(&self.data.pets(), &self.pets);
            }
            // The sizes are already ours (`on_size_all_picked`).
            Msg::SizeAll(_) => {
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

/// 「隨機輪班」's choice for `minutes`: the nearest of [`ROTATE_CHOICES`]
/// (a hand-edited number shows as the closest), off only for 0.
fn rotate_index(minutes: u32) -> i32 {
    if minutes == 0 {
        return 0;
    }
    (1..ROTATE_CHOICES.len())
        .min_by_key(|&i| ROTATE_CHOICES[i].abs_diff(minutes))
        .unwrap_or(0) as i32
}

/// Both lists of names: 「出場中」 (out on the desktop, in order) and
/// 「在寵物窩休息」, each name with its picture for dragging. With the same
/// pets in the same places, the rows are updated where they changed rather
/// than made anew: a new row is a new name under the mouse, and Slint then
/// counts a click on it as a first click (picking a pet broke the
/// double-click that makes it 常駐).
fn show_rows(ui: &SettingsWindow, st: &mut State) {
    let scale = ui.window().scale_factor();
    let (mut desk, mut den) = (Vec::new(), Vec::new());
    for p in &st.pets.pets {
        let Some(s) = p.species() else { continue };
        let model = &st.kinds[&s].model;
        let (dot, ring) = model.coats[model.coat(&p.coat).unwrap_or(0)].swatch();
        let key = format!("{}|{dot:?}|{ring:?}|{}|{scale}", p.name, p.pinned);
        let (chip, hot) = match st.chips.get(&key) {
            Some(c) => c.clone(),
            None => {
                let c = chip_image(&mut st.face, &p.name, dot, ring, p.pinned, scale);
                st.chips.insert(key, c.clone());
                c
            }
        };
        let row = PetRow {
            id: p.id.to_string().into(),
            name: p.name.clone().into(),
            dot: color(dot),
            ring: color(ring),
            pinned: p.pinned,
            chip,
            hot,
        };
        if p.on_desktop {
            desk.push(row);
        } else {
            den.push(row);
        }
    }
    let put = |model: ModelRc<PetRow>, rows: Vec<PetRow>, set: &dyn Fn(ModelRc<PetRow>)| {
        use slint::Model as _;
        let same = model.row_count() == rows.len()
            && rows
                .iter()
                .enumerate()
                .all(|(i, r)| model.row_data(i).is_some_and(|o| o.id == r.id));
        match model.as_any().downcast_ref::<VecModel<PetRow>>() {
            Some(vec) if same => {
                for (i, r) in rows.into_iter().enumerate() {
                    let old = vec.row_data(i);
                    let changed = old.is_none_or(|o| {
                        (o.name.as_str(), o.dot, o.ring, o.pinned, o.hot)
                            != (r.name.as_str(), r.dot, r.ring, r.pinned, r.hot)
                    });
                    if changed {
                        vec.set_row_data(i, r);
                    }
                }
            }
            Some(vec) => vec.set_vec(rows),
            None => set(ModelRc::new(VecModel::from(rows))),
        }
    };
    put(ui.get_desk_pets(), desk, &|m| ui.set_desk_pets(m));
    put(ui.get_den_pets(), den, &|m| ui.set_den_pets(m));
}

/// A pet's name as `PetChip` draws it (picked, lifted a little with a soft
/// shadow), at `scale` device pixels a point, for under the cursor while it
/// is dragged; and where the cursor holds it (the dot's middle), pixels.
fn chip_image(
    face: &mut chonkgene_render::text::Face,
    name: &str,
    dot: [u8; 4],
    ring: [u8; 4],
    pinned: bool,
    scale: f32,
) -> (Image, i32) {
    use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};
    const PAD: f32 = 6.0;
    let tag = tr("常駐");
    let (text_w, tag_w) = (face.measure(name, 15.0), face.measure(tag, 12.0));
    let w = text_w + 58.0 + if pinned { tag_w + 20.0 } else { 0.0 };
    let h = 44.0;
    let size = |v: f32| ((v + 2.0 * PAD) * scale).ceil() as u32;
    let Some(mut pm) = Pixmap::new(size(w), size(h)) else {
        return (Image::default(), 0);
    };
    let ts = Transform::from_scale(scale, scale).pre_translate(PAD, PAD);
    let fill = |c: [u8; 4]| {
        let mut p = Paint::default();
        p.set_color_rgba8(c[0], c[1], c[2], c[3]);
        p.anti_alias = true;
        p
    };
    let pill = |x: f32, y: f32, w: f32, h: f32| {
        let r = h / 2.0;
        let mut pb = PathBuilder::new();
        pb.push_rect(Rect::from_xywh(x + r, y, (w - 2.0 * r).max(0.0), h)?);
        pb.push_circle(x + r, y + r, r);
        pb.push_circle(x + w - r, y + r, r);
        pb.finish()
    };
    let rounded = |x: f32, y: f32, w: f32, h: f32| {
        // A rounded rectangle drawn as one outline, for the border.
        let r = h / 2.0;
        let mut pb = PathBuilder::new();
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
        pb.finish()
    };
    // A soft shadow under it: lifted.
    for k in 0..4 {
        let g = k as f32;
        if let Some(p) = pill(-g + 1.0, -g + 3.0, w + 2.0 * g - 2.0, h + 2.0 * g - 2.0) {
            pm.fill_path(
                &p,
                &fill([0xE0, 0x8A, 0x4E, 18]),
                FillRule::Winding,
                ts,
                None,
            );
        }
    }
    if let Some(p) = rounded(1.0, 1.0, w - 2.0, h - 2.0) {
        pm.fill_path(
            &p,
            &fill([0xFF, 0xFF, 0xFF, 255]),
            FillRule::Winding,
            ts,
            None,
        );
        let st = Stroke {
            width: 2.0,
            ..Stroke::default()
        };
        pm.stroke_path(&p, &fill([0xD9, 0x77, 0x2F, 255]), &st, ts, None);
    }
    if let Some(p) = PathBuilder::from_circle(22.0, 22.0, 14.0) {
        pm.fill_path(&p, &fill(dot), FillRule::Winding, ts, None);
    }
    if let Some(p) = pill(8.0, 28.0, 28.0, 8.0) {
        pm.fill_path(&p, &fill(ring), FillRule::Winding, ts, None);
    }
    let baseline = |size: f32, mid: f32, face: &chonkgene_render::text::Face| {
        let (a, d) = face.extent(size);
        mid + (a - d) / 2.0
    };
    // Text drawn in device pixels, so it stays sharp.
    let at = |x: f32, y: f32| ((x + PAD) * scale, (y + PAD) * scale);
    let (x, y) = at(44.0, baseline(15.0, 22.0, face));
    face.draw(
        &mut pm,
        name,
        x,
        y,
        15.0 * scale,
        &fill([0x4B, 0x35, 0x28, 255]),
    );
    if pinned {
        let tx = 44.0 + text_w + 8.0;
        if let Some(p) = pill(tx, 11.0, tag_w + 12.0, 22.0) {
            pm.fill_path(
                &p,
                &fill([0xFF, 0xB3, 0x7A, 255]),
                FillRule::Winding,
                ts,
                None,
            );
        }
        let (x, y) = at(tx + 6.0, baseline(12.0, 22.0, face));
        face.draw(
            &mut pm,
            tag,
            x,
            y,
            12.0 * scale,
            &fill([0xFF, 0xFF, 0xFF, 255]),
        );
    }
    (image(&pm), ((PAD + 22.0) * scale).round() as i32)
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
/// Shows the window in the language of the settings: the words in the
/// .slint file (Slint's bundled translations) and the ones made here (the
/// adoption cards' names and blurbs; the rest is made afresh by
/// [`show_pet`]).
fn use_language(ui: &SettingsWindow, st: &mut State) {
    use slint::Model as _;
    let lang = st.settings.lang();
    i18n::set(lang);
    // Names and 「牠怎麼叫你」 nobody changed follow the language (the pet
    // process does the same to its own copy and its save).
    st.pets.localize_defaults();
    let id = if lang == Lang::ZhTw { "" } else { lang.id() };
    if let Err(e) = slint::select_bundled_translation(id) {
        eprintln!("no {} words for the settings window: {e}", lang.id());
    }
    let rows = ui.get_species();
    for (i, s) in Species::ALL.into_iter().enumerate() {
        if let Some(mut row) = rows.row_data(i) {
            row.label = s.label().into();
            row.blurb = blurb(s).into();
            rows.set_row_data(i, row);
        }
    }
}

fn show_pet(ui: &SettingsWindow, st: &mut State) {
    show_rows(ui, st);
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
    ui.set_preview_tint(tint(species));
    ui.set_preview_line(body_line(species, p.chonk).into());
    let progress = p.xp * 100 / XP_PER_LEVEL;
    ui.set_level_text(
        trf(
            "親密度 Lv.{level}（{progress}%）",
            &[
                ("level", &p.level.to_string()),
                ("progress", &progress.to_string()),
            ],
        )
        .into(),
    );
    ui.set_confirm_release(false);
    let kind = &st.kinds[&species];
    let coat = kind.model.coat(&p.coat).unwrap_or(0);
    if let Some(mut renderer) = Renderer::new(
        Arc::clone(&kind.model),
        kind.quad.clone(),
        1.0,
        p.chonk,
        coat,
    ) {
        // Pops hide cuts between sprites; GPU frames blend instead.
        let pops = !renderer.is_gpu();
        // The only pet in this window: each frame shows in the tick that
        // asks for it.
        renderer.set_eager(true);
        st.preview = Some(Preview {
            pet: p.id,
            drawn: Instant::now(),
            renderer,
            anim: Animator::new(p.id, p.chonk, species.waddle())
                .with_habits(species.habits())
                .with_slow_blinks(species.slow_blinks())
                .with_pops(pops),
        });
    }
}

fn tick_preview(ui: &SettingsWindow, st: &mut State) {
    let Some(pv) = &mut st.preview else {
        return;
    };
    let fps = pv.renderer.min_fps().clamp(PREVIEW_FPS, 60);
    let now = Instant::now();
    let since = now.saturating_duration_since(pv.drawn);
    // A little early is fine: the timer's ticks are not exact.
    if since.as_secs_f32() < 0.9 / fps as f32 {
        return;
    }
    pv.drawn = now;
    pv.anim.step(since.as_secs_f32().min(0.25));
    pv.renderer.poll();
    let frame = pv.renderer.render(&pv.anim.params(), pv.anim.expression);
    ui.set_preview(image(frame));
}

/// The size every pet has, percent, for 大家的大小; 0 when they differ.
fn common_size(pets: &Pets) -> i32 {
    let mut sizes = pets.pets.iter().map(|p| (p.size * 100.0).round() as i32);
    let first = sizes.next().unwrap_or(0);
    if sizes.all(|s| s == first) { first } else { 0 }
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
    show_rows(ui, st);
}

/// Fill `ui` from the saves in `data` and wire up every callback. The timers
/// must live as long as the window.
fn setup(
    ui: &SettingsWindow,
    data: DataDir,
    pet: Option<u64>,
) -> Result<(Rc<RefCell<State>>, [Timer; 2]), String> {
    let mut settings = store::load::<Settings>(&data.settings())
        .ok()
        .flatten()
        .unwrap_or_default();
    // The pet process upgrades and saves it first; this only covers a
    // settings window opened on its own.
    settings.upgrade();
    let mut pets = store::load::<Pets>(&data.pets())
        .ok()
        .flatten()
        .unwrap_or_default();
    pets.sanitise();
    let mut kinds = HashMap::new();
    for s in Species::ALL {
        let model = builtin::load(s.id()).map_err(|e| e.to_string())?;
        let quad = chonkgene_render::skin::mesh_model(s.id())
            .ok()
            .map(|(m, _)| Arc::new(m));
        kinds.insert(
            s,
            Kind {
                model: Arc::new(model),
                quad,
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
        desk_at: Instant::now(),
        face: chonkgene_render::text::Face::ui(),
        chips: HashMap::new(),
        preview: None,
    }));

    {
        let mut st = st.borrow_mut();
        let s = st.settings.clone();
        ui.set_sound(s.sound);
        ui.set_volume(f32::from(s.volume) / 100.0);
        ui.set_irrational(s.irrational);
        ui.set_squabble(i32::from(s.squabble.min(2)));
        ui.set_language(
            Lang::ALL
                .iter()
                .position(|l| l.id() == s.language)
                .map_or(0, |i| i as i32 + 1),
        );
        use_language(ui, &mut st);
        ui.set_hide_fullscreen(s.hide_in_fullscreen);
        ui.set_react_input(s.react_to_input);
        ui.set_start_login(s.start_at_login);
        ui.set_desktop_limit(s.on_desktop.min(i32::MAX as u32) as i32);
        ui.set_rotate(rotate_index(s.rotate_minutes));
        ui.set_size_all(common_size(&st.pets));
        let path = st.data.path.display().to_string();
        let shown = if st.data.portable {
            trf("可攜模式：{path}", &[("path", &path)])
        } else {
            path
        };
        ui.set_data_path(shown.into());
        ui.set_version(env!("CARGO_PKG_VERSION").into());
        let species: Vec<SpeciesRow> = Species::ALL
            .into_iter()
            .map(|s| {
                let kind = &st.kinds[&s];
                let portrait = portrait(&kind.model, 0);
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
        if let Some(old) = st.adopt_species.take() {
            set_portrait(&ui, st, old, 0);
        }
    });
    on!(on_pick_adopt_species, |ui, st, id: SharedString| {
        if let Some(s) = Species::from_id(&id) {
            // The card left shows its first coat again.
            if let Some(old) = st.adopt_species.replace(s)
                && old != s
            {
                set_portrait(&ui, st, old, 0);
            }
            let coats = st.coats(s);
            st.adopt_coat = coats.first().map(|c| c.0.clone()).unwrap_or_default();
            ui.set_adopt_coats(coat_rows(coats));
            ui.set_adopt_species(id);
            ui.set_adopt_coat(st.adopt_coat.clone().into());
        }
    });
    on!(on_pick_adopt_coat, |ui, st, id: SharedString| {
        st.adopt_coat = id.to_string();
        if let Some(s) = st.adopt_species {
            let coat = st.kinds[&s].model.coat(&id).unwrap_or(0);
            set_portrait(&ui, st, s, coat);
        }
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
        ui.set_size_all(common_size(&st.pets));
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
    // Dragging names (Roger, 2026-11-19): each carries its pet's id.
    ui.on_drag_data(slint::DataTransfer::from);
    on!(on_pet_dropped, |ui,
                         st,
                         data: slint::DataTransfer,
                         to_desk: bool,
                         on: SharedString,
                         after: bool| {
        let Some(id) = data.plain_text().ok().and_then(|t| t.parse::<u64>().ok()) else {
            return;
        };
        let on: Option<u64> = on.parse().ok();
        // On its own name: it stays where it is.
        if on == Some(id) {
            return;
        }
        // On a name: before it, or (its right half) before the one after
        // it, none after the last.
        let side: Vec<u64> = st
            .pets
            .pets
            .iter()
            .filter(|p| p.on_desktop == to_desk)
            .map(|p| p.id)
            .collect();
        let before = on.and_then(|o| {
            let i = side.iter().position(|&x| x == o)?;
            if after {
                side.get(i + 1).copied()
            } else {
                Some(o)
            }
        });
        if before == Some(id) {
            return;
        }
        st.desk_at = Instant::now();
        // Onto a full desktop: the last one out goes to rest (換班); all
        // of them 常駐, the limit makes room. The pet process takes the
        // list as it is now.
        let over = st.pets.seat(id, to_desk, before, st.settings.on_desktop) == Room::Over;
        let roster = st.pets.roster();
        st.send(Msg::Roster(roster));
        if over {
            st.settings.on_desktop = st.pets.out() as u32;
            ui.set_desktop_limit(st.settings.on_desktop.min(i32::MAX as u32) as i32);
            st.send_settings();
        }
        st.selected = Some(id);
        show_pet(&ui, st);
    });
    on!(on_pin_pet, |ui, st, id: SharedString| {
        let Ok(id) = id.parse::<u64>() else { return };
        let Some(pinned) = st
            .pets
            .pets
            .iter()
            .find(|p| p.id == id && p.on_desktop)
            .map(|p| p.pinned)
        else {
            return;
        };
        st.desk_at = Instant::now();
        st.pets.pin(id, !pinned);
        let roster = st.pets.roster();
        st.send(Msg::Roster(roster));
        show_rows(&ui, st);
    });
    on!(on_rotate_picked, |_ui, st, i: i32| {
        let i = usize::try_from(i)
            .unwrap_or(0)
            .min(ROTATE_CHOICES.len() - 1);
        st.settings.rotate_minutes = ROTATE_CHOICES[i];
        st.send_settings();
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
    on!(on_language_picked, |ui, st, i: i32| {
        st.settings.language = usize::try_from(i - 1)
            .ok()
            .and_then(|i| Lang::ALL.get(i))
            .map_or(String::new(), |l| l.id().to_string());
        st.send_settings();
        use_language(&ui, st);
        show_pet(&ui, st);
    });
    on!(on_hide_fullscreen_toggled, |_ui, st, v: bool| {
        st.settings.hide_in_fullscreen = v;
        st.send_settings();
    });
    on!(on_react_input_toggled, |_ui, st, v: bool| {
        st.settings.react_to_input = v;
        st.send_settings();
    });
    on!(on_start_login_toggled, |_ui, st, v: bool| {
        st.settings.start_at_login = v;
        st.send_settings();
    });
    on!(on_desktop_limit_changed, |ui, st, v: i32| {
        st.desk_at = Instant::now();
        st.settings.on_desktop = v.max(1) as u32;
        ui.set_desktop_limit(v.max(1));
        // Pets go to rest or come out at once, here as on the desktop.
        let cap = st.settings.on_desktop;
        if st.pets.fit_to(cap) {
            show_rows(&ui, st);
        }
        st.send_settings();
    });
    on!(on_size_all_picked, |ui, st, pct: i32| {
        let size = pct as f32 / 100.0;
        for p in &mut st.pets.pets {
            p.size = size;
        }
        ui.set_size_all(pct);
        if st.pet().is_some() {
            ui.set_size(size);
        }
        st.send(Msg::SizeAll(size));
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
            trf(
                "已移除開機自動啟動和應用程式選單的捷徑。要完全移除，請結束肉肉賴桌，再刪除這個資料夾：{path}",
                &[("path", &st.data.path.display().to_string())],
            )
            .into(),
        );
    });

    // Preview animation, debounced sending, and noticing when the pets are gone.
    let preview_timer = Timer::default();
    {
        let weak = ui.as_weak();
        let st = st.clone();
        preview_timer.start(TimerMode::Repeated, PREVIEW_TICK, move || {
            if let Some(ui) = weak.upgrade() {
                tick_preview(&ui, &mut st.borrow_mut());
            }
        });
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
                ui.set_notice(
                    tr("肉肉賴桌目前沒有在執行。改動已經存起來，下次開啟時套用。").into(),
                );
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
                } else if !st.pet_dirty {
                    // Who is where changed in the pet process (隨機輪班):
                    // take it, unless a change of ours is still on its way.
                    if st.desk_at.elapsed() > DESK_SETTLE {
                        let roster = fresh.roster();
                        if st.pets.take_roster(&roster) {
                            show_rows(&ui, &mut st);
                        }
                    }
                    // Sizes changed for everyone at once (「大家的大小」 from
                    // the menu or the tray): take them, unless an edit of
                    // ours is still on its way.
                    let mut changed = false;
                    for p in &mut st.pets.pets {
                        if let Some(f) = fresh.pets.iter().find(|f| f.id == p.id)
                            && (f.size - p.size).abs() > 1e-3
                        {
                            p.size = f.size;
                            changed = true;
                        }
                    }
                    if changed {
                        if let Some(size) = st.pet().map(|p| p.size) {
                            ui.set_size(size);
                        }
                        ui.set_size_all(common_size(&st.pets));
                    }
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
    let _restored = repaint_when_restored(&ui);
    let _front = come_forward_when_asked(&ui, &st);
    let _icon = use_program_icon(&ui);
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

/// The pet process asks an open settings window to come to the front (its
/// 「設定」 or 「幫牠換造型」 was used again) with a line on our stdin:
/// `show`, or `show <pet id>` to open on that pet. A thread reads the lines;
/// a timer acts on them in the UI thread: restore the window if it was
/// minimised, bring it to the front, select the pet.
fn come_forward_when_asked(ui: &SettingsWindow, st: &Rc<RefCell<State>>) -> Timer {
    let (tx, rx) = std::sync::mpsc::channel::<Option<u64>>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            let mut words = line.split_whitespace();
            if words.next() != Some("show") {
                continue;
            }
            if tx.send(words.next().and_then(|w| w.parse().ok())).is_err() {
                break;
            }
        }
    });
    let timer = Timer::default();
    let weak = ui.as_weak();
    let st = st.clone();
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(ui) = weak.upgrade() else { return };
        while let Ok(pet) = rx.try_recv() {
            ui.window().set_minimized(false);
            chonkgene_platform::bring_to_front(&ui.window().window_handle());
            if let Some(id) = pet {
                let mut st = st.borrow_mut();
                st.selected = Some(id);
                ui.set_adopting(false);
                show_pet(&ui, &mut st);
            }
            // A restored window may come back with its picture lost.
            ui.set_repaint(ui.get_repaint().wrapping_add(1));
        }
    });
    timer
}

/// Repaint the whole window a few times just after it opens. On Windows the
/// software renderer only redraws what changed, and the first frame can be
/// drawn before the window is on screen and lost, leaving it black until
/// something scrolled. (A restore after minimising does not get one either
/// on Windows: see `repaint_when_restored`.)
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

/// Repaint the whole window after it comes back from being minimised
/// (Roger's seventeenth pass, item 1: on Windows only the preview, which
/// moves, was drawn again; everything else stayed blank). Watched four
/// times a second, with any change of size as well; repainted at once and
/// twice more as it settles.
fn repaint_when_restored(ui: &SettingsWindow) -> Timer {
    let timer = Timer::default();
    let weak = ui.as_weak();
    let mut was_minimized = false;
    let mut was_size = None;
    let mut pending: Vec<std::time::Instant> = Vec::new();
    timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
        let Some(ui) = weak.upgrade() else { return };
        let minimized = ui.window().is_minimized();
        let size = ui.window().size();
        let now = std::time::Instant::now();
        // Restored, or resized (if the backend did not say it was
        // minimised, its size still changed, to nothing and back).
        let resized = was_size.is_some_and(|s| s != size);
        was_size = Some(size);
        if (was_minimized && !minimized) || resized {
            pending = [0, 300, 900]
                .map(|ms| now + Duration::from_millis(ms))
                .to_vec();
        }
        was_minimized = minimized;
        let due = pending.iter().filter(|t| **t <= now).count();
        if due > 0 {
            pending.retain(|t| *t > now);
            ui.set_repaint(ui.get_repaint().wrapping_add(1));
        }
    });
    timer
}

/// The window's icon (PLAN §1.9, "Program icon"). Windows loads the exe's
/// own icon at its title-bar and Alt-Tab sizes, which needs the window to
/// exist, so it is tried as the window comes up; a single image given to
/// Slint would be squeezed down for the title bar instead.
fn use_program_icon(ui: &SettingsWindow) -> [Timer; 3] {
    if !cfg!(windows) {
        let px = 256;
        let buf = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&brand::rgba(px), px, px);
        ui.set_window_icon(Image::from_rgba8(buf));
    }
    [0, 100, 600].map(|ms| {
        let ui = ui.as_weak();
        let t = Timer::default();
        if cfg!(windows) {
            t.start(
                TimerMode::SingleShot,
                Duration::from_millis(ms),
                move || {
                    if let Some(ui) = ui.upgrade() {
                        chonkgene_platform::use_exe_icon(&ui.window().window_handle());
                    }
                },
            );
        }
        t
    })
}

pub fn open_folder(path: &std::path::Path) {
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

    /// Roger, 2026-11-19: names are dragged between 「出場中」 and
    /// 「在寵物窩休息」, a double-click makes one 常駐. Real pointer events
    /// on the window: the eighth pet, resting, dropped on the left half of
    /// the second name out goes before it, and the last one out rests in
    /// its place; a double-click on the first name pins it.
    #[test]
    fn dragging_names_moves_pets_and_a_double_click_pins() {
        use slint::LogicalPosition;
        use slint::platform::{PointerEventButton, WindowEvent};
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
        window.set_size(slint::PhysicalSize::new(880, 620));
        let dir = std::env::temp_dir().join(format!("chonkgene-drag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = DataDir::locate(Some(dir.clone())).unwrap();
        let mut rng = fastrand::Rng::with_seed(4);
        let mut pets = Pets::default();
        for (i, s) in [
            Species::Shiba,
            Species::Fold,
            Species::Duck,
            Species::Munchkin,
            Species::Owl,
            Species::Chick,
            Species::Fox,
            Species::Shiba,
        ]
        .into_iter()
        .enumerate()
        {
            let mut p = PetSave::adopt(i as u64 + 1, s, "", &mut rng);
            p.on_desktop = i < 7;
            pets.pets.push(p);
        }
        pets.next_id = 9;
        store::save(&data.pets(), &pets).unwrap();
        let ui = SettingsWindow::new().unwrap();
        let (st, _timers) = setup(&ui, data, Some(1)).unwrap();
        // In Chinese, so the names sit where the screenshots show them.
        ui.invoke_language_picked(1);
        ui.show().unwrap();
        let draw = || {
            slint::platform::update_timers_and_animations();
            window.request_redraw();
            let mut buf = vec![slint::Rgb8Pixel::default(); 880 * 620];
            window.draw_if_needed(|r| {
                r.render(&mut buf, 880);
            });
        };
        // Twice: names changed in place (the language) settle in the
        // layout on the second frame, and the pointer finds them there.
        draw();
        draw();
        let at = |x: f32, y: f32| LogicalPosition::new(x, y);
        // A little time between events, as a hand gives them (a click
        // and a drag both need the clock to move on).
        let send = |e: WindowEvent| {
            std::thread::sleep(Duration::from_millis(20));
            slint::platform::update_timers_and_animations();
            window.dispatch_event(e);
            draw();
        };
        let left = PointerEventButton::Left;
        // The resting one's name (in 「在寵物窩休息」), to 芋圓's left half
        // (the second name in 「出場中」), in small steps.
        let (from, to) = ((285.0, 380.0), (362.0, 188.0));
        // A mouse comes over a name before it presses it.
        send(WindowEvent::PointerMoved {
            position: at(from.0, from.1),
        });
        send(WindowEvent::PointerPressed {
            position: at(from.0, from.1),
            button: left,
        });
        for k in 1..=12 {
            let u = k as f32 / 12.0;
            send(WindowEvent::PointerMoved {
                position: at(from.0 + (to.0 - from.0) * u, from.1 + (to.1 - from.1) * u),
            });
        }
        send(WindowEvent::PointerReleased {
            position: at(to.0, to.1),
            button: left,
        });
        let out: Vec<u64> = st
            .borrow()
            .pets
            .pets
            .iter()
            .filter(|p| p.on_desktop)
            .map(|p| p.id)
            .collect();
        assert_eq!(
            out,
            [1, 8, 2, 3, 4, 5, 6],
            "dropped before 芋圓, 小九 rested"
        );
        assert_eq!(slint::Model::row_count(&ui.get_den_pets()), 1);
        // Saved for the pet process (not running here).
        let saved = store::load::<Pets>(&dir.join("pets.ron")).unwrap().unwrap();
        assert_eq!(saved.roster(), st.borrow().pets.roster());
        // A double-click on 阿柴's name: picked, then 常駐.
        let first = at(285.0, 188.0);
        send(WindowEvent::PointerMoved { position: first });
        for _ in 0..2 {
            send(WindowEvent::PointerPressed {
                position: first,
                button: left,
            });
            send(WindowEvent::PointerReleased {
                position: first,
                button: left,
            });
        }
        assert_eq!(st.borrow().selected, Some(1), "picked by the click");
        assert!(st.borrow().pets.pets[0].pinned, "pinned by a double-click");
        let _ = std::fs::remove_dir_all(&dir);
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
            (Species::Munchkin, "orange"),
            (Species::Owl, "brown"),
            (Species::Chick, "yellow"),
            (Species::Fox, "white"),
            // An eighth, resting in 寵物窩: one more than the desktop
            // holds, so 「一直待在桌面上」 shows.
            (Species::Shiba, "black"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut p = PetSave::adopt(i as u64 + 1, s, coat, &mut rng);
            p.level = 3;
            p.xp = 60;
            p.on_desktop = i < 7;
            pets.pets.push(p);
        }
        pets.next_id = 9;
        store::save(&data.pets(), &pets).unwrap();

        let ui = SettingsWindow::new().unwrap();
        let (_st, _timers) = setup(&ui, data, Some(1)).unwrap();
        ui.show().unwrap();
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/screens");
        std::fs::create_dir_all(&out).unwrap();
        // Each tab in Chinese, then in Japanese and English (the choice in
        // 一般, as a user would make it: 1 繁體中文, 2 日本語, 3 English).
        let shots = [
            ("pets", 0, false),
            ("adopt", 0, true),
            ("adopt-fox", 0, true),
            ("play", 1, false),
            ("quiet", 2, false),
            ("general", 3, false),
        ];
        for (lang, pick, (name, tab, adopting)) in [("", 1, ()), ("-ja", 2, ()), ("-en", 3, ())]
            .into_iter()
            .flat_map(|(l, p, ())| shots.into_iter().map(move |s| (l, p, s)))
        {
            if name == "pets" {
                ui.invoke_language_picked(pick);
            }
            ui.set_tab(tab);
            ui.set_adopting(adopting);
            if name == "adopt" {
                ui.invoke_pick_adopt_species("chick".into());
            } else if name == "adopt-fox" {
                ui.invoke_pick_adopt_species("fox".into());
                ui.invoke_pick_adopt_coat("night".into());
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
            pm.save_png(out.join(format!("settings-{name}{lang}.png")))
                .unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
