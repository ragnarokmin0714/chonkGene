//! The pet process: one winit event loop owning every pet, bubble and menu
//! window (PLAN §3.2).

use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chonkgene_core::Zone;
use chonkgene_core::anim::{Act, Animator, Expression};
use chonkgene_core::mind::{Line, Mind, Out, World, XP_PER_LEVEL};
use chonkgene_core::motion::{Area, Body, Motion};
use chonkgene_core::shake::ShakeDetector;
use chonkgene_core::skit::{Actor, Cmd, Director};
use chonkgene_core::sound::Sound;
use chonkgene_core::species::{Grumpiness, Species};
use chonkgene_core::terrain::{Terrain, Win};
use chonkgene_platform::tray::{Tray, TrayCommand};
use chonkgene_platform::{Presenter, Shared, autostart, hotkey};
use chonkgene_render::menu::{self, Icon, Item};
use chonkgene_render::text::Face;
use chonkgene_render::{Renderer, bubble};
use chonkgene_rig::{Model, builtin};
use cpu_time::ProcessTime;
use tiny_skia::Pixmap;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::ipc::{self, Claim, Msg};
use crate::lines::Lines;
use crate::store::{self, DataDir, PetSave, Pets, Settings};

const STATS_EVERY: Duration = Duration::from_secs(5);
const SAVE_EVERY: Duration = Duration::from_secs(5);
const POLL_EVERY: Duration = Duration::from_secs(1);
/// How often to read the windows pets stand on: calm, and while a pet is on
/// a window or in the air.
const TERRAIN_EVERY: Duration = Duration::from_millis(400);
const TERRAIN_BUSY_EVERY: Duration = Duration::from_millis(100);
/// A press that moves further than this (pixels) is a lift, not a poke.
const DRAG_START: f32 = 4.0;
const HAPPY_SECS: f64 = 3.0;
const ACT_SECS: f64 = 3.2;

pub struct Options {
    pub data: Option<std::path::PathBuf>,
    pub stats: bool,
}

#[derive(Debug)]
pub enum UserEvent {
    Ipc(Msg),
    Tray(TrayCommand),
}

pub fn run(opts: Options) -> Result<(), String> {
    let data = DataDir::locate(opts.data).map_err(|e| format!("cannot find a data folder: {e}"))?;
    let listener = match ipc::claim().map_err(|e| format!("cannot start: {e}"))? {
        Claim::AlreadyRunning => {
            println!("肉肉賴桌 is already running; asked it to bring the pets back.");
            return Ok(());
        }
        Claim::Ours(listener) => listener,
    };
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    chonkgene_platform::configure_event_loop(&mut builder);
    let event_loop = builder.build().map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    let ipc_proxy = proxy.clone();
    ipc::serve(listener, move |msg| {
        ipc_proxy.send_event(UserEvent::Ipc(msg)).is_ok()
    });
    let mut app = App::new(data, opts.stats, proxy)?;
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    app.save_now();
    Ok(())
}

/// Local hour and day number, for sleep rhythm and daily caps.
fn local_time() -> (f32, i64) {
    let now = jiff::Zoned::now();
    let secs = now.timestamp().as_second() + i64::from(now.offset().seconds());
    (
        secs.rem_euclid(86_400) as f32 / 3600.0,
        secs.div_euclid(86_400),
    )
}

struct Kind {
    model: Arc<Model>,
}

struct Press {
    grab: (f32, f32),
    moved: bool,
    last: Instant,
}

struct Bubble {
    window: Window,
    presenter: Presenter,
    pixmap: Pixmap,
    until: f64,
    placed: (i32, i32),
}

struct Pet {
    save: PetSave,
    species: Species,
    window: Window,
    presenter: Presenter,
    renderer: Renderer,
    body: Body,
    anim: Animator,
    mind: Mind,
    shake: ShakeDetector,
    rng: fastrand::Rng,
    cursor: (f32, f32),
    press: Option<Press>,
    placed: (i32, i32),
    next: Instant,
    last: Instant,
    switch_at: f64,
    happy_until: f64,
    asleep_since: f64,
    hello_at: Option<f64>,
    act: Option<(Grumpiness, f64)>,
    bubble: Option<Bubble>,
    /// Walking is up to the skit director while this is set.
    in_skit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Pat,
    Feed,
    Edit,
    Sleep,
    Wake,
    HideAll,
    Settings,
    Quit,
}

struct Menu {
    pet: u64,
    window: Window,
    presenter: Presenter,
    drawn: menu::Drawn,
    model: menu::Model,
    actions: Vec<Action>,
    hover: Option<usize>,
    pos: (i32, i32),
    /// Outside clicks count only after every button was released once.
    armed: bool,
}

/// Everything a pet needs from the app for one step, borrowed apart from
/// the pet list.
struct Ctx<'a> {
    el: &'a ActiveEventLoop,
    shared: &'a Shared,
    lines: &'a Lines,
    face: &'a mut Face,
    area: Area,
    terrain: &'a Terrain,
    ui_scale: f32,
    world: World,
    sounds: &'a mut Sounds,
    sound_on: bool,
    volume: f32,
}

/// Placeholder sounds, rate-limited (PLAN §1.5).
struct Sounds {
    audio: Option<chonkgene_platform::audio::Audio>,
    cache: HashMap<(Species, Sound), Vec<i16>>,
    last: f64,
    recent: HashMap<u64, Vec<f64>>,
}

/// At most this many sounds per pet per minute, and this far apart overall.
const SOUNDS_PER_MINUTE: usize = 6;
const SOUND_GAP: f64 = 0.15;

impl Sounds {
    fn new() -> Self {
        Self {
            audio: chonkgene_platform::audio::Audio::new(),
            cache: HashMap::new(),
            last: f64::NEG_INFINITY,
            recent: HashMap::new(),
        }
    }

    fn play(&mut self, pet: u64, species: Species, sound: Sound, now: f64, volume: f32) {
        let Some(audio) = &self.audio else {
            return;
        };
        if now - self.last < SOUND_GAP {
            return;
        }
        let recent = self.recent.entry(pet).or_default();
        recent.retain(|t| now - t < 60.0);
        if recent.len() >= SOUNDS_PER_MINUTE {
            return;
        }
        recent.push(now);
        self.last = now;
        let samples = self
            .cache
            .entry((species, sound))
            .or_insert_with(|| chonkgene_core::sound::synth(species, sound, 7));
        audio.play(samples, chonkgene_core::sound::RATE, volume);
    }
}

/// Which sound goes with a line.
fn line_sound(line: Line) -> Sound {
    match line {
        Line::PokeSmall | Line::TurnOver => Sound::Poke,
        Line::Laugh | Line::SkitChase => Sound::Laugh,
        Line::FedUp
        | Line::Overdone
        | Line::StillOverdone
        | Line::AfterGrumpy
        | Line::Full
        | Line::SkitShrug => Sound::Sigh,
        Line::Grumpy
        | Line::Dizzy
        | Line::GrudgePoke
        | Line::GrudgePat
        | Line::SkitGlare
        | Line::SkitHuff => Sound::Grumpy,
        Line::Eat => Sound::Eat,
        Line::SleepTalk => Sound::Snore,
        _ => Sound::Voice,
    }
}

struct App {
    data: DataDir,
    settings: Settings,
    store: Pets,
    dirty: bool,
    kinds: HashMap<Species, Kind>,
    lines: Lines,
    bubble_face: Face,
    ui_face: Face,
    shared: Option<Shared>,
    area: Area,
    /// What pets stand on: floors and window tops (PLAN §1.3).
    terrain: Terrain,
    terrain_at: Instant,
    /// When the terrain was last read, for how fast windows move.
    terrain_read: Instant,
    ui_scale: f32,
    pets: Vec<Pet>,
    menu: Option<Menu>,
    hidden: bool,
    fullscreen: bool,
    user_idle: f64,
    poll_at: Instant,
    save_at: Instant,
    start: Instant,
    rng: fastrand::Rng,
    child: Option<Child>,
    open_settings_on_start: bool,
    proxy: EventLoopProxy<UserEvent>,
    director: Director,
    sounds: Sounds,
    tray: Option<Tray>,
    hotkey: Option<hotkey::Hotkey>,
    stats: bool,
    stats_at: Instant,
    stats_cpu: ProcessTime,
    frames: u64,
    /// Time spent drawing pet frames since the last report, and the part of
    /// it spent handing them to the window system.
    drawing: Duration,
    presenting: Duration,
}

impl App {
    fn new(data: DataDir, stats: bool, proxy: EventLoopProxy<UserEvent>) -> Result<Self, String> {
        let mut kinds = HashMap::new();
        for species in Species::ALL {
            let model =
                builtin::load(species.id()).map_err(|e| format!("{}: {e}", species.id()))?;
            kinds.insert(
                species,
                Kind {
                    model: Arc::new(model),
                },
            );
        }
        let mut notes = Vec::new();
        let settings = store::load::<Settings>(&data.settings())
            .unwrap_or_else(|e| {
                notes.push(e);
                None
            })
            .unwrap_or_default();
        let mut pets = store::load::<Pets>(&data.pets())
            .unwrap_or_else(|e| {
                notes.push(e);
                None
            })
            .unwrap_or_default();
        pets.sanitise();
        for note in notes {
            eprintln!("{note}");
        }
        let now = Instant::now();
        let mut app = Self {
            data,
            settings,
            store: pets,
            dirty: false,
            kinds,
            lines: Lines::zh_tw()?,
            bubble_face: Face::bubble(),
            ui_face: Face::ui(),
            shared: None,
            area: Area {
                x: 0.0,
                y: 0.0,
                w: 1920.0,
                h: 1080.0,
            },
            terrain: Terrain::default(),
            terrain_at: now,
            terrain_read: now,
            ui_scale: 1.0,
            pets: Vec::new(),
            menu: None,
            hidden: false,
            fullscreen: false,
            user_idle: 0.0,
            poll_at: now,
            save_at: now + SAVE_EVERY,
            start: now,
            rng: fastrand::Rng::new(),
            child: None,
            open_settings_on_start: false,
            proxy,
            director: Director::default(),
            sounds: Sounds::new(),
            tray: None,
            hotkey: None,
            stats,
            stats_at: now + STATS_EVERY,
            stats_cpu: ProcessTime::now(),
            frames: 0,
            drawing: Duration::ZERO,
            presenting: Duration::ZERO,
        };
        app.first_run();
        Ok(app)
    }

    /// First launch: a Shiba and a Fold move in, and settings opens to say hello.
    fn first_run(&mut self) {
        if self.settings.welcomed {
            return;
        }
        if self.store.pets.is_empty() {
            for (species, coat) in [(Species::Shiba, "red"), (Species::Fold, "bluegrey")] {
                let id = self.store.next_id;
                self.store.next_id += 1;
                self.store
                    .pets
                    .push(PetSave::adopt(id, species, coat, &mut self.rng));
            }
        }
        self.settings.welcomed = true;
        self.open_settings_on_start = true;
        // Saved right away: the settings window reads these files as it opens.
        for result in [
            store::save(&self.data.settings(), &self.settings),
            store::save(&self.data.pets(), &self.store),
        ] {
            if let Err(e) = result {
                eprintln!("{e}");
            }
        }
    }

    fn secs(&self, at: Instant) -> f64 {
        (at - self.start).as_secs_f64()
    }

    fn world(&self, now: Instant) -> World {
        let (hour, day) = local_time();
        World {
            now: self.secs(now),
            hour,
            day,
            user_idle: self.user_idle,
        }
    }

    /// Copy the live affinity of every pet into the saves. It must run before
    /// any pet is rebuilt or removed, or recent affinity would be lost and
    /// affinity must never go down (PLAN §0).
    /// Read the desktop again: each monitor's floor and the windows to stand
    /// on (PLAN §1.3). Pets on a window that moved move with it.
    fn refresh_terrain(&mut self) {
        let Some(shared) = &self.shared else {
            return;
        };
        let mut floors: Vec<Area> = shared
            .work_areas()
            .into_iter()
            .map(|(x, y, w, h)| Area {
                x: x as f32,
                y: y as f32,
                w: w as f32,
                h: h as f32,
            })
            .collect();
        if floors.is_empty() {
            floors.push(self.area);
        }
        let ours: Vec<u32> = self.pets.iter().map(|p| p.presenter.id()).collect();
        let windows: Vec<Win> = shared
            .windows(&ours)
            .into_iter()
            .map(|r| Win {
                id: r.id,
                x: r.x as f32,
                y: r.y as f32,
                w: r.w as f32,
                h: r.h as f32,
            })
            .collect();
        // A window top with less room above it than a pet is tall is no
        // ledge (a maximised window's).
        let headroom = self
            .pets
            .iter()
            .map(|p| p.body.size.1)
            .fold(0.0, f32::max)
            .max(1.0);
        let terrain = Terrain::new(floors, &windows, headroom);
        let now = Instant::now();
        let dt = (now - self.terrain_read).as_secs_f32();
        self.terrain_read = now;
        for (i, p) in self.pets.iter_mut().enumerate() {
            let was = p.body.pos;
            p.body.carry(&self.terrain, &terrain, dt);
            if p.body.pos != was {
                println!(
                    "pet {i}: rides its window by ({:.0}, {:.0})",
                    p.body.pos.0 - was.0,
                    p.body.pos.1 - was.1
                );
            }
        }
        self.terrain = terrain;
    }

    fn collect_affinity(&mut self) {
        for p in &self.pets {
            if let Some(s) = self.store.pets.iter_mut().find(|s| s.id == p.save.id) {
                s.set_affinity(&p.mind.affinity);
            }
        }
    }

    fn save_now(&mut self) {
        self.collect_affinity();
        if let Err(e) = store::save(&self.data.pets(), &self.store) {
            eprintln!("{e}");
        }
        self.dirty = false;
    }

    fn open_settings(&mut self, pet: Option<u64>) {
        if let Some(child) = &mut self.child
            && matches!(child.try_wait(), Ok(None))
        {
            return;
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut cmd = Command::new(exe);
        cmd.arg("--settings").arg("--data").arg(&self.data.path);
        if let Some(id) = pet {
            cmd.arg("--pet").arg(id.to_string());
        }
        match cmd.spawn() {
            Ok(child) => self.child = Some(child),
            Err(e) => eprintln!("cannot open settings: {e}"),
        }
    }

    /// Spawn and remove pet windows so the desktop matches the saves.
    fn sync_desktop(&mut self, el: &ActiveEventLoop) {
        self.collect_affinity();
        let Some(shared) = &self.shared else {
            return;
        };
        let wanted: Vec<u64> = self
            .store
            .pets
            .iter()
            .filter(|p| p.on_desktop)
            .take(self.settings.on_desktop as usize)
            .map(|p| p.id)
            .collect();
        let mut removed = false;
        self.pets.retain(|p| {
            let keep = wanted.contains(&p.save.id);
            removed |= !keep;
            keep
        });
        if removed && self.menu.as_ref().is_some_and(|m| !wanted.contains(&m.pet)) {
            self.menu = None;
        }
        let now = Instant::now();
        let t = self.secs(now);
        for (slot, id) in wanted.iter().enumerate() {
            if self.pets.iter().any(|p| p.save.id == *id) {
                continue;
            }
            let Some(save) = self.store.pets.iter().find(|p| p.id == *id).cloned() else {
                continue;
            };
            match spawn(
                el,
                shared,
                &self.kinds,
                save,
                self.area,
                &self.terrain,
                self.ui_scale,
                slot,
                t,
                now,
            ) {
                Ok(pet) => {
                    if self.hidden || self.fullscreen {
                        pet.window.set_visible(false);
                    }
                    self.pets.push(pet);
                }
                Err(e) => eprintln!("cannot place a pet: {e}"),
            }
        }
    }

    /// Make the login entry match the setting, and repair it if the folder moved.
    fn sync_autostart(&self) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        if let Err(e) = autostart::sync(self.settings.start_at_login, &exe) {
            eprintln!("start at login: {e}");
        }
    }

    fn command(&mut self, el: &ActiveEventLoop, command: TrayCommand) {
        match command {
            TrayCommand::Toggle => {
                self.hidden = !self.hidden;
                self.set_visible();
            }
            TrayCommand::Settings => self.open_settings(None),
            TrayCommand::Quit => {
                self.save_now();
                el.exit();
            }
        }
    }

    fn set_visible(&mut self) {
        let visible = !self.hidden && !self.fullscreen;
        for p in &mut self.pets {
            p.window.set_visible(visible);
            if !visible {
                p.bubble = None;
            }
            p.next = Instant::now();
        }
        if !visible {
            self.menu = None;
        }
    }

    fn handle(&mut self, el: &ActiveEventLoop, msg: Msg) {
        match msg {
            Msg::Show => {
                self.hidden = false;
                self.set_visible();
                self.open_settings(None);
            }
            Msg::Settings(s) => {
                self.settings = s;
                if let Err(e) = store::save(&self.data.settings(), &self.settings) {
                    eprintln!("{e}");
                }
                self.sync_autostart();
                self.sync_desktop(el);
            }
            Msg::Pet(mut update) => {
                update.sanitise();
                self.collect_affinity();
                let Some(save) = self.store.pets.iter_mut().find(|p| p.id == update.id) else {
                    return;
                };
                // Affinity lives in the pet process; never take it from settings.
                let affinity = save.affinity();
                *save = update;
                save.set_affinity(&affinity);
                let save = save.clone();
                self.dirty = true;
                if let Some(i) = self.pets.iter().position(|p| p.save.id == save.id) {
                    let resized = (self.pets[i].save.size - save.size).abs() > 1e-3;
                    if resized {
                        self.pets.remove(i);
                    } else {
                        let p = &mut self.pets[i];
                        let kind = &self.kinds[&p.species];
                        p.renderer.set_chonk(save.chonk);
                        if let Some(coat) = kind.model.coat(&save.coat) {
                            p.renderer.set_coat(coat);
                        }
                        p.anim.set_chonk(save.chonk);
                        p.mind.personality = save.personality();
                        p.window.set_title(&save.name);
                        p.save = save;
                        p.next = Instant::now();
                    }
                }
                self.sync_desktop(el);
            }
            Msg::Adopt { species, coat } => {
                let Some(s) = Species::from_id(&species) else {
                    return;
                };
                let id = self.store.next_id;
                self.store.next_id += 1;
                let mut save = PetSave::adopt(id, s, &coat, &mut self.rng);
                let on_desktop = self.store.pets.iter().filter(|p| p.on_desktop).count();
                save.on_desktop = on_desktop < self.settings.on_desktop as usize;
                self.store.pets.push(save);
                self.dirty = true;
                self.sync_desktop(el);
            }
            Msg::Release(id) => {
                self.store.pets.retain(|p| p.id != id);
                self.dirty = true;
                self.sync_desktop(el);
            }
            Msg::RemoveTraces => {
                self.settings.start_at_login = false;
                self.sync_autostart();
            }
            Msg::Quit => el.exit(),
        }
        if self.dirty {
            self.save_now();
        }
    }

    fn open_menu(&mut self, el: &ActiveEventLoop, i: usize, world: &World) {
        let Some(shared) = &self.shared else {
            return;
        };
        let p = &self.pets[i];
        let asleep = p.mind.asleep();
        let snacks = p.mind.snacks_left(world.day);
        let kind = &self.kinds[&p.species];
        let coat = kind.model.coat(&p.save.coat).unwrap_or(0);
        let avatar = kind.model.coats[coat].swatch();
        let item = |label: &str, hint: Option<String>, icon, separated| Item {
            label: label.into(),
            hint,
            icon,
            separated,
        };
        let mut items = vec![
            item("摸摸頭", None, Icon::Heart, false),
            item(
                "餵零食",
                Some(if snacks > 0 {
                    format!("還有 {snacks} 份")
                } else {
                    "吃飽了".into()
                }),
                Icon::Snack,
                false,
            ),
            item("改名字…", None, Icon::Pencil, false),
            item("幫牠換造型…", None, Icon::Palette, false),
        ];
        let mut actions = vec![Action::Pat, Action::Feed, Action::Edit, Action::Edit];
        if asleep {
            items.push(item("輕輕叫醒牠", None, Icon::Sun, false));
            actions.push(Action::Wake);
        } else {
            items.push(item("讓牠去睡覺", None, Icon::Moon, false));
            actions.push(Action::Sleep);
        }
        items.push(item(
            "全部收起來",
            Some("Ctrl+Alt+P".into()),
            Icon::Hide,
            true,
        ));
        items.push(item("設定…", None, Icon::Sliders, false));
        items.push(item("結束肉肉賴桌", None, Icon::Exit, false));
        actions.extend([Action::HideAll, Action::Settings, Action::Quit]);
        let status = if asleep {
            Some("睡覺中".to_string())
        } else if p.mind.grumpy() {
            Some("有起床氣".to_string())
        } else {
            None
        };
        let model = menu::Model {
            name: p.save.name.clone(),
            species: p.species.label().into(),
            level: p.mind.affinity.level,
            progress: p.mind.affinity.xp as f32 / XP_PER_LEVEL as f32,
            grumpy: p.mind.grumpy() && !asleep,
            status,
            avatar,
            items,
        };
        let Some(drawn) = menu::draw(&mut self.ui_face, &model, None, self.ui_scale) else {
            return;
        };
        let (w, h) = (drawn.pixmap.width() as i32, drawn.pixmap.height() as i32);
        let cursor = shared.cursor().unwrap_or((
            p.placed.0 + p.cursor.0 as i32,
            p.placed.1 + p.cursor.1 as i32,
        ));
        let (ax, ay, aw, ah) = (
            self.area.x as i32,
            self.area.y as i32,
            self.area.w as i32,
            self.area.h as i32,
        );
        let pos = (
            (cursor.0 + 4).clamp(ax, (ax + aw - w).max(ax)),
            (cursor.1 - h / 3).clamp(ay, (ay + ah - h).max(ay)),
        );
        let attrs = chonkgene_platform::pet_window((w as u32, h as u32), pos, "肉肉賴桌");
        let Ok(window) = el.create_window(attrs) else {
            return;
        };
        let Ok(mut presenter) = Presenter::new(shared, &window) else {
            return;
        };
        if let Err(e) = presenter.present(shared, &drawn.pixmap) {
            eprintln!("present failed: {e}");
        }
        self.menu = Some(Menu {
            pet: p.save.id,
            window,
            presenter,
            drawn,
            model,
            actions,
            hover: None,
            pos,
            armed: false,
        });
    }

    fn redraw_menu(&mut self) {
        let (Some(m), Some(shared)) = (&mut self.menu, &self.shared) else {
            return;
        };
        if let Some(drawn) = menu::draw(&mut self.ui_face, &m.model, m.hover, self.ui_scale) {
            m.drawn = drawn;
            if let Err(e) = m.presenter.present(shared, &m.drawn.pixmap) {
                eprintln!("present failed: {e}");
            }
        }
    }

    fn act(&mut self, el: &ActiveEventLoop, pet: u64, action: Action) {
        self.menu = None;
        let now = Instant::now();
        let world = self.world(now);
        match action {
            Action::HideAll => {
                self.hidden = true;
                self.set_visible();
                return;
            }
            Action::Settings => return self.open_settings(None),
            Action::Edit => return self.open_settings(Some(pet)),
            Action::Quit => {
                self.save_now();
                el.exit();
                return;
            }
            _ => {}
        }
        let App {
            pets,
            shared,
            lines,
            bubble_face,
            sounds,
            settings,
            area,
            terrain,
            ui_scale,
            dirty,
            ..
        } = self;
        let (Some(shared), Some(p)) = (shared.as_ref(), pets.iter_mut().find(|p| p.save.id == pet))
        else {
            return;
        };
        let mut ctx = Ctx {
            el,
            shared,
            lines,
            face: bubble_face,
            sounds,
            sound_on: settings.sound,
            volume: f32::from(settings.volume) / 100.0,
            area: *area,
            terrain,
            ui_scale: *ui_scale,
            world,
        };
        let out = match action {
            Action::Pat => p.mind.pat(&world),
            Action::Feed => p.mind.feed(&world),
            Action::Sleep => {
                let mut rng = p.rng.clone();
                let out = p.mind.put_to_sleep(&world, &mut rng);
                p.rng = rng;
                out
            }
            Action::Wake => p.mind.wake(&world),
            _ => Out::default(),
        };
        p.apply(&mut ctx, out);
        p.next = now;
        *dirty = true;
    }

    fn report(&mut self, now: Instant) {
        let wall = STATS_EVERY.as_secs_f64();
        let cpu = self.stats_cpu.elapsed().as_secs_f64() / wall * 100.0;
        let rss = memory_stats::memory_stats()
            .map(|m| m.physical_mem as f64 / 1_048_576.0)
            .unwrap_or(f64::NAN);
        let us = |d: Duration| d.as_secs_f64() * 1e6 / self.frames.max(1) as f64;
        let (draw, present) = (us(self.drawing), us(self.presenting));
        println!(
            "[{:6.1}s] {} pets | {:5.1} frames/s, {draw:4.0} µs each ({present:.0} presenting) | CPU {cpu:5.2}% of one core | RSS {rss:.1} MiB",
            self.secs(now),
            self.pets.len(),
            self.frames as f64 / wall,
        );
        self.frames = 0;
        self.drawing = Duration::ZERO;
        self.presenting = Duration::ZERO;
        self.stats_cpu = ProcessTime::now();
        self.stats_at = now + STATS_EVERY;
    }
}

/// When the event loop next has work: the earliest of the jobs that are due.
/// A job that is not pending must be `None`, never an old time: waiting
/// until a past instant returns at once, and the loop spins at 100% CPU.
fn wake_at(due: impl IntoIterator<Item = Option<Instant>>) -> Instant {
    due.into_iter()
        .flatten()
        .min()
        .unwrap_or_else(|| Instant::now() + Duration::from_secs(1))
}

/// The next save, if anything is unsaved. `save_at` stays behind once a
/// save is done; only unsaved changes make it a reason to wake.
fn pending_save(dirty: bool, save_at: Instant) -> Option<Instant> {
    dirty.then_some(save_at)
}

/// The pets as the skit director sees them.
fn actors(pets: &[Pet]) -> Vec<Actor> {
    pets.iter()
        .map(|p| Actor {
            id: p.save.id,
            species: p.species,
            x: p.center_x(),
            available: !p.mind.asleep()
                && !p.mind.grumpy()
                && p.act.is_none()
                && p.press.is_none()
                && p.body.motion != Motion::Fall
                && p.anim.expression != Expression::Sulk,
        })
        .collect()
}

fn direct(pets: &mut [Pet], ctx: &mut Ctx, cmds: Vec<Cmd>) {
    for cmd in cmds {
        let id = match cmd {
            Cmd::Walk { id, .. }
            | Cmd::Face { id, .. }
            | Cmd::Say { id, .. }
            | Cmd::Express { id, .. } => id,
        };
        let Some(p) = pets.iter_mut().find(|p| p.save.id == id) else {
            continue;
        };
        p.next = Instant::now();
        match cmd {
            Cmd::Walk { x, pace, .. } => {
                p.body.facing = if x >= p.center_x() { 1.0 } else { -1.0 };
                p.body.motion = Motion::Walk;
                p.body.pace = pace;
            }
            Cmd::Face { x, .. } => {
                p.body.facing = if x >= p.center_x() { 1.0 } else { -1.0 };
                p.body.motion = Motion::Idle;
                p.body.pace = 1.0;
            }
            Cmd::Say { line, .. } => p.say(ctx, line),
            Cmd::Express { expression, .. } => {
                if expression == Expression::Happy && p.anim.expression != Expression::Happy {
                    p.anim.cheer();
                }
                p.anim.expression = expression;
                if expression == Expression::Happy {
                    p.happy_until = ctx.world.now + HAPPY_SECS;
                }
            }
        }
    }
}

/// A paw print for the tray, straight-alpha RGBA.
fn tray_icon(scale: f32) -> (Vec<u8>, u32) {
    let size = (32.0 * scale.max(1.0)).round() as u32;
    let Some(mut pm) = tiny_skia::Pixmap::new(size, size) else {
        return (vec![0; 4], 1);
    };
    let s = size as f32 / 24.0;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(0xE8, 0x89, 0x4A, 255);
    paint.anti_alias = true;
    let ts = tiny_skia::Transform::from_scale(s, s);
    if let Some(r) = tiny_skia::Rect::from_xywh(6.8, 11.2, 10.4, 8.6)
        && let Some(pad) = tiny_skia::PathBuilder::from_oval(r)
    {
        pm.fill_path(&pad, &paint, tiny_skia::FillRule::Winding, ts, None);
    }
    for (x, y) in [(5.8, 10.0), (9.6, 6.4), (14.4, 6.4), (18.2, 10.0)] {
        if let Some(toe) = tiny_skia::PathBuilder::from_circle(x, y, 2.3) {
            pm.fill_path(&toe, &paint, tiny_skia::FillRule::Winding, ts, None);
        }
    }
    let rgba = pm
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    (rgba, size)
}

#[allow(clippy::too_many_arguments)]
fn spawn(
    el: &ActiveEventLoop,
    shared: &Shared,
    kinds: &HashMap<Species, Kind>,
    save: PetSave,
    area: Area,
    terrain: &Terrain,
    ui_scale: f32,
    slot: usize,
    t: f64,
    now: Instant,
) -> Result<Pet, String> {
    let species = save.species().ok_or("unknown species")?;
    let kind = &kinds[&species];
    let scale = save.size * ui_scale;
    let size = Renderer::pixel_size(scale);
    let size_f = (size.0 as f32, size.1 as f32);
    let span = (area.w - size_f.0).max(1.0);
    let x = area.x + (area.w * 0.55 + slot as f32 * size_f.0 * 0.9) % span;
    let foot = chonkgene_render::raymarch::FLOOR_PX * scale;
    let body = Body::on_ground(terrain, x, size_f, foot);
    let pos = (body.pos.0.round() as i32, body.pos.1.round() as i32);
    let window = el
        .create_window(chonkgene_platform::pet_window(size, pos, &save.name))
        .map_err(|e| e.to_string())?;
    let presenter = Presenter::new(shared, &window)?;
    let coat = kind.model.coat(&save.coat).unwrap_or(0);
    let renderer = Renderer::new(Arc::clone(&kind.model), scale, save.chonk, coat)
        .ok_or("cannot allocate a pixmap")?;
    let seed = save.id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (t * 1000.0) as u64;
    Ok(Pet {
        mind: Mind::new(species, save.personality(), save.affinity()),
        anim: Animator::new(seed, save.chonk, species.waddle()),
        species,
        window,
        presenter,
        renderer,
        body,
        shake: ShakeDetector::default(),
        rng: fastrand::Rng::with_seed(seed),
        cursor: (0.0, 0.0),
        press: None,
        placed: pos,
        next: now,
        last: now,
        switch_at: t + 2.0 + slot as f64,
        happy_until: 0.0,
        asleep_since: 0.0,
        hello_at: Some(t + 1.2 + slot as f64 * 1.4),
        act: None,
        bubble: None,
        in_skit: false,
        save,
    })
}

impl Pet {
    fn center_x(&self) -> f32 {
        self.placed.0 as f32 + self.renderer.pixmap().width() as f32 / 2.0
    }

    fn say(&mut self, ctx: &mut Ctx, line: Line) {
        let Some(text) = ctx.lines.pick(
            self.species,
            line,
            &self.save.name,
            &self.save.you,
            &mut self.rng,
        ) else {
            return;
        };
        let Some(pixmap) = bubble::bubble(ctx.face, &text, ctx.ui_scale) else {
            return;
        };
        let size = (pixmap.width(), pixmap.height());
        let pos = self.bubble_pos(ctx, size);
        let until = ctx.world.now + 2.4 + 0.12 * text.chars().count() as f64;
        let reuse = self
            .bubble
            .take()
            .filter(|b| b.pixmap.width() == size.0 && b.pixmap.height() == size.1);
        let (window, presenter) = match reuse {
            Some(b) => (b.window, b.presenter),
            None => {
                let attrs = chonkgene_platform::pet_window(size, pos, "肉肉賴桌");
                let Ok(window) = ctx.el.create_window(attrs) else {
                    return;
                };
                let Ok(presenter) = Presenter::new(ctx.shared, &window) else {
                    return;
                };
                (window, presenter)
            }
        };
        window.set_outer_position(PhysicalPosition::new(pos.0, pos.1));
        let mut b = Bubble {
            window,
            presenter,
            pixmap,
            until,
            placed: pos,
        };
        if let Err(e) = b.presenter.present(ctx.shared, &b.pixmap) {
            eprintln!("present failed: {e}");
        }
        self.bubble = Some(b);
        if ctx.sound_on {
            ctx.sounds.play(
                self.save.id,
                self.species,
                line_sound(line),
                ctx.world.now,
                ctx.volume,
            );
        }
    }

    fn bubble_pos(&self, ctx: &Ctx, size: (u32, u32)) -> (i32, i32) {
        let (ax, ay) = self.renderer.anchor();
        let (x, y) = (
            self.placed.0 + ax as i32 - size.0 as i32 / 2,
            self.placed.1 + ay as i32 - size.1 as i32,
        );
        let a = ctx.area;
        (
            x.clamp(a.x as i32, (a.x + a.w) as i32 - size.0 as i32),
            y.max(a.y as i32),
        )
    }

    fn apply(&mut self, ctx: &mut Ctx, out: Out) {
        if let Some(e) = out.expression {
            // Cheered up (a pat, a snack): a happy double hop.
            if e == Expression::Happy && self.anim.expression != Expression::Happy {
                self.anim.cheer();
            }
            self.anim.expression = e;
            if e == Expression::Happy {
                self.happy_until = ctx.world.now + HAPPY_SECS;
            }
        }
        if let Some(kind) = out.act {
            self.act = Some((kind, ctx.world.now + ACT_SECS));
            match kind {
                Grumpiness::Swat | Grumpiness::PuffUp => {
                    self.anim.poke(Zone::Belly);
                    self.anim.land(1200.0);
                }
                Grumpiness::Zoomies => {
                    self.body.motion = Motion::Walk;
                    self.body.pace = 4.0;
                }
                _ => {}
            }
        }
        let line = out.line.or(out.level_up.then_some(Line::LevelUp));
        if let Some(line) = line {
            self.say(ctx, line);
        }
    }

    fn step(&mut self, ctx: &mut Ctx, i: usize, dt: f32, now: Instant, cursor: Option<(i32, i32)>) {
        let t = ctx.world.now;
        if let Some(at) = self.hello_at
            && t >= at
        {
            self.hello_at = None;
            if !self.mind.asleep() {
                self.say(ctx, Line::Hello);
                self.anim.gesture(Act::Wave);
            }
        }

        if let Some(press) = self.press.as_mut() {
            if let Some((cx, cy)) = cursor {
                let target = (cx as f32 - press.grab.0, cy as f32 - press.grab.1);
                let dist = (target.0 - self.body.pos.0).hypot(target.1 - self.body.pos.1);
                if !press.moved && dist > DRAG_START {
                    press.moved = true;
                    self.anim.lift();
                }
                if press.moved {
                    let held_dt = (now - press.last).as_secs_f32();
                    press.last = now;
                    self.body.hold_at(target, held_dt);
                    self.anim.sway = (-self.body.vel.0 * 0.03).clamp(-20.0, 20.0);
                    if self.shake.feed(target.0, t) {
                        let out = self.mind.shake(&ctx.world);
                        self.anim.poke(Zone::Belly);
                        println!("pet {i}: shaken");
                        self.apply(ctx, out);
                    }
                }
            }
        } else {
            if let Some(landing) = self.body.step(dt, ctx.terrain) {
                if self.body.on_window() && self.body.motion == Motion::Idle {
                    println!("pet {i}: lands on a window");
                }
                self.anim.land(landing.speed);
                if ctx.sound_on && landing.speed > 600.0 {
                    ctx.sounds.play(
                        self.save.id,
                        self.species,
                        Sound::Land,
                        ctx.world.now,
                        ctx.volume,
                    );
                }
            }
            let free = !self.mind.asleep() && self.act.is_none() && !self.in_skit;
            if !self.in_skit && self.act.is_none() && self.body.pace != 1.0 {
                self.body.pace = 1.0;
            }
            if self.mind.asleep() && self.body.motion == Motion::Walk {
                self.body.motion = Motion::Idle;
            }
            if free
                && matches!(self.body.motion, Motion::Idle | Motion::Walk)
                && t >= self.switch_at
            {
                // Now and then, hop up onto a window nearby (PLAN §1.3).
                let (fx, fy) = self.body.feet();
                let (w, h) = self.body.size;
                let above = (self.body.motion == Motion::Idle && self.rng.f32() < 0.35)
                    .then(|| ctx.terrain.ledge_above(fx, fy, 0.25 * h, 1.6 * h, 1.5 * w))
                    .flatten();
                if let Some((ledge, x)) = above {
                    self.body.leap(x, ledge.y);
                    self.anim.land(500.0);
                } else {
                    self.body.motion = if self.body.motion == Motion::Idle {
                        Motion::Walk
                    } else {
                        Motion::Idle
                    };
                    if self.rng.bool() {
                        self.body.facing = -self.body.facing;
                    }
                }
                self.switch_at = t + 3.0 + f64::from(self.rng.f32()) * 5.0;
            }
        }

        if let Some((kind, until)) = self.act {
            if t >= until {
                self.act = None;
                self.body.pace = 1.0;
                if self.body.motion == Motion::Walk {
                    self.body.motion = Motion::Idle;
                }
                self.anim.sway = 0.0;
            } else {
                match kind {
                    Grumpiness::Zoomies => {
                        if ((until - t) * 2.0).fract() < 0.03 {
                            self.body.facing = -self.body.facing;
                        }
                    }
                    Grumpiness::Dizzy => self.anim.sway = 12.0 * (t as f32 * 7.0).sin(),
                    Grumpiness::Spin => {
                        if ((until - t) * 6.0).fract() < 0.1 {
                            self.body.facing = -self.body.facing;
                        }
                    }
                    Grumpiness::Swat | Grumpiness::PuffUp => {}
                }
            }
        }

        let out = self.mind.tick(&ctx.world, &mut self.rng);
        if out != Out::default() {
            self.apply(ctx, out);
        }
        if self.mind.asleep() {
            if self.anim.expression != Expression::Asleep {
                self.asleep_since = t;
            }
            self.anim.expression = Expression::Asleep;
        } else if self.anim.expression == Expression::Asleep
            || (self.anim.expression == Expression::Happy && t >= self.happy_until)
        {
            self.anim.expression = Expression::Neutral;
        }
        self.anim.walking = self.body.motion == Motion::Walk;
        self.anim.pace = self.body.pace;
        self.anim.facing = self.body.facing;
        self.anim.step(dt);

        let placed = (
            self.body.pos.0.round() as i32,
            self.body.pos.1.round() as i32,
        );
        if placed != self.placed {
            self.window
                .set_outer_position(PhysicalPosition::new(placed.0, placed.1));
            self.placed = placed;
        }
        if let Some(b) = &self.bubble {
            if t >= b.until {
                self.bubble = None;
            } else {
                let size = (b.pixmap.width(), b.pixmap.height());
                let pos = self.bubble_pos(ctx, size);
                if let Some(b) = &mut self.bubble
                    && pos != b.placed
                {
                    b.window
                        .set_outer_position(PhysicalPosition::new(pos.0, pos.1));
                    b.placed = pos;
                }
            }
        }
    }

    /// Draw and show a frame; returns how long showing it took.
    fn draw(&mut self, shared: &Shared, t: f64) -> Duration {
        // Sprites rendered in the background land here; the slowest pet
        // still draws at 5 fps, so a new one shows within 0.2 s.
        self.renderer.poll();
        let params = self.anim.params();
        if self.mind.asleep() {
            self.renderer
                .render_asleep(&params, (t - self.asleep_since) as f32);
        } else {
            self.renderer.render(&params, self.anim.expression);
        }
        if !self.renderer.changed() {
            return Duration::ZERO;
        }
        let started = Instant::now();
        if let Err(e) = self.presenter.present(shared, self.renderer.pixmap()) {
            eprintln!("present failed: {e}");
        }
        started.elapsed()
    }

    fn moving(&self) -> bool {
        self.press.as_ref().is_some_and(|p| p.moved)
            || self.body.motion == Motion::Fall
            || self.act.is_some_and(|(k, _)| {
                matches!(
                    k,
                    Grumpiness::Zoomies | Grumpiness::Dizzy | Grumpiness::Spin
                )
            })
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.shared.is_some() {
            return;
        }
        let shared = match Shared::new() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                el.exit();
                return;
            }
        };
        let monitor = el.primary_monitor();
        self.ui_scale = monitor.as_ref().map_or(1.0, |m| m.scale_factor() as f32);
        let (x, y, w, h) = shared
            .work_area()
            .or_else(|| {
                monitor.map(|m| {
                    let (p, s) = (m.position(), m.size());
                    (p.x, p.y, s.width as i32, s.height as i32)
                })
            })
            .unwrap_or((0, 0, 1920, 1080));
        self.area = Area {
            x: x as f32,
            y: y as f32,
            w: w as f32,
            h: h as f32,
        };
        self.terrain = Terrain::flat(self.area);
        println!(
            "肉肉賴桌 · {} · data in {}",
            shared.describe(),
            self.data.path.display()
        );
        self.shared = Some(shared);
        self.refresh_terrain();
        self.sync_autostart();
        let proxy = std::sync::Arc::new(std::sync::Mutex::new(self.proxy.clone()));
        let send = move |c: TrayCommand| {
            if let Ok(p) = proxy.lock() {
                let _ = p.send_event(UserEvent::Tray(c));
            }
        };
        let (icon, size) = tray_icon(self.ui_scale);
        match chonkgene_platform::tray::create(icon, size, send.clone()) {
            Ok(t) => self.tray = Some(t),
            Err(e) => eprintln!("no tray icon: {e}"),
        }
        match hotkey::register(move || send(TrayCommand::Toggle)) {
            Ok(h) => self.hotkey = Some(h),
            Err(e) => eprintln!("Ctrl+Alt+P is not available: {e}"),
        }
        self.sync_desktop(el);
        if self.open_settings_on_start {
            self.open_settings(None);
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Ipc(msg) => self.handle(el, msg),
            UserEvent::Tray(command) => self.command(el, command),
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(m) = &mut self.menu
            && m.window.id() == id
        {
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    let hover = menu::row_at(&m.drawn, position.x as f32, position.y as f32);
                    if hover != m.hover {
                        m.hover = hover;
                        self.redraw_menu();
                    }
                }
                WindowEvent::CursorLeft { .. } => {
                    if m.hover.take().is_some() {
                        self.redraw_menu();
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some(row) = m.hover {
                        let (pet, action) = (m.pet, m.actions[row]);
                        self.act(el, pet, action);
                    }
                }
                _ => {}
            }
            return;
        }
        let Some(i) = self.pets.iter().position(|p| p.window.id() == id) else {
            return;
        };
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.pets[i].cursor = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let now = Instant::now();
                let world = self.world(now);
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) => {
                        self.menu = None;
                        let p = &mut self.pets[i];
                        p.press = Some(Press {
                            grab: p.cursor,
                            moved: false,
                            last: now,
                        });
                        p.shake = ShakeDetector::default();
                        // It squishes in under the finger at once.
                        p.anim.press();
                        p.next = now;
                    }
                    (MouseButton::Left, ElementState::Released) => {
                        let id = self.pets[i].save.id;
                        let clicked = self.pets[i].press.as_ref().is_some_and(|p| !p.moved);
                        let cmds = if clicked && self.director.involves(id) {
                            // 勸架: a click on a squabbling pet makes both make up.
                            let cast = actors(&self.pets);
                            self.director.interrupt(id, world.now, &cast)
                        } else {
                            Vec::new()
                        };
                        let App {
                            pets,
                            shared,
                            lines,
                            bubble_face,
                            sounds,
                            settings,
                            area,
                            terrain,
                            ui_scale,
                            ..
                        } = self;
                        let Some(shared) = shared.as_ref() else {
                            return;
                        };
                        if !cmds.is_empty() {
                            pets[i].press = None;
                            let mut ctx = Ctx {
                                el,
                                shared,
                                lines,
                                face: bubble_face,
                                sounds,
                                sound_on: settings.sound,
                                volume: f32::from(settings.volume) / 100.0,
                                area: *area,
                                terrain,
                                ui_scale: *ui_scale,
                                world,
                            };
                            direct(pets, &mut ctx, cmds);
                            return;
                        }
                        let p = &mut pets[i];
                        let Some(press) = p.press.take() else {
                            return;
                        };
                        if press.moved {
                            p.body.release();
                        } else {
                            let zone = p
                                .renderer
                                .hit(p.cursor.0, p.cursor.1)
                                .and_then(Zone::from_name)
                                .unwrap_or(Zone::Belly);
                            p.anim.poke(zone);
                            let mut rng = p.rng.clone();
                            let out = p.mind.poke(&world, zone, &mut rng);
                            p.rng = rng;
                            let mut ctx = Ctx {
                                el,
                                shared,
                                lines,
                                face: bubble_face,
                                sounds,
                                sound_on: settings.sound,
                                volume: f32::from(settings.volume) / 100.0,
                                area: *area,
                                terrain,
                                ui_scale: *ui_scale,
                                world,
                            };
                            p.apply(&mut ctx, out);
                        }
                        p.next = now;
                        self.dirty = true;
                    }
                    (MouseButton::Right, ElementState::Pressed) => self.open_menu(el, i, &world),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(shared) = &self.shared {
                    let p = &mut self.pets[i];
                    if let Err(e) = p.presenter.present(shared, p.renderer.pixmap()) {
                        eprintln!("present failed: {e}");
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        if self.shared.is_none() {
            return;
        }

        if now >= self.poll_at {
            self.poll_at = now + POLL_EVERY;
            if let Some(shared) = &self.shared {
                self.user_idle = shared.idle_secs().unwrap_or(0.0);
                let ours: Vec<u32> = self.pets.iter().map(|p| p.presenter.id()).collect();
                let fullscreen =
                    self.settings.hide_in_fullscreen && shared.fullscreen_active(&ours);
                if fullscreen != self.fullscreen {
                    self.fullscreen = fullscreen;
                    self.set_visible();
                }
            }
            if let Some(child) = &mut self.child
                && !matches!(child.try_wait(), Ok(None))
            {
                self.child = None;
            }
        }

        // Close the menu on a click anywhere else.
        if let (Some(m), Some(shared)) = (&mut self.menu, &self.shared) {
            let down = shared.button_down();
            if !down {
                m.armed = true;
            } else if m.armed {
                let inside = shared.cursor().is_some_and(|(x, y)| {
                    let (w, h) = (
                        m.drawn.pixmap.width() as i32,
                        m.drawn.pixmap.height() as i32,
                    );
                    x >= m.pos.0 && x < m.pos.0 + w && y >= m.pos.1 && y < m.pos.1 + h
                });
                if !inside {
                    self.menu = None;
                }
            }
        }

        if now >= self.terrain_at {
            self.refresh_terrain();
            // Quickly while a pet stands on a window (to ride along when it
            // moves) or is in the air; otherwise now and then.
            let busy = self.pets.iter().any(|p| {
                p.body.on_window() || matches!(p.body.motion, Motion::Fall | Motion::Held)
            });
            self.terrain_at = now
                + if busy {
                    TERRAIN_BUSY_EVERY
                } else {
                    TERRAIN_EVERY
                };
        }

        let visible = !self.hidden && !self.fullscreen;
        let world = self.world(now);
        let App {
            pets,
            shared,
            lines,
            bubble_face,
            sounds,
            area,
            terrain,
            ui_scale,
            frames,
            drawing,
            presenting,
            director,
            settings,
            rng,
            ..
        } = self;
        let shared = shared.as_ref().expect("checked above");
        let cursor = if pets.iter().any(|p| p.press.is_some()) {
            shared.cursor()
        } else {
            None
        };
        let mut ctx = Ctx {
            el,
            shared,
            lines,
            face: bubble_face,
            sounds,
            sound_on: settings.sound,
            volume: f32::from(settings.volume) / 100.0,
            area: *area,
            terrain,
            ui_scale: *ui_scale,
            world,
        };
        if visible {
            let cast = actors(pets);
            let cmds = director.tick(
                world.now,
                settings.irrational,
                settings.squabble,
                &cast,
                rng,
            );
            direct(pets, &mut ctx, cmds);
            for p in pets.iter_mut() {
                p.in_skit = director.involves(p.save.id);
            }
        }
        for (i, p) in pets.iter_mut().enumerate() {
            if !visible || p.next > now {
                continue;
            }
            let dt = (now - p.last).as_secs_f32().min(0.1);
            p.last = now;
            p.step(&mut ctx, i, dt, now, cursor);
            let started = Instant::now();
            *presenting += p.draw(ctx.shared, world.now);
            *drawing += started.elapsed();
            *frames += 1;
            // A pet blending between poses on the GPU needs frames to
            // show it, even when its animation is otherwise at rest.
            let mut fps = p.anim.fps(p.moving());
            if p.renderer.blending() {
                fps = fps.max(30);
            }
            p.next = now + Duration::from_secs_f64(1.0 / f64::from(fps));
        }

        if self.dirty && now >= self.save_at {
            self.save_at = now + SAVE_EVERY;
            self.save_now();
        }
        if self.stats && now >= self.stats_at {
            self.report(now);
        }

        let due = [
            Some(self.poll_at),
            pending_save(self.dirty, self.save_at),
            if visible {
                self.pets.iter().map(|p| p.next).min()
            } else {
                None
            },
            self.menu.is_some().then(|| now + Duration::from_millis(33)),
            self.stats.then_some(self.stats_at),
            // Always ahead of now: it is moved on whenever it is reached.
            visible.then_some(self.terrain_at),
        ];
        el.set_control_flow(ControlFlow::WaitUntil(wake_at(due)));
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        self.save_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The beta-1 bug: five seconds in, with nothing to save, the old save
    /// time kept the loop waking at once, burning a whole core.
    #[test]
    fn a_past_save_time_with_nothing_unsaved_does_not_wake_the_loop() {
        let now = Instant::now();
        let poll_at = now + Duration::from_millis(500);
        let stale = now - Duration::from_secs(3);
        assert_eq!(
            wake_at([Some(poll_at), pending_save(false, stale)]),
            poll_at
        );
        // Unsaved changes do make it due: save straight away.
        assert_eq!(wake_at([Some(poll_at), pending_save(true, stale)]), stale);
    }

    #[test]
    fn the_loop_wakes_for_the_earliest_pending_job() {
        let now = Instant::now();
        let (a, b) = (
            now + Duration::from_millis(30),
            now + Duration::from_millis(80),
        );
        assert_eq!(wake_at([Some(b), None, Some(a)]), a);
        assert!(wake_at([None, None]) > now);
    }
}
