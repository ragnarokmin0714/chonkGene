//! The pet process: one winit event loop owning every pet, bubble and menu
//! window (PLAN §3.2).

use chonkgene_core::i18n::{tr, trf};
use std::collections::HashMap;
use std::io::Write as _;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chonkgene_core::Zone;
use chonkgene_core::anim::{Act, Animator, Expression, HOLD_SECS, HUG_SECS, Hug, HugHold, HugMood};
use chonkgene_core::mind::{Line, Mind, Out, World, XP_PER_LEVEL};
use chonkgene_core::motion::{Area, Body, Heading, Motion};
use chonkgene_core::play::{Command as Ask, Scene, Thing};
use chonkgene_core::shake::ShakeDetector;
use chonkgene_core::skit::{Actor, Awe, Cmd, Director, Spell};
use chonkgene_core::sound::{Class, Sound, TAKES};
use chonkgene_core::species::{Fondness, Grumpiness, Play, Species};
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
use crate::store::{self, DataDir, FormLock, PetSave, Pets, Room, Settings};
use chonkgene_render::props::{self, Fx};

mod play;

const STATS_EVERY: Duration = Duration::from_secs(5);
const SAVE_EVERY: Duration = Duration::from_secs(5);
/// A pet gone to rest in 寵物窩 says goodbye this long before its window
/// goes (換班, 輪班).
const LEAVE_SECS: f64 = 2.2;
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
/// A hand on a spot it does not like: it pulls away after this long held,
/// seconds (a quick poke there is just a poke).
const DISLIKE_SECS: f32 = 0.35;
/// The belly trap lasts this long, seconds.
const TRAP_SECS: f32 = 1.8;
/// Running, a pet goes this many times its walking pace; a run ends in a
/// belly slide this often (Roger's eighteenth pass: 奔跑, 滑行).
const RUN_PACE: f32 = 2.6;
const SLIDE_CHANCE: f32 = 0.5;

/// How likely a pet setting off is to fly somewhere instead: the birds
/// most, the fox on her magic, the rest (balloons, bamboo-copters) now and
/// then.
fn fly_chance(species: Species) -> f32 {
    if move_test() {
        return 0.5;
    }
    match species {
        Species::Owl => 0.08,
        Species::Chick | Species::Duck => 0.05,
        Species::Fox => 0.05,
        _ => 0.025,
    }
}

/// `CHONKGENE_STAGE=move`: running, sliding and flying much more often,
/// for trying them out (and their frames dumped with `CHONKGENE_DUMP`).
fn move_test() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CHONKGENE_STAGE").is_ok_and(|v| v == "move"))
}

/// `CHONKGENE_STAGE=morph`: the fox changing form from her menu and back,
/// her frames dumped with `CHONKGENE_DUMP`.
fn morph_test() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CHONKGENE_STAGE").is_ok_and(|v| v == "morph"))
}

/// The duckling patted on the bottom waddles off this much quicker.
const DUCK_HURRY: f32 = 1.9;

/// How likely a pet falling from high up is to glide down instead: a bird
/// nearly always, the fox with her tails fanned mostly, the rest (legs out
/// like a flying squirrel) half the time.
fn glide_chance(species: Species) -> f32 {
    match species {
        Species::Owl | Species::Chick | Species::Duck => 0.9,
        Species::Fox => 0.8,
        _ => 0.5,
    }
}

/// What a tap does on each spot (PLAN §8, Roger's twenty-seventh pass,
/// item 2): two taps (or one, as 「動作要點幾下」 says) on its head pat it,
/// on a front paw (a bird's wing) ask for 握手 (a high five), on a hind
/// paw lift it up high, on its shoulders hug it round, on its back rock it
/// like a baby, on its bottom pat it lightly (each its own way, item 6
/// (b)). Belly, cheeks, chin and tail have none: a tap there is a poke.
/// The fox's human form has hers on her head, hands and shoulders only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TapAction {
    Pat,
    Paw,
    Hug(HugHold),
    PatBottom,
}

fn tap_action(human: bool, zone: Zone) -> Option<TapAction> {
    match zone {
        Zone::Head => Some(TapAction::Pat),
        Zone::Paw | Zone::Wing => Some(TapAction::Paw),
        Zone::Shoulder => Some(TapAction::Hug(HugHold::Wrap)),
        Zone::Foot if !human => Some(TapAction::Hug(HugHold::Carry)),
        Zone::Back if !human => Some(TapAction::Hug(HugHold::Cradle)),
        Zone::Bottom if !human => Some(TapAction::PatBottom),
        _ => None,
    }
}
/// Held on a spot it loves: hearts from this long on, seconds, and again
/// this often.
const LOVE_HEARTS_AFTER: f32 = 0.8;
const LOVE_HEARTS_EVERY: f64 = 1.6;
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
        "chonkGene {} ({}) on {}",
        env!("CARGO_PKG_VERSION"),
        env!("CHONKGENE_COMMIT"),
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
    /// The touch has had its answer already, held: pulled away from a spot
    /// it does not like, or the belly trap sprung (PLAN §8, Roger's
    /// sixteenth pass, item 4).
    answered: bool,
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
    /// When she next changes form by herself (隨牠變; the fox where she
    /// can).
    morph_at: f64,
    happy_until: f64,
    asleep_since: f64,
    hello_at: Option<f64>,
    /// Gone to rest in 寵物窩 (換班, 輪班, a lower limit, switched off by
    /// hand): it says goodbye and its window goes at this time.
    leave_at: Option<f64>,
    /// It has said its goodbye (see `leave_at`).
    said_bye: bool,
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
    /// Held on a spot it loves: when the next hearts rise.
    love_at: f64,
    /// A quick tap on a spot with an action, let go then (and the spot,
    /// kept: it may move before the second): a second tap soon after does
    /// that spot's action.
    tapped: Option<(Instant, Zone)>,
    /// That tap's poke, waiting until then in case a second tap comes
    /// (see [`Pet::poke_now`]).
    poke_due: Option<(Instant, Zone)>,
    /// Running (not walking) until then (Roger's eighteenth pass: 奔跑).
    dash_until: f64,
    /// This fall has been looked at for a glide.
    glide_checked: bool,
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

/// What the fox's spells offer (PLAN §8, Roger's thirteenth pass): the
/// 施法 section of 叫牠….
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
    /// Things to ask it (叫牠…), the fox's spells in a section of it.
    Commands,
    /// Whom to go and hug (叫牠… › 去抱抱…).
    HugWho,
    /// How to hold it (抱抱…): 環抱, 舉高高, 搖搖抱.
    HugHow,
    /// Playing (一起玩…): hide-and-seek, its scenes (or why not now), and
    /// the things for them all: the grass stalk, a ball, a frisbee,
    /// catnip, the spotlight.
    Toys,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Pat,
    Feed,
    Edit,
    Sleep,
    Wake,
    HideAll,
    Quit,
    /// Take out the grass stalk, or put it away.
    Toy,
    /// The page of things to ask it (叫牠…), and back from it.
    Commands,
    Back,
    /// One of the fox's spells.
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
    /// 抱抱: you hug it, held as it suits it now (asleep, cradled and
    /// sleeping on).
    Hug,
    /// The page of how to hold it, and one of them.
    HugHow,
    HugAs(HugHold),
    /// The page of whom it could go and hug, and one of them.
    HugWho,
    HugWith(u64),
    /// One of its scenes, and putting one away.
    CallScene(Scene),
    EndScene,
    /// The page of things to play with (一起玩…).
    Toys,
    /// 變身: the fox into her human form, or back.
    Morph,
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
    /// Drawn at this scale: the UI's, or less to fit on the screen.
    scale: f32,
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
    /// 「大家怎麼叫你」 (empty: unset), see [`PetSave::calls_you`].
    everyone_you: &'a str,
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
        Line::Pat | Line::PatLove if matches!(species, Species::Munchkin | Species::Fold) => {
            Sound::Purr
        }
        Line::Hugged if matches!(species, Species::Munchkin | Species::Fold) => Sound::Purr,
        Line::PatLove | Line::Hug | Line::Hugged => Sound::Greet,
        Line::BearHugged => Sound::Laugh,
        Line::Dislike => Sound::Grumpy,
        Line::Trap => Sound::Laugh,
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
    /// `pond`: the duckling's pond scene started; `hug`: the first pet
    /// hugged by you (your hands, dumped with `CHONKGENE_DUMP`);
    /// `hug-asleep`: put to sleep first, hugged 3 s later; `pat`: 摸摸頭
    /// from the first pet's menu (your hand on its head); `bone`: the
    /// Shiba's bone scene (taken up off the floor, chewed, let fall);
    /// `settings-quit`: the settings opened on the first pet, then 結束肉肉
    /// 賴桌 5 s later (the settings window closes too); `hug-other`: the
    /// first pet asked to go and hug the second (叫牠… › 去抱抱…);
    /// `morph`: the fox 變身 from her menu, and back 10 s later.
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
    /// When 隨機輪班 next changes a pet out for one resting (seconds, as
    /// `secs`); none while it is off.
    shift_at: Option<f64>,
    /// Pets to say `called_you` and when (seconds, as `shift_at`): told by
    /// 「大家怎麼叫你」, one after another.
    called: Vec<(f64, u64)>,
    /// Where pets just gone to rest stood (their feet's middle, x): the
    /// ones coming out in their place appear there (換班).
    handover: Vec<f32>,
    /// The fox changing form: her id and where she stood (her feet's
    /// middle, x); she comes back there in her other form.
    morphs: Vec<(u64, f32)>,
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
        let mut settings = store::load::<Settings>(&data.settings())
            .unwrap_or_else(|e| {
                notes.push(e);
                None
            })
            .unwrap_or_default();
        let upgraded = settings.upgrade();
        settings.sanitise();
        chonkgene_core::i18n::set(settings.lang());
        if upgraded && let Err(e) = store::save(&data.settings(), &settings) {
            notes.push(e);
        }
        let mut pets = store::load::<Pets>(&data.pets())
            .unwrap_or_else(|e| {
                notes.push(e);
                None
            })
            .unwrap_or_default();
        pets.sanitise();
        // Saves from before the limit moved pets (more out than it allows),
        // and an upgrade that raised the limit (as changing it in settings
        // would, a pet resting only for the old limit comes out).
        let fitted = (upgraded || pets.out() > settings.on_desktop as usize)
            && pets.fit_to(settings.on_desktop);
        // Names and 「牠怎麼叫你」 nobody changed, in the language shown.
        let localized = pets.localize_defaults();
        for note in notes {
            eprintln!("{note}");
        }
        let now = Instant::now();
        let mut app = Self {
            data,
            settings,
            store: pets,
            dirty: fitted | localized,
            kinds,
            lines: Lines::load()?,
            bubble_face: Face::ui(),
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
            shift_at: None,
            called: Vec::new(),
            handover: Vec::new(),
            morphs: Vec::new(),
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
    /// The tray icon and its menu, in the current language.
    fn make_tray(&mut self) {
        let proxy = std::sync::Arc::new(std::sync::Mutex::new(self.proxy.clone()));
        let send = move |c: TrayCommand| {
            if let Ok(p) = proxy.lock() {
                let _ = p.send_event(UserEvent::Tray(c));
            }
        };
        match chonkgene_platform::tray::create(brand::tray(self.ui_scale), send) {
            Ok(t) => self.tray = Some(t),
            Err(e) => eprintln!("no tray icon: {e}"),
        }
    }

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

    /// Closes the settings window if it is open (Roger, 2026-11-20: quit
    /// from the tray with it open, it stayed on the taskbar). It is asked
    /// to, so what was being typed in it is still kept.
    fn close_settings(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        let told = child
            .stdin
            .as_mut()
            .is_some_and(|i| i.write_all(b"quit\n").and_then(|()| i.flush()).is_ok());
        if !told {
            let _ = child.kill();
        }
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

    /// Spawn and remove pet windows so the desktop matches the saves. A pet
    /// gone to rest in 寵物窩 (換班, 輪班, a lower limit, switched off by
    /// hand) first says goodbye where it stands and goes [`LEAVE_SECS`]
    /// later (not while you hold it); one coming out then appears where it
    /// stood, in a puff (Roger, 2026-11-19). One released, or any while
    /// they are put away, goes at once.
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
        let now = Instant::now();
        let t = self.secs(now);
        let shown = !self.hidden && !self.fullscreen;
        let (store, handover) = (&self.store, &mut self.handover);
        self.pets.retain_mut(|p| {
            if wanted.contains(&p.save.id) {
                // Called back out while saying goodbye: it stays.
                p.leave_at = None;
                return true;
            }
            let resting = store.pets.iter().any(|s| s.id == p.save.id);
            match p.leave_at {
                None if resting && shown => {
                    p.leave_at = Some(t + LEAVE_SECS);
                    p.next = now;
                    true
                }
                Some(at) if shown && (t < at || p.press.is_some()) => true,
                Some(_) => {
                    handover.push(p.body.feet().0);
                    false
                }
                None => false,
            }
        });
        if self
            .menu
            .as_ref()
            .is_some_and(|m| !self.pets.iter().any(|p| p.save.id == m.pet))
        {
            self.menu = None;
        }
        // The ones coming out wait for those saying goodbye to go.
        if self.pets.iter().any(|p| p.leave_at.is_some()) {
            return;
        }
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
                Ok(mut pet) => {
                    if self.hidden || self.fullscreen {
                        pet.window.set_visible(false);
                        pet.presenter.restyle();
                    }
                    if let Some(k) = self.morphs.iter().position(|m| m.0 == *id) {
                        // Changed form: where she stood, in a burst of
                        // fox-fire and sparkles, and hello at once.
                        let (_, x) = self.morphs.remove(k);
                        let (w, _) = pet.body.size;
                        pet.body = Body::on_ground(
                            &self.terrain,
                            x - w / 2.0,
                            pet.body.size,
                            pet.body.foot,
                        );
                        pet.fun.effect(Fx::Spell, t);
                        pet.fun.effect(Fx::Sparkles, t);
                        pet.hello_at = Some(t + 0.5);
                        pet.switch_at = t + 3.0;
                    } else if !self.handover.is_empty() {
                        // In the place of one just gone, in a puff, and
                        // hello at once.
                        let x = self.handover.remove(0);
                        let (w, _) = pet.body.size;
                        pet.body = Body::on_ground(
                            &self.terrain,
                            x - w / 2.0,
                            pet.body.size,
                            pet.body.foot,
                        );
                        pet.fun.effect(Fx::Dust, t);
                        pet.fun.effect(Fx::Sparkles, t);
                        pet.hello_at = Some(t + 0.4);
                        pet.switch_at = t + 3.0;
                    }
                    self.pets.push(pet);
                }
                Err(e) => eprintln!("cannot place a pet: {e}"),
            }
        }
        self.handover.clear();
    }

    /// A pet called out by hand or just adopted with every other one out
    /// 常駐: the limit grows to match (the settings window raises its own
    /// the same way).
    fn limit_to_fit(&mut self) {
        let out = self.store.out() as u32;
        if out > self.settings.on_desktop {
            self.settings.on_desktop = out;
            if let Err(e) = store::save(&self.data.settings(), &self.settings) {
                eprintln!("{e}");
            }
        }
    }

    /// The pets queued to try out what 「大家怎麼叫你」 now makes them call
    /// you say it when their turn comes (not asleep, not while put away).
    fn called_step(&mut self, el: &ActiveEventLoop, now: Instant, t: f64) {
        if self.called.is_empty() {
            return;
        }
        if self.hidden {
            self.called.clear();
            return;
        }
        let due: Vec<u64> = self
            .called
            .iter()
            .filter(|c| c.0 <= t)
            .map(|c| c.1)
            .collect();
        self.called.retain(|c| c.0 > t);
        for id in due {
            let world = self.world(now);
            self.with_pet(el, id, world, now, |_, p, ctx| {
                if !p.mind.asleep() {
                    p.say(ctx, Line::CalledYou);
                }
            });
        }
    }

    /// 隨機輪班 (Roger, 2026-11-19; off by default): about every
    /// 「rotate_minutes」 (a fifth either way, so it does not run like a
    /// clock) one pet out that is free to go changes places with one
    /// resting in 寵物窩, both picked at random. Not while they are put
    /// away, a menu is open or one is still saying goodbye; with nobody
    /// free or resting, it tries again in a minute.
    fn rotate_step(&mut self, el: &ActiveEventLoop, t: f64) {
        let minutes = self.settings.rotate_minutes;
        if minutes == 0 {
            self.shift_at = None;
            return;
        }
        // `CHONKGENE_SHIFT=<seconds>`: a shorter round, for trying it out.
        let round = std::env::var("CHONKGENE_SHIFT")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(60.0 * f64::from(minutes.max(5)));
        let Some(at) = self.shift_at else {
            self.shift_at = Some(t + round * (0.8 + 0.4 * self.rng.f64()));
            return;
        };
        if t < at {
            return;
        }
        let again = t + round.min(60.0);
        if self.hidden
            || self.fullscreen
            || self.menu.is_some()
            || self.pets.iter().any(|p| p.leave_at.is_some())
        {
            self.shift_at = Some(again);
            return;
        }
        if !self.shift_now(el) {
            self.shift_at = Some(again);
            return;
        }
        self.shift_at = Some(t + round * (0.8 + 0.4 * self.rng.f64()));
    }

    /// The fox changes form (PLAN §8, her human form): from her menu, or
    /// by herself now and then (隨牠變). Her window is made again for the
    /// other form, where she stood, in a burst of fox-fire. Asked from the
    /// menu with her kept in one form (維持原型 / 維持人型), she is kept in
    /// the one asked for.
    fn morph(&mut self, el: &ActiveEventLoop, id: u64, by_herself: bool) {
        let Some(i) = self.pets.iter().position(|p| p.save.id == id) else {
            return;
        };
        if !morphs(self.pets[i].species) || self.hidden || self.fullscreen {
            return;
        }
        let Some(save) = self.store.pets.iter_mut().find(|p| p.id == id) else {
            return;
        };
        save.human = !save.human;
        if !by_herself && save.form_lock.keeps().is_some() {
            save.form_lock = if save.human {
                FormLock::Human
            } else {
                FormLock::Beast
            };
        }
        diag!(
            "morph: {} into her {} form{}",
            save.name,
            if save.human { "human" } else { "beast" },
            if by_herself { " by herself" } else { "" }
        );
        self.collect_affinity();
        self.dirty = true;
        let x = self.pets[i].body.feet().0;
        self.pets.remove(i);
        self.morphs.retain(|m| m.0 != id);
        self.morphs.push((id, x));
        self.sync_desktop(el);
    }

    /// Now and then the fox changes form by herself (隨牠變), when she is
    /// awake and up to nothing; not while a menu is open or anyone is
    /// leaving.
    fn morph_step(&mut self, el: &ActiveEventLoop, t: f64) {
        if self.hidden || self.fullscreen || self.menu.is_some() {
            return;
        }
        let due = self.pets.iter().find(|p| {
            t >= p.morph_at
                && morphs(p.species)
                && p.save.form_lock == FormLock::Free
                && !p.mind.asleep()
                && p.free_to_go()
                && p.act.is_none()
        });
        let Some(id) = due.map(|p| p.save.id) else {
            // Not free when due: try again in a while.
            for p in self.pets.iter_mut().filter(|p| t >= p.morph_at) {
                p.morph_at = t + 60.0;
            }
            return;
        };
        if self.pets.iter().any(|p| p.leave_at.is_some()) {
            return;
        }
        self.morph(el, id, true);
    }

    /// One pet out that is free to go changes places with one resting in
    /// 寵物窩, both picked at random (隨機輪班, or 「現在換一次」 from the
    /// settings). False if there is nobody free or resting.
    fn shift_now(&mut self, el: &ActiveEventLoop) -> bool {
        let pets = &self.pets;
        let free = |id: u64| pets.iter().any(|p| p.save.id == id && p.free_to_go());
        let Some((going, coming)) = self.store.pick_shift(&mut self.rng, free) else {
            diag!("shift: nobody free to go, or nobody resting");
            return false;
        };
        self.collect_affinity();
        self.store.swap(going, coming);
        let name = |id: u64| {
            self.store
                .pets
                .iter()
                .find(|p| p.id == id)
                .map_or(String::new(), |p| p.name.clone())
        };
        diag!(
            "shift: {} goes to rest, {} comes out",
            name(going),
            name(coming)
        );
        // Now, so the settings window reads it back at once.
        self.save_now();
        self.sync_desktop(el);
        true
    }

    /// Every pet `size` at once (Roger's nineteenth pass: 一鍵式調整寵物大
    /// 小): those out on the desktop change where they stand, facing the
    /// way they faced, without coming in and saying hello again; those
    /// resting in 寵物窩 take the size for when they come out.
    fn size_all(&mut self, el: &ActiveEventLoop, size: f32) {
        self.collect_affinity();
        let size = size.clamp(0.5, 2.0);
        let mut was: Vec<(u64, f32, f32)> = Vec::new();
        for save in &mut self.store.pets {
            if (save.size - size).abs() < 1e-3 {
                continue;
            }
            save.size = size;
            if let Some(p) = self.pets.iter().find(|p| p.save.id == save.id) {
                was.push((save.id, p.body.feet().0, p.body.facing));
            }
        }
        if was.is_empty() {
            return;
        }
        diag!("size of every pet: {:.0}%", size * 100.0);
        self.dirty = true;
        self.pets.retain(|p| !was.iter().any(|w| w.0 == p.save.id));
        self.menu = None;
        self.sync_desktop(el);
        for (id, x, facing) in was {
            let Some(p) = self.pets.iter_mut().find(|p| p.save.id == id) else {
                continue;
            };
            let (w, _) = p.body.size;
            let foot = p.body.foot;
            p.body = Body::on_ground(&self.terrain, x - w / 2.0, p.body.size, foot);
            p.body.facing = facing;
            p.hello_at = None;
            // A little bounce as it grows or shrinks.
            p.anim.cheer();
        }
        self.save_now();
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
            TrayCommand::Stage => self.tray_stage(el),
            TrayCommand::Scene(i) => {
                if let Some(&scene) = TRAY_SCENES.get(usize::from(i)) {
                    self.tray_scene(el, scene);
                }
            }
            TrayCommand::Settings => self.open_settings(None),
            TrayCommand::Quit => {
                self.save_now();
                el.exit();
            }
        }
    }

    /// The tray's 「場景」 (Roger's twentieth pass, item 3): put it away if
    /// someone is in it, else ask whoever can and is free, nearest the
    /// middle of the screen.
    fn tray_scene(&mut self, el: &ActiveEventLoop, scene: Scene) {
        if self.hidden || self.fullscreen {
            return;
        }
        let now = Instant::now();
        let world = self.world(now);
        let pets: Vec<_> = self
            .pets
            .iter()
            .map(|p| {
                let own = p.fun.scene.filter(|r| !r.keep).map(|r| r.scene);
                let free = play::not_now(p, scene, world.now).is_none();
                (p.save.id, p.species, p.center_x(), own, free)
            })
            .collect();
        let mid = self.area.x + self.area.w / 2.0;
        match tray_pick(&pets, scene, mid) {
            Some((pet, true)) => {
                if let Some(p) = self.pets.iter_mut().find(|p| p.save.id == pet) {
                    play::wind_down(&mut self.play, p, world.now);
                    p.next = now;
                }
            }
            Some((pet, false)) => {
                diag!("tray: {scene:?} for pet {pet}");
                self.call_scene(el, pet, scene, world, now);
            }
            None => diag!("tray: nobody free for {scene:?}"),
        }
    }

    /// The tray's 「放聚光燈 / 收起來」: taken away if it is out, else put
    /// down beside the pet nearest the middle of the screen, a free one
    /// if there is one (it joins in and dances).
    fn tray_stage(&mut self, el: &ActiveEventLoop) {
        if self.hidden || self.fullscreen {
            return;
        }
        let Some(shared) = &self.shared else {
            return;
        };
        let now = self.secs(Instant::now());
        let mid = self.area.x + self.area.w / 2.0;
        let Some(pet) = self
            .pets
            .iter()
            .min_by(|a, b| {
                let busy = |p: &Pet| play::not_now(p, Scene::Dance, now).is_some();
                (busy(a), (a.center_x() - mid).abs())
                    .partial_cmp(&(busy(b), (b.center_x() - mid).abs()))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.save.id)
        else {
            return;
        };
        self.play
            .toggle_stage(el, shared, &mut self.pets, pet, self.area, now);
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

    /// Start `scene` for `pet` now, if it is free (跳舞: a spotlight comes
    /// down on it and it dances the 肉肉舞; 施法… › 狐火).
    fn start_scene(
        &mut self,
        el: &ActiveEventLoop,
        pet: u64,
        scene: Scene,
        world: World,
        now: Instant,
    ) {
        self.with_pet(el, pet, world, now, |play, p, ctx| {
            if !(p.fun.busy(world.now) || p.mind.asleep() || p.in_skit) {
                play::start_scene(play, p, ctx, scene);
            }
        });
    }

    /// Asked for `scene` from 一起玩…'s 場景 section or the tray (Roger's
    /// twentieth pass, item 3).
    fn call_scene(
        &mut self,
        el: &ActiveEventLoop,
        pet: u64,
        scene: Scene,
        world: World,
        now: Instant,
    ) {
        self.with_pet(el, pet, world, now, |play, p, ctx| {
            let _ = play::call_scene(play, p, ctx, scene);
        });
    }

    /// Run `f` on pet `pet` with the playground and a step's context,
    /// then let the pet take its next step straight away.
    fn with_pet(
        &mut self,
        el: &ActiveEventLoop,
        pet: u64,
        world: World,
        now: Instant,
        f: impl FnOnce(&mut play::Playground, &mut Pet, &mut Ctx),
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
        let mut ctx = Ctx {
            el,
            shared,
            lines,
            face: bubble_face,
            sounds,
            sound_on: settings.sound,
            everyone_you: &settings.you,
            volume: f32::from(settings.volume) / 100.0,
            area: *area,
            terrain,
            ui_scale: *ui_scale,
            world,
        };
        f(play, p, &mut ctx);
        p.next = now;
    }

    /// Put down a sprig of catnip under the cursor, or take it away (PLAN
    /// §8 "Catnip").
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
            Msg::Settings(mut s) => {
                s.sanitise();
                // 「同時在桌面上的寵物」 sends pets to rest or calls them out
                // (the settings window does the same to its list).
                if s.on_desktop != self.settings.on_desktop {
                    self.collect_affinity();
                    self.dirty |= self.store.fit_to(s.on_desktop);
                }
                let new_language = s.lang() != self.settings.lang();
                if s.rotate_minutes != self.settings.rotate_minutes {
                    self.shift_at = None;
                }
                // 「大家怎麼叫你」 changed: each pet out that now calls you
                // something else tries it out, a moment apart.
                let told = told_anew(
                    self.pets.iter().map(|p| &p.save),
                    &self.settings.you,
                    &s.you,
                );
                if !told.is_empty() {
                    let mut at = self.secs(Instant::now()) + 0.3;
                    for id in told {
                        self.called.retain(|c| c.1 != id);
                        self.called.push((at, id));
                        at += 1.2 + 0.8 * self.rng.f64();
                    }
                }
                self.settings = s;
                if let Err(e) = store::save(&self.data.settings(), &self.settings) {
                    eprintln!("{e}");
                }
                if new_language {
                    chonkgene_core::i18n::set(self.settings.lang());
                    // The tray's menu is made once: made again in the new
                    // words (the pets' menus are made each time they open).
                    self.tray = None;
                    self.make_tray();
                    // Untouched names and 「牠怎麼叫你」 follow the language,
                    // on the desktop as in the save.
                    if self.store.localize_defaults() {
                        self.dirty = true;
                        for p in &mut self.pets {
                            if let Some(s) = self.store.pets.iter().find(|s| s.id == p.save.id) {
                                p.save.name = s.name.clone();
                                p.save.you = s.you.clone();
                                p.window.set_title(&s.name);
                            }
                        }
                    }
                }
                self.sync_autostart();
                self.sync_desktop(el);
            }
            Msg::Pet(mut update) => {
                update.sanitise();
                let update_id = update.id;
                self.collect_affinity();
                let Some(save) = self.store.pets.iter_mut().find(|p| p.id == update.id) else {
                    return;
                };
                // Affinity lives in the pet process; never take it from settings.
                let affinity = save.affinity();
                let everyone = &self.settings.you;
                let renamed_you = save.calls_you(everyone) != update.calls_you(everyone);
                // Nor who is out or 常駐: only `Msg::Roster` moves those (an
                // edit sent just after 隨機輪班 moved this pet would undo it).
                update.on_desktop = save.on_desktop;
                update.pinned = save.pinned;
                // Her form is hers to change (she may have changed by
                // herself since the settings window read it), unless she
                // is now kept in the other one.
                let was_human = save.human;
                update.human = update.form_lock.keeps().unwrap_or(save.human);
                let morphed = update.human != was_human;
                let restyled =
                    update.human && (update.hairdo != save.hairdo || update.hair != save.hair);
                *save = update;
                save.set_affinity(&affinity);
                let save = save.clone();
                self.dirty = true;
                if let Some(i) = self.pets.iter().position(|p| p.save.id == save.id) {
                    let resized = (self.pets[i].save.size - save.size).abs() > 1e-3;
                    if morphed || restyled {
                        // Made again in her new form or look, where she
                        // stands, in a burst of fox-fire.
                        let x = self.pets[i].body.feet().0;
                        self.pets.remove(i);
                        self.morphs.retain(|m| m.0 != save.id);
                        self.morphs.push((save.id, x));
                    } else if resized {
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
                // Told what to call you: it tries the new name out at once
                // (Roger, 2026-11-20: the lines never seemed to use it).
                if renamed_you {
                    let now = Instant::now();
                    let world = self.world(now);
                    self.with_pet(el, update_id, world, now, |_, p, ctx| {
                        if !p.mind.asleep() {
                            p.say(ctx, Line::CalledYou);
                        }
                    });
                }
                self.sync_desktop(el);
            }
            Msg::Adopt { species, coat } => {
                let Some(s) = Species::from_id(&species) else {
                    return;
                };
                let id = self.store.next_id;
                self.store.next_id += 1;
                let save = PetSave::adopt(id, s, &coat, &mut self.rng);
                self.collect_affinity();
                // Out at once, in the place of the last one out if the
                // desktop is full.
                if self.store.adopt(save, self.settings.on_desktop) == Room::Over {
                    self.limit_to_fit();
                }
                self.dirty = true;
                self.sync_desktop(el);
            }
            Msg::Roster(seats) => {
                self.collect_affinity();
                if self.store.take_roster(&seats) {
                    // All of them 常駐 and one more called out: the limit
                    // grows to match, as the settings window's did.
                    self.limit_to_fit();
                    self.dirty = true;
                    self.sync_desktop(el);
                }
            }
            Msg::SizeAll(size) => self.size_all(el, size),
            Msg::Release(id) => {
                self.store.pets.retain(|p| p.id != id);
                self.dirty = true;
                self.sync_desktop(el);
            }
            Msg::ShiftNow => {
                // The next round counts from now.
                if self.shift_now(el) {
                    self.shift_at = None;
                }
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
    /// `page` (see [`menu_page`]).
    fn open_menu(
        &mut self,
        el: &ActiveEventLoop,
        i: usize,
        world: &World,
        page: Page,
        at: Option<(i32, i32)>,
    ) {
        let facts = self.menu_facts(i, world);
        let (items, actions) = menu_page(page, &facts);
        self.show_menu(el, i, items, actions, at);
    }

    /// What pet `i`'s menu pages are made from, now.
    fn menu_facts(&self, i: usize, world: &World) -> MenuFacts {
        let p = &self.pets[i];
        let scenes = if let Some(run) = p.fun.scene {
            SceneRows::In(run.scene, run.keep)
        } else {
            match play::not_now(p, Scene::Foxfire, world.now) {
                Some(play::NotNow::Asleep) => SceneRows::Asleep,
                Some(_) => SceneRows::Busy,
                None => SceneRows::Free(
                    p.species
                        .menu_scenes()
                        .into_iter()
                        .map(|s| (s, play::not_now(p, s, world.now).is_some()))
                        .collect(),
                ),
            }
        };
        MenuFacts {
            asleep: p.mind.asleep(),
            snacks: p.mind.snacks_left(world.day),
            fox: p.species == Species::Fox,
            morph: morphs(p.species).then_some(p.save.human),
            others: self
                .pets
                .iter()
                .filter(|q| q.save.id != p.save.id)
                .map(|q| (q.save.id, q.save.name.clone()))
                .collect(),
            toy: self.toy.is_some(),
            catnip: self.play.has(Thing::Catnip),
            stage: self.play.stage.is_some(),
            scenes,
        }
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
            Some(tr("睡覺中").to_string())
        } else if p.mind.grumpy() {
            Some(tr("有起床氣").to_string())
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
        // A long page on a short screen (or a big UI scale) is drawn
        // smaller rather than run off the bottom.
        let scale = menu::fit_scale(&model, self.ui_scale, self.area.h);
        let Some(drawn) = menu::draw(&mut self.ui_face, &model, None, scale) else {
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
            scale,
            armed: false,
        });
    }

    fn redraw_menu(&mut self) {
        let (Some(m), Some(shared)) = (&mut self.menu, &self.shared) else {
            return;
        };
        if let Some(drawn) = menu::draw(&mut self.ui_face, &m.model, m.hover, m.scale) {
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
            Action::Commands | Action::Back | Action::HugWho | Action::Toys | Action::HugHow => {
                let page = match action {
                    Action::Commands => Page::Commands,
                    Action::HugWho => Page::HugWho,
                    Action::HugHow => Page::HugHow,
                    Action::Toys => Page::Toys,
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
                self.start_scene(el, pet, Scene::Foxfire, world, now);
                return;
            }
            Action::Morph => {
                self.morph(el, pet, false);
                return;
            }
            Action::CallScene(scene) => {
                if !(self.hidden || self.fullscreen) {
                    self.call_scene(el, pet, scene, world, now);
                }
                return;
            }
            Action::EndScene => {
                if let Some(p) = self.pets.iter_mut().find(|p| p.save.id == pet) {
                    play::wind_down(&mut self.play, p, world.now);
                    p.next = now;
                }
                return;
            }
            Action::HugWith(_) | Action::Magic(Magic::Spell(_)) => {
                let cast = actors(&self.pets, world.now);
                let cmds = match action {
                    Action::HugWith(whom) => self.director.hug(pet, whom, world.now, &cast),
                    Action::Magic(Magic::Spell(spell)) => {
                        self.director.cast(pet, spell, world.now, &cast)
                    }
                    _ => Vec::new(),
                };
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
                    everyone_you: &settings.you,
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
                    everyone_you: &settings.you,
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
            Action::Edit => return self.open_settings(Some(pet)),
            Action::Toy => return self.toggle_toy(el),
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
            everyone_you: &settings.you,
            volume: f32::from(settings.volume) / 100.0,
            area: *area,
            terrain,
            ui_scale: *ui_scale,
            world,
        };
        let out = match action {
            Action::Pat => {
                p.pat_you(&mut ctx);
                p.next = now;
                *dirty = true;
                return;
            }
            Action::Feed => p.mind.feed(&world),
            Action::Sleep => {
                let mut rng = p.rng.clone();
                let out = p.mind.put_to_sleep(&world, &mut rng);
                p.rng = rng;
                out
            }
            Action::Wake => p.mind.wake(&world),
            Action::Hug | Action::HugAs(_) => {
                let hold = match action {
                    Action::HugAs(h) => Some(h),
                    _ => None,
                };
                p.hug_you(&mut ctx, hold);
                p.next = now;
                *dirty = true;
                return;
            }
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
                "frames: {} {:.1}/s shown of {:.1}/s asked; sent {} ({} past the budget, {} walking), held back {}, late {}; GPU {ms:.1} ms a frame",
                p.save.name,
                f64::from(f.shown) / secs,
                f64::from(draws) / secs,
                f.sent,
                f.pushed,
                f.forced,
                f.held_back,
                f.late,
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

/// The pets (of `saves`) that call you something else once 「大家怎麼叫你」
/// goes from `was` to `now`: those whose own was never changed, unless
/// the shared one and their default give the same.
fn told_anew<'a>(saves: impl Iterator<Item = &'a PetSave>, was: &str, now: &str) -> Vec<u64> {
    saves
        .filter(|p| p.calls_you(was) != p.calls_you(now))
        .map(|p| p.id)
        .collect()
}

/// The next save, if anything is unsaved. `save_at` stays behind once a
/// save is done; only unsaved changes make it a reason to wake.
fn pending_save(dirty: bool, save_at: Instant) -> Option<Instant> {
    dirty.then_some(save_at)
}

/// What the pages of a pet's menu are made from, gathered from the pet and
/// the desktop by `App::menu_facts` (apart, so the pages can be tested).
struct MenuFacts {
    asleep: bool,
    /// Snacks left today.
    snacks: u8,
    fox: bool,
    /// She can change form here (the fox, on a real GPU): whether she is in
    /// her human form now.
    morph: Option<bool>,
    /// The other pets out, to go and hug: id and name.
    others: Vec<(u64, String)>,
    /// The grass stalk, catnip, the spotlight are out.
    toy: bool,
    catnip: bool,
    stage: bool,
    scenes: SceneRows,
}

/// The 場景 section of 一起玩….
enum SceneRows {
    /// In one: its scene, and whether it only joined someone else's.
    In(Scene, bool),
    Asleep,
    Busy,
    /// Free: each scene it can be asked for, its own first, and whether it
    /// is up on a window (with no floor beside it for that one).
    Free(Vec<(Scene, bool)>),
}

/// A menu page's rows and what each does, built a row at a time.
#[derive(Default)]
struct Rows {
    items: Vec<Item>,
    actions: Vec<Action>,
}

impl Rows {
    /// A row, returned to add a hint, a separator or a caption to.
    fn add(&mut self, label: &str, icon: Icon, action: Action) -> &mut Item {
        self.items.push(Item {
            label: label.into(),
            hint: None,
            icon,
            separated: false,
            caption: None,
        });
        self.actions.push(action);
        let last = self.items.len() - 1;
        &mut self.items[last]
    }

    /// 返回, set apart at the bottom.
    fn back(&mut self, to: Action) {
        self.add(tr("返回"), Icon::Back, to).separated = true;
    }
}

/// The rows of a pet's menu on `page`, and what each does. Roger's
/// twenty-sixth pass, item 1 (「右鍵選單太亂」): the first page in three
/// short groups (the pet itself; 叫牠… and 一起玩…; settings and leaving).
/// What is set once and kept (its name, its look, goodbye, everyone's
/// size) lives in the settings window, which 設定… opens on this pet.
fn menu_page(page: Page, f: &MenuFacts) -> (Vec<Item>, Vec<Action>) {
    let mut r = Rows::default();
    match page {
        Page::Main => {
            if f.asleep {
                r.add(tr("抱抱"), Icon::Hug, Action::Hug);
            } else {
                r.add(tr("抱抱…"), Icon::Hug, Action::HugHow);
            }
            r.add(tr("摸摸頭"), Icon::Heart, Action::Pat);
            r.add(tr("餵零食"), Icon::Snack, Action::Feed).hint = Some(if f.snacks > 0 {
                trf("還有 {snacks} 份", &[("snacks", &f.snacks.to_string())])
            } else {
                tr("吃飽了").into()
            });
            if f.asleep {
                r.add(tr("輕輕叫醒牠"), Icon::Sun, Action::Wake);
            } else {
                r.add(tr("讓牠去睡覺"), Icon::Moon, Action::Sleep);
            }
            // Asking it to do something only while it is awake.
            if !f.asleep {
                r.add(tr("叫牠…"), Icon::Paw, Action::Commands).separated = true;
            }
            r.add(tr("一起玩…"), Icon::Ball, Action::Toys).separated = f.asleep;
            let settings = r.add(tr("設定…"), Icon::Sliders, Action::Edit);
            settings.hint = Some(tr("名字・造型").into());
            settings.separated = true;
            r.add(tr("全部收起來"), Icon::Hide, Action::HideAll).hint = Some("Ctrl+Alt+P".into());
            r.add(tr("結束肉肉賴桌"), Icon::Exit, Action::Quit);
        }
        Page::Commands => {
            let asks = [tr("坐下"), tr("趴下"), tr("過來"), tr("轉圈"), tr("握手")];
            for (label, &ask) in asks.iter().zip(&Ask::ALL) {
                r.add(label, Icon::Paw, Action::Command(ask));
            }
            r.add(tr("跳舞"), Icon::Note, Action::Dance);
            // Someone else on the desktop to go and hug.
            if !f.others.is_empty() {
                r.add(tr("去抱抱…"), Icon::Hug, Action::HugWho);
            }
            // The fox casts spells when you ask (Roger, 2026-11-06), in a
            // section of its own here.
            if f.fox {
                let spells = [
                    (tr("定身術"), Icon::Bubble, Magic::Spell(Spell::Freeze)),
                    (tr("狐火"), Icon::Flame, Magic::Foxfire),
                    (tr("祝福術"), Icon::Heart, Magic::Spell(Spell::Bless)),
                    (tr("招喚術"), Icon::Paw, Magic::Spell(Spell::Call)),
                ];
                for (n, &(label, icon, magic)) in spells.iter().enumerate() {
                    let row = r.add(label, icon, Action::Magic(magic));
                    if n == 0 {
                        row.caption = Some(tr("施法").into());
                    }
                }
                // Her human form (PLAN §8), where she can take it.
                if let Some(human) = f.morph {
                    r.add(tr("變身"), Icon::Wand, Action::Morph).hint = Some(
                        if human {
                            tr("變回原型")
                        } else {
                            tr("變成人型")
                        }
                        .into(),
                    );
                }
            }
            r.back(Action::Back);
        }
        Page::Toys => {
            if !f.asleep {
                r.add(tr("玩躲貓貓"), Icon::Peek, Action::Hide);
            }
            // Its scenes (Roger's twentieth pass, item 3): one it is in can
            // be put away; asleep or busy, the section says so.
            let scenes = r.items.len();
            match &f.scenes {
                SceneRows::In(scene, keep) => {
                    // Its thing stays for the others when it only joined in
                    // (someone's pond, the spotlight put down from the menu).
                    let (label, icon) = match scene {
                        Scene::Pond if *keep => (tr("從水池上岸").to_string(), Icon::Pond),
                        Scene::Dance if *keep => (tr("不跳了").to_string(), Icon::Note),
                        &scene => (
                            trf("把{thing}收起來", &[("thing", scene_thing(scene))]),
                            Icon::Hide,
                        ),
                    };
                    r.add(&label, icon, Action::EndScene);
                }
                SceneRows::Asleep => {
                    r.add(tr("牠睡著了，醒來再玩"), Icon::Moon, Action::Toys);
                }
                SceneRows::Busy => {
                    r.add(tr("牠在忙，等等再叫牠"), Icon::Paw, Action::Toys);
                }
                SceneRows::Free(list) => {
                    // The fox's 狐火 is a spell, under 叫牠… › 施法.
                    for &(scene, up) in list.iter().filter(|s| s.0 != Scene::Foxfire) {
                        let action = if up {
                            Action::Toys
                        } else {
                            Action::CallScene(scene)
                        };
                        let row = r.add(scene_label(scene), scene_icon(scene), action);
                        if up {
                            row.hint = Some(tr("在高處").into());
                        }
                    }
                }
            }
            if let Some(row) = r.items.get_mut(scenes) {
                row.caption = Some(tr("場景").into());
            }
            // Things for them all (PLAN §8, Roger's fourth pass, item 2,
            // and "Catnip").
            r.add(
                if f.toy {
                    tr("把狗尾草收起來")
                } else {
                    tr("拿狗尾草逗牠們")
                },
                Icon::Grass,
                Action::Toy,
            )
            .caption = Some(tr("大家一起玩").into());
            r.add(tr("丟球給牠們"), Icon::Ball, Action::Throw(Thing::Ball));
            r.add(
                tr("丟飛盤給牠們"),
                Icon::Frisbee,
                Action::Throw(Thing::Frisbee),
            );
            r.add(
                if f.catnip {
                    tr("把貓薄荷收起來")
                } else {
                    tr("放一把貓薄荷")
                },
                Icon::Leaf,
                Action::Catnip,
            );
            r.add(
                if f.stage {
                    tr("把聚光燈收起來")
                } else {
                    tr("放聚光燈")
                },
                Icon::Spotlight,
                Action::Stage,
            );
            r.back(Action::Back);
        }
        Page::HugHow => {
            // Roger's twenty-first pass: three ways to hold it, picked
            // here (or by two taps on shoulders, a hind paw, its back).
            for (label, hold) in [
                (tr("環抱"), HugHold::Wrap),
                (tr("舉高高"), HugHold::Carry),
                (tr("搖搖抱"), HugHold::Cradle),
            ] {
                r.add(label, Icon::Hug, Action::HugAs(hold));
            }
            r.back(Action::Back);
        }
        Page::HugWho => {
            for (id, name) in &f.others {
                r.add(name, Icon::Hug, Action::HugWith(*id));
            }
            r.back(Action::Commands);
        }
    }
    (r.items, r.actions)
}

/// A scene's row in 一起玩…'s 場景 section and the tray (PLAN §1.11: a
/// plain label).
fn scene_label(scene: Scene) -> &'static str {
    match scene {
        Scene::Pond => tr("玩水池"),
        Scene::Sunbeam => tr("曬太陽"),
        Scene::Butterfly => tr("追蝴蝶"),
        Scene::Bone => tr("啃骨頭"),
        Scene::Seeds => tr("啄種子"),
        Scene::Perch => tr("飛上樹枝"),
        Scene::Foxfire => tr("狐火"),
        Scene::Dance => tr("跳舞"),
    }
}

/// What 「把…收起來」 puts away.
fn scene_thing(scene: Scene) -> &'static str {
    match scene {
        Scene::Pond => tr("水池"),
        Scene::Sunbeam => tr("陽光"),
        Scene::Butterfly => tr("蝴蝶"),
        Scene::Bone => tr("骨頭"),
        Scene::Seeds => tr("種子"),
        Scene::Perch => tr("樹枝"),
        Scene::Foxfire => tr("狐火"),
        Scene::Dance => tr("聚光燈"),
    }
}

fn scene_icon(scene: Scene) -> Icon {
    match scene {
        Scene::Pond => Icon::Pond,
        Scene::Sunbeam => Icon::Sun,
        Scene::Butterfly => Icon::Butterfly,
        Scene::Bone => Icon::Snack,
        Scene::Seeds => Icon::Seeds,
        Scene::Perch => Icon::Branch,
        Scene::Foxfire => Icon::Flame,
        Scene::Dance => Icon::Note,
    }
}

/// The tray's 「場景」 submenu, in the order of its labels
/// ([`chonkgene_platform::tray::SCENES`]).
const TRAY_SCENES: [Scene; 7] = [
    Scene::Pond,
    Scene::Sunbeam,
    Scene::Butterfly,
    Scene::Bone,
    Scene::Seeds,
    Scene::Perch,
    Scene::Foxfire,
];

/// Whom the tray asks into `scene`: the pet already in it (to put it
/// away), or else of those that can be asked for it and are free now,
/// the one nearest the middle of the screen (`mid`); none if nobody can.
fn tray_pick(
    pets: &[(u64, Species, f32, Option<Scene>, bool)],
    scene: Scene,
    mid: f32,
) -> Option<(u64, bool)> {
    if let Some(&(id, ..)) = pets.iter().find(|p| p.3 == Some(scene)) {
        return Some((id, true));
    }
    pets.iter()
        .filter(|p| p.4 && p.1.can_call(scene))
        .min_by(|a, b| (a.2 - mid).abs().total_cmp(&(b.2 - mid).abs()))
        .map(|p| (p.0, false))
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
            Cmd::Hug { role, .. } => {
                // Face to face (turned by the Face before it), sitting
                // into the hug; the one in front over the other (its window
                // goes on top with the next restack, see `play::stack_key`).
                if p.body.motion == Motion::Walk {
                    p.body.motion = Motion::Idle;
                }
                p.anim.hug(role);
                if p.anim.expression != Expression::Happy {
                    p.anim.cheer();
                }
                p.anim.expression = Expression::Happy;
                p.happy_until = ctx.world.now + f64::from(HUG_SECS) + HAPPY_SECS;
                p.fun.effect(Fx::Hearts, ctx.world.now);
                diag!("{}: hugs as {role:?}", p.save.name);
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

/// Whether a pet of `species` can change form here: the fox, on a real GPU
/// (her human form is meshes made in Blender; the CPU's sprites are
/// ray-marched from a model's parts and would show only her head).
fn morphs(species: Species) -> bool {
    species == Species::Fox && chonkgene_render::shared_gpu().is_some()
}

/// How long until the fox next changes form by herself: half an hour to
/// an hour and a half. `CHONKGENE_MORPH=<seconds>` makes it about that,
/// for trying it out.
fn morph_wait(rng: &mut fastrand::Rng) -> f64 {
    let mean = std::env::var("CHONKGENE_MORPH")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(3600.0);
    mean * (0.5 + rng.f64())
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
    // The fox in her human form, where she can take it (her figure is
    // meshes: on sprites she stays as she is on all fours).
    let human = (save.human && morphs(species))
        .then(|| chonkgene_render::skin::human(species.id(), &save.hairdo, &save.hair))
        .transpose()
        .unwrap_or_else(|e| {
            eprintln!("{}: no human form: {e}", save.name);
            None
        })
        .map(|(m, _)| Arc::new(m));
    let mesh = human.or_else(|| kind.quad.clone());
    let coat = mesh
        .as_deref()
        .unwrap_or(&kind.model)
        .coat(&save.coat)
        .unwrap_or(0);
    // The renderer first: a pet on all fours needs a wider window.
    let mut renderer = Renderer::new(Arc::clone(&kind.model), mesh, scale, save.chonk, coat)
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
        morph_at: t + morph_wait(&mut fastrand::Rng::with_seed(seed)),
        happy_until: 0.0,
        asleep_since: 0.0,
        hello_at: Some(t + 1.2 + slot as f64 * 1.4),
        leave_at: None,
        said_bye: false,
        act: None,
        bubble: None,
        in_skit: false,
        play_fight: false,
        playing: false,
        play_at: 0.0,
        pounce_at: None,
        purr_at: 0.0,
        love_at: 0.0,
        tapped: None,
        poke_due: None,
        dash_until: 0.0,
        glide_checked: false,
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

    /// Somewhere to fly to: a spot on a floor at least 400 px away (at
    /// 100%), anywhere across the screens.
    fn flight_spot(&mut self, terrain: &Terrain) -> Option<(f32, f32)> {
        let (fx, _) = self.body.feet();
        let (left, right) = terrain.bounds();
        let half = self.body.size.0 / 2.0;
        for _ in 0..8 {
            let x = left + half + self.rng.f32() * (right - left - 2.0 * half).max(0.0);
            if (x - fx).abs() < 400.0 * self.fun.scale {
                continue;
            }
            if let Some(l) = terrain.floor_under(x) {
                return Some((x, l.y));
            }
        }
        None
    }

    /// A quick click's poke, once no second click came: a start, hopping
    /// back from where the finger was (still facing the same way), and
    /// what it makes of it.
    /// A tap's action (see [`tap_action`]).
    fn tap(&mut self, ctx: &mut Ctx, zone: Zone, action: TapAction) {
        diag!("{}: tapped on {zone:?}: {action:?}", self.save.name);
        match action {
            TapAction::Pat => self.pat_you(ctx),
            TapAction::Paw => self.shake_paw(ctx),
            TapAction::Hug(hold) => self.hug_you(ctx, Some(hold)),
            TapAction::PatBottom => self.pat_bottom(ctx),
        }
    }

    /// Its cheeks squeezed between your hands and let go (PLAN §8, Roger's
    /// twenty-seventh pass, item 6 (a)): its face springs back out past
    /// round and wobbles, rosy for a few seconds (the renderer's); on a
    /// cheek it loves it rubs its face on your hand, pleased, otherwise it
    /// pouts and rubs its cheeks with its paws (a bird with its wings, the
    /// fox's human form holds hers).
    fn cheeks_let_go(&mut self, ctx: &mut Ctx) {
        let out = self.mind.squeeze_cheeks(&ctx.world);
        let free = out.line == Some(Line::CheekSqueeze)
            && !self.in_skit
            && matches!(self.body.motion, Motion::Idle | Motion::Walk);
        let act = if self.species.fondness(Zone::Cheek) == Fondness::Loves {
            Act::Nuzzle
        } else {
            Act::CheekRub
        };
        diag!(
            "{}: cheeks squeezed and let go: {}",
            self.save.name,
            if free {
                format!("{act:?}")
            } else {
                format!("{:?}", out.line)
            }
        );
        if free {
            self.body.motion = Motion::Idle;
            self.body.speed = 0.0;
            self.body.pace = 1.0;
            self.anim.walking = false;
            self.anim.gesture(act);
        }
        self.apply(ctx, out);
    }

    /// A light pat on the bottom (PLAN §8, Roger's twenty-seventh pass,
    /// item 6 (b)), each its own way (see [`Species::bottom_pat`]): a cat
    /// pushes its rump up, the Shiba kicks a hind leg and looks back
    /// pleased, the chick hops a step forward, the duckling hops and
    /// waddles off quacking, the owl stares round at you, the fox turns,
    /// glares and swats your hand with her tails.
    fn pat_bottom(&mut self, ctx: &mut Ctx) {
        use chonkgene_core::species::BottomPat;
        let out = self.mind.pat_bottom(&ctx.world);
        let free = out.line == Some(Line::BottomPat)
            && !self.in_skit
            && matches!(self.body.motion, Motion::Idle | Motion::Walk);
        let how = self.species.bottom_pat();
        diag!(
            "{}: patted on the bottom: {}",
            self.save.name,
            if free {
                format!("{how:?}")
            } else {
                format!("{:?}", out.line)
            }
        );
        if free {
            let (w, s) = (self.body.size.0, self.fun.scale);
            // The hand is behind it: away is the other way.
            let away = if self.cursor.0 < 0.5 * w { 1.0 } else { -1.0 };
            let halt = |p: &mut Pet| {
                p.body.motion = Motion::Idle;
                p.body.speed = 0.0;
                p.body.pace = 1.0;
                p.anim.walking = false;
            };
            match how {
                BottomPat::ButtUp => {
                    halt(self);
                    self.anim.gesture(Act::ButtUp);
                }
                BottomPat::Kick => self.anim.pat_rump(),
                BottomPat::Hop => {
                    self.body
                        .spring(self.body.feet().0 + away * 0.12 * w, 20.0 * s);
                }
                BottomPat::HopAway => {
                    self.anim.startle_hop();
                    self.body.facing = away;
                    self.body.motion = Motion::Walk;
                    self.body.pace = DUCK_HURRY;
                    self.switch_at = ctx.world.now + 1.6;
                }
                // Looks round at the hand behind it, as at a poke there.
                BottomPat::LookRound => self.anim.noticed_at(Zone::Bottom, self.cursor),
                BottomPat::Swat => {
                    halt(self);
                    self.body.facing = -away;
                    self.anim.gesture(Act::Swish);
                }
            }
        }
        self.apply(ctx, out);
    }

    /// Your hand offered for 握手 (two taps on a front paw): it gives its
    /// paw (a bird its wing, a high five) at once and gladly, though a paw
    /// held down is still disliked; asleep or with a grudge it pulls the
    /// paw back.
    fn shake_paw(&mut self, ctx: &mut Ctx) {
        let out = self.mind.shake_hands(&ctx.world);
        if out.line.is_some() {
            diag!("{}: paw pulled back", self.save.name);
            self.anim.flinch(self.cursor);
        } else if !self.in_skit && matches!(self.body.motion, Motion::Idle | Motion::Walk) {
            self.fun.ask_now(Ask::Paw, ctx.world.now);
        }
        self.apply(ctx, out);
    }

    fn poke_now(&mut self, ctx: &mut Ctx, zone: Zone) {
        if !self.mind.asleep() && matches!(self.body.motion, Motion::Idle | Motion::Walk) {
            let (w, s) = (self.body.size.0, self.fun.scale);
            let away = if self.cursor.0 < 0.5 * w { 1.0 } else { -1.0 };
            let facing = self.body.facing;
            self.body
                .spring(self.body.feet().0 + away * 0.06 * w, 14.0 * s);
            self.body.facing = facing;
        }
        let mut rng = self.rng.clone();
        let out = self.mind.poke(&ctx.world, zone, &mut rng);
        self.rng = rng;
        diag!(
            "{}: a poke: {:?} {:?}",
            self.save.name,
            out.line,
            out.expression
        );
        self.apply(ctx, out);
    }

    /// 摸摸頭 from its menu (Roger, 2026-11-19: no hand came, and it wanted
    /// more going on): your hand comes down onto its head and pats it
    /// three times, then strokes it to and fro, and goes (see
    /// [`props::pat_pose`]). Asleep it is only stroked and sleeps on; in a
    /// mood it ducks the hand. What it says comes once it is being
    /// stroked, the happy hop and hearts once the hand has gone. Without
    /// the hand (dragged, in a skit, in the air, up to something, already
    /// in your hands) it is pleased at once, as before.
    fn pat_you(&mut self, ctx: &mut Ctx) {
        let now = ctx.world.now;
        let out = self.mind.pat(&ctx.world, Zone::Head);
        let asleep = out.line == Some(Line::TurnOver);
        let kind = if asleep {
            props::PatKind::Asleep
        } else if out.line == Some(Line::GrudgePat) {
            props::PatKind::Dodge
        } else {
            props::PatKind::Pat
        };
        let why_not = if self.in_skit {
            Some("in a skit")
        } else if self.press.is_some() {
            Some("being held")
        } else if !asleep && !matches!(self.body.motion, Motion::Idle | Motion::Walk) {
            Some("not on its feet")
        } else if self.fun.engaged() {
            Some("busy")
        } else if self.fun.hug_by_you.is_some() || self.fun.pat_by_you.is_some() {
            Some("in your hands")
        } else if self.leave_at.is_some() {
            Some("going to rest")
        } else {
            None
        };
        if let Some(why) = why_not {
            diag!("{}: patted on the head, no hand ({why})", self.save.name);
            self.apply(ctx, out);
            return;
        }
        diag!("{}: patted on the head ({kind:?})", self.save.name);
        self.fun.end_leisure();
        if !asleep {
            self.body.motion = Motion::Idle;
            self.body.speed = 0.0;
        }
        self.fun.pat_by_you = Some((now, kind));
        self.fun.pat_down = false;
        self.fun.pat_out = Some(out);
        self.switch_at = self.switch_at.max(now + f64::from(kind.secs()) + 1.0);
    }

    /// The pat under way, every frame: its body pressed in under the hand
    /// at each pat and let go between (it squishes, squints, pops back),
    /// rubbed along while stroked (it leans into the hand, eyes shut, the
    /// dent following); ducking in a mood. Cut short if you take hold of
    /// it or a skit takes it.
    fn pat_step(&mut self, ctx: &mut Ctx) {
        let Some((since, kind)) = self.fun.pat_by_you else {
            return;
        };
        let now = ctx.world.now;
        let t = (now - since) as f32;
        let pose = props::pat_pose(kind, t).filter(|_| self.press.is_none() && !self.in_skit);
        let crown = self.renderer.anchor();
        let (facing, scale) = (self.anim.facing, self.fun.scale);
        let at = |p: &props::PatPose| props::pat_hand(crown, facing, scale, p).press;
        let feels = kind != props::PatKind::Asleep;
        match pose {
            Some(p) if feels && p.down && !self.fun.pat_down => {
                self.anim.press(Zone::Head, at(&p));
                self.anim.fond = Some(self.species.fondness(Zone::Head));
            }
            Some(p) if feels && p.down && p.stroking => self.anim.knead(at(&p)),
            Some(p) if !p.down && self.fun.pat_down => self.anim.poke(Zone::Head),
            None if self.fun.pat_down => self.anim.poke(Zone::Head),
            _ => {}
        }
        self.fun.pat_down = feels && pose.is_some_and(|p| p.down);
        // Under the hand it does not look up at it.
        self.anim.patted = pose.is_some();
        // The words: once it is being stroked; at once for a duck, as it
        // flinches from the hand.
        let say_at = match kind {
            props::PatKind::Dodge => 0.3,
            _ => kind.strokes_from(),
        };
        if t >= say_at
            && let Some(out) = self.fun.pat_out.as_mut()
            && let Some(line) = out.line.take()
        {
            if kind == props::PatKind::Dodge {
                self.anim.flinch(at(&props::PatPose::default()));
            }
            self.say(ctx, line);
        }
        if pose.is_none() {
            self.fun.pat_by_you = None;
            self.fun.pat_down = false;
            if let Some(out) = self.fun.pat_out.take() {
                self.apply(ctx, out);
            }
        }
    }

    /// Free to go to rest for 隨機輪班: not in your hands (held, hugged,
    /// patted), not in a skit or up to something, on its feet or asleep,
    /// not already going.
    fn free_to_go(&self) -> bool {
        self.press.is_none()
            && self.leave_at.is_none()
            && !self.in_skit
            && self.fun.hug_by_you.is_none()
            && self.fun.pat_by_you.is_none()
            && !self.fun.engaged()
            && (self.mind.asleep() || matches!(self.body.motion, Motion::Idle | Motion::Walk))
    }

    /// 抱抱 (the menu, or two taps): it says how it takes it, and if
    /// it is awake and free your hands hold it, `hold` (none: as suits how
    /// it is now, see [`HugHold::pick`]), and it snuggles in; asleep, it is
    /// cradled and sleeps on.
    fn hug_you(&mut self, ctx: &mut Ctx, hold: Option<HugHold>) {
        let now = ctx.world.now;
        let mut out = self.mind.hug(&ctx.world);
        // Lounging while you read, sitting by you or on its way to you, it
        // is still yours to hug (Roger's log, 2026-11-13: eight
        // double-clicks in a row on the chick, lying preening, all came to
        // nothing).
        // Asleep, it is picked up and rocked without waking, curled as it
        // lay (Roger's log, 2026-11-16: nine double-clicks on the sleeping
        // fox, each turned down "asleep").
        let asleep = out.line == Some(Line::TurnOver);
        let why_not = if asleep && hold.is_some_and(|h| h != HugHold::Cradle) {
            Some("asleep")
        } else if asleep {
            None
        } else if out.line != Some(Line::Hugged) {
            Some("in a mood")
        } else if self.in_skit {
            Some("in a skit")
        } else if self.press.as_ref().is_some_and(|p| p.moved) {
            Some("being dragged")
        } else if !matches!(self.body.motion, Motion::Idle | Motion::Walk) {
            Some("not on its feet")
        } else if self.fun.engaged() {
            Some("busy")
        } else {
            None
        };
        let free = why_not.is_none();
        let hold = if asleep { Some(HugHold::Cradle) } else { hold };
        let hold = hold.unwrap_or_else(|| {
            let hour = ctx.world.hour;
            let mood = HugMood {
                sleepy: chonkgene_core::species::in_hours(hour, self.species.sleep_hours())
                    || !(6.0..23.0).contains(&hour),
                tired: self.anim.params().puffed,
                pleased: now < self.happy_until,
                energy: self.mind.personality.energy,
            };
            HugHold::pick(self.species, mood, self.rng.f32())
        });
        if free {
            self.fun.end_leisure();
            self.body.motion = Motion::Idle;
            self.anim.hug(Hug::ByYou(hold));
            self.fun.hug_by_you = Some((now, hold));
            // Not off for a walk while in your hands (a 搖搖抱 is long: it
            // got up and ran off half way).
            self.switch_at = self.switch_at.max(now + f64::from(hold.secs()) + 0.5);
            if asleep {
                // No word and no face: it sleeps on.
                out = chonkgene_core::mind::Out::default();
            } else if hold == HugHold::Cradle {
                // Rocked, not cheered: no happy hop on its back; the fox
                // says she does not want this, and melts later.
                out.line = Some(Line::Cradled);
                out.expression = Some(Expression::Neutral);
                self.fun.cradle_melt = Some(now + 0.4 * f64::from(hold.secs()));
            }
        }
        diag!(
            "{}: hugged ({})",
            self.save.name,
            if free {
                match hold {
                    HugHold::Wrap => "wrapped round",
                    HugHold::Carry => "lifted up",
                    HugHold::Cradle if asleep => "cradled asleep",
                    HugHold::Cradle => "cradled",
                }
            } else {
                why_not.unwrap_or_default()
            }
        );
        self.apply(ctx, out);
    }

    /// Held in your hands: let go if the hug was cut short (picked up,
    /// startled); rocked in a 搖搖抱, drowsy murmurs (the fox melting) part
    /// way, and asleep at the end (the sleep of PLAN §1.12).
    fn cradle_step(&mut self, ctx: &mut Ctx) {
        let now = ctx.world.now;
        if let Some((since, _)) = self.fun.hug_by_you
            && now - since > 0.2
            && self.anim.doing() != Some(Act::Hug)
        {
            self.fun.hug_by_you = None;
        }
        let Some((since, HugHold::Cradle)) = self.fun.hug_by_you else {
            self.fun.cradle_melt = None;
            return;
        };
        if self.fun.cradle_melt.is_some_and(|at| now >= at) {
            self.fun.cradle_melt = None;
            self.say(ctx, Line::CradleMelt);
        }
        if now - since >= f64::from(HugHold::Cradle.secs()) - 0.15 && !self.mind.asleep() {
            self.fun.hug_by_you = None;
            let out = self.mind.put_to_sleep(&ctx.world, &mut self.rng);
            diag!("{}: rocked to sleep", self.save.name);
            self.apply(ctx, out);
        }
    }

    /// A finger held on it, every frame: by the spot (PLAN §8, Roger's
    /// sixteenth pass, item 4). One it does not like: it pulls away at once
    /// and says so. One it loves: hearts and purring (a dog panting) while
    /// it lasts. The 曼赤肯's belly rubbed long: the trap.
    fn touch_step(&mut self, ctx: &mut Ctx) {
        let t = ctx.world.now;
        let Some((zone, _, held)) = self.anim.params().touch.held else {
            return;
        };
        if self.press.as_ref().is_none_or(|p| p.moved || p.answered)
            || self.mind.asleep()
            || self.in_skit
        {
            return;
        }
        let fond = self.species.fondness(zone);
        if fond == Fondness::Dislikes && held >= DISLIKE_SECS {
            if let Some(p) = &mut self.press {
                p.answered = true;
            }
            // Out from under the finger: a hop away, and a word.
            if matches!(self.body.motion, Motion::Idle | Motion::Walk) {
                let (w, s) = (self.body.size.0, self.fun.scale);
                let away = if self.cursor.0 < 0.5 * w { 1.0 } else { -1.0 };
                let facing = self.body.facing;
                self.body
                    .spring(self.body.feet().0 + away * 0.08 * w, 16.0 * s);
                self.body.facing = facing;
            }
            self.anim.poke(zone);
            let mut rng = self.rng.clone();
            let out = self.mind.dislike(&ctx.world, zone, &mut rng);
            self.rng = rng;
            diag!(
                "{}: pulled away from a hand on its {zone:?}",
                self.save.name
            );
            self.apply(ctx, out);
            return;
        }
        if zone == Zone::Belly
            && let Some(after) = self.species.belly_trap()
            && held >= after
        {
            if let Some(p) = &mut self.press {
                p.answered = true;
            }
            self.anim.spring_trap(TRAP_SECS);
            let out = self.mind.trap(&ctx.world);
            diag!("{}: the belly trap", self.save.name);
            self.apply(ctx, out);
            return;
        }
        if fond == Fondness::Loves && held >= LOVE_HEARTS_AFTER && t >= self.love_at {
            self.love_at = t + LOVE_HEARTS_EVERY;
            self.fun
                .effect(chonkgene_render::props::Fx::Hearts, ctx.world.now);
            let sound = match self.species {
                Species::Munchkin | Species::Fold => Some(Sound::Purr),
                Species::Shiba | Species::Fox => Some(Sound::Pant),
                _ => None,
            };
            if let Some(sound) = sound
                && ctx.sound_on
                && t >= self.purr_at
            {
                self.purr_at = t + 2.6;
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
            self.save.calls_you(ctx.everyone_you),
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
        // A spot it loves: hearts whether or not it was happy already.
        if out.bliss {
            self.fun
                .effect(chonkgene_render::props::Fx::Hearts, ctx.world.now);
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
        self.touch_step(ctx);
        self.cradle_step(ctx);
        self.pat_step(ctx);
        if let Some((due, zone)) = self.poke_due
            && now >= due
        {
            self.poke_due = None;
            self.tapped = None;
            self.anim.noticed();
            self.poke_now(ctx, zone);
        }
        if let Some(at) = self.hello_at
            && t >= at
        {
            self.hello_at = None;
            if !self.mind.asleep() {
                self.say(ctx, Line::Hello);
                self.anim.gesture(Act::Wave);
            }
        }
        // Gone to rest in 寵物窩: stops where it is, waves and says so (a
        // sleeping one just goes).
        match self.leave_at {
            Some(_) if !self.said_bye => {
                self.said_bye = true;
                self.hello_at = None;
                if !self.mind.asleep() && self.press.is_none() {
                    self.body.motion = Motion::Idle;
                    self.body.speed = 0.0;
                    self.switch_at = self.switch_at.max(t + LEAVE_SECS + 1.0);
                    self.say(ctx, Line::ToDen);
                    self.anim.gesture(Act::Wave);
                }
            }
            None => self.said_bye = false,
            _ => {}
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
            // Falling from high up, it may spread out and glide down
            // instead (滑行 through the air), once per fall.
            if self.body.motion == Motion::Fall {
                if !self.glide_checked && self.body.vel.1 > 0.0 && self.press.is_none() {
                    self.glide_checked = true;
                    let high = 160.0 * self.fun.scale;
                    if self.body.drop_below(ctx.terrain).is_some_and(|d| d > high)
                        && self.rng.f32() < glide_chance(self.species)
                    {
                        self.body.glide();
                        diag!("{}: glides down", self.save.name);
                    }
                }
            } else {
                self.glide_checked = false;
            }
            let busy = self.fun.busy(t);
            let free = !self.mind.asleep() && self.act.is_none() && !self.in_skit && !busy;
            if !self.in_skit
                && self.act.is_none()
                && !busy
                && t >= self.dash_until
                && self.body.pace != 1.0
            {
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
                let energy = self.mind.personality.energy;
                let flight = (self.body.motion == Motion::Idle
                    && above.is_none()
                    && self.rng.f32() < fly_chance(self.species))
                .then(|| self.flight_spot(ctx.terrain))
                .flatten();
                if let Some((ledge, x)) = above {
                    self.body.leap(x, ledge.y);
                    self.anim.land(500.0);
                } else if let Some((x, y)) = flight {
                    // Off through the air instead (Roger's eighteenth
                    // pass: everyone flies, the four-legged ones under a
                    // balloon or a bamboo-copter).
                    diag!("{}: flies to ({x:.0}, {y:.0})", self.save.name);
                    self.body.fly_to(x, y);
                } else if self.body.motion == Motion::Idle {
                    // Off for a walk, either way. (Not on stopping: it
                    // slows down going the way it went.) Sometimes a run.
                    self.body.motion = Motion::Walk;
                    if self.rng.bool() {
                        self.body.facing = -self.body.facing;
                    }
                    // On the sand it runs about more (Roger's eighteenth
                    // pass: a beach to walk and run on).
                    let run = if self.fun.on_sand {
                        0.7
                    } else {
                        0.15 + 0.35 * energy
                    };
                    if move_test() || self.rng.f32() < run {
                        self.body.pace = RUN_PACE;
                        diag!("{}: runs", self.save.name);
                    }
                } else if self.body.pace >= RUN_PACE - 0.1
                    && (move_test() || self.rng.f32() < SLIDE_CHANCE)
                {
                    // At the end of a run, a belly slide (滑行).
                    let speed = chonkgene_core::motion::WALK_SPEED * self.body.pace * 1.2;
                    self.body.slide(speed);
                    let fx = if self.fun.on_sand { Fx::Sand } else { Fx::Dust };
                    self.fun.effect(fx, t);
                    self.dash_until = 0.0;
                    diag!("{}: slides", self.save.name);
                } else {
                    self.body.motion = Motion::Idle;
                    self.dash_until = 0.0;
                    // Stopping on the sand, a four-legged one may dig in it.
                    let digs =
                        !matches!(self.species, Species::Owl | Species::Chick | Species::Duck);
                    if self.fun.on_sand && digs && self.rng.f32() < 0.3 {
                        self.anim.walking = false;
                        self.anim.gesture(Act::Dig);
                        self.fun.effect(Fx::Sand, t);
                    }
                }
                // Walks of 3–7 s; rests a little longer (4–11 s), long
                // enough to sit down and look about.
                self.switch_at = t + if self.body.motion == Motion::Walk {
                    3.0 + f64::from(self.rng.f32()) * 4.0
                } else {
                    4.0 + f64::from(self.rng.f32()) * 7.0
                };
                // A run lasts the walk (and a moment to choose a slide).
                if self.body.motion == Motion::Walk && self.body.pace == RUN_PACE {
                    self.dash_until = self.switch_at + 0.5;
                }
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

        // In a pond it stands in the water, further in than its feet; on
        // a dune, up on the sand.
        let placed = (
            self.body.pos.0.round() as i32,
            (self.body.pos.1 - self.fun.pond_lift() - self.fun.hug_lift(t) - self.fun.dune_lift)
                .round() as i32,
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
        // Your hands in a hug, or squeezing its cheeks: found on its last
        // picture, pressing in on this one, drawn over it (`compose`).
        let hug = self
            .fun
            .hug_by_you
            .map(|(since, hold)| (hold, (t - since) as f32, hold.secs()));
        let squeeze = params.touch.squeeze;
        self.fun.hands = if squeeze > 0.0 {
            self.renderer.cheek_hands(squeeze, self.fun.scale)
        } else {
            self.renderer
                .hug_hands(hug, self.anim.facing, self.fun.scale)
        };
        // Your hand patting its head, on the top of its head as last shown.
        self.fun.pat_hand = self.fun.pat_by_you.and_then(|(since, kind)| {
            let pose = props::pat_pose(kind, (t - since) as f32)?;
            Some(props::pat_hand(
                self.renderer.anchor(),
                self.anim.facing,
                self.fun.scale,
                &pose,
            ))
        });
        self.renderer.set_holding(self.fun.carry.is_some());
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
        if crate::diag::dumping()
            && (self.fun.pond.lately(t)
                || self.fun.on_sand
                || self.anim.doing() == Some(Act::Hug)
                || self.fun.pat_by_you.is_some()
                || self.fun.carry.is_some()
                || self.fun.scene.is_some_and(|r| r.scene == Scene::Bone)
                || move_test()
                || morph_test())
        {
            let wading = if self.fun.pond.now.is_some() { "w" } else { "" };
            let what = format_args!("pet{}{wading}", self.save.id);
            crate::diag::dump(what, self.placed, pixmap);
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

    /// Bring its window above every other (see `play::stack_order`).
    fn raise_window(&self, shared: &Shared) {
        if let Err(e) = self.presenter.raise(shared) {
            eprintln!("raise failed: {e}");
        }
    }

    /// Bring its bubble, if it has one, above every other window.
    fn raise_bubble(&self, shared: &Shared) {
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
    /// leap, thrown), gliding down, sliding on its belly, held up by the
    /// spell.
    fn travelling(&self) -> bool {
        self.float.is_some() || moves_itself(self.body.motion)
    }
}

/// A motion that moves the pet's window by itself every frame, so its
/// frames must keep up (full rate, past the GPU budget). Roger,
/// 2026-11-26: thrown up, its glide down was jerky; gliding (and sliding)
/// had been left out and were drawn at the resting rate.
fn moves_itself(motion: Motion) -> bool {
    match motion {
        Motion::Fly | Motion::Fall | Motion::Glide | Motion::Slide => true,
        Motion::Idle | Motion::Walk | Motion::Held => false,
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
        self.make_tray();
        let proxy = std::sync::Arc::new(std::sync::Mutex::new(self.proxy.clone()));
        let send = move |c: TrayCommand| {
            if let Ok(p) = proxy.lock() {
                let _ = p.send_event(UserEvent::Tray(c));
            }
        };
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
                        diag!("menu: {action:?} (row {row}) for pet {pet}");
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
                        // The second of two taps: what the spot the first
                        // one found does; that one's poke, still waiting,
                        // is dropped.
                        let second = p
                            .tapped
                            .take()
                            .filter(|&(at, _)| now - at <= chonkgene_platform::double_click_time())
                            .and_then(|(_, zone)| {
                                tap_action(p.save.human, zone).map(|a| (zone, a))
                            });
                        if let Some((zone, action)) = second {
                            p.poke_due = None;
                            p.anim.untap();
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
                                everyone_you: &settings.you,
                                volume: f32::from(settings.volume) / 100.0,
                                area: *area,
                                terrain,
                                ui_scale: *ui_scale,
                                world,
                            };
                            let p = &mut pets[i];
                            p.tap(&mut ctx, zone, action);
                            p.next = now;
                            self.dirty = true;
                            return;
                        }
                        p.window.set_cursor(hand(Hand::Poke));
                        p.press = Some(Press {
                            grab: p.cursor,
                            moved: false,
                            last: now,
                            answered: false,
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
                        p.anim.fond = Some(p.species.fondness(zone));
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
                                everyone_you: &settings.you,
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
                            // Held a while anywhere it does not mind, it was
                            // a rub or a scratch (a pat), not a poke; how
                            // pleased it is goes by the spot.
                            let rub = p.anim.held_secs() >= HOLD_SECS
                                && p.species.fondness(zone) != Fondness::Dislikes;
                            // A quick poke: a start, hopping back from
                            // the finger (still facing the same way).
                            let quick =
                                p.anim.held_secs() < 0.3 && p.anim.params().touch.scratch < 0.2;
                            let held = p.anim.held_secs();
                            // Held still on a cheek, your hands came to
                            // squeeze it: let go, that is its answer.
                            let squeezed = p.anim.params().touch.squeeze > 0.0;
                            let action = tap_action(p.save.human, zone).filter(|_| quick && !rub);
                            let mut ctx = Ctx {
                                el,
                                shared,
                                lines,
                                face: bubble_face,
                                sounds,
                                sound_on: settings.sound,
                                everyone_you: &settings.you,
                                volume: f32::from(settings.volume) / 100.0,
                                area: *area,
                                terrain,
                                ui_scale: *ui_scale,
                                world,
                            };
                            match action {
                                // One tap does it.
                                Some(action) if settings.taps == 1 => {
                                    p.anim.tap(zone);
                                    p.anim.untap();
                                    p.tap(&mut ctx, zone, action);
                                    p.next = now;
                                    self.dirty = true;
                                    return;
                                }
                                // The first of two: a small poke now; what it
                                // does as a poke waits a moment, so the pet is
                                // still there for the second (Roger's
                                // eighteenth pass: the first click sent it
                                // hopping away).
                                Some(_) => {
                                    p.anim.tap(zone);
                                    p.tapped = Some((now, zone));
                                    p.poke_due =
                                        Some((now + chonkgene_platform::double_click_time(), zone));
                                    p.next = now;
                                    self.dirty = true;
                                    return;
                                }
                                // Nothing to do there: a poke, at once.
                                None if quick && !rub => {
                                    p.anim.poke(zone);
                                    p.poke_now(&mut ctx, zone);
                                    p.next = now;
                                    self.dirty = true;
                                    return;
                                }
                                None => p.anim.poke(zone),
                            }
                            if squeezed {
                                p.cheeks_let_go(&mut ctx);
                                p.next = now;
                                self.dirty = true;
                                return;
                            }
                            let out = if rub {
                                p.mind.pat(&world, zone)
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
                    if crate::diag::dumping() && p.fun.pond.now.is_some() {
                        crate::diag::dump(format_args!("pet{}redraw", p.save.id), p.placed, shown);
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

        // 換班: those saying goodbye go when it is time; 隨機輪班.
        let t = self.secs(now);
        if self
            .pets
            .iter()
            .any(|p| p.leave_at.is_some_and(|at| t >= at))
        {
            self.sync_desktop(el);
        }
        self.rotate_step(el, t);
        self.morph_step(el, t);
        self.called_step(el, now, t);

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
            everyone_you: &settings.you,
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
            } else if self.stage_test == "hug-asleep" {
                // Put to sleep, then hugged as 抱抱 would.
                self.act(el, pet, Action::Sleep);
                self.stage_test = "hug".into();
                self.stage_at = Some(t + 3.0);
                diag!("CHONKGENE_STAGE=hug-asleep: put the first pet to sleep");
            } else if self.stage_test == "hug" {
                self.act(el, pet, Action::Hug);
                diag!("CHONKGENE_STAGE=hug: hugged the first pet");
            } else if self.stage_test == "pat" {
                self.act(el, pet, Action::Pat);
                diag!("CHONKGENE_STAGE=pat: patted the first pet on the head");
            } else if let Some(hold) = match self.stage_test.as_str() {
                "wrap" => Some(HugHold::Wrap),
                "carry" => Some(HugHold::Carry),
                "cradle" => Some(HugHold::Cradle),
                _ => None,
            } {
                self.act(el, pet, Action::HugAs(hold));
                diag!("CHONKGENE_STAGE={}: held the first pet so", self.stage_test);
            } else if self.stage_test.starts_with("tray-pond") {
                // The tray's 場景 › 玩水池, then again 8 s later: put away.
                self.tray_scene(el, Scene::Pond);
                diag!("CHONKGENE_STAGE={}: the tray's 玩水池", self.stage_test);
                if self.stage_test == "tray-pond" {
                    self.stage_test = "tray-pond-again".into();
                    self.stage_at = Some(t + 8.0);
                }
            } else if self.stage_test.starts_with("morph") {
                let fox = self.pets.iter().find(|p| p.species == Species::Fox);
                if let Some(fox) = fox.map(|p| p.save.id) {
                    self.act(el, fox, Action::Morph);
                    diag!("CHONKGENE_STAGE={}: the fox's 變身", self.stage_test);
                }
                if self.stage_test == "morph" {
                    self.stage_test = "morph-back".into();
                    self.stage_at = Some(t + 10.0);
                }
            } else if self.stage_test == "hug-other" {
                if let Some(other) = self.pets.get(1).map(|p| p.save.id) {
                    self.act(el, pet, Action::HugWith(other));
                    diag!("CHONKGENE_STAGE=hug-other: the first pet hugs the second");
                }
            } else if self.stage_test == "settings-quit" {
                self.act(el, pet, Action::Edit);
                self.stage_test = "quit".into();
                self.stage_at = Some(t + 5.0);
                diag!("CHONKGENE_STAGE=settings-quit: the settings opened");
            } else if self.stage_test == "quit" {
                diag!("CHONKGENE_STAGE=settings-quit: 結束肉肉賴桌");
                self.act(el, pet, Action::Quit);
            } else if self.stage_test == "bone" {
                let shiba = self.pets.iter().find(|p| p.species == Species::Shiba);
                if let Some(shiba) = shiba.map(|p| p.save.id) {
                    self.start_scene(el, shiba, Scene::Bone, world, now);
                    diag!("CHONKGENE_STAGE=bone: the Shiba's bone");
                }
            } else if self.stage_test == "pond" || self.stage_test == "beach" {
                let duck = self.pets.iter().find(|p| p.species == Species::Duck);
                if let Some(duck) = duck.map(|p| p.save.id) {
                    self.start_scene(el, duck, Scene::Pond, world, now);
                    diag!("CHONKGENE_STAGE={}: the duckling's pond", self.stage_test);
                }
                // The beach: the Shiba put on the near slope of a dune,
                // where it buries itself (see `play::beach_test`).
                let beach = self
                    .play
                    .things
                    .iter()
                    .find(|w| w.prop.thing == chonkgene_core::play::Thing::Beach)
                    .map(|w| w.prop);
                let shiba = self.pets.iter_mut().find(|p| p.species == Species::Shiba);
                if let (true, Some(b), Some(p)) = (self.stage_test == "beach", beach, shiba) {
                    let side = if b.pos.0 > self.area.x + self.area.w / 2.0 {
                        -1.0
                    } else {
                        1.0
                    };
                    p.body.pos.0 = b.pos.0 + side * 470.0 * b.scale - p.body.size.0 / 2.0;
                    p.body.motion = Motion::Idle;
                    p.body.speed = 0.0;
                    diag!("CHONKGENE_STAGE=beach: the Shiba on a dune");
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
            // The next pet to try out what 「大家怎麼叫你」 has it call you.
            self.called
                .iter()
                .map(|c| self.start + Duration::from_secs_f64(c.0.max(0.0)))
                .min(),
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
        self.close_settings();
        self.save_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Thrown up, falling, gliding down or sliding on its belly, its window
    /// moves by itself and its frames keep up (Roger, 2026-11-26: the glide
    /// down was jerky); walking, idle or in your hand, the rest decides.
    #[test]
    fn a_window_moving_by_itself_gets_full_frames() {
        for m in [Motion::Fly, Motion::Fall, Motion::Glide, Motion::Slide] {
            assert!(moves_itself(m), "{m:?}");
        }
        for m in [Motion::Idle, Motion::Walk, Motion::Held] {
            assert!(!moves_itself(m), "{m:?}");
        }
    }

    /// Roger's twenty-seventh pass, item 2: each spot's action, the human
    /// form's on her head, hands and shoulders only; belly, cheeks, chin,
    /// tail (and a human form's legs and back) a poke.
    #[test]
    fn a_tap_does_what_the_spot_does() {
        use TapAction::*;
        for (zone, beast, human) in [
            (Zone::Head, Some(Pat), Some(Pat)),
            (Zone::Paw, Some(Paw), Some(Paw)),
            (Zone::Wing, Some(Paw), Some(Paw)),
            (Zone::Foot, Some(Hug(HugHold::Carry)), None),
            (
                Zone::Shoulder,
                Some(Hug(HugHold::Wrap)),
                Some(Hug(HugHold::Wrap)),
            ),
            (Zone::Back, Some(Hug(HugHold::Cradle)), None),
            (Zone::Bottom, Some(PatBottom), None),
            (Zone::Belly, None, None),
            (Zone::Cheek, None, None),
            (Zone::Chin, None, None),
            (Zone::Tail, None, None),
        ] {
            assert_eq!(tap_action(false, zone), beast, "{zone:?}");
            assert_eq!(tap_action(true, zone), human, "{zone:?} in human form");
        }
    }

    #[test]
    fn a_shared_name_is_tried_out_by_the_pets_that_take_it() {
        let mut rng = fastrand::Rng::with_seed(3);
        let shiba = PetSave::adopt(1, Species::Shiba, "red", &mut rng);
        let mut owl = PetSave::adopt(2, Species::Owl, "brown", &mut rng);
        owl.you = "學長".into();
        let fold = PetSave::adopt(3, Species::Fold, "grey", &mut rng);
        let saves = [shiba.clone(), owl, fold];
        // Set: the two untouched take it, the owl keeps 「學長」.
        assert_eq!(told_anew(saves.iter(), "", "Roger"), vec![1, 3]);
        // Changed: the same two.
        assert_eq!(told_anew(saves.iter(), "Roger", "阿傑"), vec![1, 3]);
        // Unset: back to their defaults, so they say so.
        assert_eq!(told_anew(saves.iter(), "阿傑", ""), vec![1, 3]);
        // Set to a pet's own default: it calls you the same as before.
        assert_eq!(told_anew(saves.iter(), "", &shiba.you), vec![3]);
        assert!(told_anew(saves.iter(), "Roger", "Roger").is_empty());
    }

    fn facts(asleep: bool, fox: bool, scenes: SceneRows) -> MenuFacts {
        MenuFacts {
            asleep,
            snacks: 2,
            fox,
            morph: None,
            others: vec![(2, "芋圓".into())],
            toy: false,
            catnip: false,
            stage: false,
            scenes,
        }
    }

    /// The Shiba's scenes, free: its bone, then the shared pond and sun.
    fn free() -> SceneRows {
        SceneRows::Free(vec![
            (Scene::Bone, false),
            (Scene::Pond, false),
            (Scene::Sunbeam, false),
        ])
    }

    /// Draws the menu's pages as `menu_page` builds them, without a
    /// display: `cargo test -p chonkgene -- --ignored menu_screenshots`
    /// writes target/screens/menu-*.png.
    #[test]
    #[ignore = "writes screenshots; run by hand"]
    fn menu_screenshots() {
        let shiba = (
            "阿柴",
            "柴犬",
            ([0xE8, 0x95, 0x5A, 255], [0xFF, 0xF3, 0xE2, 255]),
        );
        let fox = (
            "藻藻",
            "九尾狐",
            ([0xEF, 0xE2, 0xD2, 255], [0xFF, 0xFF, 0xFF, 255]),
        );
        let pages = [
            ("main", shiba, Page::Main, facts(false, false, free())),
            (
                "main-asleep",
                shiba,
                Page::Main,
                facts(true, false, SceneRows::Asleep),
            ),
            (
                "commands-fox",
                fox,
                Page::Commands,
                MenuFacts {
                    morph: Some(false),
                    ..facts(false, true, free())
                },
            ),
            ("toys", shiba, Page::Toys, facts(false, false, free())),
            (
                "toys-asleep",
                shiba,
                Page::Toys,
                facts(true, false, SceneRows::Asleep),
            ),
        ];
        let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/screens");
        std::fs::create_dir_all(&out).unwrap();
        let mut face = Face::ui();
        for (name, (pet, species, avatar), page, f) in pages {
            let model = menu::Model {
                name: pet.into(),
                species: species.into(),
                level: 3,
                progress: 0.6,
                status: f.asleep.then(|| tr("睡覺中").to_string()),
                grumpy: false,
                avatar,
                items: menu_page(page, &f).0,
            };
            let drawn = menu::draw(&mut face, &model, None, 1.25).unwrap();
            drawn
                .pixmap
                .save_png(out.join(format!("menu-{name}.png")))
                .unwrap();
        }
    }

    /// The rows with a line (or a caption) drawn above them.
    fn breaks(items: &[Item]) -> Vec<usize> {
        (0..items.len())
            .filter(|&k| items[k].separated || items[k].caption.is_some())
            .collect()
    }

    /// Roger's twenty-sixth pass, item 1 (「右鍵選單太亂」): the first page
    /// is three short groups, and what is set once and kept (the name, the
    /// look, goodbye, everyone's size) is left to the settings window,
    /// which 設定… opens on this pet.
    #[test]
    fn the_first_page_is_three_short_groups_and_settings_open_on_the_pet() {
        let (items, actions) = menu_page(Page::Main, &facts(false, false, free()));
        assert_eq!(
            actions,
            [
                Action::HugHow,
                Action::Pat,
                Action::Feed,
                Action::Sleep,
                Action::Commands,
                Action::Toys,
                Action::Edit,
                Action::HideAll,
                Action::Quit,
            ]
        );
        assert_eq!(breaks(&items), [4, 6]);
        assert!(
            items[6].hint.is_some(),
            "設定… says the name and look are there"
        );
        // Asleep: nothing to ask it, woken gently, the groups kept.
        let (items, actions) = menu_page(Page::Main, &facts(true, false, SceneRows::Asleep));
        assert_eq!(
            actions,
            [
                Action::Hug,
                Action::Pat,
                Action::Feed,
                Action::Wake,
                Action::Toys,
                Action::Edit,
                Action::HideAll,
                Action::Quit,
            ]
        );
        assert_eq!(breaks(&items), [4, 5]);
    }

    /// Where she can change form (the fox, on a real GPU), 變身 ends her
    /// spells, saying which way it goes; elsewhere it is not there.
    #[test]
    fn the_fox_changes_form_from_her_spells() {
        let morph = |f: &MenuFacts| {
            let (items, actions) = menu_page(Page::Commands, f);
            actions
                .iter()
                .position(|a| *a == Action::Morph)
                .map(|i| (i, items[i].hint.clone(), actions.len()))
        };
        assert_eq!(morph(&facts(false, true, free())), None);
        let beast = MenuFacts {
            morph: Some(false),
            ..facts(false, true, free())
        };
        let (at, hint, len) = morph(&beast).unwrap();
        assert_eq!(at, len - 2, "last of the spells, before 返回");
        assert_eq!(hint.as_deref(), Some(tr("變成人型")));
        let human = MenuFacts {
            morph: Some(true),
            ..facts(false, true, free())
        };
        assert_eq!(morph(&human).unwrap().1.as_deref(), Some(tr("變回原型")));
    }

    /// The fox's spells are a captioned section of 叫牠…, not a page.
    #[test]
    fn the_foxs_spells_are_a_section_of_what_you_can_ask() {
        let magic = |a: &&Action| matches!(a, Action::Magic(_));
        let (_, actions) = menu_page(Page::Commands, &facts(false, false, free()));
        assert!(!actions.iter().any(|a| magic(&a)));
        assert!(actions.contains(&Action::HugWho));
        assert_eq!(actions.last(), Some(&Action::Back));
        let (items, actions) = menu_page(Page::Commands, &facts(false, true, free()));
        let first = actions.iter().position(|a| magic(&a)).unwrap();
        assert_eq!(actions.iter().filter(magic).count(), 4);
        assert!(items[first].caption.is_some());
        assert_eq!(
            first,
            actions.len() - 5,
            "the spells end the page, before 返回"
        );
        // Alone on the desktop: nobody to go and hug.
        let alone = MenuFacts {
            others: Vec::new(),
            ..facts(false, false, free())
        };
        assert!(
            !menu_page(Page::Commands, &alone)
                .1
                .contains(&Action::HugWho)
        );
    }

    /// 一起玩… holds hide-and-seek, its scenes (captioned 場景) and the
    /// things for them all (captioned 大家一起玩); a scene it cannot have
    /// up on a window says so, the fox's 狐火 is only a spell, and asleep
    /// the scenes say why not while the toys are still there.
    #[test]
    fn playing_holds_hide_and_seek_its_scenes_and_the_toys() {
        let toys = [
            Action::Toy,
            Action::Throw(Thing::Ball),
            Action::Throw(Thing::Frisbee),
            Action::Catnip,
            Action::Stage,
            Action::Back,
        ];
        let fox = SceneRows::Free(vec![
            (Scene::Foxfire, false),
            (Scene::Pond, true),
            (Scene::Sunbeam, false),
        ]);
        let (items, actions) = menu_page(Page::Toys, &facts(false, true, fox));
        let mut want = vec![
            Action::Hide,
            Action::Toys,
            Action::CallScene(Scene::Sunbeam),
        ];
        want.extend(toys);
        assert_eq!(actions, want);
        assert!(items[1].caption.is_some() && items[3].caption.is_some());
        assert!(items[1].hint.is_some(), "the pond: up on a window");
        assert_eq!(breaks(&items), [1, 3, 8]);

        let (items, actions) = menu_page(Page::Toys, &facts(true, false, SceneRows::Asleep));
        assert_eq!(actions[0], Action::Toys, "asleep: why not, no 躲貓貓");
        assert!(items[0].caption.is_some());
        assert_eq!(actions[1..], toys);

        let in_one = facts(false, false, SceneRows::In(Scene::Bone, false));
        let (_, actions) = menu_page(Page::Toys, &in_one);
        assert_eq!(actions[..2], [Action::Hide, Action::EndScene]);
    }

    #[test]
    fn the_trays_scenes_read_as_the_pet_menus_and_cover_every_species() {
        for (label, scene) in chonkgene_platform::tray::SCENES.iter().zip(TRAY_SCENES) {
            assert_eq!(*label, scene_label(scene));
        }
        for s in Species::ALL {
            for scene in s.menu_scenes() {
                assert!(TRAY_SCENES.contains(&scene), "{s:?} {scene:?}");
            }
        }
        // A label fits its row beside the icon (at most nine characters).
        for scene in TRAY_SCENES {
            assert!(
                trf("把{thing}收起來", &[("thing", scene_thing(scene))])
                    .chars()
                    .count()
                    <= 9
            );
        }
    }

    #[test]
    fn the_tray_puts_a_scene_away_or_asks_the_free_pet_nearest_the_middle() {
        let pets = [
            (1, Species::Duck, 100.0, None, true),
            (2, Species::Shiba, 900.0, None, true),
            (3, Species::Munchkin, 1000.0, None, false),
            (4, Species::Fold, 1500.0, None, true),
        ];
        // Anyone for a pond: the free one nearest the middle.
        assert_eq!(tray_pick(&pets, Scene::Pond, 1000.0), Some((2, false)));
        // Only cats chase butterflies; the busy one is passed over.
        assert_eq!(tray_pick(&pets, Scene::Butterfly, 1000.0), Some((4, false)));
        // Nobody perches but the owl.
        assert_eq!(tray_pick(&pets, Scene::Perch, 1000.0), None);
        // One already in it: put away.
        let mut out = pets;
        out[0].3 = Some(Scene::Pond);
        assert_eq!(tray_pick(&out, Scene::Pond, 1000.0), Some((1, true)));
    }

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
