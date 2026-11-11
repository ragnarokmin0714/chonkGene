//! The pet process: one winit event loop owning every pet, bubble and menu
//! window (PLAN §3.2).

use std::collections::HashMap;
use std::io::Write as _;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chonkgene_core::Zone;
use chonkgene_core::anim::{Act, Animator, Expression, HOLD_SECS};
use chonkgene_core::mind::{Line, Mind, Out, World, XP_PER_LEVEL};
use chonkgene_core::motion::{Area, Body, Heading, Motion};
use chonkgene_core::play::{Command as Ask, Scene, Thing};
use chonkgene_core::shake::ShakeDetector;
use chonkgene_core::skit::{Actor, Awe, Cmd, Director, Spell};
use chonkgene_core::sound::{Class, Sound, TAKES};
use chonkgene_core::species::{Grumpiness, Play, Species};
use chonkgene_core::terrain::{Terrain, Win};
use chonkgene_platform::tray::{Tray, TrayCommand};
use chonkgene_platform::{Presenter, Shared, autostart, hotkey};
use chonkgene_render::cursor::{self, Hand};
use chonkgene_render::menu::{self, Icon, Item};
use chonkgene_render::text::Face;
use chonkgene_render::toy::{self as grass, Stalk};
use chonkgene_render::{Renderer, bubble};
use chonkgene_rig::{Model, builtin};
use cpu_time::ProcessTime;
use tiny_skia::Pixmap;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Cursor, CursorIcon, CustomCursor, Window, WindowId};

use crate::brand;
use crate::diag::diag;
use crate::ipc::{self, Claim, Msg};
use crate::lines::Lines;
use crate::store::{self, DataDir, PetSave, Pets, Settings};
use chonkgene_render::props::{self, Fx};

mod play;

const STATS_EVERY: Duration = Duration::from_secs(5);
const SAVE_EVERY: Duration = Duration::from_secs(5);
const POLL_EVERY: Duration = Duration::from_secs(1);
/// How often to read the windows pets stand on: calm, and while a pet is on
/// a window or in the air.
const TERRAIN_EVERY: Duration = Duration::from_millis(400);
const TERRAIN_BUSY_EVERY: Duration = Duration::from_millis(100);
/// A press pulled up further than this, or moved this far any way (pixels
/// at 100%), is a lift; rubbing about inside that is a scratch.
const LIFT_UP: f32 = 12.0;
const LIFT_ANY: f32 = 36.0;
const HAPPY_SECS: f64 = 3.0;
/// A giggle the app or the director starts lasts this long.
const GIGGLE_SECS: f64 = 3.0;
/// The log notes the program held up for longer than this, and a pet's
/// picture unchanged for longer than [`FROZEN`] while it should move.
const STALL: Duration = Duration::from_millis(300);
const FROZEN: Duration = Duration::from_millis(1500);
/// How often the log sums up each pet's frames and the GPU time.
const PERF_EVERY: Duration = Duration::from_secs(10);
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
    // The pet process first, then its log: a second copy started while one
    // runs used to start the log afresh under the running one, which then
    // wrote on at its old place (Roger's log began with 31 KB of NULs).
    let listener = match ipc::claim().map_err(|e| format!("cannot start: {e}"))? {
        Claim::AlreadyRunning => {
            println!("肉肉賴桌 is already running; asked it to bring the pets back.");
            return Ok(());
        }
        Claim::Ours(listener) => listener,
    };
    crate::diag::open(&data.path);
    diag!(
        "chonkGene {} on {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
    match chonkgene_render::shared_gpu() {
        Some(gpu) => diag!(
            "GPU: {}, budget {:.0}%, frames timed {}",
            gpu.name(),
            100.0 * gpu.budget(),
            gpu.times_frames()
        ),
        None => diag!("no GPU in use: drawing from sprites"),
    }
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
    /// The species on all fours, drawn as a skinned mesh on a real GPU.
    quad: Option<Arc<Model>>,
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
    /// Where it hangs from on the pet's picture (the top of the head),
    /// eased toward it: the head bobs with every step, and a bubble that
    /// followed each bob would shake.
    anchor: (f32, f32),
}

/// How quickly a bubble follows the head about, seconds (time constant).
const BUBBLE_EASE: f32 = 0.3;

/// A bubble's anchor `dt` seconds on, easing from `at` toward the head.
fn ease_bubble(at: (f32, f32), head: (f32, f32), dt: f32) -> (f32, f32) {
    let k = 1.0 - (-dt / BUBBLE_EASE).exp();
    (at.0 + (head.0 - at.0) * k, at.1 + (head.1 - at.1) * k)
}

/// The foxtail grass (狗尾草) on the cursor (PLAN §8, Roger's sixth
/// pass): a window that follows the cursor, lets every click through and
/// swings; the pets go for its seed head.
struct Toy {
    window: Window,
    presenter: Presenter,
    pixmap: Pixmap,
    stalk: Stalk,
    placed: (i32, i32),
    cursor: (i32, i32),
    /// When the cursor last moved: left still long enough, the stalk is
    /// put away by itself.
    moved: Instant,
    last: Instant,
}

/// Left still this long, the grass stalk is put away.
const TOY_IDLE: Duration = Duration::from_secs(120);
/// A pet goes for the stalk within this far either side, pixels.
const TOY_RANGE: f32 = 900.0;

/// Where the grass stalk's window goes for the cursor at `cursor`: its
/// grip under the cursor's tip.
fn toy_pos(cursor: (i32, i32), scale: f32) -> (i32, i32) {
    (
        cursor.0 - (grass::GRIP.0 * scale).round() as i32,
        cursor.1 - (grass::GRIP.1 * scale).round() as i32,
    )
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
    /// The window was just shown again: present the picture even if it has
    /// not changed, since a hidden window may have dropped it.
    repaint: bool,
    last: Instant,
    switch_at: f64,
    happy_until: f64,
    asleep_since: f64,
    hello_at: Option<f64>,
    act: Option<(Grumpiness, f64)>,
    bubble: Option<Bubble>,
    /// Walking is up to the skit director while this is set.
    in_skit: bool,
    /// In a play-fight (a skit, not a squabble).
    play_fight: bool,
    /// Going for the grass stalk; when it next tries something, and when a
    /// crouch turns into a pounce.
    playing: bool,
    play_at: f64,
    pounce_at: Option<f64>,
    /// Scratched: when a cat may purr (or a dog pant) again.
    purr_at: f64,
    /// When its picture last changed, and whether the log has noted it
    /// standing still since (see [`FROZEN`]).
    shown_at: Instant,
    frozen: bool,
    /// The longest wait between two of its frames since its picture last
    /// changed, for the log.
    draw_gap: Duration,
    /// Held up by the fox's spell.
    float: Option<Float>,
    /// The fox's face glowing with a spell: since when, until when.
    glow: Option<(f64, f64)>,
    /// Frames asked for since the log's last summary.
    draws: u32,
    /// A giggle not of the mind's making (the director's, a tickle that
    /// shook a toy loose) ends then.
    giggle_until: Option<f64>,
    /// Toys, commands, scenes, effects (app/play.rs).
    fun: play::Fun,
}

/// What the fox's spells page offers (PLAN §8, Roger's thirteenth pass).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Magic {
    Spell(Spell),
    /// Its fox-fire scene.
    Foxfire,
}

/// Which page of a pet's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Main,
    Commands,
    Spells,
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
    /// Say goodbye to this pet for good (asks once more first).
    Bye,
    /// Take out the grass stalk, or put it away.
    Toy,
    /// The page of things to ask it (叫牠…), and back from it.
    Commands,
    Back,
    /// The fox's page of spells (施法…), and one of them.
    Spells,
    Magic(Magic),
    Command(Ask),
    /// Throw a ball or a frisbee; put down catnip (or take it away).
    Throw(Thing),
    Catnip,
    /// 躲貓貓: it goes and hides.
    Hide,
    /// 跳舞: a spotlight comes down on it and it dances the 肉肉舞.
    Dance,
    /// 放聚光燈: a spotlight put down beside it (or taken away).
    Stage,
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
    /// The 「說再見」 row was clicked once and now asks to be sure.
    sure: bool,
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

/// The pets' sounds (PLAN §1.5), several at once, each class rate-limited
/// on its own so a purr or the bell does not use up the calls.
struct Sounds {
    audio: Option<chonkgene_platform::audio::Audio>,
    /// Each sound's takes, made the first time they are wanted.
    cache: HashMap<(Species, Sound, u32), Vec<i16>>,
    /// The take each pet's sound played last.
    takes: HashMap<(u64, Sound), u32>,
    /// When each pet last made each class of sound, over the last minute.
    recent: HashMap<(u64, Class), Vec<f64>>,
}

/// At most this many sounds of a class per pet per minute.
fn per_minute(class: Class) -> usize {
    match class {
        Class::Call => 6,
        Class::Knock => 10,
        Class::Soft => 8,
    }
}

/// One pet's sounds of one class come at least this far apart.
const SOUND_GAP: f64 = 0.15;

impl Sounds {
    fn new() -> Self {
        Self {
            audio: chonkgene_platform::audio::Audio::new(),
            cache: HashMap::new(),
            takes: HashMap::new(),
            recent: HashMap::new(),
        }
    }

    /// Play `sound` in `species`' voice, `pitch` times higher (see
    /// [`voice_pitch`]), unless this pet has made enough of its kind lately.
    #[allow(clippy::too_many_arguments)]
    fn play(
        &mut self,
        pet: u64,
        species: Species,
        sound: Sound,
        now: f64,
        volume: f32,
        pitch: f32,
    ) {
        let Some(audio) = &self.audio else {
            return;
        };
        let class = sound.class();
        let recent = self.recent.entry((pet, class)).or_default();
        recent.retain(|t| now - t < 60.0);
        if recent.len() >= per_minute(class) || recent.last().is_some_and(|t| now - t < SOUND_GAP) {
            return;
        }
        recent.push(now);
        // The takes in turn, so the same sound twice is never the same.
        let take = self.takes.entry((pet, sound)).or_insert(0);
        *take = (*take + 1) % TAKES;
        let take = *take;
        let samples = self
            .cache
            .entry((species, sound, take))
            .or_insert_with(|| chonkgene_core::sound::synth(species, sound, take));
        audio.play(samples, chonkgene_core::sound::RATE, volume, pitch);
    }
}

/// How high a pet's voice is: a bigger or chubbier pet sounds lower.
fn voice_pitch(size: f32, chonk: f32) -> f32 {
    (size.max(0.2).powf(-0.3) * (1.0 - 0.12 * (chonk - 0.33))).clamp(0.8, 1.25)
}

/// Which sound goes with a line: a patted cat purrs.
fn line_sound(line: Line, species: Species) -> Sound {
    match line {
        Line::Pat if matches!(species, Species::Munchkin | Species::Fold) => Sound::Purr,
        Line::Hello | Line::WelcomeBack | Line::MadeUp | Line::Forgive | Line::LevelUp => {
            Sound::Greet
        }
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
    toy: Option<Toy>,
    /// Things on the desktop to play with (app/play.rs).
    play: play::Playground,
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
    /// The skit last written to the log (see [`Director::describe`]).
    skit_seen: Option<String>,
    /// For trying the 肉肉舞 out (`CHONKGENE_STAGE`): a spotlight put down
    /// beside the first pet at this time.
    stage_at: Option<f64>,
    /// `CHONKGENE_STAGE=dance`: 跳舞 asked of the first pet instead;
    /// `pond`: the duckling's pond scene started.
    stage_test: String,
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
    /// When the event loop was told to wake next (for the log: woken late
    /// means the whole program was held up).
    wake: Option<Instant>,
    /// When the log next sums up how the frames went.
    perf_at: Instant,
}

impl App {
    fn new(data: DataDir, stats: bool, proxy: EventLoopProxy<UserEvent>) -> Result<Self, String> {
        let mut kinds = HashMap::new();
        for species in Species::ALL {
            let model =
                builtin::load(species.id()).map_err(|e| format!("{}: {e}", species.id()))?;
            let quad = chonkgene_render::skin::mesh_model(species.id())
                .ok()
                .map(|(m, _)| Arc::new(m));
            kinds.insert(
                species,
                Kind {
                    model: Arc::new(model),
                    quad,
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
        // Saves from before the limit moved pets (more out than it allows).
        let fitted = pets.out() > settings.on_desktop as usize && pets.fit_to(settings.on_desktop);
        for note in notes {
            eprintln!("{note}");
        }
        let now = Instant::now();
        let mut app = Self {
            data,
            settings,
            store: pets,
            dirty: fitted,
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
            toy: None,
            play: play::Playground::default(),
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
            skit_seen: None,
            stage_at: std::env::var_os("CHONKGENE_STAGE").map(|_| 3.0),
            stage_test: std::env::var("CHONKGENE_STAGE").unwrap_or_default(),
            sounds: Sounds::new(),
            tray: None,
            hotkey: None,
            stats,
            stats_at: now + STATS_EVERY,
            stats_cpu: ProcessTime::now(),
            frames: 0,
            drawing: Duration::ZERO,
            presenting: Duration::ZERO,
            wake: None,
            perf_at: now + PERF_EVERY,
        };
        // Trying the skits out: the next one soon, of the kind named.
        if let Ok(kind) = std::env::var("CHONKGENE_SKIT") {
            let known = app.director.rush(&kind);
            diag!(
                "CHONKGENE_SKIT={kind}: the next skit soon ({})",
                if known { "that kind" } else { "any kind" }
            );
        }
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
        let terrain = self
            .play
            .with_perch(Terrain::new(floors, &windows, headroom));
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
        // Already open (perhaps minimised or behind other windows): ask it
        // to come to the front, on this pet if one was asked for.
        if let Some(child) = &mut self.child
            && matches!(child.try_wait(), Ok(None))
        {
            // We just got the click, so we may hand the foreground on.
            chonkgene_platform::allow_foreground(child.id());
            let line = match pet {
                Some(id) => format!("show {id}\n"),
                None => "show\n".into(),
            };
            let told = child.stdin.as_mut().is_some_and(|i| {
                i.write_all(line.as_bytes())
                    .and_then(|()| i.flush())
                    .is_ok()
            });
            if told {
                return;
            }
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut cmd = Command::new(exe);
        cmd.arg("--settings").arg("--data").arg(&self.data.path);
        if let Some(id) = pet {
            cmd.arg("--pet").arg(id.to_string());
        }
        cmd.stdin(std::process::Stdio::piped());
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
                        pet.presenter.restyle();
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
        // The app-menu entry, if asked for once, follows a moved folder too.
        if let Err(e) = chonkgene_platform::menu_entry::repair(&exe, crate::brand::png_256()) {
            eprintln!("app-menu entry: {e}");
        }
    }

    fn command(&mut self, el: &ActiveEventLoop, command: TrayCommand) {
        match command {
            TrayCommand::Toggle => {
                self.hidden = !self.hidden;
                self.set_visible();
            }
            TrayCommand::Toy => self.toggle_toy(el),
            TrayCommand::Ball => self.throw(el, Thing::Ball),
            TrayCommand::Frisbee => self.throw(el, Thing::Frisbee),
            TrayCommand::Catnip => self.catnip(el),
            TrayCommand::Settings => self.open_settings(None),
            TrayCommand::Quit => {
                self.save_now();
                el.exit();
            }
        }
    }

    /// Throw a ball or a frisbee from where the cursor is (PLAN §8,
    /// Roger's fourth pass, item 2).
    fn throw(&mut self, el: &ActiveEventLoop, thing: Thing) {
        let Some(shared) = &self.shared else {
            return;
        };
        if self.hidden || self.fullscreen {
            return;
        }
        let at = shared.cursor().unwrap_or((
            (self.area.x + self.area.w / 2.0) as i32,
            (self.area.y + self.area.h / 2.0) as i32,
        ));
        let now = self.secs(Instant::now());
        self.play.throw(
            el,
            shared,
            &mut self.pets,
            thing,
            (at.0 as f32, at.1 as f32),
            self.area,
            self.ui_scale,
            now,
        );
    }

    /// Put down a sprig of catnip under the cursor, or take it away (PLAN
    /// §8 "Catnip").
    /// Start `scene` for `pet` now, if it is free (跳舞: a spotlight comes
    /// down on it and it dances the 肉肉舞).
    fn start_scene(
        &mut self,
        el: &ActiveEventLoop,
        pet: u64,
        scene: Scene,
        world: World,
        now: Instant,
    ) {
        let App {
            pets,
            play,
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
        let (Some(shared), Some(p)) = (shared.as_ref(), pets.iter_mut().find(|p| p.save.id == pet))
        else {
            return;
        };
        if p.fun.busy(world.now) || p.mind.asleep() || p.in_skit {
            return;
        }
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
        play::start_scene(play, p, &mut ctx, scene);
        p.next = now;
    }

    fn catnip(&mut self, el: &ActiveEventLoop) {
        let Some(shared) = &self.shared else {
            return;
        };
        if self.hidden || self.fullscreen {
            return;
        }
        let at = shared.cursor().unwrap_or((
            (self.area.x + self.area.w / 2.0) as i32,
            (self.area.y + self.area.h / 2.0) as i32,
        ));
        // Dropped from just under the cursor onto whatever is below.
        let now = self.secs(Instant::now());
        self.play.catnip(
            el,
            shared,
            &mut self.pets,
            (at.0 as f32, at.1 as f32 + 20.0),
            self.ui_scale,
            now,
        );
    }

    /// Take out the grass stalk (a window on the cursor that lets clicks
    /// through), or put it away.
    fn toggle_toy(&mut self, el: &ActiveEventLoop) {
        if self.toy.take().is_some() {
            for p in &mut self.pets {
                p.anim.watch(None);
            }
            return;
        }
        let Some(shared) = &self.shared else {
            return;
        };
        let s = self.ui_scale;
        let size = (
            (grass::SIZE.0 * s).ceil() as u32,
            (grass::SIZE.1 * s).ceil() as u32,
        );
        let cursor = shared.cursor().unwrap_or((0, 0));
        let pos = toy_pos(cursor, s);
        let Ok(window) = el.create_window(chonkgene_platform::pet_window(size, pos, "肉肉賴桌"))
        else {
            return;
        };
        let Ok(mut presenter) = Presenter::new(shared, &window) else {
            return;
        };
        if let Err(e) = presenter.set_ghost(shared) {
            eprintln!("grass stalk: {e}");
        }
        // Clicks pass through to the pets and the desktop under it.
        let _ = window.set_cursor_hittest(false);
        let Some(mut pixmap) = Pixmap::new(size.0, size.1) else {
            return;
        };
        let stalk = Stalk::default();
        stalk.draw(&mut pixmap, s);
        if let Err(e) = presenter.present(shared, &pixmap) {
            eprintln!("present failed: {e}");
        }
        let now = Instant::now();
        self.toy = Some(Toy {
            window,
            presenter,
            pixmap,
            stalk,
            placed: pos,
            cursor,
            moved: now,
            last: now,
        });
    }

    /// Move the grass stalk with the cursor and swing it; where its seed
    /// head is now (screen pixels), if it is out. Left still a long while,
    /// it is put away.
    fn step_toy(&mut self, now: Instant) -> Option<(f32, f32)> {
        let (toy, shared) = (self.toy.as_mut()?, self.shared.as_ref()?);
        let cursor = shared.cursor().unwrap_or(toy.cursor);
        let dt = (now - toy.last).as_secs_f32();
        toy.last = now;
        let moved = cursor != toy.cursor;
        if moved {
            toy.cursor = cursor;
            toy.moved = now;
        } else if now - toy.moved > TOY_IDLE {
            self.toy = None;
            for p in &mut self.pets {
                p.anim.watch(None);
            }
            return None;
        }
        let s = self.ui_scale;
        let swinging = toy.stalk.step((cursor.0 as f32, cursor.1 as f32), dt, s);
        let pos = toy_pos(cursor, s);
        if pos != toy.placed {
            toy.window
                .set_outer_position(PhysicalPosition::new(pos.0, pos.1));
            toy.placed = pos;
        }
        if swinging || moved {
            toy.stalk.draw(&mut toy.pixmap, s);
            if let Err(e) = toy.presenter.present(shared, &toy.pixmap) {
                eprintln!("present failed: {e}");
            }
        }
        let head = toy.stalk.head(s);
        Some((cursor.0 as f32 + head.0, cursor.1 as f32 + head.1))
    }

    fn set_visible(&mut self) {
        let visible = !self.hidden && !self.fullscreen;
        for p in &mut self.pets {
            p.window.set_visible(visible);
            p.presenter.restyle();
            if !visible {
                p.bubble = None;
            }
            p.repaint = true;
            p.next = Instant::now();
        }
        if !visible {
            self.menu = None;
        }
        if let Some(toy) = &mut self.toy {
            toy.window.set_visible(visible);
            toy.presenter.restyle();
        }
        if let Some(shared) = &self.shared {
            let now = self.secs(Instant::now());
            self.play.set_visible(shared, now, visible);
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
                // 「同時在桌面上的寵物」 sends pets to rest or calls them out
                // (the settings window does the same to its list).
                if s.on_desktop != self.settings.on_desktop {
                    self.collect_affinity();
                    self.dirty |= self.store.fit_to(s.on_desktop);
                }
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
                // Called out by hand with the desktop full: the limit makes
                // room (the settings window raises its own the same way).
                let out = self.store.out() as u32;
                if out > self.settings.on_desktop {
                    self.settings.on_desktop = out;
                    if let Err(e) = store::save(&self.data.settings(), &self.settings) {
                        eprintln!("{e}");
                    }
                }
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
                if let Err(e) = chonkgene_platform::menu_entry::remove() {
                    eprintln!("app-menu entry: {e}");
                }
            }
            Msg::Quit => el.exit(),
        }
        if self.dirty {
            self.save_now();
        }
    }

    /// The pet's right-click menu at the cursor (or with its top-left at
    /// `at`: turning to another page and back keeps it in place), on
    /// `page`: the first, the things to ask it (叫牠…) or the fox's spells
    /// (施法…).
    fn open_menu(
        &mut self,
        el: &ActiveEventLoop,
        i: usize,
        world: &World,
        page: Page,
        at: Option<(i32, i32)>,
    ) {
        let p = &self.pets[i];
        let asleep = p.mind.asleep();
        let snacks = p.mind.snacks_left(world.day);
        let item = |label: &str, hint: Option<String>, icon, separated| Item {
            label: label.into(),
            hint,
            icon,
            separated,
        };
        if page == Page::Spells {
            let spells = [
                ("定身術", Icon::Bubble, Magic::Spell(Spell::Freeze)),
                ("狐火", Icon::Flame, Magic::Foxfire),
                ("祝福術", Icon::Heart, Magic::Spell(Spell::Bless)),
                ("招喚術", Icon::Paw, Magic::Spell(Spell::Call)),
            ];
            let mut items: Vec<Item> = spells
                .iter()
                .map(|&(label, icon, _)| item(label, None, icon, false))
                .collect();
            let mut actions: Vec<Action> = spells.iter().map(|&(.., m)| Action::Magic(m)).collect();
            items.push(item("返回", None, Icon::Back, true));
            actions.push(Action::Back);
            return self.show_menu(el, i, items, actions, at);
        }
        if page == Page::Commands {
            let mut items: Vec<Item> = ["坐下", "趴下", "過來", "轉圈", "握手"]
                .iter()
                .map(|label| item(label, None, Icon::Paw, false))
                .collect();
            let mut actions: Vec<Action> = Ask::ALL.iter().map(|&c| Action::Command(c)).collect();
            items.push(item("跳舞", None, Icon::Note, false));
            actions.push(Action::Dance);
            items.push(item("返回", None, Icon::Back, true));
            actions.push(Action::Back);
            return self.show_menu(el, i, items, actions, at);
        }
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
            if self.toy.is_some() {
                "把狗尾草收起來"
            } else {
                "拿狗尾草逗牠們"
            },
            None,
            Icon::Grass,
            false,
        ));
        actions.push(Action::Toy);
        // Things to play with (PLAN §8, Roger's fourth pass, item 2, and
        // "Catnip"); asking it to do something only while it is awake.
        if !asleep {
            items.push(item("叫牠…", None, Icon::Paw, false));
            actions.push(Action::Commands);
            // The fox casts spells when you ask (Roger, 2026-11-06).
            if p.species == Species::Fox {
                items.push(item("施法…", None, Icon::Wand, false));
                actions.push(Action::Spells);
            }
            items.push(item("玩躲貓貓", None, Icon::Peek, false));
            actions.push(Action::Hide);
        }
        items.push(item("丟球給牠們", None, Icon::Ball, false));
        actions.push(Action::Throw(Thing::Ball));
        items.push(item("丟飛盤給牠們", None, Icon::Frisbee, false));
        actions.push(Action::Throw(Thing::Frisbee));
        items.push(item(
            if self.play.has(Thing::Catnip) {
                "把貓薄荷收起來"
            } else {
                "放一把貓薄荷"
            },
            None,
            Icon::Leaf,
            false,
        ));
        actions.push(Action::Catnip);
        items.push(item(
            if self.play.stage.is_some() {
                "把聚光燈收起來"
            } else {
                "放聚光燈"
            },
            None,
            Icon::Spotlight,
            false,
        ));
        actions.push(Action::Stage);
        items.push(item("說再見…", None, Icon::Bye, false));
        actions.push(Action::Bye);
        items.push(item(
            "全部收起來",
            Some("Ctrl+Alt+P".into()),
            Icon::Hide,
            true,
        ));
        items.push(item("設定…", None, Icon::Sliders, false));
        items.push(item("結束肉肉賴桌", None, Icon::Exit, false));
        actions.extend([Action::HideAll, Action::Settings, Action::Quit]);
        self.show_menu(el, i, items, actions, at);
    }

    /// Draw and open the menu of pet `i` with these rows.
    fn show_menu(
        &mut self,
        el: &ActiveEventLoop,
        i: usize,
        items: Vec<Item>,
        actions: Vec<Action>,
        at: Option<(i32, i32)>,
    ) {
        let Some(shared) = &self.shared else {
            return;
        };
        let p = &self.pets[i];
        let asleep = p.mind.asleep();
        let kind = &self.kinds[&p.species];
        let coat = kind.model.coat(&p.save.coat).unwrap_or(0);
        let avatar = kind.model.coats[coat].swatch();
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
        let pos = at.unwrap_or((cursor.0 + 4, cursor.1 - h / 3));
        let pos = (
            pos.0.clamp(ax, (ax + aw - w).max(ax)),
            pos.1.clamp(ay, (ay + ah - h).max(ay)),
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
            sure: false,
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
        let at = self.menu.take().map(|m| m.pos);
        let now = Instant::now();
        let world = self.world(now);
        match action {
            Action::Commands | Action::Spells | Action::Back => {
                let page = match action {
                    Action::Commands => Page::Commands,
                    Action::Spells => Page::Spells,
                    _ => Page::Main,
                };
                if let Some(i) = self.pets.iter().position(|p| p.save.id == pet) {
                    self.open_menu(el, i, &world, page, at);
                }
                return;
            }
            Action::Stage => {
                if self.hidden || self.fullscreen {
                    return;
                }
                let Some(shared) = &self.shared else {
                    return;
                };
                self.play
                    .toggle_stage(el, shared, &mut self.pets, pet, self.area, world.now);
                return;
            }
            Action::Dance => {
                self.start_scene(el, pet, Scene::Dance, world, now);
                return;
            }
            Action::Magic(Magic::Foxfire) => {
                let App {
                    pets,
                    play,
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
                let (Some(shared), Some(p)) =
                    (shared.as_ref(), pets.iter_mut().find(|p| p.save.id == pet))
                else {
                    return;
                };
                if p.fun.busy(world.now) || p.mind.asleep() || p.in_skit {
                    return;
                }
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
                play::start_scene(play, p, &mut ctx, Scene::Foxfire);
                p.next = now;
                return;
            }
            Action::Magic(Magic::Spell(spell)) => {
                let cast = actors(&self.pets, world.now);
                let cmds = self.director.cast(pet, spell, world.now, &cast);
                if cmds.is_empty() {
                    return;
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
                    ..
                } = self;
                let Some(shared) = shared.as_ref() else {
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
                direct(pets, &mut ctx, cmds);
                return;
            }
            Action::Throw(thing) => return self.throw(el, thing),
            Action::Catnip => return self.catnip(el),
            Action::Hide => {
                let cursor = self.shared.as_ref().and_then(|s| s.cursor());
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
                let (Some(shared), Some(p)) =
                    (shared.as_ref(), pets.iter_mut().find(|p| p.save.id == pet))
                else {
                    return;
                };
                if p.fun.busy(world.now) || p.mind.asleep() {
                    return;
                }
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
                p.start_hiding(&mut ctx, cursor);
                p.next = now;
                return;
            }
            Action::Command(command) => {
                if let Some(p) = self.pets.iter_mut().find(|p| p.save.id == pet) {
                    let moody = p.mind.grumpy()
                        || matches!(p.anim.expression, Expression::FedUp | Expression::Sulk);
                    let roll = p.rng.f32();
                    p.fun
                        .ask(command, world.now, p.mind.affinity.level, moody, roll);
                    p.next = now;
                }
                return;
            }
            Action::HideAll => {
                self.hidden = true;
                self.set_visible();
                return;
            }
            Action::Settings => return self.open_settings(None),
            Action::Edit => return self.open_settings(Some(pet)),
            Action::Toy => return self.toggle_toy(el),
            Action::Bye => {
                self.store.pets.retain(|p| p.id != pet);
                self.dirty = true;
                self.sync_desktop(el);
                self.save_now();
                return;
            }
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

    /// For the log: each pet's frames over the last [`PERF_EVERY`] (new
    /// pictures a second against the frames asked for, frames pushed past
    /// the budget, the times it held one back, GPU time a frame) and the
    /// share of GPU time all of them took against the budget: enough to
    /// tell a budget too tight from a GPU too slow.
    fn log_frames(&mut self, now: Instant) {
        let secs = (now - (self.perf_at - PERF_EVERY)).as_secs_f64().max(1.0);
        self.perf_at = now + PERF_EVERY;
        let mut gpu = 0.0;
        for p in &mut self.pets {
            let draws = std::mem::take(&mut p.draws);
            let Some(f) = p.renderer.take_stats() else {
                continue;
            };
            gpu += f.gpu_secs;
            let ms = 1000.0 * f.gpu_secs / f64::from(f.gpu_frames.max(1));
            diag!(
                "frames: {} {:.1}/s shown of {:.1}/s asked; sent {} ({} past the budget, {} walking), held back {}; GPU {ms:.1} ms a frame",
                p.save.name,
                f64::from(f.shown) / secs,
                f64::from(draws) / secs,
                f.sent,
                f.pushed,
                f.forced,
                f.held_back,
            );
        }
        if let Some(g) = chonkgene_render::shared_gpu() {
            diag!(
                "frames: GPU time {:.1}% of the last {secs:.0} s (budget {:.0}%)",
                100.0 * gpu / secs,
                100.0 * g.budget()
            );
        }
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
fn actors(pets: &[Pet], now: f64) -> Vec<Actor> {
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
                && p.anim.expression != Expression::Sulk
                // Busy with a toy, a scene, catnip or something asked.
                && !p.fun.busy(now),
            asleep: p.mind.asleep() && p.press.is_none() && p.body.motion == Motion::Idle,
            energy: p.mind.personality.energy,
        })
        .collect()
}

/// When a pet given `cmd` at `now` draws next, if it was to at `next`:
/// at once, but never later than it would have. The director sends walks
/// and turns again every go round (招喚術, running at each other), and
/// setting it to now each time left it just after the loop's own now: the
/// pet was never stepped, stood still for the whole 招喚術 with a frame
/// left waiting (Roger's log: pictures unchanged 4.5 s) while the loop
/// spun. A walk or a turn is taken up at its next frame anyway.
fn woken(next: Instant, cmd: &Cmd, now: Instant) -> Instant {
    match cmd {
        Cmd::Walk { .. } | Cmd::Face { .. } => next,
        _ => next.min(now),
    }
}

fn direct(pets: &mut [Pet], ctx: &mut Ctx, cmds: Vec<Cmd>) {
    for cmd in cmds {
        let id = cmd.id();
        let Some(p) = pets.iter_mut().find(|p| p.save.id == id) else {
            continue;
        };
        p.next = woken(p.next, &cmd, Instant::now());
        if matches!(
            cmd,
            Cmd::Tussle { .. }
                | Cmd::Cast { .. }
                | Cmd::Freeze { .. }
                | Cmd::Scare { .. }
                | Cmd::Awe { .. }
                | Cmd::Bless { .. }
        ) {
            diag!("{}: {cmd:?}", p.save.name);
        }
        match cmd {
            Cmd::Walk { x, pace, .. } => {
                // Stopping short of the spot, never turned back and forth
                // at an edge it cannot pass (see `Body::head_for`).
                let near = 0.12 * p.body.size.0;
                p.body.head_for(x, pace, near, ctx.terrain);
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
                if expression == Expression::Giggle {
                    p.giggle(ctx.world.now);
                }
                if expression == Expression::Happy {
                    p.happy_until = ctx.world.now + HAPPY_SECS;
                }
            }
            Cmd::Act { act, .. } => {
                if p.body.motion == Motion::Walk {
                    p.body.motion = Motion::Idle;
                }
                // Stopped just now: the animator would still count it as
                // walking until its next step and drop the gesture (the
                // play-fights' moves came late or not at all).
                p.anim.walking = false;
                p.anim.gesture(act);
                // The fox's bell rings as its tails swing.
                if act == Act::Swish && ctx.sound_on {
                    ctx.sounds.play(
                        p.save.id,
                        p.species,
                        Sound::Bell,
                        ctx.world.now,
                        ctx.volume,
                        1.0,
                    );
                }
            }
            Cmd::Hop { .. } => p.anim.pounce(),
            Cmd::Tussle { with, secs, .. } => {
                // One cloud for both: drawn over this one, the other gone
                // into it; both come out panting.
                let now = ctx.world.now;
                let x = p.center_x();
                let species = p.species;
                p.stop_here();
                p.fun.effect(Fx::Brawl, now);
                p.anim.puff(secs + 3.0);
                if ctx.sound_on {
                    ctx.sounds
                        .play(id, species, Sound::Land, now, ctx.volume, 1.0);
                }
                if let Some(q) = pets.iter_mut().find(|q| q.save.id == with) {
                    q.stop_here();
                    q.fun.vanish_until = now + f64::from(secs);
                    q.anim.puff(secs + 3.0);
                    q.next = Instant::now();
                    // Out of the cloud a little to the side it went in.
                    let side = if q.center_x() < x { -1.0 } else { 1.0 };
                    q.body.facing = -side;
                }
            }
            Cmd::Cast { .. } => {
                // Up on its hind legs, tails swung, the bell ringing, a
                // ring of fox-fire round its feet.
                if p.body.motion == Motion::Walk {
                    p.body.motion = Motion::Idle;
                }
                p.anim.walking = false;
                p.anim.gesture(Act::Swish);
                p.fun.effect(Fx::Spell, ctx.world.now);
                // Its forehead and eyes glow, seal lines spreading over its
                // face, until the spell is done.
                p.glow = Some((ctx.world.now, ctx.world.now + SPELL_GLOW_SECS));
                if ctx.sound_on {
                    ctx.sounds.play(
                        p.save.id,
                        p.species,
                        Sound::Bell,
                        ctx.world.now,
                        ctx.volume,
                        1.0,
                    );
                }
            }
            Cmd::Freeze { secs, .. } => {
                if p.press.is_none() && !matches!(p.body.motion, Motion::Held | Motion::Fly) {
                    let now = ctx.world.now;
                    p.stop_here();
                    p.float = Some(Float {
                        since: now,
                        until: now + f64::from(secs),
                        base: p.body.pos,
                    });
                    // Asleep, it floats up still asleep, in a dream bubble.
                    let fx = if p.mind.asleep() {
                        Fx::Dream
                    } else {
                        Fx::Bubble
                    };
                    p.fun.effect(fx, now);
                    // Big enough for all of it (its pose stays put while
                    // it floats).
                    let pm = p.renderer.pixmap();
                    let canvas = (pm.width() as f32, pm.height() as f32);
                    p.fun.bubble_fit =
                        props::opaque_bounds(pm).map(|b| props::bubble_fit(b, canvas, p.fun.scale));
                }
            }
            Cmd::Scare { from, .. } => {
                // Jumps back from the fox, still facing it, with a start,
                // shock lines and a drop of sweat.
                let now = ctx.world.now;
                let toward = if from >= p.center_x() { 1.0 } else { -1.0 };
                if p.press.is_none() && matches!(p.body.motion, Motion::Idle | Motion::Walk) {
                    let (w, s) = (p.body.size.0, p.fun.scale);
                    p.body
                        .spring(p.body.feet().0 - toward * SCARE_JUMP * w, 26.0 * s);
                    p.body.facing = toward;
                }
                let (w, h) = p.renderer.size();
                let head = p.renderer.anchor().1;
                let side = if toward > 0.0 { 0.95 } else { 0.05 };
                p.anim.startle((side * w as f32, head.min(0.4 * h as f32)));
                p.anim.expression = Expression::Neutral;
                p.fun.effect(Fx::Fright, now);
                if ctx.sound_on {
                    ctx.sounds
                        .play(p.save.id, p.species, Sound::Land, now, ctx.volume, 1.0);
                }
            }
            Cmd::Awe { from, how, .. } => {
                // The fox's fire going up near by: turned to look, then
                // frightened, thrilled or stalking it, as it is made.
                let now = ctx.world.now;
                let toward = if from >= p.center_x() { 1.0 } else { -1.0 };
                let free =
                    p.press.is_none() && matches!(p.body.motion, Motion::Idle | Motion::Walk);
                if free {
                    p.body.motion = Motion::Idle;
                    p.anim.walking = false;
                    p.body.facing = toward;
                }
                // Bigger than at first (Roger's fifteenth pass, item 2: not
                // seen at all): a yelp, a big start and a tremble while it
                // keeps low; three hops, hearts and a happy call; a long
                // wiggle before the pounce.
                let voice = |p: &mut Pet, ctx: &mut Ctx, sound, pitch| {
                    if ctx.sound_on {
                        let base = p.pitch();
                        ctx.sounds
                            .play(p.save.id, p.species, sound, now, ctx.volume, base * pitch);
                    }
                };
                match how {
                    Awe::Shrink => {
                        // Startled where it stands, no hop back: in the
                        // air it would not be free, and the director calls
                        // the spell off if the one it picked on is not.
                        let (w, h) = p.renderer.size();
                        let head = p.renderer.anchor().1;
                        let side = if toward > 0.0 { 0.95 } else { 0.05 };
                        p.anim.flinch((side * w as f32, head.min(0.4 * h as f32)));
                        p.anim.expression = Expression::Neutral;
                        p.fun.effect(Fx::Fright, now);
                        p.fun.cower(now);
                        // A yelp, or (with its own call) a word about it.
                        if p.rng.f32() < 0.5 {
                            p.say(ctx, Line::SkitScared);
                        } else {
                            voice(p, ctx, Sound::Voice, 1.25);
                        }
                    }
                    Awe::Bounce => {
                        p.anim.leap_for_joy();
                        p.anim.expression = Expression::Happy;
                        p.happy_until = now + HAPPY_SECS + 1.5;
                        p.fun.effect(Fx::Sparkles, now);
                        p.fun.effect(Fx::Hearts, now);
                        voice(p, ctx, Sound::Greet, 1.0);
                    }
                    Awe::Crouch => {
                        p.anim.gesture(Act::Wiggle);
                        p.fun.stalk(now);
                    }
                }
            }
            Cmd::Bless { .. } => {
                let now = ctx.world.now;
                p.fun.effect(Fx::Hearts, now);
                p.fun.effect(Fx::Sparkles, now);
                if p.anim.expression != Expression::Happy {
                    p.anim.cheer();
                }
                p.anim.expression = Expression::Happy;
                p.happy_until = now + HAPPY_SECS;
            }
        }
    }
}

thread_local! {
    /// The hands over pets and toys (`chonkgene_render::cursor`), made
    /// once the event loop runs.
    static HANDS: std::cell::RefCell<Vec<(Hand, CustomCursor)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Make the hands at the screen's scale (Roger: cuter than the system's).
fn make_hands(el: &ActiveEventLoop, scale: f32) {
    let px = (cursor::SIZE * scale).round() as u32;
    let made: Vec<_> = Hand::ALL
        .into_iter()
        .filter_map(|h| {
            let (rgba, (x, y)) = cursor::draw(h, px);
            let side = px.clamp(16, 256) as u16;
            match CustomCursor::from_rgba(rgba, side, side, x, y) {
                Ok(source) => Some((h, el.create_custom_cursor(source))),
                Err(e) => {
                    eprintln!("the hand cursor: {e}");
                    None
                }
            }
        })
        .collect();
    HANDS.with(|hands| *hands.borrow_mut() = made);
}

/// The hand `h`, or the system's nearest one if ours could not be made.
pub(crate) fn hand(h: Hand) -> Cursor {
    HANDS
        .with(|hands| {
            hands
                .borrow()
                .iter()
                .find(|(k, _)| *k == h)
                .map(|(_, c)| Cursor::Custom(c.clone()))
        })
        .unwrap_or(Cursor::Icon(match h {
            Hand::Open | Hand::Poke => CursorIcon::Pointer,
            Hand::Grab => CursorIcon::Grabbing,
        }))
}

/// The fox's face glows this long from the start of a cast (through the
/// others floating, for 定身術), seconds.
const SPELL_GLOW_SECS: f64 = 5.2;
/// Frightened by the spell, a pet jumps back this share of its width.
const SCARE_JUMP: f32 = 0.12;

/// Held up in the air by the fox's spell (定身術): since when, until when,
/// and where the window stood when it began.
#[derive(Clone, Copy, Debug)]
struct Float {
    since: f64,
    until: f64,
    base: (f32, f32),
}

/// How high the spell lifts a pet (pixels at 100%; Roger: higher), and
/// how far it bobs.
const FLOAT_RISE: f32 = 72.0;
const FLOAT_BOB: f32 = 4.0;

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
    let coat = kind.model.coat(&save.coat).unwrap_or(0);
    // The renderer first: a pet on all fours needs a wider window.
    let mut renderer = Renderer::new(
        Arc::clone(&kind.model),
        kind.quad.clone(),
        scale,
        save.chonk,
        coat,
    )
    .ok_or("cannot allocate a pixmap")?;
    // Its fidgets: as lively as it is, on a clock of its own.
    renderer.set_fidgets(
        save.personality().energy,
        save.id as u32 ^ (save.id >> 32) as u32,
    );
    let size = renderer.size();
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
    // A hand over the pet (the window takes the mouse only where it is
    // drawn): something to pat.
    window.set_cursor(hand(Hand::Open));
    let seed = save.id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (t * 1000.0) as u64;
    Ok(Pet {
        mind: Mind::new(species, save.personality(), save.affinity()),
        anim: Animator::new(seed, save.chonk, species.waddle())
            .with_habits(species.habits())
            .with_joys(species.joys())
            .with_slow_blinks(species.slow_blinks())
            .with_pops(!renderer.is_gpu()),
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
        repaint: true,
        last: now,
        switch_at: t + 2.0 + slot as f64,
        happy_until: 0.0,
        asleep_since: 0.0,
        hello_at: Some(t + 1.2 + slot as f64 * 1.4),
        act: None,
        bubble: None,
        in_skit: false,
        play_fight: false,
        playing: false,
        play_at: 0.0,
        pounce_at: None,
        purr_at: 0.0,
        shown_at: now,
        frozen: false,
        draw_gap: Duration::ZERO,
        float: None,
        glow: None,
        draws: 0,
        giggle_until: None,
        fun: play::Fun::new(scale),
        save,
    })
}

impl Pet {
    fn center_x(&self) -> f32 {
        self.placed.0 as f32 + self.renderer.pixmap().width() as f32 / 2.0
    }

    /// Go for the grass stalk's seed head at `tip` (screen pixels), as the
    /// species likes (PLAN §8, Roger's sixth pass): watch it, walk up to
    /// it, then crouch and pounce (cats), pounce and swat (dogs, the fox),
    /// or peck and flap at it (birds); the owl only watches. False if it
    /// is out of reach (too far, or below the floor the pet is on).
    fn play(&mut self, tip: (f32, f32), t: f64, terrain: &Terrain) -> bool {
        let (w, h) = self.body.size;
        let (fx, fy) = self.body.feet();
        let dx = tip.0 - fx;
        if dx.abs() > TOY_RANGE || tip.1 > fy + 0.3 * h || tip.1 < fy - 5.0 * h {
            return false;
        }
        self.anim
            .watch(Some((tip.0 - self.body.pos.0, tip.1 - self.body.pos.1)));
        // Not off on a walk of its own while it plays.
        self.switch_at = t + 1.0;
        let keen = self.species.toy_keen();
        if keen <= 0.0 {
            if self.body.motion == Motion::Walk {
                self.body.motion = Motion::Idle;
            }
            return true;
        }
        // Up to it (or as close as it gets, watching it from there), never
        // turned back and forth at an edge it cannot pass.
        match self
            .body
            .head_for(tip.0, 1.0 + 0.8 * keen, 0.2 * w, terrain)
        {
            Heading::Going => {
                self.pounce_at = None;
                return true;
            }
            Heading::Blocked => return true,
            Heading::There => {}
        }
        // Close by, it turns to whichever side the tip is on, once stopped.
        if self.body.speed == 0.0 && dx.abs() > 0.1 * w {
            self.body.facing = if dx < 0.0 { -1.0 } else { 1.0 };
        }
        if let Some(at) = self.pounce_at
            && t >= at
        {
            self.pounce_at = None;
            self.anim.pounce();
        }
        if t < self.play_at {
            return true;
        }
        self.play_at = t + 0.9 + f64::from(self.rng.f32()) * 1.4 / f64::from(keen);
        // Low: down near the floor it stands on.
        let low = tip.1 > fy - 0.45 * h;
        match self.species.plays_like() {
            // Down on the front, bottom up and wiggling, then the pounce.
            Play::Stalk => {
                self.anim.gesture(Act::Stretch);
                self.pounce_at = Some(t + 0.9);
            }
            Play::Pounce => {
                if self.rng.bool() {
                    self.anim.pounce();
                } else {
                    self.anim.gesture(Act::Wave);
                }
            }
            Play::Peck => {
                if low {
                    self.anim.gesture(Act::Peck);
                } else {
                    self.anim.gesture(Act::Flap);
                    self.pounce_at = Some(t + 0.4);
                }
            }
        }
        true
    }

    /// Giggle for a moment (not the mind's tickle laugh, which it ends
    /// itself).
    fn giggle(&mut self, now: f64) {
        self.anim.expression = Expression::Giggle;
        self.giggle_until = Some(now + GIGGLE_SECS);
    }

    /// Stop walking where it is (the director's commands).
    fn stop_here(&mut self) {
        if self.body.motion == Motion::Walk {
            self.body.motion = Motion::Idle;
        }
        self.body.pace = 1.0;
    }

    /// Held up by the spell: rise, hang there bobbing, frozen mid-pose;
    /// at the end (or popped early by a click) the bubble bursts and it
    /// drops onto its feet. True while it floats.
    fn float(&mut self, t: f64, dt: f32) -> bool {
        let Some(f) = self.float else {
            return false;
        };
        if t >= f.until {
            self.float = None;
            self.fun
                .fx
                .retain(|(fx, _)| !matches!(fx, Fx::Bubble | Fx::Dream));
            self.fun.effect(Fx::Sparkles, t);
            self.body.release();
            return false;
        }
        let s = self.fun.scale;
        let age = (t - f.since) as f32;
        let rise = FLOAT_RISE * s * chonkgene_rig::math::smoothstep(0.0, 0.6, age);
        let bob = FLOAT_BOB * s * (std::f32::consts::TAU * 0.5 * age).sin();
        self.body.hold_at((f.base.0, f.base.1 - rise - bob), dt);
        true
    }

    /// For the log: a pet that should be moving (pleased, lively, in a
    /// gesture) whose picture has not changed for [`FROZEN`], with what it
    /// is up to.
    fn note_frozen(&mut self, now: Instant) {
        if self.renderer.changed() {
            self.shown_at = now;
            self.frozen = false;
            self.draw_gap = Duration::ZERO;
            return;
        }
        let params = self.anim.params();
        let should_move = params.lively || self.anim.doing().is_some() || self.anim.walking;
        if self.frozen || !should_move || now - self.shown_at < FROZEN {
            return;
        }
        self.frozen = true;
        diag!(
            "{}: picture unchanged for {} ms ({:?}, {:?}, {:?}, motion {:?}, frame waiting {}, press {}, spell-held {}, drawn every {} ms)",
            self.save.name,
            (now - self.shown_at).as_millis(),
            self.anim.expression,
            self.anim.doing(),
            params.posture,
            self.body.motion,
            self.renderer.waiting(),
            self.press.is_some(),
            self.float.is_some(),
            self.draw_gap.as_millis(),
        );
    }

    /// How high this pet's voice is (see [`voice_pitch`]).
    fn pitch(&self) -> f32 {
        voice_pitch(self.save.size, self.save.chonk)
    }

    /// The sound of a fidget that just began: the Shiba panting, the fox's
    /// bell, a bird's wings.
    fn fidget_sound(&mut self, ctx: &mut Ctx) {
        use chonkgene_render::fidget::Kind;
        // A good scratch: a cat purrs, a dog pants, every few seconds.
        let t = ctx.world.now;
        if self.anim.params().touch.scratch > 0.5 && t >= self.purr_at {
            self.purr_at = t + 2.6;
            let sound = match self.species {
                Species::Munchkin | Species::Fold => Some(Sound::Purr),
                Species::Shiba | Species::Fox => Some(Sound::Pant),
                _ => None,
            };
            if let Some(sound) = sound
                && ctx.sound_on
            {
                ctx.sounds.play(
                    self.save.id,
                    self.species,
                    sound,
                    t,
                    ctx.volume,
                    self.pitch(),
                );
            }
        }
        let Some(kind) = self.renderer.take_fidget_start() else {
            return;
        };
        let sound = match kind {
            Kind::Pant => Sound::Pant,
            Kind::Jingle => Sound::Bell,
            Kind::Flutter | Kind::Ruffle => Sound::Flutter,
            _ => return,
        };
        if ctx.sound_on {
            let pitch = if sound == Sound::Bell {
                1.0
            } else {
                self.pitch()
            };
            ctx.sounds.play(
                self.save.id,
                self.species,
                sound,
                ctx.world.now,
                ctx.volume,
                pitch,
            );
        }
    }

    fn say(&mut self, ctx: &mut Ctx, line: Line) {
        // Hiding, a bubble would give it away.
        if self.in_hiding_place() {
            return;
        }
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
        let anchor = self
            .bubble
            .as_ref()
            .map_or_else(|| self.renderer.bubble_at(), |b| b.anchor);
        let pos = self.bubble_pos(ctx, size, anchor);
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
            anchor,
        };
        if let Err(e) = b.presenter.present(ctx.shared, &b.pixmap) {
            eprintln!("present failed: {e}");
        }
        // Over its own pet (a click may have raised the pet since).
        if let Err(e) = b.presenter.raise(ctx.shared) {
            eprintln!("raise failed: {e}");
        }
        self.bubble = Some(b);
        if ctx.sound_on {
            ctx.sounds.play(
                self.save.id,
                self.species,
                line_sound(line, self.species),
                ctx.world.now,
                ctx.volume,
                self.pitch(),
            );
        }
    }

    /// Where a bubble of `size` goes, hanging from `anchor` on the pet's
    /// picture.
    fn bubble_pos(&self, ctx: &Ctx, size: (u32, u32), anchor: (f32, f32)) -> (i32, i32) {
        let (ax, ay) = (anchor.0.round() as i32, anchor.1.round() as i32);
        let (x, y) = (
            self.placed.0 + ax - size.0 as i32 / 2,
            self.placed.1 + ay - size.1 as i32,
        );
        let a = ctx.area;
        (
            x.clamp(a.x as i32, (a.x + a.w) as i32 - size.0 as i32),
            y.max(a.y as i32),
        )
    }

    fn apply(&mut self, ctx: &mut Ctx, out: Out) {
        // A snack is eaten on the spot: stop, bite, chew (no hop).
        let eating = out.line == Some(Line::Eat);
        if eating {
            if self.body.motion == Motion::Walk {
                self.body.motion = Motion::Idle;
            }
            self.anim.eat();
            // Not off for a walk before the snack is gone.
            let done = ctx.world.now + f64::from(chonkgene_core::anim::EAT_SECS) + 1.0;
            self.switch_at = self.switch_at.max(done);
        }
        if let Some(e) = out.expression {
            // Cheered up (a pat): a happy double hop, and hearts.
            if e == Expression::Happy && self.anim.expression != Expression::Happy && !eating {
                self.anim.cheer();
                self.fun
                    .effect(chonkgene_render::props::Fx::Hearts, ctx.world.now);
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
        if out.level_up {
            self.fun
                .effect(chonkgene_render::props::Fx::Sparkles, ctx.world.now);
        }
        let line = out.line.or(out.level_up.then_some(Line::LevelUp));
        if let Some(line) = line {
            self.say(ctx, line);
        }
    }

    fn step(
        &mut self,
        ctx: &mut Ctx,
        i: usize,
        dt: f32,
        now: Instant,
        cursor: Option<(i32, i32)>,
        toy: Option<(f32, f32)>,
    ) {
        let t = ctx.world.now;
        self.fidget_sound(ctx);
        if let Some(at) = self.hello_at
            && t >= at
        {
            self.hello_at = None;
            if !self.mind.asleep() {
                self.say(ctx, Line::Hello);
                self.anim.gesture(Act::Wave);
            }
        }

        let floating = self.float(t, dt);
        if floating {
            // Held up by the spell: nothing else moves it.
        } else if self.hiding() {
            // 躲貓貓: sitting just past the edge, peeking now and then.
            self.hide_step(ctx);
        } else if let Some(press) = self.press.as_mut() {
            if let Some((cx, cy)) = cursor {
                let target = (cx as f32 - press.grab.0, cy as f32 - press.grab.1);
                let (dx, dy) = (target.0 - self.body.pos.0, target.1 - self.body.pos.1);
                let s = self.fun.scale;
                if !press.moved && (-dy > LIFT_UP * s || dx.hypot(dy) > LIFT_ANY * s) {
                    press.moved = true;
                    self.anim.lift();
                    self.window.set_cursor(hand(Hand::Grab));
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
                if landing.speed > 600.0 {
                    self.fun.effect(chonkgene_render::props::Fx::Dust, t);
                }
                if ctx.sound_on && landing.speed > 600.0 {
                    let pitch = self.pitch();
                    ctx.sounds.play(
                        self.save.id,
                        self.species,
                        Sound::Land,
                        ctx.world.now,
                        ctx.volume,
                        pitch,
                    );
                    // The fox's bell rings as it lands.
                    if self.species == Species::Fox {
                        ctx.sounds.play(
                            self.save.id,
                            self.species,
                            Sound::Bell,
                            ctx.world.now,
                            ctx.volume,
                            1.0,
                        );
                    }
                }
            }
            let busy = self.fun.busy(t);
            let free = !self.mind.asleep() && self.act.is_none() && !self.in_skit && !busy;
            if !self.in_skit && self.act.is_none() && !busy && self.body.pace != 1.0 {
                self.body.pace = 1.0;
            }
            if self.mind.asleep() && self.body.motion == Motion::Walk && !busy {
                self.body.motion = Motion::Idle;
            }
            let playing = free
                && matches!(self.body.motion, Motion::Idle | Motion::Walk)
                && toy.is_some_and(|tip| self.play(tip, t, ctx.terrain));
            if self.playing && !playing {
                self.anim.watch(None);
                if self.body.motion == Motion::Walk {
                    self.body.motion = Motion::Idle;
                }
            }
            self.playing = playing;
            if free
                && !playing
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
                } else if self.body.motion == Motion::Idle {
                    // Off for a walk, either way. (Not on stopping: it
                    // slows down going the way it went.)
                    self.body.motion = Motion::Walk;
                    if self.rng.bool() {
                        self.body.facing = -self.body.facing;
                    }
                } else {
                    self.body.motion = Motion::Idle;
                }
                // Walks of 3–7 s; rests a little longer (4–11 s), long
                // enough to sit down and look about.
                self.switch_at = t + if self.body.motion == Motion::Walk {
                    3.0 + f64::from(self.rng.f32()) * 4.0
                } else {
                    4.0 + f64::from(self.rng.f32()) * 7.0
                };
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
        // Cowering from the fox's fire, it trembles: a quick lean to and
        // fro, about five times a second.
        if self.fun.cowering(t) && self.act.is_none() {
            self.anim.sway = 2.5 * (t as f32 * 31.0).sin();
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
            || (self.anim.expression == Expression::Giggle
                && self.giggle_until.is_some_and(|u| t >= u))
        {
            self.anim.expression = Expression::Neutral;
            self.giggle_until = None;
        }
        self.anim.walking = self.body.motion == Motion::Walk;
        self.anim.pace = self.body.pace;
        self.anim.facing = self.body.facing;
        // Frozen by the spell, its animation stands still; asleep, it
        // sleeps on (breathing, dreaming).
        if !floating || self.mind.asleep() {
            self.anim.step(dt);
        }

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
                let anchor = ease_bubble(b.anchor, self.renderer.bubble_at(), dt);
                let pos = self.bubble_pos(ctx, size, anchor);
                if let Some(b) = &mut self.bubble {
                    b.anchor = anchor;
                    if pos != b.placed {
                        b.window
                            .set_outer_position(PhysicalPosition::new(pos.0, pos.1));
                        b.placed = pos;
                    }
                }
            }
        }
    }

    /// Draw and show a frame; returns how long showing it took.
    fn draw(&mut self, shared: &Shared, t: f64) -> Duration {
        // Sprites rendered in the background land here; the slowest pet
        // still draws at 5 fps, so a new one shows within 0.2 s.
        self.renderer.poll();
        let mut params = self.anim.params();
        params.travelling = self.travelling();
        params.glow = self.glow_at(t);
        if self.mind.asleep() {
            self.renderer
                .render_asleep(&params, (t - self.asleep_since) as f32);
        } else {
            self.renderer.render(&params, self.anim.expression);
        }
        // Effects, a toy in its mouth, water, fire: drawn over a copy of
        // its picture, every frame while they are on.
        let had = self.fun.had_overlay;
        let changed = self.renderer.changed();
        let overlay = self.compose(t);
        let pixmap = match &self.fun.canvas {
            Some(pm) if overlay => pm,
            _ if changed || self.repaint || had => self.renderer.pixmap(),
            _ => return Duration::ZERO,
        };
        let started = Instant::now();
        if let Err(e) = self.presenter.present(shared, pixmap) {
            eprintln!("present failed: {e}");
        }
        self.repaint = false;
        self.fun.had_overlay = overlay;
        started.elapsed()
    }

    fn moving(&self) -> bool {
        self.press.as_ref().is_some_and(|p| p.moved)
            || self.travelling()
            || self.act.is_some_and(|(k, _)| {
                matches!(
                    k,
                    Grumpiness::Zoomies | Grumpiness::Dizzy | Grumpiness::Spin
                )
            })
    }

    /// Bring its window above every other (things on the floor came out
    /// over it), its bubble above that.
    fn raise(&self, shared: &Shared) {
        if let Err(e) = self.presenter.raise(shared) {
            eprintln!("raise failed: {e}");
        }
        if let Some(b) = &self.bubble
            && let Err(e) = b.presenter.raise(shared)
        {
            eprintln!("raise failed: {e}");
        }
    }

    /// How brightly its face glows with a spell at `t`, 0 … 1: coming up
    /// over half a second, flickering, dying away at the end.
    fn glow_at(&self, t: f64) -> f32 {
        let Some((since, until)) = self.glow else {
            return 0.0;
        };
        let rise = ((t - since) / 0.5).clamp(0.0, 1.0);
        let fall = ((until - t) / 0.6).clamp(0.0, 1.0);
        let flicker = 0.92 + 0.08 * (std::f64::consts::TAU * 2.3 * t).sin();
        (rise.min(fall) * flicker) as f32
    }

    /// Its window moves on its own, not walking: flying, falling (a hop, a
    /// leap, thrown), held up by the spell.
    fn travelling(&self) -> bool {
        self.float.is_some() || matches!(self.body.motion, Motion::Fly | Motion::Fall)
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
        make_hands(el, self.ui_scale);
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
        match chonkgene_platform::tray::create(brand::tray(self.ui_scale), send.clone()) {
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
                        // Goodbye is for good: the first click only asks.
                        if action == Action::Bye && !m.sure {
                            m.sure = true;
                            m.model.items[row].label = "真的要說再見嗎？".into();
                            m.model.items[row].hint = Some("再按一次".into());
                            self.redraw_menu();
                            return;
                        }
                        self.act(el, pet, action);
                    }
                }
                _ => {}
            }
            return;
        }
        // A toy on the desktop: picked up, dragged and thrown, or knocked
        // along with a click.
        if self.play.owns(id) {
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    self.play
                        .cursor_on(id, (position.x as f32, position.y as f32));
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    if state == ElementState::Pressed {
                        self.menu = None;
                        let at = self.shared.as_ref().and_then(|s| s.cursor());
                        self.play.press(id, &mut self.pets, at);
                    } else {
                        let now = self.world(Instant::now()).now;
                        self.play.release(id, &mut self.pets, now);
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
                let p = &mut self.pets[i];
                p.cursor = (position.x as f32, position.y as f32);
                // Rubbing about under a held finger (not yet a lift): shown
                // at once, not at the next frame due.
                if p.press.as_ref().is_some_and(|p| !p.moved) {
                    p.anim.knead(p.cursor);
                    p.next = p.next.min(Instant::now());
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let now = Instant::now();
                let world = self.world(now);
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) => {
                        self.menu = None;
                        let p = &mut self.pets[i];
                        // 躲貓貓: a click on what shows of it finds it.
                        if p.find() {
                            diag!("{}: found", p.save.name);
                            p.next = now;
                            return;
                        }
                        // Held up by the spell: a click pops its bubble.
                        if let Some(f) = &mut p.float {
                            diag!("{}: bubble popped", p.save.name);
                            f.until = world.now;
                            p.next = now;
                            return;
                        }
                        p.window.set_cursor(hand(Hand::Poke));
                        p.press = Some(Press {
                            grab: p.cursor,
                            moved: false,
                            last: now,
                        });
                        p.shake = ShakeDetector::default();
                        // It squishes in under the finger at once, where
                        // the finger is.
                        let zone = p
                            .renderer
                            .hit(p.cursor.0, p.cursor.1)
                            .and_then(Zone::from_name)
                            .unwrap_or(Zone::Belly);
                        p.anim.press(zone, p.cursor);
                        diag!(
                            "{}: pressed on {zone:?} at ({:.0}, {:.0}): {:?}, {:?}, motion {:?}, skit {}, asleep {}, frame waiting {}",
                            p.save.name,
                            p.cursor.0,
                            p.cursor.1,
                            p.anim.expression,
                            p.anim.doing(),
                            p.body.motion,
                            p.in_skit,
                            p.mind.asleep(),
                            p.renderer.waiting(),
                        );
                        p.next = now;
                    }
                    (MouseButton::Left, ElementState::Released) => {
                        let id = self.pets[i].save.id;
                        let clicked = self.pets[i].press.as_ref().is_some_and(|p| !p.moved);
                        let cmds = if clicked && self.director.involves(id) {
                            // 勸架: a click on a squabbling pet makes both make up
                            // (a click on the fox breaks its spell).
                            let cast = actors(&self.pets, world.now);
                            self.director.interrupt(id, world.now, &cast)
                        } else {
                            Vec::new()
                        };
                        if !cmds.is_empty() {
                            for q in &mut self.pets {
                                if let Some(f) = &mut q.float {
                                    f.until = world.now;
                                }
                            }
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
                            ..
                        } = self;
                        let Some(shared) = shared.as_ref() else {
                            return;
                        };
                        if !cmds.is_empty() {
                            pets[i].press = None;
                            pets[i].window.set_cursor(hand(Hand::Open));
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
                        p.window.set_cursor(hand(Hand::Open));
                        if press.moved {
                            diag!("{}: put down", p.save.name);
                            p.body.release();
                        } else {
                            let zone = p
                                .anim
                                .params()
                                .touch
                                .held
                                .map_or(Zone::Belly, |(zone, ..)| zone);
                            // Held a while on a nice spot, it was a rub or a
                            // scratch (a pat), not a poke; a tail or a bottom
                            // held is still a poke.
                            let rub = p.anim.held_secs() >= HOLD_SECS
                                && matches!(
                                    zone,
                                    Zone::Head | Zone::Chin | Zone::Belly | Zone::Cheek
                                );
                            // A quick poke: a start, hopping back from
                            // the finger (still facing the same way).
                            let quick =
                                p.anim.held_secs() < 0.3 && p.anim.params().touch.scratch < 0.2;
                            if quick
                                && !rub
                                && !p.mind.asleep()
                                && matches!(p.body.motion, Motion::Idle | Motion::Walk)
                            {
                                let (w, s) = (p.body.size.0, p.fun.scale);
                                let away = if p.cursor.0 < 0.5 * w { 1.0 } else { -1.0 };
                                let facing = p.body.facing;
                                p.body.spring(p.body.feet().0 + away * 0.06 * w, 14.0 * s);
                                p.body.facing = facing;
                            }
                            let held = p.anim.held_secs();
                            p.anim.poke(zone);
                            let out = if rub {
                                p.mind.pat(&world)
                            } else {
                                let mut rng = p.rng.clone();
                                let out = p.mind.poke(&world, zone, &mut rng);
                                p.rng = rng;
                                out
                            };
                            diag!(
                                "{}: let go after {held:.2} s, {}: {:?} {:?}",
                                p.save.name,
                                if rub { "a pat" } else { "a poke" },
                                out.line,
                                out.expression,
                            );
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
                    (MouseButton::Right, ElementState::Pressed) => {
                        self.open_menu(el, i, &world, Page::Main, None)
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                // Shown again as it was last shown: with its overlays (the
                // pond's water over its legs, a toy in its mouth, effects)
                // while it has them. Windows asks for this on every restack,
                // so the plain picture here flickered the pond's water off.
                if let Some(shared) = &self.shared {
                    let p = &mut self.pets[i];
                    let shown = match &p.fun.canvas {
                        Some(pm) if p.fun.had_overlay => pm,
                        _ => p.renderer.pixmap(),
                    };
                    if let Err(e) = p.presenter.present(shared, shown) {
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
        if let Some(late) = self.wake.take().and_then(|w| now.checked_duration_since(w))
            && late > STALL
        {
            diag!("held up: woke {} ms late", late.as_millis());
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
        let toy = if visible { self.step_toy(now) } else { None };
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
            skit_seen,
            settings,
            rng,
            play,
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
            let cast = actors(pets, world.now);
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
                p.play_fight = director.playing(p.save.id);
            }
            // Every step of a skit in the log (招喚術 could not be judged
            // from Roger's: who was called, and when it ended).
            let skit = director.describe();
            if skit != *skit_seen {
                // Where everyone is, to see who came when called.
                let at: Vec<String> = pets
                    .iter()
                    .map(|p| format!("{} {:.0}", p.save.id, p.center_x()))
                    .collect();
                diag!(
                    "skit: {} (at {})",
                    skit.as_deref().unwrap_or("over"),
                    at.join(", ")
                );
                *skit_seen = skit;
            }
        }
        play.irrational = settings.irrational;
        play::tick(play, pets, &mut ctx, visible, settings.react_to_input);
        for (i, p) in pets.iter_mut().enumerate() {
            if !visible || p.next > now {
                continue;
            }
            let dt = (now - p.last).as_secs_f32().min(0.1);
            p.draw_gap = p.draw_gap.max(now - p.last);
            p.last = now;
            p.step(&mut ctx, i, dt, now, cursor, toy);
            let started = Instant::now();
            *presenting += p.draw(ctx.shared, world.now);
            *drawing += started.elapsed();
            *frames += 1;
            p.draws += 1;
            p.note_frozen(now);
            // A pet blending between poses on the GPU, or with a frame on
            // its way, needs frames to show it, even when its animation is
            // otherwise at rest; so does one slowing to a stop.
            let gliding = if p.body.moving_on() || p.fun.had_overlay || p.fun.cowering(world.now) {
                30
            } else {
                0
            };
            let fps = p
                .anim
                .fps(p.moving())
                .max(p.renderer.min_fps())
                .max(gliding);
            p.next = now + Duration::from_secs_f64(1.0 / f64::from(fps));
        }

        // A branch came or went: the owl's perch.
        if std::mem::take(&mut self.play.terrain_dirty) {
            self.refresh_terrain();
        }
        if self.dirty && now >= self.save_at {
            self.save_at = now + SAVE_EVERY;
            self.save_now();
        }
        if let Some(at) = self.stage_at
            && self.secs(now) >= at
            && let (Some(shared), Some(first)) = (&self.shared, self.pets.first())
        {
            self.stage_at = None;
            let (pet, t) = (first.save.id, self.secs(now));
            let world = self.world(now);
            if self.stage_test == "dance" {
                self.start_scene(el, pet, Scene::Dance, world, now);
                diag!("CHONKGENE_STAGE=dance: asked to dance");
            } else if self.stage_test == "pond" {
                let duck = self.pets.iter().find(|p| p.species == Species::Duck);
                if let Some(duck) = duck.map(|p| p.save.id) {
                    self.start_scene(el, duck, Scene::Pond, world, now);
                    diag!("CHONKGENE_STAGE=pond: the duckling's pond");
                }
            } else {
                self.play
                    .toggle_stage(el, shared, &mut self.pets, pet, self.area, t);
                diag!("CHONKGENE_STAGE: a spotlight put down");
            }
        }
        if self.stats && now >= self.stats_at {
            self.report(now);
        }
        if visible && now >= self.perf_at {
            self.log_frames(now);
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
            // The grass stalk follows the cursor smoothly.
            (visible && self.toy.is_some()).then(|| now + Duration::from_millis(16)),
            // Things thrown fly smoothly; the rest move on their own.
            (visible && !self.play.things.is_empty()).then(|| now + Duration::from_millis(16)),
            // The input timing is sampled a few times a second.
            (visible && self.settings.react_to_input).then(|| now + Duration::from_millis(200)),
            self.stats.then_some(self.stats_at),
            // Always ahead of now: it is moved on whenever it is reached.
            visible.then_some(self.terrain_at),
        ];
        let busy = now.elapsed();
        if busy > STALL {
            diag!("held up: one turn of the loop took {} ms", busy.as_millis());
        }
        let wake = wake_at(due);
        self.wake = Some(wake);
        el.set_control_flow(ControlFlow::WaitUntil(wake));
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        self.save_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pet the director keeps sending a walk is still stepped at its
    /// own rate (招喚術: the called pets never moved), and a command never
    /// puts its next frame off.
    #[test]
    fn the_directors_commands_never_put_a_pets_frame_off() {
        let now = Instant::now();
        let due = now - Duration::from_millis(5);
        let later = now + Duration::from_millis(30);
        let walk = Cmd::Walk {
            id: 1,
            x: 0.0,
            pace: 1.0,
        };
        // Sent again every go round, a walk leaves a due frame due.
        let mut next = due;
        for k in 0..100 {
            next = woken(next, &walk, now + Duration::from_micros(k));
        }
        assert_eq!(next, due);
        assert_eq!(woken(later, &walk, now), later);
        // Anything else brings it forward, never back.
        let hop = Cmd::Hop { id: 1 };
        assert_eq!(woken(later, &hop, now), now);
        assert_eq!(woken(due, &hop, now), due);
    }

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

    /// Roger saw the bubble shake along with a walking pet: it follows the
    /// head's slow moves (sitting down) but not each step's bob.
    #[test]
    fn a_bubble_rides_out_the_bob_of_each_step() {
        let dt = 1.0 / 30.0;
        // The top of the head bobs 3 px at two steps a second, two beats
        // a step.
        let head = |t: f32| (130.0, 40.0 + 3.0 * (t * std::f32::consts::TAU * 4.0).sin());
        let mut at = head(0.0);
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        for f in 1..120 {
            at = ease_bubble(at, head(f as f32 * dt), dt);
            if f > 60 {
                (low, high) = (low.min(at.1), high.max(at.1));
            }
        }
        assert!(high - low < 1.5, "the bubble shook {:.1} px", high - low);
        // Sitting down lowers the head for good: the bubble follows.
        for _ in 0..60 {
            at = ease_bubble(at, (130.0, 60.0), dt);
        }
        assert!((at.1 - 60.0).abs() < 1.0, "the bubble stayed at {}", at.1);
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
