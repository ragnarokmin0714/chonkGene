//! Procedural animation (PLAN §3.3): breathing, blinking, the belly spring,
//! the walk cycle, turning round, hops, idle gestures and tail wags, reduced
//! to a few numbers every renderer understands.

use crate::Zone;
use crate::species::Fondness;
use crate::spring::Spring;

const BREATH_PERIOD: f32 = 3.2;
const BLINK_SECS: f32 = 0.12;
/// A cat's slow blink (a sign of trust): the lids come down and up slowly.
const SLOW_BLINK_SECS: f32 = 0.7;
/// A double blink follows the first after this long.
const DOUBLE_BLINK_GAP: f32 = 0.16;
const GIGGLE_EVERY: f32 = 0.22;
/// Steps a second at walking pace.
const STEP_HZ: f32 = 2.2;
/// Turning round: long enough to see a 3D turn through facing you (a
/// squash-flip when frames are pre-rendered sprites).
const TURN_SECS: f32 = 0.3;
/// Hops fall at this many design pixels/s².
const HOP_GRAVITY: f32 = 1800.0;
/// How far a held finger squashes the body, in spring units.
const PRESS_SQUASH: f32 = 0.28;
/// A steady walk shows this many distinct frames a step, so a renderer can
/// draw each once and copy it after (PLAN §5).
pub const BEATS: u8 = 12;

/// How the face and tail read. Placeholder for the keyframed poses (belly-up
/// laughing, playing dead, …) that need real art (PLAN §3.3, §10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expression {
    Neutral,
    /// Squinting, bouncing with giggles, tail going wild.
    Giggle,
    /// Half-lidded, limp tail: 厭世.
    FedUp,
    /// Eyes shut, tail down: the overdone finale.
    Sulk,
    /// Content squint after making up or a head pat.
    Happy,
    /// Eyes shut, tail still, slow breathing; the renderer adds the snot bubble.
    Asleep,
}

/// What the body is doing besides its expression: a walking step or an idle
/// gesture. Each has its own pose in the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Rest,
    /// Walking: one diagonal pair of legs forward…
    StepA,
    /// …then the other.
    StepB,
    LookLeft,
    LookRight,
    /// A play bow.
    Stretch,
    Yawn,
    Wave,
    /// Eating a snack: a few bites and some chewing.
    Eat,
    /// Digging a hole: the front paws scoop at the floor in turn.
    Dig,
    /// Scratching an itch behind the ear with a hind paw, sitting.
    Scratch,
    /// A cat washing: licking a front paw and wiping it over the face.
    Groom,
    /// A shake from nose to tail, as after a bath.
    Shake,
    /// A bird preening: the head turns back to nibble at a wing.
    Preen,
    /// A bird pecking about at the floor.
    Peck,
    /// A bird flapping its wings.
    Flap,
    /// Pleased: the bottom wiggling side to side, the tail going (a bird:
    /// a little dance from foot to foot).
    Wiggle,
    /// Pleased: two little hops on the spot.
    Bounce,
    /// A turn round on the spot (pleased, or asked to: 轉圈).
    Spin,
    /// A cat kneading: sitting, the front paws treading in turn.
    Knead,
    /// 握手: a front paw held out to you, shaking (a bird: a wing).
    Paw,
    /// Typing along with you: the front paws tapping quickly.
    Tap,
    /// The head tipped right over to one side, listening (a fast burst of
    /// typing).
    Tilt,
    /// Nose down at the floor, sniffing.
    Sniff,
    /// A cheek rubbed along the floor (catnip).
    Rub,
    /// Rolled onto its back, paws kicking, eyes glazed (catnip).
    Bliss,
    /// Lying down, gnawing at a bone held in the front paws.
    Chew,
    /// The fox's tails swung round hard at someone (a play-fight).
    Swish,
    /// Nose down to a ball and a bump that sends it rolling.
    Nudge,
    /// 肉肉舞 in a spotlight (see [`dance_at`]).
    Dance,
    /// A hug (PLAN §8, Roger's sixteenth pass): how depends on its
    /// [`Hug`] role.
    Hug,
}

/// Its part in a hug (see [`Act::Hug`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hug {
    /// Face to face with another, cheek to cheek, swaying together; the
    /// one in front (`front`) has a paw or wing over the other's shoulder.
    Cuddle { front: bool },
    /// The fox, up on its hind legs, front paws round the other (Roger:
    /// 霸道姊姊, a bear hug whether it wants one or not).
    Bear,
    /// Held in the fox's bear hug: squeezed, a small one lifted off its
    /// feet, legs dangling.
    Squeezed,
    /// Hugged by you, held one of three ways (your hands only, Roger's
    /// twentieth and twenty-first passes).
    ByYou(HugHold),
}

impl Default for Hug {
    fn default() -> Self {
        Self::ByYou(HugHold::Wrap)
    }
}

impl Hug {
    /// How long the hug lasts, seconds: rocked like a baby, long enough to
    /// grow sleepy.
    pub fn secs(self) -> f32 {
        match self {
            Self::ByYou(HugHold::Cradle) => CRADLE_SECS,
            _ => HUG_SECS,
        }
    }
}

/// How you hold a pet you hug (PLAN §8, Roger's twenty-first pass).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HugHold {
    /// 環抱: from behind, as on your lap facing out, your hands in front
    /// of it (crossed on a dog's chest or a cat's belly, over a bird's
    /// folded wings).
    #[default]
    Wrap,
    /// 舉高高 (前抱): lifted up in front of you, thumbs on its chest and
    /// fingers round behind it (a cat with one hand under its bottom, a
    /// bird cupped from below).
    Carry,
    /// 搖搖抱: cradled on its back like a baby, one hand under its head
    /// and shoulders, the other under its bottom, rocked until its eyes
    /// close (the fox sits sideways for her tails; a bird is cupped).
    Cradle,
}

impl HugHold {
    pub const ALL: [HugHold; 3] = [Self::Wrap, Self::Carry, Self::Cradle];

    /// How long you hold it so, seconds.
    pub fn secs(self) -> f32 {
        Hug::ByYou(self).secs()
    }
}

/// How it is when you double-click it to hug, for which hold (see
/// [`HugHold::pick`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HugMood {
    /// Its sleeping hours, or late at night.
    pub sleepy: bool,
    /// Worn out from running about or playing.
    pub tired: bool,
    /// Just pleased (a pat, a snack, a game).
    pub pleased: bool,
    /// How lively it is by nature, 0 … 1.
    pub energy: f32,
}

impl HugHold {
    /// The hold a double-click picks, from how it is (Roger's twenty-first
    /// pass: so a double-click differs from time to time, for a reason one
    /// can see): sleepy or tired, rocked like a baby; pleased or full of
    /// beans, lifted up high; otherwise wrapped round. Weighted by the
    /// species: most cats do not like being lifted. `roll` is 0 … 1.
    pub fn pick(species: crate::species::Species, mood: HugMood, roll: f32) -> HugHold {
        use crate::species::Species;
        let mut wrap = 1.0;
        let mut carry = 0.35;
        let mut cradle = 0.15;
        if mood.sleepy || mood.tired {
            cradle += 3.0;
        }
        if mood.pleased {
            carry += 1.5;
        }
        if mood.energy > 0.65 {
            carry += 0.8;
        }
        if matches!(species, Species::Munchkin | Species::Fold) {
            carry *= 0.2;
            wrap += 0.3;
        }
        let roll = roll.clamp(0.0, 0.9999) * (wrap + carry + cradle);
        if roll < wrap {
            HugHold::Wrap
        } else if roll < wrap + carry {
            HugHold::Carry
        } else {
            HugHold::Cradle
        }
    }
}

/// How long a hug lasts, seconds.
pub const HUG_SECS: f32 = 3.2;
/// How long you rock it in a 搖搖抱, seconds.
pub const CRADLE_SECS: f32 = 9.0;

/// How open its eyes are `t` seconds into a 搖搖抱 `secs` long: open at
/// first, then closing by degrees as it is rocked, shut well before the
/// end.
pub fn cradle_eyes(t: f32, secs: f32) -> f32 {
    let u = ((t - 1.5) / (0.7 * secs - 1.5)).clamp(0.0, 1.0);
    1.0 - 0.94 * u * u * (3.0 - 2.0 * u)
}

/// How long the 肉肉舞 lasts, seconds, and its beat (120 a minute).
pub const DANCE_SECS: f32 = 7.0;
pub const DANCE_BEAT: f32 = 0.5;

/// A part of the 肉肉舞 (Roger's fifteenth pass, item 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DanceMove {
    /// Bouncing to the beat, the belly jiggling.
    Bounce,
    /// The bottom wiggling side to side.
    Wiggle,
    /// Front paws (wings) waving in turn; a cat kneads the air.
    Paws,
    /// A turn right round.
    Spin,
    /// 「登登！」: a paw (a wing) up high, held.
    Pose,
}

/// Where the 肉肉舞 is `t` seconds in: the move, how far into it (0 … 1),
/// and where in the beat (0 … 1, the beat falling at 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DanceStep {
    pub step: DanceMove,
    pub u: f32,
    pub beat: f32,
    /// Which beat of the dance this is (0 …), for moves done in turn.
    pub count: u32,
}

/// The 肉肉舞's moves, each to the beat: four beats bouncing, three
/// wiggling, three waving, a two-beat spin, two beats of the pose.
pub fn dance_at(t: f32) -> DanceStep {
    const MOVES: [(DanceMove, f32); 5] = [
        (DanceMove::Bounce, 4.0),
        (DanceMove::Wiggle, 3.0),
        (DanceMove::Paws, 3.0),
        (DanceMove::Spin, 2.0),
        (DanceMove::Pose, 2.0),
    ];
    let beats = (t / DANCE_BEAT).max(0.0);
    let mut start = 0.0;
    for (step, len) in MOVES {
        if beats < start + len || step == DanceMove::Pose {
            return DanceStep {
                step,
                u: ((beats - start) / len).clamp(0.0, 1.0),
                beat: beats.fract(),
                count: beats as u32,
            };
        }
        start += len;
    }
    unreachable!("the pose is last")
}

/// How the whole body is held for a while, under any gesture: as it likes,
/// or as it was asked or as a scene has it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Posture {
    #[default]
    Free,
    /// Sitting and staying (坐下).
    Sit,
    /// Lying down awake (趴下, a still mouse, chewing a bone).
    Lie,
    /// Flying: wings beating, legs tucked.
    Fly,
    /// Swimming: low in the water, paddling.
    Swim,
    /// Sliding along on its belly (Roger's eighteenth pass: 滑行).
    Slide,
    /// Gliding down through the air, spread out.
    Glide,
}

impl Act {
    /// Done sitting down, so a sitting pet stays sat (the rest of the
    /// gestures stand it up).
    pub fn seated(self) -> bool {
        matches!(
            self,
            Act::Yawn
                | Act::Scratch
                | Act::Groom
                | Act::Preen
                | Act::Knead
                | Act::Paw
                | Act::Tap
                | Act::Tilt
                | Act::Chew
                | Act::Hug
        )
    }
}

/// The idle gestures a species picks from, with their weights.
pub type Habits = &'static [(Act, u32)];

/// What a pet does when nothing says otherwise: glances, a stretch, a yawn,
/// a wave.
pub const PLAIN_HABITS: Habits = &[
    (Act::LookLeft, 30),
    (Act::LookRight, 30),
    (Act::Stretch, 15),
    (Act::Yawn, 15),
    (Act::Wave, 10),
];

/// What a pleased pet does, smiling at you, when nothing says otherwise: a
/// wiggle, a hop, a wave (Roger: a smiling pet held still too long).
pub const PLAIN_JOYS: Habits = &[(Act::Wiggle, 40), (Act::Bounce, 35), (Act::Wave, 25)];

/// Pleased, the next little thing comes this soon after the last (seconds,
/// and up to as many again more): sooner than at rest, so a smile is never
/// a still picture.
const JOY_GAP: (f32, f32) = (0.5, 0.8);

/// Walking at this pace or faster is running; after this many seconds of
/// it a pet is out of breath, for this long after it stops plus as long as
/// it ran (at most [`RUN_MOST`] seconds counted).
const RUN_PACE: f32 = 1.5;
const PUFF_AFTER: f32 = 1.0;
const PUFF_SECS: f32 = 3.0;
const RUN_MOST: f32 = 6.0;

/// How long a snack takes, and each bite of it.
pub const EAT_SECS: f32 = 3.6;
pub const BITE_SECS: f32 = 1.2;

/// How long a gesture that has a fixed length lasts, seconds: the idle
/// stretch, yawn and wave. None for the rest (looks vary, steps and rest
/// have no length).
pub fn gesture_secs(act: Act) -> Option<f32> {
    match act {
        Act::Stretch => Some(1.9),
        Act::Yawn => Some(1.7),
        Act::Wave => Some(1.6),
        Act::Eat => Some(EAT_SECS),
        Act::Dig => Some(2.6),
        Act::Scratch => Some(2.2),
        Act::Groom => Some(3.0),
        Act::Shake => Some(1.2),
        Act::Preen => Some(2.6),
        Act::Peck => Some(1.8),
        Act::Flap => Some(1.4),
        Act::Wiggle => Some(1.6),
        Act::Bounce => Some(1.3),
        Act::Spin => Some(1.4),
        Act::Knead => Some(2.4),
        Act::Paw => Some(2.4),
        Act::Tap => Some(1.8),
        Act::Tilt => Some(1.1),
        Act::Sniff => Some(1.6),
        Act::Rub => Some(2.4),
        Act::Bliss => Some(4.0),
        Act::Chew => Some(6.0),
        Act::Swish => Some(1.2),
        Act::Nudge => Some(0.8),
        Act::Dance => Some(DANCE_SECS),
        Act::Hug => Some(HUG_SECS),
        _ => None,
    }
}

/// Where the pet is being touched, for renderers that show it (the
/// meshes press in under the finger and react per zone; sprites only
/// squash).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Touch {
    /// A finger is down on this zone, at this pixel of the picture, and has
    /// been this many seconds.
    pub held: Option<(Zone, (f32, f32), f32)>,
    /// The last touch let go: its zone, where, and how many seconds ago.
    pub last: Option<(Zone, (f32, f32), f32)>,
    /// Something it is watching (a toy on the cursor), picture pixels.
    pub watch: Option<(f32, f32)>,
    /// How hard it is being scratched, 0 … 1: the finger rubbing about
    /// while held, fading once it stops.
    pub scratch: f32,
    /// How it likes the spot held, or last touched (see
    /// [`crate::species::Species::fondness`]).
    pub fond: Option<Fondness>,
    /// The 曼赤肯's belly trap: clutching the hand and bunny-kicking, 0 … 1.
    pub trap: f32,
}

impl Touch {
    /// Touched just now or still held: the pet turns to you (PLAN §1.2).
    pub fn attending(&self) -> bool {
        self.held.is_some()
            || self.last.is_some_and(|(.., ago)| ago < ATTEND_SECS)
            || self.watch.is_some()
    }

    /// Where the finger is or last was, or what it watches, picture pixels.
    pub fn at(&self) -> Option<(f32, f32)> {
        self.held.or(self.last).map(|(_, at, _)| at).or(self.watch)
    }
}

/// After a touch, the pet keeps looking at you this long, seconds.
pub const ATTEND_SECS: f32 = 2.5;
/// Held this long without moving away, a press is a rub or a scratch
/// rather than a poke.
pub const HOLD_SECS: f32 = 0.6;
/// A finger rubbing about this far (picture pixels) makes a full scratch;
/// a scratch fades by this rate a second once it stops.
const SCRATCH_PX: f32 = 80.0;
const SCRATCH_FADE: f32 = 1.2;
/// How long anything shows a touch let go (the look back, a paw lifted).
pub const TOUCH_SHOWS_SECS: f32 = 4.0;

/// Inputs a renderer maps onto bones by convention (see chonkgene-render).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseParams {
    /// −1.0 … 1.0 over a breath.
    pub breath: f32,
    /// Whole-body squash: > 0 flattens, < 0 stretches.
    pub squash: f32,
    /// Extra belly jiggle on top of `squash`.
    pub belly: f32,
    /// 1.0 open, 0.0 shut.
    pub eyes_open: f32,
    /// Tail wag away from the pose's own tail, degrees.
    pub tail: f32,
    /// Walking bob and hops, design units (negative is up).
    pub bob: f32,
    /// Lean about the feet, degrees: swinging while held, waddling.
    pub sway: f32,
    /// 1.0 faces right, −1.0 faces left; in between mid-turn, squeezed thin.
    pub facing: f32,
    pub act: Act,
    /// Mid-blink: every face blinks, smiling ones too.
    pub blink: bool,
    /// Set on a steady walk: which of the step's [`BEATS`] this frame is.
    /// Everything else then follows from `act`, `beat` and `facing`, so
    /// frames with the same three look the same.
    pub beat: Option<u8>,
    /// Where the walk is in its two-step cycle, 0.0 … 2.0: step A, then
    /// step B (snapped to the beat on a steady walk); 0.0 standing.
    pub stride: f32,
    /// Seconds on the animation's clock, for motion drawn from it (a waving
    /// paw, blending between poses).
    pub clock: f32,
    /// Walking speed against the usual (zoomies run faster, a skit may
    /// slow it): legs drawn every frame step to match.
    pub pace: f32,
    pub touch: Touch,
    pub posture: Posture,
    /// Pleased, or busy with something fun (a scene, a toy, catnip, a
    /// play-fight): the tail, ears and wings keep going all the while, even
    /// through gestures (Roger: smiles and scenes still looked still).
    pub lively: bool,
    /// Out of breath from a run (a fetch, zoomies, a chase), or still
    /// running: a dog pants with its tongue out.
    pub puffed: bool,
    /// Counts up each time it should lick its nose (a toy just put down):
    /// the renderer licks when it sees a new count.
    pub licks: u32,
    /// Its window is on the move without walking (flying, falling, a leap,
    /// held up by a spell): the picture has to keep up with it every frame
    /// (Roger: the owl's flight up to its perch lagged). Set by the app.
    pub travelling: bool,
    /// The fox casting: its forehead and eyes glow and seal lines grow over
    /// its face, 0 … 1 (Roger's thirteenth pass). Set by the app.
    pub glow: f32,
    /// Its part in a hug under way (see [`Act::Hug`]).
    pub hug: Hug,
}

impl Default for PoseParams {
    fn default() -> Self {
        Self {
            breath: 0.0,
            squash: 0.0,
            belly: 0.0,
            eyes_open: 1.0,
            tail: 0.0,
            bob: 0.0,
            sway: 0.0,
            facing: 1.0,
            act: Act::Rest,
            blink: false,
            beat: None,
            stride: 0.0,
            clock: 0.0,
            pace: 1.0,
            touch: Touch::default(),
            posture: Posture::Free,
            lively: false,
            puffed: false,
            licks: 0,
            travelling: false,
            glow: 0.0,
            hug: Hug::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Animator {
    t: f32,
    body: Spring,
    belly: Spring,
    next_blink: f32,
    /// The last blink: when it began and how long it lasts.
    blink: (f32, f32),
    /// Some blinks are slow (cats, the owl).
    slow_blinks: bool,
    next_giggle: f32,
    rng: fastrand::Rng,
    pub expression: Expression,
    pub walking: bool,
    /// Walking speed multiplier (zoomies run faster).
    pub pace: f32,
    /// How the body is held (asked to sit, lying down, flying, swimming);
    /// lying, flying or swimming it picks no gestures of its own.
    pub posture: Posture,
    /// Busy with something fun (set from outside: a scene, a toy, catnip,
    /// a play-fight); pleased counts too (see [`PoseParams::lively`]).
    pub lively: bool,
    /// Seconds run so far on this run, and until when it is out of breath.
    run: f32,
    puffed_until: f32,
    pub sway: f32,
    /// The way the pet wants to face; the shown facing turns towards it.
    pub facing: f32,
    /// Pop into a new pose to hide the cut: frames cut from pre-rendered
    /// sprites need it; frames drawn on the GPU blend instead.
    pub pops: bool,
    turn: f32,
    /// Species gait, 0.0 (trots) – 1.0 (waddles).
    waddle: f32,
    /// Steps taken on this walk, fractional.
    stride: f32,
    /// Height above the floor (negative is up) and its speed.
    hop: f32,
    hop_vel: f32,
    hops_left: u8,
    pressed: bool,
    /// The finger down: zone, pixel and when it went down; the last touch
    /// let go: zone, pixel and when.
    held: Option<(Zone, (f32, f32), f32)>,
    last_touch: Option<(Zone, (f32, f32), f32)>,
    watch: Option<(f32, f32)>,
    /// See [`Touch::scratch`].
    scratch: f32,
    /// Its part in the hug it is giving or getting (see [`Act::Hug`]); set
    /// by the app with the gesture.
    pub hug: Hug,
    /// See [`Touch::fond`]; set by the app with each press.
    pub fond: Option<Fondness>,
    /// The belly trap springs until then (the animator's clock).
    trap_until: f32,
    /// See [`PoseParams::licks`].
    licks: u32,
    /// The gesture under way, when it started and when it ends.
    gesture: Option<(Act, f32, f32)>,
    next_gesture: f32,
    /// The gestures it picks from at rest, and pleased (see
    /// `Species::joys`).
    habits: Habits,
    joys: Habits,
    /// Pleased last update: turning pleased brings the next gesture close.
    was_happy: bool,
    /// Walking last update: stopping brings the next gesture closer.
    was_walking: bool,
    /// The current or next burst of idle wagging.
    wag: (f32, f32),
    next_huff: f32,
}

impl Animator {
    /// `waddle` is the species gait (see `Species::waddle`).
    pub fn new(seed: u64, chonk: f32, waddle: f32) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed);
        let t = rng.f32() * BREATH_PERIOD;
        let wag_at = t + 3.0 + rng.f32() * 5.0;
        Self {
            t,
            body: Spring::for_chonk(chonk),
            belly: Spring::new(220.0, 7.0),
            next_blink: t + 1.0 + rng.f32() * 3.0,
            blink: (f32::NEG_INFINITY, BLINK_SECS),
            slow_blinks: false,
            next_giggle: 0.0,
            next_gesture: t + 1.5 + rng.f32() * 2.5,
            habits: PLAIN_HABITS,
            joys: PLAIN_JOYS,
            was_happy: false,
            was_walking: false,
            wag: (wag_at, wag_at + 1.5),
            next_huff: 0.0,
            rng,
            expression: Expression::Neutral,
            walking: false,
            pace: 1.0,
            posture: Posture::Free,
            lively: false,
            run: 0.0,
            puffed_until: f32::NEG_INFINITY,
            sway: 0.0,
            facing: 1.0,
            pops: true,
            turn: 1.0,
            waddle: waddle.clamp(0.0, 1.0),
            stride: 0.0,
            hop: 0.0,
            hop_vel: 0.0,
            hops_left: 0,
            pressed: false,
            fond: None,
            hug: Hug::default(),
            trap_until: 0.0,
            held: None,
            last_touch: None,
            scratch: 0.0,
            licks: 0,
            watch: None,
            gesture: None,
        }
    }

    /// The same animator, picking its idle gestures from `habits` (see
    /// `Species::habits`).
    pub fn with_habits(mut self, habits: Habits) -> Self {
        if habits.iter().any(|&(_, w)| w > 0) {
            self.habits = habits;
        }
        self
    }

    /// The same animator, doing `joys` when pleased (see `Species::joys`).
    pub fn with_joys(mut self, joys: Habits) -> Self {
        if joys.iter().any(|&(_, w)| w > 0) {
            self.joys = joys;
        }
        self
    }

    /// The same animator, now and then blinking slowly (see
    /// `Species::slow_blinks`).
    pub fn with_slow_blinks(mut self, slow: bool) -> Self {
        self.slow_blinks = slow;
        self
    }

    /// The same animator, popping into new poses or not (see `pops`).
    pub fn with_pops(mut self, pops: bool) -> Self {
        self.pops = pops;
        self
    }

    pub fn set_chonk(&mut self, chonk: f32) {
        let (pos, vel, target) = (self.body.pos, self.body.vel, self.body.target);
        self.body = Spring::for_chonk(chonk);
        self.body.pos = pos;
        self.body.vel = vel;
        self.body.target = target;
    }

    /// A finger is down on the pet, on `zone` at pixel `at` of its picture:
    /// it squishes in under it, ready to pop back when the finger lets go
    /// ([`Animator::poke`]).
    pub fn press(&mut self, zone: Zone, at: (f32, f32)) {
        self.pressed = true;
        self.held = Some((zone, at, self.t));
        self.body.target = PRESS_SQUASH;
    }

    /// The finger moved while held (kneading, a scratch): where it is now.
    pub fn knead(&mut self, at: (f32, f32)) {
        if let Some(h) = &mut self.held {
            let moved = (at.0 - h.1.0).hypot(at.1 - h.1.1);
            h.1 = at;
            self.scratch = (self.scratch + moved / SCRATCH_PX).min(1.0);
        }
    }

    /// The belly trap: it grabs the hand and bunny-kicks for `secs`.
    pub fn spring_trap(&mut self, secs: f32) {
        self.trap_until = self.t + secs;
    }

    /// How long the finger has been down, seconds (0 if it is not).
    pub fn held_secs(&self) -> f32 {
        self.held.map_or(0.0, |(.., since)| self.t - since)
    }

    /// The press turned into a lift: let go of the squish and stretch.
    pub fn lift(&mut self) {
        self.pressed = false;
        self.held = None;
        self.body.target = 0.0;
        self.body.kick(-3.0);
    }

    /// The finger lets go. A belly held in springs back out by itself; a
    /// quick tap that had no time to squash it kicks it instead.
    pub fn poke(&mut self, zone: Zone) {
        let held = (self.body.pos / PRESS_SQUASH).clamp(0.0, 1.0);
        let at = self.held.take().map_or((0.0, 0.0), |(_, at, _)| at);
        self.last_touch = Some((zone, at, self.t));
        self.pressed = false;
        self.body.target = 0.0;
        let (body, belly) = match zone {
            Zone::Belly => (6.0, 10.0),
            Zone::Bottom => (5.0, 3.0),
            Zone::Cheek | Zone::Head | Zone::Chin => (2.5, 0.0),
            Zone::Back => (4.0, 1.0),
            Zone::Paw | Zone::Tail | Zone::Wing => (1.5, 0.0),
        };
        self.body.kick(body * (1.0 - held));
        self.belly.kick(belly);
        // A poke in the bottom makes it jump.
        if zone == Zone::Bottom {
            self.jump(220.0, 0);
        }
    }

    /// A fright: a start, as at a quick poke on the head, then a look at
    /// `at` (picture pixels: where the fright came from).
    pub fn startle(&mut self, at: (f32, f32)) {
        self.last_touch = Some((Zone::Head, at, self.t));
        self.body.kick(3.0);
    }

    /// Out of breath for `secs` (after a tussle).
    pub fn puff(&mut self, secs: f32) {
        self.puffed_until = self.puffed_until.max(self.t + secs);
    }

    /// A lick of the nose (a toy just put down).
    pub fn lick(&mut self) {
        self.licks = self.licks.wrapping_add(1);
    }

    /// A happy double hop (a pat, a snack).
    pub fn cheer(&mut self) {
        self.jump(250.0, 1);
    }

    /// Thrilled to bits (the fox's fire): three hops, higher than a cheer.
    pub fn leap_for_joy(&mut self) {
        self.jump(320.0, 2);
    }

    /// A big start where it stands (the fox's fire flaring up near by):
    /// twice the jolt of [`Animator::startle`].
    pub fn flinch(&mut self, at: (f32, f32)) {
        self.startle(at);
        self.body.kick(3.0);
    }

    /// A pounce at a toy: one big hop.
    pub fn pounce(&mut self) {
        self.jump(300.0, 0);
    }

    /// Keep an eye on this spot of the picture (a toy), or nothing.
    pub fn watch(&mut self, at: Option<(f32, f32)>) {
        self.watch = at;
    }

    fn jump(&mut self, speed: f32, again: u8) {
        if !self.hopping() {
            self.hop_vel = -speed;
            self.hops_left = again;
        }
    }

    /// In the air in a hop (a cheer, a pounce).
    pub fn hopping(&self) -> bool {
        self.hop < 0.0 || self.hop_vel != 0.0
    }

    fn turning(&self) -> bool {
        self.turn != self.facing.signum()
    }

    /// Eat a snack now: stops whatever gesture was under way. The caller
    /// stops the pet walking first.
    pub fn eat(&mut self) {
        self.walking = false;
        self.start(Act::Eat);
    }

    /// The gesture under way, if any.
    pub fn doing(&self) -> Option<Act> {
        self.gesture.map(|(act, ..)| act)
    }

    /// Start a gesture now if the pet is free for it (a wave hello).
    pub fn gesture(&mut self, act: Act) {
        // Nothing cuts into the 肉肉舞 (a hello wave did, half way).
        if self.free() && self.doing() != Some(Act::Dance) {
            self.start(act);
        }
    }

    /// Hug (or be hugged) as `role` now, whatever it was doing, unless a
    /// finger is down on it: the hug's other half is waiting.
    pub fn hug(&mut self, role: Hug) {
        if self.pressed {
            return;
        }
        self.walking = false;
        self.hug = role;
        self.start(Act::Hug);
    }

    /// Free for an idle gesture: sitting about with nothing else going on.
    fn free(&self) -> bool {
        !self.walking
            && !self.pressed
            && !self.hopping()
            && matches!(self.expression, Expression::Neutral | Expression::Happy)
    }

    fn start(&mut self, act: Act) {
        let secs = match act {
            Act::LookLeft | Act::LookRight => 1.4 + self.rng.f32() * 0.8,
            Act::Hug => self.hug.secs(),
            _ => match gesture_secs(act) {
                Some(secs) => secs,
                None => return,
            },
        };
        self.gesture = Some((act, self.t, self.t + secs));
        if act == Act::Bounce {
            self.jump(170.0, 1);
        }
        // A pop hides the cut to the new pose, where poses cut.
        if self.pops {
            self.body.kick(0.8);
        }
        self.body.target = match act {
            Act::Yawn => -0.14,
            Act::Stretch => 0.06,
            _ => 0.0,
        };
    }

    fn pick_gesture(&mut self) -> Act {
        let list = if self.expression == Expression::Happy {
            self.joys
        } else {
            self.habits
        };
        let total: u32 = list.iter().map(|&(_, w)| w).sum();
        let mut roll = self.rng.u32(0..total.max(1));
        for &(act, w) in list {
            if roll < w {
                return act;
            }
            roll -= w;
        }
        Act::LookLeft
    }

    /// A landing squashes the body in proportion to the impact.
    pub fn land(&mut self, speed: f32) {
        self.body.kick((speed / 120.0).min(12.0));
    }

    pub fn is_settling(&self) -> bool {
        !self.body.is_resting() || !self.belly.is_resting()
    }

    /// Wobbling enough to need every frame; the last of a wobble is a pixel
    /// or so, and half the frames show it as well.
    fn is_wobbling(&self) -> bool {
        self.body.is_wobbling() || self.belly.is_wobbling()
    }

    fn wagging(&self) -> bool {
        self.params().tail != 0.0
    }

    /// Frames per second this pet needs now (PLAN §5).
    pub fn fps(&self, moving: bool) -> u32 {
        let waving = self.gesture.is_some_and(|(act, ..)| {
            !matches!(
                act,
                Act::Stretch | Act::Yawn | Act::LookLeft | Act::LookRight
            )
        });
        // Touched (held, rubbed), its answer shows at full rate (PLAN §5:
        // poked, 60).
        if moving
            || self.pressed
            || self.scratch > 0.0
            || self.is_wobbling()
            || self.expression == Expression::Giggle
            || self.turning()
            || self.hopping()
        {
            60
        } else if self.walking || waving || self.wagging() {
            30
        } else if self.expression == Expression::Asleep {
            5
        } else {
            // Glancing, stretching and yawning hold still poses, and the last
            // of a wobble moves less than a pixel.
            10
        }
    }

    pub fn step(&mut self, dt: f32) {
        let was = self.t;
        self.t += dt;
        // Dancing: a little hop and a jolt of the belly on every beat while
        // it bounces, the belly on the beat all through (肉肉舞).
        if let Some((Act::Dance, since, _)) = self.gesture {
            let (a, b) = (dance_at(was - since), dance_at(self.t - since));
            if b.count != a.count {
                if b.step == DanceMove::Bounce {
                    self.jump(150.0, 0);
                    self.belly.kick(-2.2);
                } else if b.step != DanceMove::Spin {
                    self.belly.kick(-1.4);
                    self.body.kick(0.9);
                }
            }
        }
        self.body.step(dt);
        self.belly.step(dt);
        // A scratch fades once the finger stops, quicker once it lets go.
        let fade = if self.held.is_some() {
            SCRATCH_FADE
        } else {
            3.0 * SCRATCH_FADE
        };
        self.scratch *= (-fade * dt).exp();
        if self.scratch < 0.01 {
            self.scratch = 0.0;
        }
        self.sway *= (1.0 - 6.0 * dt).max(0.0);
        // Settle exactly, so a resting pose stops changing (the renderer caches on it).
        if self.sway.abs() < 0.01 {
            self.sway = 0.0;
        }
        if self.expression == Expression::Giggle && self.t >= self.next_giggle {
            self.body.kick(2.5);
            self.next_giggle = self.t + GIGGLE_EVERY;
        }
        if self.t >= self.next_blink {
            // Now and then a slow blink, or a quick double one.
            let slow = self.slow_blinks && self.rng.f32() < 0.3;
            let len = if slow { SLOW_BLINK_SECS } else { BLINK_SECS };
            self.blink = (self.t, len);
            self.next_blink = if !slow && self.rng.f32() < 0.25 {
                self.t + len + DOUBLE_BLINK_GAP
            } else {
                self.t + 3.5 + self.rng.f32() * 2.0
            };
        }

        // Turning round: squeeze thin, flip, fill out, with a little hop.
        let goal = self.facing.signum();
        if self.turn != goal {
            if self.turn.abs() == 1.0 {
                self.jump(110.0, 0);
            }
            let reach = 2.0 * dt / TURN_SECS;
            self.turn = if (goal - self.turn).abs() <= reach {
                goal
            } else {
                self.turn + reach * goal
            };
        }

        if self.walking {
            self.stride += STEP_HZ * self.pace * dt;
        } else {
            self.stride = 0.0;
        }
        // Running hard puts it out of breath for a while after: longer the
        // longer it ran.
        if self.running() {
            self.run = (self.run + dt).min(RUN_MOST);
        } else if self.run > 0.0 {
            if self.run >= PUFF_AFTER {
                self.puffed_until = self.t + PUFF_SECS + self.run;
            }
            self.run = 0.0;
        }

        if self.hopping() {
            self.hop_vel += HOP_GRAVITY * dt;
            self.hop += self.hop_vel * dt;
            if self.hop >= 0.0 {
                self.body.kick((self.hop_vel / 90.0).min(4.0));
                self.hop = 0.0;
                self.hop_vel = 0.0;
                if self.hops_left > 0 {
                    let again = self.hops_left - 1;
                    self.jump(200.0, again);
                }
            }
        }

        // Idle gestures: now and then, while nothing else is going on. A
        // pet that has just stopped walking soon finds something to do.
        if self.was_walking && !self.walking {
            self.next_gesture = self.next_gesture.min(self.t + 1.2 + self.rng.f32() * 1.3);
        }
        self.was_walking = self.walking;
        // Turning pleased (a pat, a snack, making up), it soon shows it,
        // once the cheer's hops have landed.
        let happy = self.expression == Expression::Happy;
        if happy && !self.was_happy {
            self.next_gesture = self.next_gesture.min(self.t + 0.7 + self.rng.f32() * 0.4);
        }
        self.was_happy = happy;
        if let Some((act, _, until)) = self.gesture {
            // The 肉肉舞 hops on its own beat, and a hug outlasts the happy
            // hops it starts with: only a press or a walk stops them.
            let dancing = matches!(act, Act::Dance | Act::Hug) && !self.walking && !self.pressed;
            if !(self.free() || dancing) || self.t >= until {
                self.gesture = None;
                self.body.target = if self.pressed { PRESS_SQUASH } else { 0.0 };
                // Rests are filled with little things to do (Roger: idling
                // went on too long), a short breath apart.
                self.next_gesture = if happy {
                    self.t + JOY_GAP.0 + self.rng.f32() * JOY_GAP.1
                } else {
                    self.t + 1.5 + self.rng.f32() * 2.5
                };
                // A yawn ends with a little shake of the head.
                if act == Act::Yawn {
                    self.sway = 6.0;
                }
            }
        } else if self.free()
            && matches!(self.posture, Posture::Free | Posture::Sit)
            && self.t >= self.next_gesture
        {
            let act = self.pick_gesture();
            self.start(act);
        }

        // Bursts of wagging at rest; sulking flicks the tail now and then.
        if self.t >= self.wag.1 {
            let (len, gap) = match self.expression {
                Expression::Sulk => (0.7, 2.0 + self.rng.f32() * 3.0),
                _ => (1.2 + self.rng.f32() * 0.8, 4.0 + self.rng.f32() * 7.0),
            };
            let start = self.t + gap;
            self.wag = (start, start + len);
        }
        // Sulking: a huffy 哼 every few seconds.
        if self.expression == Expression::Sulk && self.t >= self.next_huff {
            self.body.kick(-1.4);
            self.next_huff = self.t + 2.5 + self.rng.f32() * 2.0;
        }
    }

    fn running(&self) -> bool {
        self.walking && self.pace >= RUN_PACE
    }

    /// Out of breath, or running long enough to be.
    fn puffed(&self) -> bool {
        (self.running() && self.run >= PUFF_AFTER) || self.t < self.puffed_until
    }

    /// Where the walk is: which step and how far through it (0.0–1.0).
    fn step_phase(&self) -> (bool, f32) {
        let n = self.stride.floor();
        ((n as i64) % 2 == 0, self.stride - n)
    }

    fn tail(&self) -> f32 {
        let tau = std::f32::consts::TAU;
        let wave = |hz: f32, deg: f32| (self.t * tau * hz).sin() * deg;
        let burst = |hz: f32, deg: f32| {
            let (from, to) = self.wag;
            if self.t < from || self.t >= to {
                return 0.0;
            }
            let k = (self.t - from) / (to - from);
            (k * std::f32::consts::PI).sin() * ((self.t - from) * tau * hz).sin() * deg
        };
        match self.expression {
            Expression::Giggle => wave(4.0, 16.0),
            Expression::Happy => wave(3.5, 18.0),
            Expression::FedUp => wave(0.5, 3.0),
            Expression::Sulk => burst(1.4, 10.0),
            Expression::Asleep => 0.0,
            // One wag a step, in time with the feet (see `params`).
            Expression::Neutral if self.walking => {
                let (_, p) = self.step_phase();
                (p * tau).sin() * 14.0
            }
            Expression::Neutral => burst(2.5, 12.0),
        }
    }

    pub fn params(&self) -> PoseParams {
        let tau = std::f32::consts::TAU;
        let pi = std::f32::consts::PI;
        let into = (self.t - self.blink.0) / self.blink.1;
        let blinking = (0.0..1.0).contains(&into);
        // How shut the lids are mid-blink: a slow one passes half shut on
        // its way down and up.
        let lids = if self.blink.1 > BLINK_SECS {
            1.0 - 0.92 * (std::f32::consts::PI * into).sin()
        } else {
            0.12
        };

        let (mut act, mut bob, mut roll, mut footfall) = (Act::Rest, 0.0, 0.0, 0.0);
        // A steady walk with nothing else going on repeats the same few
        // frames: snap it to them.
        let steady = self.walking
            && self.expression == Expression::Neutral
            && !self.is_settling()
            && !self.turning()
            && !self.hopping()
            && self.sway == 0.0;
        let (mut beat, mut stride) = (None, 0.0);
        if self.walking {
            // Each step lifts the body in an arc and rocks it onto the
            // planted foot; it lands with a small squash.
            let (a, mut p) = self.step_phase();
            if steady {
                let n = ((p * f32::from(BEATS)) as u8).min(BEATS - 1);
                p = f32::from(n) / f32::from(BEATS);
                beat = Some(n);
            }
            let side = if a { 1.0 } else { -1.0 };
            act = if a { Act::StepA } else { Act::StepB };
            stride = if a { p } else { 1.0 + p };
            bob = -(pi * p).sin() * (7.0 + 5.0 * self.waddle);
            roll = side * (pi * p).sin() * (3.0 + 6.0 * self.waddle);
            footfall = (0.06 + 0.12 * self.waddle) * (1.0 - p / 0.22).max(0.0).powi(2);
        } else if let Some((g, since, _)) = self.gesture {
            act = g;
            if g == Act::Wave {
                roll = 4.0 * ((self.t - since) * tau * 2.2).sin();
            }
        }
        if self.expression == Expression::Giggle {
            // Rolling about with laughter.
            roll += 6.0 * (self.t * tau * 1.8).sin();
        }
        let tail = match beat {
            Some(n) => (f32::from(n) / f32::from(BEATS) * tau).sin() * 14.0,
            None => self.tail(),
        };
        // Rocked in a 搖搖抱, its eyes close by degrees, whatever its face.
        let cradled = match self.gesture {
            Some((Act::Hug, since, until)) if self.hug == Hug::ByYou(HugHold::Cradle) => {
                Some(cradle_eyes(self.t - since, until - since))
            }
            _ => None,
        };
        let blinking = blinking && cradled.is_none();
        PoseParams {
            breath: if beat.is_some() {
                0.0
            } else if self.expression == Expression::Asleep {
                1.6 * (self.t * tau / (BREATH_PERIOD * 1.6)).sin()
            } else {
                (self.t * tau / BREATH_PERIOD).sin()
            },
            squash: (self.body.pos + footfall).clamp(-0.4, 0.4),
            belly: self.belly.pos.clamp(-0.4, 0.4),
            eyes_open: cradled.unwrap_or(match self.expression {
                Expression::Giggle | Expression::Happy => 0.18,
                Expression::Sulk => 0.12,
                Expression::Asleep => 0.06,
                Expression::FedUp if blinking => lids.min(0.5),
                Expression::FedUp => 0.5,
                Expression::Neutral if blinking => lids,
                Expression::Neutral => 1.0,
            }),
            tail,
            bob: bob + self.hop,
            sway: self.sway + roll,
            facing: self.turn,
            act,
            blink: blinking,
            beat,
            stride,
            clock: self.t,
            pace: self.pace,
            touch: Touch {
                held: self.held.map(|(z, at, since)| (z, at, self.t - since)),
                // Forgotten once nothing shows it any more, so resting
                // frames repeat again.
                last: self
                    .last_touch
                    .map(|(z, at, at_t)| (z, at, self.t - at_t))
                    .filter(|&(.., ago)| ago < TOUCH_SHOWS_SECS),
                watch: self.watch,
                scratch: self.scratch,
                fond: self.fond.filter(|_| {
                    self.held.is_some() || self.last_touch.is_some_and(|l| self.t - l.2 < 1.5)
                }),
                trap: {
                    let left = self.trap_until - self.t;
                    // In fast, out over its last quarter second.
                    (left / 0.25).clamp(0.0, 1.0)
                },
            },
            posture: self.posture,
            lively: match self.expression {
                Expression::Happy => true,
                Expression::Neutral => self.lively,
                _ => false,
            },
            puffed: self.expression != Expression::Asleep && self.puffed(),
            licks: self.licks,
            travelling: false,
            glow: 0.0,
            hug: self.hug,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn run(a: &mut Animator, secs: f32, mut each: impl FnMut(&Animator)) {
        for _ in 0..(secs / DT).round() as usize {
            a.step(DT);
            each(a);
        }
    }

    #[test]
    fn idle_runs_at_ten_fps_and_a_poke_raises_it() {
        let mut a = Animator::new(1, 0.33, 0.35);
        assert_eq!(a.fps(false), 10);
        a.poke(Zone::Belly);
        assert_eq!(a.fps(false), 60);
        run(&mut a, 2.5, |_| {});
        assert!(!a.is_settling());
        assert_eq!(a.fps(false), 10);
    }

    /// Idle pets are mostly still, so frames can be cached (PLAN §5): over
    /// ten minutes they average under the walking rate. (Since 2026-11-03
    /// they do little things a few seconds apart, which costs a few frames
    /// a second more than the old 18.)
    #[test]
    fn idle_pets_mostly_run_slow() {
        for seed in 0..4 {
            let mut a = Animator::new(seed, 0.33, 0.35);
            let (mut t, mut frames) = (0.0, 0);
            while t < 600.0 {
                let dt = 1.0 / a.fps(false) as f32;
                a.step(dt);
                t += dt;
                frames += 1;
            }
            let fps = frames as f32 / 600.0;
            assert!(fps < 22.0, "seed {seed}: {fps:.1} fps");
        }
    }

    /// Roger: pets idled too long. A pet that stops walking soon finds
    /// something to do, picked from its own species' habits, and keeps
    /// finding things a few seconds apart.
    #[test]
    fn a_resting_pet_soon_does_its_own_little_things() {
        use crate::species::Species;
        for species in Species::ALL {
            let mut a = Animator::new(5, 0.33, species.waddle()).with_habits(species.habits());
            a.walking = true;
            run(&mut a, 30.0, |_| {});
            a.walking = false;
            let mut first = None;
            let mut seen = Vec::new();
            let mut t = 0.0;
            run(&mut a, 120.0, |a| {
                t += 1.0 / 60.0;
                if let Some((act, ..)) = a.gesture {
                    first.get_or_insert(t);
                    if !seen.contains(&act) {
                        seen.push(act);
                    }
                }
            });
            let first = first.expect("no gesture at all");
            assert!(
                first < 2.6,
                "{species:?}: first gesture {first:.1} s after stopping"
            );
            for act in &seen {
                assert!(
                    species.habits().iter().any(|&(h, _)| h == *act),
                    "{species:?} did {act:?}, not one of its habits"
                );
            }
            assert!(seen.len() >= 5, "{species:?}: only {seen:?} in two minutes");
        }
    }

    #[test]
    fn a_pleased_pet_never_just_sits_smiling() {
        use crate::species::Species;
        for species in Species::ALL {
            let mut a = Animator::new(7, 0.33, species.waddle())
                .with_habits(species.habits())
                .with_joys(species.joys());
            run(&mut a, 10.0, |_| {});
            a.gesture = None;
            a.expression = Expression::Happy;
            // Pleased, it shows it within a second or so, and keeps on with
            // its own joys, never still for long.
            let (mut t, mut first, mut idle, mut longest) = (0.0, None, 0.0f32, 0.0f32);
            run(&mut a, 30.0, |a| {
                t += 1.0 / 60.0;
                match a.gesture {
                    Some((act, ..)) => {
                        first.get_or_insert(t);
                        idle = 0.0;
                        assert!(
                            species.joys().iter().any(|&(j, _)| j == act),
                            "{species:?} did {act:?} pleased, not one of its joys"
                        );
                    }
                    None => idle += 1.0 / 60.0,
                }
                if first.is_some() {
                    longest = longest.max(idle);
                }
            });
            let first = first.expect("no joy at all");
            assert!(first < 1.2, "{species:?}: first joy {first:.1} s in");
            assert!(longest < 1.4, "{species:?}: sat {longest:.1} s smiling");
        }
    }

    /// Pleased, or busy with something fun, a pet shows it all the while
    /// (the renderers keep tail, ears and wings going); not in a mood.
    #[test]
    fn pleased_or_at_play_is_lively_and_a_mood_is_not() {
        let mut a = Animator::new(3, 0.33, 0.35);
        assert!(!a.params().lively);
        a.expression = Expression::Happy;
        assert!(a.params().lively);
        a.expression = Expression::Neutral;
        a.lively = true;
        assert!(a.params().lively);
        for mood in [Expression::FedUp, Expression::Sulk, Expression::Asleep] {
            a.expression = mood;
            assert!(!a.params().lively, "{mood:?}");
        }
    }

    /// Roger: the dog should stick its tongue out too. A run (a fetch,
    /// zoomies) puts it out of breath, for longer after a longer run; an
    /// ordinary walk does not.
    #[test]
    fn a_run_leaves_it_out_of_breath_for_a_while() {
        let mut a = Animator::new(5, 0.33, 0.35);
        a.walking = true;
        run(&mut a, 8.0, |a| {
            assert!(!a.params().puffed, "puffed from a walk")
        });
        a.pace = 2.2;
        run(&mut a, 0.5, |_| {});
        assert!(!a.params().puffed, "puffed at once");
        run(&mut a, 3.0, |_| {});
        assert!(a.params().puffed, "not puffed running");
        a.walking = false;
        a.pace = 1.0;
        run(&mut a, 5.5, |a| {
            assert!(a.params().puffed, "got its breath back early")
        });
        run(&mut a, 2.0, |_| {});
        assert!(!a.params().puffed, "still out of breath");
    }

    #[test]
    fn giggling_keeps_the_belly_bouncing_at_sixty_fps() {
        let mut a = Animator::new(3, 0.33, 0.35);
        a.expression = Expression::Giggle;
        run(&mut a, 4.0, |a| assert_eq!(a.fps(false), 60));
        assert!(a.params().eyes_open < 0.5);
    }

    #[test]
    fn fed_up_is_half_lidded_and_slow() {
        let mut a = Animator::new(4, 0.33, 0.35);
        a.expression = Expression::FedUp;
        a.step(0.5);
        assert!(a.fps(false) <= 30);
        assert!(a.params().tail.abs() <= 3.0);
    }

    #[test]
    fn blinks_every_few_seconds() {
        let mut a = Animator::new(2, 0.33, 0.35);
        let mut shut_frames = 0;
        run(&mut a, 12.0, |a| {
            if a.params().eyes_open < 0.5 {
                shut_frames += 1;
            }
        });
        // 12 s holds 2–3 blinks of ~7 frames each.
        assert!((10..=30).contains(&shut_frames), "{shut_frames}");
    }

    #[test]
    fn walking_alternates_steps_rocks_and_bobs() {
        let mut a = Animator::new(6, 0.33, 1.0);
        a.walking = true;
        let (mut steps, mut lowest, mut rolls) = (Vec::new(), 0.0f32, (0.0f32, 0.0f32));
        run(&mut a, 2.0, |a| {
            let p = a.params();
            if steps.last() != Some(&p.act) {
                steps.push(p.act);
            }
            lowest = lowest.min(p.bob);
            rolls = (rolls.0.min(p.sway), rolls.1.max(p.sway));
        });
        assert!(steps.len() >= 4, "{steps:?}");
        assert!(steps.windows(2).all(|w| w[0] != w[1]));
        assert!(steps.iter().all(|s| matches!(s, Act::StepA | Act::StepB)));
        assert!(lowest < -6.0, "bob {lowest}");
        assert!(rolls.0 < -5.0 && rolls.1 > 5.0, "a duck waddles: {rolls:?}");
        a.walking = false;
        a.step(DT);
        assert_eq!(a.params().act, Act::Rest);
    }

    /// A steady walk is a loop of a few frames; the same beat looks the same.
    #[test]
    fn a_steady_walk_repeats_its_beats() {
        let mut a = Animator::new(13, 0.33, 0.35);
        a.walking = true;
        let mut seen: Vec<PoseParams> = Vec::new();
        run(&mut a, 5.0, |a| {
            let p = a.params();
            let beat = p.beat.expect("steady");
            if let Some(q) = seen.iter().find(|q| q.beat == Some(beat) && q.act == p.act) {
                assert_eq!(
                    PoseParams {
                        eyes_open: q.eyes_open,
                        blink: q.blink,
                        clock: q.clock,
                        ..p
                    },
                    *q
                );
            } else {
                seen.push(p);
            }
        });
        assert!(seen.len() <= 2 * usize::from(BEATS));
        a.poke(Zone::Belly);
        a.step(DT);
        assert_eq!(a.params().beat, None, "a poke is not part of the loop");
    }

    #[test]
    fn birds_waddle_more_than_dogs() {
        let widest = |waddle| {
            let mut a = Animator::new(6, 0.33, waddle);
            a.walking = true;
            let mut w = 0.0f32;
            run(&mut a, 2.0, |a| w = w.max(a.params().sway.abs()));
            w
        };
        assert!(widest(1.0) > widest(0.35) * 1.5);
    }

    #[test]
    fn turning_round_squeezes_through_thin() {
        let mut a = Animator::new(7, 0.33, 0.35);
        a.facing = -1.0;
        let mut thinnest = 1.0f32;
        run(&mut a, 0.4, |a| {
            thinnest = thinnest.min(a.params().facing.abs())
        });
        assert!(thinnest < 0.2, "{thinnest}");
        assert_eq!(a.params().facing, -1.0);
    }

    #[test]
    fn a_held_finger_squashes_and_letting_go_pops_back() {
        let mut a = Animator::new(8, 0.33, 0.35);
        a.press(Zone::Belly, (0.0, 0.0));
        run(&mut a, 0.5, |_| {});
        let held = a.params().squash;
        assert!(held > 0.2, "held {held}");
        a.poke(Zone::Head);
        let mut stretched = 0.0f32;
        run(&mut a, 0.6, |a| {
            stretched = stretched.min(a.params().squash)
        });
        assert!(
            stretched < -0.05,
            "should overshoot into a stretch: {stretched}"
        );
    }

    #[test]
    fn a_quick_tap_still_squashes() {
        let mut a = Animator::new(8, 0.33, 0.35);
        a.press(Zone::Belly, (0.0, 0.0));
        a.poke(Zone::Belly);
        let mut most = 0.0f32;
        run(&mut a, 0.3, |a| most = most.max(a.params().squash));
        assert!(most > 0.2, "{most}");
    }

    /// The 肉肉舞 goes on through its own hops to the end, the moves in
    /// order, and a press stops it.
    #[test]
    fn the_dance_hops_to_the_beat_and_lasts_its_length() {
        let mut a = Animator::new(9, 0.33, 0.35);
        a.gesture(Act::Dance);
        assert_eq!(a.doing(), Some(Act::Dance));
        let (mut hops, mut up, mut t) = (0, false, 0.0);
        while t < DANCE_SECS - 0.1 {
            a.step(1.0 / 60.0);
            t += 1.0 / 60.0;
            assert_eq!(a.doing(), Some(Act::Dance), "stopped at {t:.2} s");
            let air = a.params().bob < 0.0;
            hops += u32::from(air && !up);
            up = air;
        }
        assert!(hops >= 3, "{hops} hops");
        let steps: Vec<DanceMove> = (0..14)
            .map(|k| dance_at(0.5 * k as f32 + 0.1).step)
            .collect();
        assert_eq!(steps[0], DanceMove::Bounce);
        assert_eq!(steps[4], DanceMove::Wiggle);
        assert_eq!(steps[7], DanceMove::Paws);
        assert_eq!(steps[10], DanceMove::Spin);
        assert_eq!(steps[12], DanceMove::Pose);
        // Another gesture asked for meanwhile (a hello wave) waits.
        let mut c = Animator::new(9, 0.33, 0.35);
        c.gesture(Act::Dance);
        c.step(0.6);
        c.gesture(Act::Wave);
        assert_eq!(c.doing(), Some(Act::Dance));
        let mut b = Animator::new(9, 0.33, 0.35);
        b.gesture(Act::Dance);
        b.press(Zone::Belly, (10.0, 10.0));
        b.step(1.0 / 60.0);
        assert_ne!(b.doing(), Some(Act::Dance));
    }

    /// Cursor curiosity (PLAN §1.3): something watched turns it to you and
    /// its head to the spot, as a finger does, and letting it go ends that.
    #[test]
    fn a_watched_spot_draws_its_look_like_a_finger() {
        let mut a = Animator::new(9, 0.33, 0.35);
        assert!(!a.params().touch.attending());
        a.watch(Some((40.0, 10.0)));
        let touch = a.params().touch;
        assert!(touch.attending());
        assert_eq!(touch.at(), Some((40.0, 10.0)));
        a.watch(None);
        assert!(!a.params().touch.attending());
    }

    /// A hug outlasts the happy hops that come with it (they cut every
    /// other gesture short), and a walk or a press ends it.
    #[test]
    fn a_hug_lasts_through_its_cheer_and_ends_on_its_own() {
        let mut a = Animator::new(9, 0.33, 0.35);
        a.hug(Hug::Cuddle { front: true });
        a.cheer();
        a.expression = Expression::Happy;
        run(&mut a, HUG_SECS - 0.2, |a| {
            assert_eq!(a.doing(), Some(Act::Hug), "cut short");
            assert_eq!(a.params().hug, Hug::Cuddle { front: true });
        });
        run(&mut a, 0.5, |_| {});
        assert_ne!(a.doing(), Some(Act::Hug));
        a.hug(Hug::default());
        a.walking = true;
        run(&mut a, 0.1, |_| {});
        assert_ne!(a.doing(), Some(Act::Hug));
    }

    /// Rocked in a 搖搖抱 it is held longer than a hug, its eyes open at
    /// first and closing by degrees until shut, never blinking open again.
    #[test]
    fn a_cradle_rocks_its_eyes_shut() {
        let mut a = Animator::new(9, 0.33, 0.35);
        a.expression = Expression::Happy;
        a.hug(Hug::ByYou(HugHold::Cradle));
        let mut last = 1.0;
        run(&mut a, CRADLE_SECS - 0.2, |a| {
            assert_eq!(a.doing(), Some(Act::Hug), "cut short");
            let eyes = a.params().eyes_open;
            assert!(eyes <= last + 1e-4, "opened again: {eyes} after {last}");
            last = eyes;
        });
        assert!(last < 0.1, "still open: {last}");
        const { assert!(CRADLE_SECS > HUG_SECS) };
        assert_eq!(HugHold::Wrap.secs(), HUG_SECS);
    }

    /// A double-click's hold goes by how it is: sleepy, mostly cradled;
    /// pleased and lively, mostly lifted, a cat less often than a dog;
    /// nothing special, mostly wrapped round.
    #[test]
    fn a_double_click_holds_it_by_how_it_is() {
        use crate::species::Species;
        let share = |species, mood, hold| {
            (0..1000)
                .filter(|&k| HugHold::pick(species, mood, k as f32 / 1000.0) == hold)
                .count() as f32
                / 1000.0
        };
        let sleepy = HugMood {
            sleepy: true,
            ..HugMood::default()
        };
        assert!(share(Species::Shiba, sleepy, HugHold::Cradle) > 0.6);
        let lively = HugMood {
            pleased: true,
            energy: 0.8,
            ..HugMood::default()
        };
        let dog = share(Species::Shiba, lively, HugHold::Carry);
        let cat = share(Species::Fold, lively, HugHold::Carry);
        assert!(dog > 0.5, "{dog}");
        assert!(cat < dog / 2.0, "{cat} vs {dog}");
        let calm = HugMood::default();
        assert!(share(Species::Owl, calm, HugHold::Wrap) > 0.6);
        assert!(share(Species::Owl, calm, HugHold::Cradle) > 0.0);
    }

    #[test]
    fn cheering_hops_twice_and_lands() {
        let mut a = Animator::new(9, 0.33, 0.35);
        a.cheer();
        let (mut high, mut takeoffs, mut up) = (0.0f32, 0, false);
        run(&mut a, 1.5, |a| {
            let bob = a.params().bob;
            high = high.min(bob);
            if bob < 0.0 && !up {
                takeoffs += 1;
            }
            up = bob < 0.0;
        });
        assert_eq!(takeoffs, 2);
        assert!(high < -14.0, "{high}");
        assert_eq!(a.params().bob, 0.0);
    }

    #[test]
    fn idle_pets_glance_stretch_and_yawn_but_not_while_walking() {
        let mut a = Animator::new(10, 0.33, 0.35);
        let mut seen = Vec::new();
        run(&mut a, 90.0, |a| {
            let act = a.params().act;
            if act != Act::Rest && !seen.contains(&act) {
                seen.push(act);
            }
        });
        assert!(seen.len() >= 3, "{seen:?}");
        a.walking = true;
        run(&mut a, 20.0, |a| {
            assert!(matches!(a.params().act, Act::StepA | Act::StepB));
        });
    }

    #[test]
    fn a_wave_hello_starts_at_once() {
        let mut a = Animator::new(11, 0.33, 0.35);
        a.gesture(Act::Wave);
        a.step(DT);
        assert_eq!(a.params().act, Act::Wave);
        run(&mut a, 2.0, |_| {});
        assert_eq!(a.params().act, Act::Rest);
    }

    #[test]
    fn the_tail_rests_between_bursts_of_wagging() {
        let mut a = Animator::new(12, 0.33, 0.35);
        let (mut still, mut wag) = (0, 0);
        run(&mut a, 30.0, |a| {
            if a.params().tail == 0.0 {
                still += 1;
            } else {
                wag += 1;
            }
        });
        assert!(wag > 60, "wags now and then: {wag}");
        assert!(still > wag, "rests more than it wags: {still} vs {wag}");
    }

    #[test]
    fn a_snack_is_eaten_on_the_spot() {
        let mut a = Animator::new(5, 0.33, 0.35);
        a.walking = true;
        a.eat();
        let p = a.params();
        assert_eq!(p.act, Act::Eat);
        assert!(!a.walking);
        run(&mut a, EAT_SECS + 0.2, |_| {});
        assert_ne!(a.params().act, Act::Eat, "still eating after {EAT_SECS} s");
    }
}
