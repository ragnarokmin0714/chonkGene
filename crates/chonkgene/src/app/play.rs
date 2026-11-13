//! Play on the desktop (PLAN §8, Roger's fourth pass, items 2, 4 and 5,
//! and "Catnip"): things in windows of their own (a ball or a frisbee
//! thrown, catnip put down, the props of a species' scene), the pets going
//! after them, doing as they are asked (「叫牠…」), reacting to a still
//! mouse and to typing, their own little scenes, and the effects drawn
//! over a pet's picture (hearts, sparkles, dust, the water it swims in, the
//! fox's fire, a toy in its mouth).

use crate::diag::diag;
use chonkgene_core::activity::{Activity, Watcher};
use chonkgene_core::anim::{Act, Expression, Posture};
use chonkgene_core::mind::Line;
use chonkgene_core::motion::{Heading, Motion};
use chonkgene_core::play::{
    Chase, Command, Fetch, Order, Prop, SCENE_GAP, Scene, Thing, obey_delay, stay_secs,
};
use chonkgene_core::sound::Sound;
use chonkgene_core::species::Species;
use chonkgene_core::terrain::Terrain;
use chonkgene_platform::{Presenter, Shared};
use chonkgene_render::props::{self, Fx};
use chonkgene_render::raymarch::FLOOR_PX;
use tiny_skia::{Pixmap, PixmapPaint, Transform};
use winit::dpi::PhysicalPosition;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use chonkgene_render::cursor::Hand;

use super::{Ctx, Pet};

/// A thrown toy stays this long after it comes to rest unplayed with,
/// seconds; catnip is used up over this long.
const TOY_STAYS: f64 = 60.0;
const CATNIP_SECS: f64 = 60.0;
/// A cat that likes catnip goes back to it after this long, seconds.
const NIP_AGAIN: f64 = 20.0;
/// Cats this far from the fox (pixels, centre to centre) come to bat at
/// its fire.
const SWAT_REACH: f32 = 700.0;
/// Pets go for a thing within this far either side, pixels.
const REACH: f32 = 1100.0;
/// How often the input timing is sampled.
const WATCH_EVERY: f64 = 0.2;
/// Pixels a world unit at 100% (raymarch.rs).
const PPU: f32 = 58.0;
/// Things that move on their own (smell rising, water rippling, a
/// butterfly) are drawn at this many frames a second.
const ALIVE_FPS: f64 = 15.0;
/// Swimming, the duckling paddles this share of the pond's width either
/// side of its middle, so its body stays inside the water.
const POND_PADDLE: f32 = 0.16;
/// Wading in a pond, the water comes this share of the picture's height
/// up its legs (Roger: deeper water; was 0.07, the paws only).
const WADE: f32 = 0.18;
/// The water rises up the legs over this long going in, seconds, and
/// sinks as fast with the feet out (within [`POND_HOLD`]): it jumped
/// in one frame (Roger's sixteenth pass, item 3).
const WADE_EASE: f64 = 0.3;
/// A pond fades out over its last this many seconds, and the water on
/// the pets in it sinks with it.
const POND_FADE: f64 = 1.0;

/// Thrown with the mouse, a toy goes at most this fast (pixels a second
/// at 100%); let go slower than this, it just drops.
const THROW_MOST: f32 = 2400.0;
const THROW_LEAST: f32 = 80.0;
/// Scratched this hard (see `Touch::scratch`), a pet lets go of what it
/// holds in its mouth.
const LET_GO: f32 = 0.6;
/// A press on a toy that moves further than this (pixels) is a drag.
const DRAG_START: f32 = 4.0;
/// A click on a toy knocks it along this fast (pixels a second at 100%).
const KNOCK: (f32, f32) = (380.0, -260.0);

/// A toy picked up with the mouse.
#[derive(Clone, Copy, Debug)]
struct Grab {
    /// Where on its window the cursor took hold.
    at: (f32, f32),
    /// Dragged (not just a click).
    moved: bool,
    /// The cursor's screen position when pressed.
    down: (f32, f32),
    /// Its speed, smoothed, pixels a second.
    vel: (f32, f32),
}

/// A thing in a window of its own. The ball and the frisbee can be picked
/// up, dragged and thrown with the mouse, or knocked along with a click;
/// everything else lets every click through.
pub(super) struct PropWin {
    pub id: u32,
    pub prop: Prop,
    window: Window,
    presenter: Presenter,
    pixmap: Pixmap,
    foot: (f32, f32),
    placed: (i32, i32),
    /// When it goes (the app's clock).
    pub until: f64,
    /// In a pet's mouth (the pet draws it), so not shown here.
    pub carrier: Option<u64>,
    /// Being pushed home by this pet: the others leave it to it.
    claimed: Option<u64>,
    /// Branches grow out from the right edge unless flipped.
    flip: bool,
    born: f64,
    drawn: Option<(f64, (f32, f32), f32, bool)>,
    shown: bool,
    /// Where the cursor is on its window, and the mouse holding it.
    cursor: (f32, f32),
    grab: Option<Grab>,
    /// A pond: the screen x of each pet in it, for the rings they stir.
    stirs: Vec<f32>,
    /// A pond: water thrown in a water fight, from and to (screen x) and
    /// when.
    pub sprays: Vec<(f32, f32, f64)>,
}

impl PropWin {
    fn new(
        el: &ActiveEventLoop,
        shared: &Shared,
        id: u32,
        prop: Prop,
        now: f64,
        until: f64,
    ) -> Option<Self> {
        let (size, foot) = props::canvas(prop.thing, prop.scale);
        let pos = Self::pos_for(&prop, foot);
        let window = el
            .create_window(chonkgene_platform::pet_window(size, pos, "肉肉賴桌"))
            .ok()?;
        let mut presenter = Presenter::new(shared, &window).ok()?;
        if prop.thing.grabbed() {
            window.set_cursor(super::hand(Hand::Open));
        } else {
            if let Err(e) = presenter.set_ghost(shared) {
                eprintln!("a thing on the desktop: {e}");
            }
            let _ = window.set_cursor_hittest(false);
        }
        Some(Self {
            id,
            prop,
            window,
            presenter,
            pixmap: Pixmap::new(size.0, size.1)?,
            foot,
            placed: pos,
            until,
            carrier: None,
            claimed: None,
            flip: false,
            born: now,
            drawn: None,
            shown: true,
            cursor: (0.0, 0.0),
            grab: None,
            stirs: Vec::new(),
            sprays: Vec::new(),
        })
    }

    fn pos_for(prop: &Prop, foot: (f32, f32)) -> (i32, i32) {
        (
            (prop.pos.0 - foot.0).round() as i32,
            (prop.pos.1 - foot.1).round() as i32,
        )
    }

    /// How used up it is (catnip, seeds; a sunbeam fading), 0 … 1.
    fn used(&self, now: f64) -> f32 {
        match self.prop.thing {
            Thing::Catnip => ((now - self.born) / CATNIP_SECS).clamp(0.0, 1.0) as f32,
            // A spotlight goes out over its last second.
            Thing::Spotlight => (1.0 - (self.until - now)).clamp(0.0, 1.0) as f32,
            // A pond dries up over its last moments (it was there one
            // frame and gone the next, the water on the pets in it left
            // behind as boxes: Roger's sixteenth pass, item 3).
            Thing::Pond | Thing::Beach => {
                (1.0 - (self.until - now) / POND_FADE).clamp(0.0, 1.0) as f32
            }
            Thing::Seeds | Thing::Sunbeam => {
                let span = (self.until - self.born).max(1.0);
                let left = (self.until - now) / span;
                if self.prop.thing == Thing::Seeds {
                    (1.0 - left).clamp(0.0, 1.0) as f32
                } else {
                    // A sunbeam fades out over its last two seconds.
                    (1.0 - (self.until - now) / 2.0).clamp(0.0, 1.0) as f32
                }
            }
            _ => 0.0,
        }
    }

    /// Move the window with the thing and draw it again if it changed.
    fn show(&mut self, shared: &Shared, now: f64, visible: bool) {
        let visible = visible && self.carrier.is_none();
        if visible != self.shown {
            self.window.set_visible(visible);
            self.presenter.restyle();
            self.shown = visible;
            self.drawn = None;
        }
        if !visible {
            return;
        }
        let pos = Self::pos_for(&self.prop, self.foot);
        if pos != self.placed {
            self.window
                .set_outer_position(PhysicalPosition::new(pos.0, pos.1));
            self.placed = pos;
        }
        // The beach is still, but for fading in and out.
        let fading =
            self.prop.thing == Thing::Beach && (now - self.born < 1.2 || self.used(now) > 0.0);
        let alive = fading
            || matches!(
                self.prop.thing,
                Thing::Catnip
                    | Thing::Pond
                    | Thing::Sunbeam
                    | Thing::Butterfly
                    | Thing::Branch
                    | Thing::Spotlight
            );
        let grounded = self.prop.resting || (self.prop.vel.1 == 0.0 && self.prop.thing.falls());
        // Water flying about: drawn at twice the rate.
        self.sprays.retain(|s| now - s.2 < SPRAY_SECS);
        let fps = if self.sprays.is_empty() {
            ALIVE_FPS
        } else {
            2.0 * ALIVE_FPS
        };
        let fresh = self.drawn.is_some_and(|(at, _, spin, g)| {
            (!alive || now - at < 1.0 / fps) && spin == self.prop.spin && g == grounded
        });
        if fresh {
            return;
        }
        self.pixmap.fill(tiny_skia::Color::TRANSPARENT);
        let mut sprays = [None; 4];
        for (slot, &(from, to, since)) in sprays.iter_mut().zip(&self.sprays) {
            *slot = Some((
                from - self.prop.pos.0,
                to - self.prop.pos.0,
                ((now - since) / SPRAY_SECS) as f32,
            ));
        }
        let look = props::Look {
            spin: self.prop.spin,
            t: (now - self.born) as f32,
            used: self.used(now),
            grounded,
            flip: self.flip,
            sprays,
            ..props::Look::default()
        };
        props::draw(
            &mut self.pixmap,
            self.prop.thing,
            self.foot,
            self.prop.scale,
            look,
        );
        if !self.stirs.is_empty() {
            let xs: Vec<f32> = self
                .stirs
                .iter()
                .map(|x| x - self.placed.0 as f32)
                .collect();
            props::pond_stirs(&mut self.pixmap, self.foot, &xs, self.prop.scale, look.t);
        }
        if self.prop.thing == Thing::Pond {
            let left = 1.0 - look.used;
            if left < 1.0 {
                for v in self.pixmap.data_mut() {
                    *v = (f32::from(*v) * left) as u8;
                }
            }
        }
        if let Err(e) = self.presenter.present(shared, &self.pixmap) {
            eprintln!("present failed: {e}");
        }
        if matches!(self.prop.thing, Thing::Pond | Thing::Beach) {
            let what = if self.prop.thing == Thing::Pond {
                "pond"
            } else {
                "beach"
            };
            crate::diag::dump(format_args!("{what}{}", self.id), self.placed, &self.pixmap);
        }
        self.drawn = Some((now, self.prop.pos, self.prop.spin, grounded));
    }
}

/// A scene under way.
#[derive(Clone, Copy, Debug)]
pub(super) struct Run {
    pub scene: Scene,
    /// Its thing's window.
    prop: Option<u32>,
    /// 0 going there, 1 doing it, 2 coming back.
    phase: u8,
    since: f64,
    pub until: f64,
    /// Where it was when the scene began (the owl flies back down here).
    home: (f32, f32),
    /// Where on its thing it goes (the owl lands here on the branch).
    spot: f32,
    /// Its thing is shared (a spotlight put down from the menu): it stays
    /// when this pet is done.
    keep: bool,
    /// Beats counted so far (the 肉肉舞).
    count: u32,
}

/// At the catnip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nip {
    Going,
    /// Doing something there (the gesture under way), until.
    At(u8),
    Leaving,
}

/// 躲貓貓 (PLAN §8, Roger's tenth pass, item 5): the pet hides just past
/// the edge of the screens and peeks round it now and then; a click on
/// whatever shows of it finds it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Hide {
    /// The edge it hides past (screen x), and which way past it: −1 the
    /// left edge, 1 the right.
    edge: f32,
    side: f32,
    /// Off to hide (gone in a puff) until then; it gives up and comes out
    /// by itself at `until`.
    since: f64,
    until: f64,
    /// Its window's left edge with its front just out of sight.
    base: Option<f32>,
    /// When it next leans out to look.
    peek_at: f64,
    /// Clicked on: found.
    found: bool,
}

/// Gone this long before it shows at its hiding place, seconds; it comes
/// out by itself after [`HIDE_SECS`].
const HIDE_GO: f64 = 1.2;
const HIDE_SECS: f64 = 40.0;
/// Hidden, this share of the window's width sticks out round the edge (an
/// ear, the nose); peeking, this much, for [`PEEK_SECS`].
const HINT: f32 = 0.1;
const PEEK: f32 = 0.42;
const PEEK_SECS: f64 = 1.6;
/// A sitting pet's front from the middle of its window, as a share of the
/// window's width.
const FRONT: f32 = 0.36;

impl Hide {
    /// How far round the edge it leans now (shares of its width).
    fn lean(&self, now: f64) -> f32 {
        let into = (now - self.peek_at) / PEEK_SECS;
        if !(0.0..1.0).contains(&into) {
            return HINT;
        }
        // Out quickly, a look, back in.
        let u = into as f32;
        let out = (u / 0.25).min(1.0).min((1.0 - u) / 0.25);
        HINT + (PEEK - HINT) * out * out * (3.0 - 2.0 * out)
    }
}

impl Pet {
    /// Off to hide for 躲貓貓: a puff and it is gone, to show again just
    /// past the edge of the screens on the side away from the cursor.
    pub(super) fn start_hiding(&mut self, ctx: &mut Ctx, cursor: Option<(i32, i32)>) {
        let now = ctx.world.now;
        self.stop();
        self.say(ctx, Line::HideStart);
        self.fun.effect(Fx::Dust, now);
        let (left, right) = ctx.terrain.bounds();
        let from = cursor.map_or(self.body.feet().0, |c| c.0 as f32);
        let side = if from - left > right - from {
            -1.0
        } else {
            1.0
        };
        self.fun.hide = Some(Hide {
            edge: if side < 0.0 { left } else { right },
            side,
            since: now,
            until: now + HIDE_SECS,
            base: None,
            peek_at: now + HIDE_GO + 2.5,
            found: false,
        });
        println!(
            "{}: hides past the {} edge",
            self.save.name,
            if side < 0.0 { "left" } else { "right" }
        );
    }

    /// Hiding: where its window goes this frame (it never walks or falls
    /// there). False once it is not hiding.
    pub(super) fn hide_step(&mut self, ctx: &mut Ctx) -> bool {
        let now = ctx.world.now;
        let Some(mut h) = self.fun.hide else {
            return false;
        };
        if h.found {
            // Found: as pleased as with a pat.
            self.come_out(ctx, Line::HideFound);
            let mut out = self.mind.pat(&ctx.world, chonkgene_core::Zone::Head);
            out.line = None;
            self.apply(ctx, out);
            return false;
        }
        if self.mind.asleep() || now >= h.until {
            self.come_out(ctx, Line::HideLost);
            return false;
        }
        let (w, h_px) = self.body.size;
        let front = -h.side;
        if h.base.is_none() && now >= h.since + HIDE_GO {
            // Sitting just past the edge, facing in, on the floor there.
            let x = h.edge - 0.5 * w - front * FRONT * w;
            let floor = ctx
                .terrain
                .floor_under(h.edge - h.side * 20.0)
                .map_or(self.body.pos.1, |l| l.y - h_px + self.body.foot);
            self.body.pos.1 = floor;
            h.base = Some(x);
            // Its last words would hang there and give it away.
            self.bubble = None;
        }
        if let Some(base) = h.base {
            self.body.facing = front;
            self.body.motion = Motion::Idle;
            self.body.speed = 0.0;
            self.body.pos.0 = base + front * h.lean(now) * w;
            self.anim.posture = Posture::Sit;
            if now >= h.peek_at + PEEK_SECS {
                h.peek_at = now + 2.0 + 1.5 * f64::from(self.rng.f32());
            }
        }
        self.fun.hide = Some(h);
        true
    }

    /// Found (a click on it), or giving up: out it hops into sight, with a
    /// line.
    pub(super) fn come_out(&mut self, ctx: &mut Ctx, line: Line) {
        let Some(h) = self.fun.hide.take() else {
            return;
        };
        let w = self.body.size.0;
        let now = ctx.world.now;
        if h.base.is_none() {
            // Not there yet: back where it was.
            self.say(ctx, line);
            return;
        }
        self.body.facing = -h.side;
        self.body
            .spring(h.edge - h.side * 0.62 * w, 30.0 * self.fun.scale);
        self.say(ctx, line);
        if line == Line::HideFound {
            cheer(self, ctx);
        } else {
            self.giggle(now);
        }
        println!("{}: {:?}", self.save.name, line);
    }

    /// Is it hiding (in a puff, or past the edge)?
    pub(super) fn hiding(&self) -> bool {
        self.fun.hide.is_some()
    }

    /// Sitting in its hiding place (not on its way there).
    pub(super) fn in_hiding_place(&self) -> bool {
        self.fun.hide.is_some_and(|h| h.base.is_some())
    }

    /// A click on it while it hides: found (true), if it is in its place.
    pub(super) fn find(&mut self) -> bool {
        match &mut self.fun.hide {
            Some(h) if h.base.is_some() => {
                h.found = true;
                true
            }
            _ => false,
        }
    }
}

/// Frightened by the fox's spell, it keeps low this long, seconds.
const AWE_HOLD: f64 = 3.5;
/// A cat crouched at the fox's flames pounces this long after, seconds
/// (its bottom wiggling first).
const STALK_SECS: f64 = 1.4;

/// Into a pond with the feet within this share of its half-width...
const POND_IN: f32 = 0.85;
/// ...and out again only past this one, so a pet at the edge does not
/// flip in and out with every step (Roger: the pond flickered as a pet
/// walked in).
const POND_OUT: f32 = 0.95;
/// Out of the water this long before it counts as gone (a hop, a step's
/// bob), seconds.
const POND_HOLD: f64 = 0.4;
/// No second splash into a pond so soon after leaving one, seconds.
const SPLASH_AGAIN: f64 = 2.0;

/// Whether feet at `feet` (screen pixels) are in the water of the pond at
/// `pos`: in at [`POND_IN`] of its half-width and within 8 px of its level,
/// or, already `inside`, out to [`POND_OUT`] and 14 px.
fn in_water(pos: (f32, f32), scale: f32, feet: (f32, f32), inside: bool) -> bool {
    let (cx, _, rx, _) = props::pond_oval(pos, scale);
    let (reach, rise) = if inside {
        (POND_OUT, 14.0)
    } else {
        (POND_IN, 8.0)
    };
    (feet.1 - pos.1).abs() < rise * scale && (feet.0 - cx).abs() < reach * rx
}

/// A pet's time in a pond, steadied: in at once, out only after
/// [`POND_HOLD`] out of the water, and one splash a visit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Wade {
    /// The pond it is in, by the middle of its bottom (screen pixels).
    pub now: Option<(f32, f32)>,
    /// Out of the water since then, still counted in.
    pub out_since: Option<f64>,
    /// Left a pond then.
    left_at: Option<f64>,
}

impl Wade {
    /// Out of the pond at once: it is gone.
    fn leave(&mut self, now: f64) {
        self.now = None;
        self.out_since = None;
        self.left_at = Some(now);
    }

    /// In a pond, or out of one less than two seconds ago (frame dumps).
    pub(super) fn lately(&self, now: f64) -> bool {
        self.now.is_some() || self.left_at.is_some_and(|t| now - t < 2.0)
    }

    /// The pond the feet are over this frame, if any: whether it splashes.
    fn step(&mut self, under: Option<(f32, f32)>, now: f64) -> bool {
        if let Some(pond) = under {
            let fresh = self.now.is_none();
            self.now = Some(pond);
            self.out_since = None;
            return fresh && self.left_at.is_none_or(|t| now - t >= SPLASH_AGAIN);
        }
        if self.now.is_some() {
            let since = *self.out_since.get_or_insert(now);
            if now - since >= POND_HOLD {
                self.now = None;
                self.out_since = None;
                self.left_at = Some(now);
            }
        }
        false
    }
}

/// A pet in a pond stands this far into it by default, as a share of the
/// pond's height up from its near edge (its feet were on the near edge:
/// Roger's seventeenth pass, item 3, they looked to stand above the water).
const POND_DEEP: f32 = 0.30;

impl Fun {
    /// How far up its window goes while it is in a pond, pixels: its feet
    /// into the water rather than on the near edge, eased in and out with
    /// the water (`wet`).
    /// How far up its window goes while you hold it up in front (前抱),
    /// pixels, at `t`.
    pub fn hug_lift(&self, t: f64) -> f32 {
        self.hug_by_you.map_or(0.0, |(since, hold)| {
            let secs = chonkgene_core::anim::HUG_SECS;
            props::hug_lift(hold, self.scale, (t - since) as f32, secs)
        })
    }

    pub fn pond_lift(&self) -> f32 {
        if self.pond.now.is_none() {
            return 0.0;
        }
        let deep = if self.pond_deep > 0.0 {
            self.pond_deep
        } else {
            POND_DEEP
        };
        self.wet.0 * deep * Thing::Pond.size().1 * self.scale
    }
}

/// What a pet is up to besides its ordinary life.
#[derive(Default)]
pub(super) struct Fun {
    pub scale: f32,
    chase: Option<(u32, Chase)>,
    /// A nose bump under way: when it lands on the ball, and how hard.
    nudge: Option<(f64, (f32, f32))>,
    /// Gone into another pet's tussle cloud until then: not drawn.
    pub vanish_until: f64,
    /// Hiding for 躲貓貓.
    pub hide: Option<Hide>,
    /// In its mouth.
    pub carry: Option<Thing>,
    /// Asked to do something: what, when it gets round to it, and whether
    /// it was in a mood.
    order: Option<(Command, f64, bool)>,
    /// Walking over to here (screen x of its feet).
    goto: Option<f32>,
    /// A held posture lasts until then.
    hold_until: f64,
    pub scene: Option<Run>,
    next_scene: f64,
    /// In a pond (swimming, or wading): the pond's front is drawn over the
    /// pet.
    pub pond: Wade,
    nip: Option<(Nip, f64, u8)>,
    /// Not back to the catnip before this (it has just been; one that
    /// does not care for it, not to this sprig again).
    nip_again: f64,
    /// A cat batting at the fox's fire: this fox's.
    swat: Option<u64>,
    /// Crouched at the fox's spell, bottom wiggling: it pounces then.
    pounce_at: Option<f64>,
    /// Frightened by the fox's spell, keeping low until then. Not busy:
    /// the spell must still be able to take it (as `hold_until` it was
    /// left out, and the spell called off when it was the one picked on).
    cower_until: f64,
    /// Effects under way, since when.
    pub fx: Vec<(Fx, f64)>,
    /// Lying about while you read, until then; little things to do there.
    lounge_until: f64,
    next_habit: f64,
    next_tap: f64,
    next_tilt: f64,
    /// Watching the cursor (cursor curiosity, PLAN §1.3): since when, and
    /// until when it keeps watching where the cursor last was; not again
    /// before `curious_rest`; the cursor last seen, for how fast it moves;
    /// when it may pounce at it again.
    curious: Option<(f64, f64)>,
    curious_rest: f64,
    cursor_was: Option<(f32, f32)>,
    cursor_pounce: f64,
    /// Curled up by you for this stretch of typing; dozed off to it.
    curled: bool,
    dozed: bool,
    /// Its picture with the effects over it.
    pub canvas: Option<Pixmap>,
    /// The spell's (or a dream's) bubble round its picture: middle and
    /// radii, pixels (see [`props::bubble_fit`]); found as it floats up.
    pub bubble_fit: Option<((f32, f32), (f32, f32))>,
    /// How brightly the sunbeam lights it, 0 … 1, easing in and out, and
    /// the clock when that was last worked out.
    sun: (f32, f64),
    /// How far up it the pond's water comes, 0 … 1 of the way to the
    /// waterline (see [`WADE_EASE`]), and the clock then.
    wet: (f32, f64),
    /// How much is left of the pond it is in, 1 … 0 as it dries up.
    pond_left: f32,
    /// Its feet are on a beach (see [`Playground::on_beach`]).
    pub on_sand: bool,
    /// How far into the pond it stands, as a share of the pond's height up
    /// from its near edge (see [`Fun::pond_lift`]); 0 for the default.
    pond_deep: f32,
    /// Hugged by you: since when, and how your hands hold it; where they
    /// are this frame (found on its picture each frame, see
    /// `Renderer::hug_hands`).
    pub hug_by_you: Option<(f64, props::HugHold)>,
    pub hands: Option<props::HugHands>,
    /// When it last finished a 肉肉舞 (it does not dance again at once).
    pub danced: f64,
    /// When it last went into a pond to join the others.
    bathed: f64,
    /// Drawn with effects last frame: the plain picture must be shown again
    /// once they are gone.
    pub had_overlay: bool,
}

impl Fun {
    pub fn new(scale: f32) -> Self {
        Self {
            scale,
            ..Self::default()
        }
    }

    /// Frightened by the fox's spell: low to the floor a while.
    pub fn cower(&mut self, now: f64) {
        self.cower_until = now + AWE_HOLD;
    }

    /// Cowering from the fox's fire (it trembles).
    pub fn cowering(&self, now: f64) -> bool {
        now < self.cower_until
    }

    /// Crouched for the fox's flames: a pounce at them shortly.
    pub fn stalk(&mut self, now: f64) {
        self.pounce_at = Some(now + STALK_SECS);
    }

    /// Up to something that the pet's own wandering must leave alone.
    pub fn busy(&self, now: f64) -> bool {
        self.chase.is_some()
            || self.order.is_some()
            || self.goto.is_some()
            || self.scene.is_some()
            || self.nip.is_some()
            || self.swat.is_some()
            || self.carry.is_some()
            || self.hide.is_some()
            || now < self.hold_until
            || now < self.lounge_until
    }

    pub fn effect(&mut self, fx: Fx, now: f64) {
        self.fx.retain(|(f, _)| *f != fx);
        self.fx.push((fx, now));
    }

    /// Asked from the menu: done after a moment's thought (more in a bad
    /// mood).
    pub fn ask(&mut self, command: Command, now: f64, level: u32, moody: bool, roll: f32) {
        self.order = Some((command, now + obey_delay(level, moody, roll), moody));
        self.hold_until = 0.0;
        self.lounge_until = 0.0;
    }
}

/// Everything on the desktop to play with, and an eye on the input.
#[derive(Default)]
pub(super) struct Playground {
    pub things: Vec<PropWin>,
    next_id: u32,
    watcher: Watcher,
    watch_at: f64,
    pub activity: Option<Activity>,
    /// Where the cursor was when last looked at (screen pixels).
    pub cursor: Option<(i32, i32)>,
    /// The clock last go round.
    last: f64,
    /// The owl's branch to stand on: (id, y, x0, x1).
    pub perch: Option<(u32, f32, f32, f32)>,
    /// The terrain must be read again (a branch came or went).
    pub terrain_dirty: bool,
    /// A thing on the floor just came out: its window went on top of the
    /// pets', so the pets (and their bubbles) go back above it (Roger: the
    /// pond was still drawn over the duckling on Windows).
    restack: bool,
    /// Where every pet stands this go round: (id, feet x, feet y, the half
    /// width of its body, settled: asleep or busy, so it will not move), so
    /// a pond or a sunbeam comes out where nobody is.
    standing: Vec<(u64, f32, f32, f32, bool)>,
    /// The spotlight put down from the menu (「放聚光燈」), if any.
    pub stage: Option<u32>,
    /// 非理性模式 is on: pets near a spotlight run into it and join in.
    pub irrational: bool,
    /// The pet whose dance the tune goes with, until when: dancing
    /// together, only one plays it.
    tune: Option<(u64, f64)>,
    /// The next water thrown in a pond, and by whom if it is throwing
    /// back.
    splash_at: f64,
    splash_back: Option<u64>,
    /// Splashed, and when it lands: (pet, when, from screen x).
    hits: Vec<(u64, f64, f32)>,
    rng: Option<fastrand::Rng>,
}

impl Playground {
    /// The pond whose water a pet with its feet at `feet` (screen pixels)
    /// stands in, if any: where the middle of its bottom is. The pond it is
    /// already `in` keeps it out to a wider edge (see [`in_water`]).
    fn pond_under(&self, feet: (f32, f32), now_in: Option<(f32, f32)>) -> Option<(f32, f32)> {
        let ponds = || {
            self.things
                .iter()
                .map(|w| w.prop)
                .filter(|p| p.thing == Thing::Pond)
        };
        ponds()
            .find(|p| Some(p.pos) == now_in && in_water(p.pos, p.scale, feet, true))
            .or_else(|| ponds().find(|p| in_water(p.pos, p.scale, feet, false)))
            .map(|p| p.pos)
    }

    /// Feet at `feet` (screen pixels) stand on a beach's sand.
    fn on_beach(&self, feet: (f32, f32)) -> bool {
        self.things.iter().any(|w| {
            w.prop.thing == Thing::Beach
                && (feet.1 - w.prop.pos.1).abs() < 30.0 * w.prop.scale
                && (feet.0 - w.prop.pos.0).abs() < 0.44 * Thing::Beach.size().0 * w.prop.scale
        })
    }

    /// How much is left of the pond at `pos` (1 … 0 as it dries up); none
    /// if it is gone.
    fn pond_left(&self, pos: (f32, f32), now: f64) -> Option<f32> {
        self.things
            .iter()
            .find(|w| w.prop.thing == Thing::Pond && w.prop.pos == pos)
            .map(|w| 1.0 - w.used(now))
    }

    fn roll(&mut self) -> f32 {
        self.rng.get_or_insert_with(fastrand::Rng::new).f32()
    }

    fn add(
        &mut self,
        el: &ActiveEventLoop,
        shared: &Shared,
        prop: Prop,
        now: f64,
        until: f64,
    ) -> Option<u32> {
        self.next_id += 1;
        let id = self.next_id;
        let w = PropWin::new(el, shared, id, prop, now, until)?;
        self.things.push(w);
        // A butterfly flutters in front of everything.
        self.restack |= prop.thing != Thing::Butterfly;
        Some(id)
    }

    fn get(&self, id: u32) -> Option<&PropWin> {
        self.things.iter().find(|w| w.id == id)
    }

    fn get_mut(&mut self, id: u32) -> Option<&mut PropWin> {
        self.things.iter_mut().find(|w| w.id == id)
    }

    fn remove(&mut self, id: u32) {
        self.things.retain(|w| w.id != id);
        if self.perch.is_some_and(|p| p.0 == id) {
            self.perch = None;
            self.terrain_dirty = true;
        }
    }

    pub fn has(&self, thing: Thing) -> bool {
        self.things.iter().any(|w| w.prop.thing == thing)
    }

    /// Show or hide every thing (the pets are put away, a full-screen app).
    pub fn set_visible(&mut self, shared: &Shared, now: f64, visible: bool) {
        for w in &mut self.things {
            let was = w.shown;
            w.show(shared, now, visible);
            // Shown again, it may have come up over the pets.
            self.restack |= w.shown && !was && w.prop.thing != Thing::Butterfly;
        }
    }

    /// Throw a ball or a frisbee from `from` (screen pixels) toward the
    /// side with more room; every pet awake goes after it its own way.
    #[allow(clippy::too_many_arguments)]
    pub fn throw(
        &mut self,
        el: &ActiveEventLoop,
        shared: &Shared,
        pets: &mut [Pet],
        thing: Thing,
        from: (f32, f32),
        area: chonkgene_core::motion::Area,
        scale: f32,
        now: f64,
    ) {
        // One of each at a time: a new throw takes the old one back.
        let old: Vec<u32> = self
            .things
            .iter()
            .filter(|w| w.prop.thing == thing)
            .map(|w| w.id)
            .collect();
        for id in old {
            for p in pets.iter_mut() {
                if p.fun.chase.as_ref().is_some_and(|(c, _)| *c == id) {
                    p.fun.chase = None;
                    p.fun.carry = None;
                }
            }
            self.remove(id);
        }
        let side = if from.0 < area.x + area.w / 2.0 {
            1.0
        } else {
            -1.0
        };
        let prop = Prop::thrown(thing, from, side, scale);
        let Some(id) = self.add(el, shared, prop, now, now + TOY_STAYS) else {
            return;
        };
        println!("a {thing:?} thrown");
        self.after(pets, id, thing, from.0, now);
    }

    /// Every pet awake and free goes after toy `id`, thrown from screen x
    /// `from`, its own way.
    fn after(&mut self, pets: &mut [Pet], id: u32, thing: Thing, from: f32, now: f64) {
        for p in pets.iter_mut() {
            if !p.free_to_play(now) {
                continue;
            }
            println!("{}: goes for the {thing:?}", p.save.name);
            let roll = self.roll();
            p.fun.chase = Some((id, Chase::new(p.species.fetch(), thing, from, now, roll)));
            p.fun.lounge_until = 0.0;
            p.fun.hold_until = 0.0;
            p.anim.posture = Posture::Free;
        }
    }

    /// Is this window one of the things'?
    pub fn owns(&self, window: winit::window::WindowId) -> bool {
        self.things.iter().any(|w| w.window.id() == window)
    }

    /// The cursor moved on a thing's window, to `at` on it.
    pub fn cursor_on(&mut self, window: winit::window::WindowId, at: (f32, f32)) {
        if let Some(w) = self.things.iter_mut().find(|w| w.window.id() == window) {
            w.cursor = at;
        }
    }

    /// The mouse pressed on a toy's window, the cursor at `screen`: it is
    /// picked up (pets after it stop and wait).
    pub fn press(
        &mut self,
        window: winit::window::WindowId,
        pets: &mut [Pet],
        screen: Option<(i32, i32)>,
    ) {
        let Some(w) = self
            .things
            .iter_mut()
            .find(|w| w.window.id() == window && w.prop.thing.grabbed() && w.carrier.is_none())
        else {
            return;
        };
        let down = screen.map_or(
            (
                w.placed.0 as f32 + w.cursor.0,
                w.placed.1 as f32 + w.cursor.1,
            ),
            |(x, y)| (x as f32, y as f32),
        );
        w.grab = Some(Grab {
            at: w.cursor,
            moved: false,
            down,
            vel: (0.0, 0.0),
        });
        w.window.set_cursor(super::hand(Hand::Grab));
        w.claimed = None;
        let (id, catnip) = (w.id, w.prop.thing == Thing::Catnip);
        for p in pets.iter_mut() {
            let at_it = catnip && p.fun.nip.is_some();
            if at_it || p.fun.chase.as_ref().is_some_and(|(c, _)| *c == id) {
                p.fun.chase = None;
                p.fun.nip = None;
                p.stop();
            }
        }
    }

    /// The mouse let go of a toy: dragged, it flies off at the speed it was
    /// moving; clicked, it is knocked along away from the cursor. Either
    /// way the pets go after it.
    pub fn release(&mut self, window: winit::window::WindowId, pets: &mut [Pet], now: f64) {
        let Some(w) = self.things.iter_mut().find(|w| w.window.id() == window) else {
            return;
        };
        let Some(g) = w.grab.take() else {
            return;
        };
        w.window.set_cursor(super::hand(Hand::Open));
        if w.prop.thing == Thing::Catnip {
            // Moved where you like: it drops onto whatever is below, and
            // the pets come over again.
            w.prop.vel = (0.0, 1.0);
            w.prop.resting = false;
            println!("catnip moved to ({:.0}, {:.0})", w.prop.pos.0, w.prop.pos.1);
            for p in pets.iter_mut() {
                p.fun.nip_again = 0.0;
            }
            return;
        }
        let s = w.prop.scale.max(0.5);
        if g.moved {
            let (vx, vy) = g.vel;
            let speed = vx.hypot(vy);
            let most = THROW_MOST * s;
            w.prop.vel = if speed < THROW_LEAST * s {
                (0.0, 1.0)
            } else if speed > most {
                (vx * most / speed, vy * most / speed)
            } else {
                (vx, vy.max(-most))
            };
            if w.prop.vel.1 == 0.0 {
                // Level throws still count as in the air.
                w.prop.vel.1 = -1.0;
            }
            println!(
                "a {:?} thrown by hand at ({:.0}, {:.0}) px/s",
                w.prop.thing, w.prop.vel.0, w.prop.vel.1
            );
        } else {
            let side = if g.at.0 < w.foot.0 { 1.0 } else { -1.0 };
            w.prop.bat((side * KNOCK.0 * s, KNOCK.1 * s));
            println!("a {:?} knocked along", w.prop.thing);
        }
        w.prop.resting = false;
        w.until = now + TOY_STAYS;
        let (id, thing, from) = (w.id, w.prop.thing, w.prop.pos.0);
        self.after(pets, id, thing, from, now);
    }

    /// Put down a sprig of catnip under `at`, or take it away if there is
    /// one already.
    pub fn catnip(
        &mut self,
        el: &ActiveEventLoop,
        shared: &Shared,
        pets: &mut [Pet],
        at: (f32, f32),
        scale: f32,
        now: f64,
    ) {
        if let Some(id) = self
            .things
            .iter()
            .find(|w| w.prop.thing == Thing::Catnip)
            .map(|w| w.id)
        {
            self.remove(id);
            for p in pets.iter_mut() {
                p.fun.nip = None;
            }
            return;
        }
        let prop = Prop::put(Thing::Catnip, at, scale);
        if self.add(el, shared, prop, now, now + CATNIP_SECS).is_none() {
            return;
        }
        println!("catnip put down at ({:.0}, {:.0})", at.0, at.1);
        for p in pets.iter_mut() {
            p.fun.nip_again = 0.0;
        }
    }

    /// The terrain with the owl's branch on it, if out.
    pub fn with_perch(
        &self,
        t: chonkgene_core::terrain::Terrain,
    ) -> chonkgene_core::terrain::Terrain {
        match self.perch {
            Some((id, y, x0, x1)) => t.with_perch(id, y, x0, x1),
            None => t,
        }
    }
}

impl Pet {
    /// Awake and free to go after a thing.
    fn free_to_play(&self, now: f64) -> bool {
        !self.mind.asleep()
            && !self.in_skit
            && self.act.is_none()
            && self.press.is_none()
            && matches!(self.body.motion, Motion::Idle | Motion::Walk)
            && self.fun.scene.is_none()
            && now >= self.fun.hold_until
    }

    /// Walk to screen x `x` (its feet) at `pace` (see [`Body::head_for`]):
    /// there, or as close as it gets, or still on its way.
    fn walk_to(&mut self, x: f32, pace: f32, terrain: &Terrain) -> Heading {
        let near = 0.12 * self.body.size.0;
        self.body.head_for(x, pace, near, terrain)
    }

    fn stop(&mut self) {
        if self.body.motion == Motion::Walk {
            self.body.motion = Motion::Idle;
        }
        self.body.pace = 1.0;
    }

    /// A gesture if it is standing free for one (not still walking); true
    /// if it started.
    fn try_gesture(&mut self, act: Act) -> bool {
        if self.body.speed > 0.0 || self.anim.walking {
            self.stop();
            return false;
        }
        self.anim.gesture(act);
        self.anim.doing() == Some(act)
    }

    fn bell(&self, ctx: &mut Ctx) {
        if ctx.sound_on && self.species == Species::Fox {
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

    /// A thing drawn in its mouth, effects over its picture, the water it
    /// swims in, the fox's fire: its picture with them in `fun.canvas`;
    /// false if none are on.
    pub(super) fn compose(&mut self, t: f64) -> bool {
        let s = self.fun.scale;
        self.fun
            .fx
            .retain(|(fx, since)| t - since < f64::from(fx.secs()));
        let swimming = self.anim.posture == Posture::Swim;
        // Into the water at once, but the waterline rises over a moment,
        // and sinks once the feet are out; picked up, or the hold over,
        // it is dry at once.
        let (wet, last) = self.fun.wet;
        let step = ((t - last).clamp(0.0, 0.1) / WADE_EASE) as f32;
        let wet = match self.fun.pond.now {
            None => 0.0,
            Some(_) if swimming || self.fun.pond.out_since.is_none() => (wet + step).min(1.0),
            Some(_) => (wet - step).max(0.0),
        };
        let wet = wet.min(self.fun.pond_left);
        self.fun.wet = (wet, t);
        let in_pond = self.fun.pond.now.is_some() && wet > 0.0;
        // Flying (Roger's eighteenth pass): the fox on her fox-fire, the
        // four-legged ones under balloons or a bamboo-copter; birds flap.
        let flying = self.body.motion == Motion::Fly;
        let bird = matches!(self.species, Species::Owl | Species::Chick | Species::Duck);
        let fire = self
            .fun
            .scene
            .is_some_and(|r| r.scene == Scene::Foxfire && r.phase == 1)
            || (flying && self.species == Species::Fox);
        let lifted = flying && !bird && self.species != Species::Fox;
        let hide = self.fun.hide;
        let vanished = t < self.fun.vanish_until || hide.is_some_and(|h| h.base.is_none());
        // Lying in the sunbeam, lit by it: in over a second and a half,
        // out as the beam fades.
        let basking = self
            .fun
            .scene
            .is_some_and(|r| r.scene == Scene::Sunbeam && r.phase == 1);
        let (sun, last) = self.fun.sun;
        let step = ((t - last).clamp(0.0, 0.1) / 1.5) as f32;
        let sun = if basking {
            (sun + step).min(1.0)
        } else {
            (sun - step).max(0.0)
        };
        self.fun.sun = (sun, t);
        let hug_secs = f64::from(chonkgene_core::anim::HUG_SECS);
        if self
            .fun
            .hug_by_you
            .is_some_and(|(since, ..)| t - since >= hug_secs)
        {
            self.fun.hug_by_you = None;
        }
        let any = sun > 0.0
            || lifted
            || self.fun.hands.is_some()
            || !self.fun.fx.is_empty()
            || self.fun.carry.is_some()
            || in_pond
            || fire
            || vanished
            || hide.is_some();
        if !any {
            return false;
        }
        let base = self.renderer.pixmap();
        let fits = self
            .fun
            .canvas
            .as_ref()
            .is_some_and(|c| c.width() == base.width() && c.height() == base.height());
        if !fits {
            self.fun.canvas = Pixmap::new(base.width(), base.height());
        }
        let Some(canvas) = self.fun.canvas.as_mut() else {
            return false;
        };
        let (w, h) = (base.width() as f32, base.height() as f32);
        let floor = h - FLOOR_PX * s;
        canvas.fill(tiny_skia::Color::TRANSPARENT);
        if vanished {
            return true;
        }
        if fire {
            // Behind it, then the pet, then the ones in front: drawn twice
            // over is near enough, the orbs being small.
            props::foxfire(
                canvas,
                (w / 2.0, floor - 0.4 * h),
                (0.42 * w, 0.18 * h),
                s,
                t as f32,
            );
        }
        canvas.draw_pixmap(
            0,
            0,
            base.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        if sun > 0.0 && !fire {
            props::sunlit(canvas, sun);
        }
        if let Some(pond) = self.fun.pond.now.filter(|_| in_pond) {
            // The pond's front over the lower body, matching the pond's
            // own window behind it: swimming, the waterline a little in
            // front of the pond's middle; wading, over the lower legs;
            // part of the way there as it gets in or out.
            let at = (pond.0 - self.placed.0 as f32, pond.1 - self.placed.1 as f32);
            let (_, cy, _, ry) = props::pond_oval(at, s);
            let full = if swimming {
                cy - 0.05 * ry
            } else {
                floor - WADE * h
            };
            let line = floor + (full - floor) * wet;
            props::pond_front(canvas, at, w / 2.0, line, s, t as f32);
        }
        if lifted {
            let head = self.renderer.anchor();
            // The strings are tied at its shoulders, behind the head.
            let back = (w / 2.0 - 0.15 * w * self.anim.facing, head.1 + 0.5 * h);
            if self.species == Species::Fold {
                props::copter(canvas, head, s, t as f32);
            } else {
                let hue = (self.save.id % 3) as u8;
                props::balloon(canvas, head, back, hue, s, t as f32);
            }
        }
        if let Some(hands) = &self.fun.hands {
            let since = self.fun.hug_by_you.map_or(t, |(since, _)| since);
            props::draw_hug_hands(canvas, hands, s, (t - since) as f32);
        }
        if let Some(thing) = self.fun.carry {
            let at = self.renderer.mouth(self.anim.facing);
            // Turned with the head as it gnaws or trots along.
            let look = props::Look {
                spin: if thing == Thing::Bone { 0.0 } else { 0.4 },
                t: t as f32,
                turn: self.renderer.head_roll(),
                ..props::Look::default()
            };
            props::carried(canvas, thing, at, self.anim.facing, s, look);
        }
        let head = self.renderer.anchor();
        let mouth = self.renderer.mouth(self.anim.facing);
        let hidden_part = hide.map(|h| {
            // Nothing of it past the edge of the screens shows (across a
            // monitor's edge it would).
            let edge = h.edge - self.placed.0 as f32;
            if h.side < 0.0 {
                (0.0, edge.clamp(0.0, w))
            } else {
                (edge.clamp(0.0, w), w)
            }
        });
        for &(fx, since) in &self.fun.fx {
            if let (Fx::Bubble | Fx::Dream, Some((at, size))) = (fx, self.fun.bubble_fit) {
                let t = (t - since) as f32;
                props::bubble_round(canvas, fx == Fx::Dream, at, size, s, t);
                continue;
            }
            let at = match fx {
                Fx::Hearts | Fx::Fright | Fx::Notes => head,
                Fx::Sparkles => mouth,
                Fx::Dust | Fx::Sand => (w / 2.0, floor),
                Fx::Splash => (w / 2.0, floor - 0.13 * PPU * s),
                Fx::Spell => (w / 2.0, floor),
                Fx::Bubble | Fx::Dream | Fx::Brawl => (w / 2.0, floor - 0.95 * PPU * s),
            };
            props::effect(canvas, fx, at, s, (t - since) as f32);
        }
        if let Some((x0, x1)) = hidden_part
            && let Some(r) = tiny_skia::Rect::from_ltrb(x0, 0.0, x1, h)
        {
            let clear = tiny_skia::Paint {
                blend_mode: tiny_skia::BlendMode::Clear,
                ..tiny_skia::Paint::default()
            };
            canvas.fill_rect(r, &clear, Transform::identity(), None);
        }
        true
    }
}

/// One go round the playground: the input timing, the things' physics,
/// every pet's chase, order, scene and catnip, and reactions to what you
/// are doing. `visible`: the pets are out.
pub(super) fn tick(
    ground: &mut Playground,
    pets: &mut [Pet],
    ctx: &mut Ctx,
    visible: bool,
    react: bool,
) {
    let now = ctx.world.now;
    let dt = (now - ground.last).clamp(0.0, 0.1) as f32;
    ground.last = now;
    if now >= ground.watch_at {
        ground.watch_at = now + WATCH_EVERY;
        ground.cursor = ctx.shared.cursor();
        ground
            .watcher
            .feed(now, ground.cursor, ctx.shared.idle_secs());
        ground.activity = Some(ground.watcher.activity(now));
    }
    let cursor = ground.cursor;
    // The things fall, bounce and roll; the used up go.
    let gone: Vec<u32> = ground
        .things
        .iter()
        .filter(|w| now >= w.until && w.carrier.is_none() && w.grab.is_none())
        .map(|w| w.id)
        .collect();
    for id in gone {
        for p in pets.iter_mut() {
            if p.fun.chase.as_ref().is_some_and(|(c, _)| *c == id) {
                p.fun.chase = None;
            }
        }
        if ground
            .get(id)
            .is_some_and(|w| w.prop.thing == Thing::Catnip)
        {
            for p in pets.iter_mut() {
                if p.fun.nip.is_some() {
                    p.fun.nip = Some((Nip::Leaving, now, 0));
                }
            }
        }
        ground.remove(id);
    }
    // A toy held by the mouse follows the cursor, keeping its speed for
    // the throw.
    let held = ground.things.iter().any(|w| w.grab.is_some());
    if held && let Some((cx, cy)) = ctx.shared.cursor() {
        for w in &mut ground.things {
            if let Some(g) = &mut w.grab {
                let (cx, cy) = (cx as f32, cy as f32);
                if !g.moved && (cx - g.down.0).hypot(cy - g.down.1) > DRAG_START {
                    g.moved = true;
                }
                if g.moved {
                    let to = (cx - g.at.0 + w.foot.0, cy - g.at.1 + w.foot.1);
                    if dt > 0.0 {
                        let v = ((to.0 - w.prop.pos.0) / dt, (to.1 - w.prop.pos.1) / dt);
                        let k = (dt * 12.0).min(1.0);
                        g.vel = (g.vel.0 + (v.0 - g.vel.0) * k, g.vel.1 + (v.1 - g.vel.1) * k);
                    }
                    w.prop.pos = to;
                    w.prop.vel = (0.0, 0.0);
                    w.prop.resting = false;
                }
            }
        }
    }
    for w in &mut ground.things {
        if w.carrier.is_none()
            && w.grab.is_none()
            && let Some(hit) = w.prop.step(dt, ctx.terrain)
            && w.prop.thing == Thing::Ball
            && hit > 300.0
            && ctx.sound_on
        {
            ctx.sounds.play(
                u64::MAX,
                Species::Shiba,
                Sound::Land,
                now,
                ctx.volume * 0.6,
                1.7,
            );
        }
        if w.prop.resting && w.prop.thing.falls() && w.carrier.is_none() {
            // Left lying about: it goes after a while.
            w.until = w.until.min(now + TOY_STAYS);
        }
    }
    if !visible {
        ground.set_visible(ctx.shared, now, false);
        return;
    }

    let activity = if react {
        ground.activity.unwrap_or(Activity::Busy)
    } else {
        Activity::Busy
    };
    let typing_for = ground.watcher.typing_for(now);
    ground.standing = pets
        .iter()
        .map(|p| {
            let (x, y) = p.body.feet();
            let settled = p.mind.asleep() || p.fun.busy(now);
            (p.save.id, x, y, FOOTPRINT * p.body.size.0, settled)
        })
        .collect();
    let scene_on = pets.iter().any(|p| p.fun.scene.is_some());
    // The fox calling up its fire: where, for the cats to bat at it.
    let fire = pets.iter().find_map(|p| {
        p.fun
            .scene
            .filter(|r| r.scene == Scene::Foxfire && r.phase == 1)
            .map(|_| Fire {
                fox: p.save.id,
                x: p.center_x(),
                feet: p.body.feet().1,
                half: 0.5 * p.body.size.0,
            })
    });
    ground.stage_step(pets, now, dt);
    ground.pond_step(pets, now, dt);
    water_fight(ground, pets, ctx);
    let mut start_scene = !scene_on;
    for i in 0..pets.len() {
        chase(ground, pets, i, ctx);
        let p = &mut pets[i];
        tickled_loose(ground, p, ctx);
        order(p, ctx, cursor);
        smell_nip(ground, p, now);
        nip(ground, p, ctx);
        swat(p, fire, ctx);
        if p.fun.pounce_at.is_some_and(|at| now >= at) {
            p.fun.pounce_at = None;
            let free = p.press.is_none() && matches!(p.body.motion, Motion::Idle | Motion::Walk);
            if free && !p.mind.asleep() {
                p.anim.pounce();
            }
        }
        if start_scene && maybe_scene(ground, p, ctx) {
            start_scene = false;
        }
        scene(ground, p, ctx);
        // Every frame, busy or not: it also stops watching when busy.
        curious(p, ctx, activity, cursor, dt);
        if !p.fun.busy(now) || now < p.fun.lounge_until {
            react_to(p, ctx, activity, typing_for, cursor, dt);
        }
        // Busy with something fun, it shows it all the while: the tail,
        // ears and wings keep going (Roger: scenes looked still).
        p.anim.lively = p.fun.scene.is_some()
            || p.fun.chase.is_some()
            || p.fun.nip.is_some()
            || p.fun.swat.is_some()
            || p.playing
            || p.play_fight;
        // In a pond: the duckling swims as soon as its feet are over the
        // water, anyone else wades; a splash going in.
        let under = match p.body.motion {
            Motion::Idle | Motion::Walk | Motion::Slide => {
                ground.pond_under(p.body.feet(), p.fun.pond.now)
            }
            // A hop or a flutter keeps it in for the hold; picked up, out.
            Motion::Fall | Motion::Fly | Motion::Glide => None,
            Motion::Held => {
                p.fun.pond.now = None;
                None
            }
        };
        if p.fun.pond.step(under, now) {
            p.fun.effect(Fx::Splash, now);
        }
        p.fun.on_sand = ground.on_beach(p.body.feet());
        // Its pond drying up, the water on it goes too; gone, it is out.
        if let Some(pos) = p.fun.pond.now {
            match ground.pond_left(pos, now) {
                Some(left) => p.fun.pond_left = left,
                None => p.fun.pond.leave(now),
            }
        }
        let pond = p.fun.pond.now;
        // How the body is held this frame.
        p.anim.posture = if p.body.motion == Motion::Fly {
            Posture::Fly
        } else if p.body.motion == Motion::Slide {
            Posture::Slide
        } else if p.body.motion == Motion::Glide {
            Posture::Glide
        } else if pond.is_some() && p.species == Species::Duck {
            Posture::Swim
        } else if now < p.fun.hold_until || now < p.fun.lounge_until || now < p.fun.cower_until {
            p.anim.posture.max_hold()
        } else {
            Posture::Free
        };
    }
    // Rings on each pond from whoever is in it.
    for w in &mut ground.things {
        if w.prop.thing == Thing::Pond {
            w.stirs = pets
                .iter()
                .filter(|p| p.fun.pond.now == Some(w.prop.pos))
                .map(|p| p.body.feet().0)
                .collect();
        }
    }
    ground.set_visible(ctx.shared, now, true);
    // In the same go round as a new thing's first picture, so it never
    // shows over the pets for a tick (a new pond covered the sleeping owl
    // for 0.1 s in Roger's clip).
    if std::mem::take(&mut ground.restack) {
        for p in pets.iter() {
            p.raise(ctx.shared);
        }
    }
}

trait Held {
    /// The held posture to keep (sitting or lying), or lying if none.
    fn max_hold(self) -> Posture;
}

impl Held for Posture {
    fn max_hold(self) -> Posture {
        match self {
            Posture::Sit => Posture::Sit,
            _ => Posture::Lie,
        }
    }
}

/// A pet going after a thrown toy.
fn chase(ground: &mut Playground, pets: &mut [Pet], i: usize, ctx: &mut Ctx) {
    let now = ctx.world.now;
    let Some((id, mut c)) = pets[i].fun.chase.take() else {
        return;
    };
    let me = pets[i].save.id;
    if ground.get(id).is_none() {
        pets[i].fun.carry = None;
        pets[i].stop();
        return;
    }
    let p = &mut pets[i];
    // The nose reaches the ball partway into the bump.
    if let Some((when, vel)) = p.fun.nudge
        && now >= when
    {
        p.fun.nudge = None;
        if let Some(w) = ground.get_mut(id) {
            w.prop.bat(vel);
        }
    }
    let Some(w) = ground.get(id) else {
        return;
    };
    let prop = w.prop;
    let taken = w.carrier.or(w.claimed).is_some_and(|c| c != me);
    c.set_reach(p.body.reach(ctx.terrain));
    c.set_moving(p.body.speed > 0.0 || p.body.motion != Motion::Idle);
    let mut order = c.tick(now, p.body.feet(), p.body.size, &prop, taken);
    // The owl flies to the ball, and home with it.
    let flies = p.species.fetch() == Fetch::Watch && prop.thing == Thing::Ball;
    if flies && let Order::Run { x, .. } = order {
        let (fx, fy) = p.body.feet();
        let far = (x - fx).abs() > 0.3 * p.body.size.0;
        if far && matches!(p.body.motion, Motion::Idle | Motion::Walk) {
            let y = ctx.terrain.floor_under(x).map_or(fy, |l| l.y);
            p.body.fly_to(x, y);
        }
        order = Order::Wait;
    }
    // Out of reach (across a gap between monitors, past the edge of the
    // screens): put down where it got to, or given up on.
    if let Order::Run { x, pace } = order
        && p.walk_to(x, pace, ctx.terrain) == Heading::Blocked
        && (c.carrying() || !prop.flying())
    {
        println!("{}: the {:?} is out of reach", p.save.name, prop.thing);
        order = if c.carrying() {
            Order::Drop
        } else {
            Order::Done
        };
    }
    let mut keep = true;
    match order {
        Order::Watch => {
            p.stop();
            if p.body.speed == 0.0 {
                p.body.facing = if prop.pos.0 < p.body.feet().0 {
                    -1.0
                } else {
                    1.0
                };
            }
        }
        Order::Run { .. } => {}
        Order::Grab => {
            let (pw, ph) = p.renderer.size();
            crate::diag::diag!(
                "{}: got the {:?}, its mouth at {:?} in a {pw}x{ph} picture, facing {}",
                p.save.name,
                prop.thing,
                p.renderer.mouth(p.anim.facing),
                p.anim.facing
            );
            if prop.flying() && p.body.motion != Motion::Fall {
                p.anim.pounce();
            }
            p.stop();
            p.fun.carry = Some(prop.thing);
            p.fun.effect(Fx::Sparkles, now);
            if let Some(w) = ground.get_mut(id) {
                w.carrier = Some(me);
            }
            p.bell(ctx);
        }
        Order::Drop => {
            p.stop();
            p.fun.carry = None;
            let (fx, fy) = p.body.feet();
            if let Some(w) = ground.get_mut(id) {
                w.carrier = None;
                w.prop = Prop {
                    resting: true,
                    vel: (0.0, 0.0),
                    ..Prop::put(
                        prop.thing,
                        (fx + p.body.facing * 0.2 * p.body.size.0, fy),
                        prop.scale,
                    )
                };
                w.until = now + TOY_STAYS;
            }
            // Back where you threw it, wagging, a lick of the nose: throw
            // it again!
            p.body.facing = -p.body.facing;
            p.anim.lick();
            p.say(ctx, Line::Fetch);
            cheer(p, ctx);
            keep = false;
        }
        Order::Bat { side } => {
            if p.try_gesture(Act::Wave)
                && let Some(w) = ground.get_mut(id)
            {
                let s = prop.scale.max(0.5);
                w.prop.bat((side * 380.0 * s, -260.0 * s));
            }
        }
        Order::Peck => {
            p.try_gesture(Act::Peck);
        }
        Order::Leap { x, height } => {
            println!("{}: leaps for the {:?}", p.save.name, prop.thing);
            p.stop();
            p.body.spring(x, height);
        }
        Order::Wait => {}
        Order::Nose { side, speed } => {
            p.body.facing = side;
            // A nose, a paw, a beak; the push lands partway in.
            let (act, lands) = match p.species.fetch() {
                Fetch::Bat => (Act::Wave, 0.35),
                Fetch::Chase => (Act::Peck, 0.3),
                _ => (Act::Nudge, 0.4),
            };
            let nosed = p.try_gesture(act);
            if nosed {
                let s = prop.scale.max(0.5);
                p.fun.nudge = Some((now + lands, (side * speed, -50.0 * s)));
                if let Some(w) = ground.get_mut(id) {
                    w.claimed = Some(me);
                }
            }
            println!(
                "{}: noses the {:?} on ({}; speed {:.2}, walking {}, doing {:?}, {:?})",
                p.save.name,
                prop.thing,
                if nosed { "bump" } else { "not free" },
                p.body.speed,
                p.anim.walking,
                p.anim.doing(),
                p.anim.expression,
            );
        }
        Order::Brought => {
            println!("{}: brought the {:?} back", p.save.name, prop.thing);
            // Nosed all the way back to you, wagging: throw it again!
            p.stop();
            p.body.facing = -p.body.facing;
            p.anim.lick();
            p.say(ctx, Line::Fetch);
            cheer(p, ctx);
            keep = false;
        }
        Order::Done => {
            p.stop();
            if p.fun.carry.take().is_some()
                && let Some(w) = ground.get_mut(id)
            {
                let (fx, fy) = p.body.feet();
                w.carrier = None;
                w.prop = Prop::put(prop.thing, (fx, fy), prop.scale);
            }
            // A cat that has had enough sits and washes; a bird gives up.
            match p.species.fetch() {
                Fetch::Bat => {
                    p.try_gesture(Act::Groom);
                }
                Fetch::Chase => p.say(ctx, Line::Fetch),
                _ => {}
            }
            keep = false;
        }
    }
    if keep {
        p.fun.chase = Some((id, c));
    } else if let Some(w) = ground.get_mut(id)
        && w.claimed == Some(me)
    {
        w.claimed = None;
    }
}

/// Scratched hard enough with something in its mouth, it laughs and lets
/// go: the toy (or the bone) drops at its feet (Roger, 2026-11-06).
fn tickled_loose(ground: &mut Playground, p: &mut Pet, ctx: &mut Ctx) {
    let Some(thing) = p.fun.carry else {
        return;
    };
    if p.anim.params().touch.scratch < LET_GO {
        return;
    }
    let now = ctx.world.now;
    let me = p.save.id;
    p.fun.carry = None;
    let (fx, fy) = p.body.feet();
    let at = (fx + p.body.facing * 0.25 * p.body.size.0, fy);
    // Whatever it held: a toy it was bringing back, or its scene's bone.
    let id = p
        .fun
        .chase
        .take()
        .map(|(id, _)| id)
        .or_else(|| p.fun.scene.and_then(|r| r.prop));
    if let Some(w) = id.and_then(|id| ground.get_mut(id))
        && w.carrier == Some(me)
    {
        w.carrier = None;
        w.claimed = None;
        w.prop = Prop::put(thing, at, w.prop.scale);
        w.until = w.until.max(now + TOY_STAYS.min(10.0));
    }
    // A scene ends here (its bone goes when the scene does).
    if let Some(run) = &mut p.fun.scene {
        run.until = now;
        run.prop = None;
    }
    p.stop();
    p.say(ctx, Line::Laugh);
    p.giggle(now);
    println!("{}: tickled into dropping the {thing:?}", p.save.name);
}

/// Pleased: a hop and hearts.
fn cheer(p: &mut Pet, ctx: &mut Ctx) {
    let now = ctx.world.now;
    if p.anim.expression != Expression::Happy {
        p.anim.cheer();
    }
    p.anim.expression = Expression::Happy;
    p.happy_until = now + super::HAPPY_SECS;
    p.fun.effect(Fx::Hearts, now);
}

/// Doing as it was asked, once it gets round to it.
fn order(p: &mut Pet, ctx: &mut Ctx, cursor: Option<(i32, i32)>) {
    let now = ctx.world.now;
    if let Some(x) = p.fun.goto {
        if p.walk_to(x, 1.6, ctx.terrain) != Heading::Going || p.mind.asleep() {
            p.fun.goto = None;
            p.stop();
            // There: sits by you.
            p.anim.posture = Posture::Sit;
            p.fun.hold_until = now + 4.0;
            if p.fun.curled {
                p.fun.hold_until = 0.0;
                p.fun.lounge_until = now + 30.0;
                p.anim.posture = Posture::Lie;
            }
        }
        return;
    }
    let Some((command, due, moody)) = p.fun.order else {
        return;
    };
    if now < due {
        return;
    }
    if p.mind.asleep() {
        p.fun.order = None;
        return;
    }
    // A mood passes once it gets round to it.
    if matches!(
        p.anim.expression,
        Expression::FedUp | Expression::Sulk | Expression::Giggle
    ) {
        p.anim.expression = Expression::Neutral;
    }
    let done = match command {
        Command::Sit | Command::Lie => {
            p.stop();
            p.anim.posture = if command == Command::Sit {
                Posture::Sit
            } else {
                Posture::Lie
            };
            p.fun.hold_until = now + stay_secs(command);
            true
        }
        Command::Come => {
            let to = cursor.map_or(p.body.feet().0, |c| c.0 as f32);
            p.fun.goto = Some(to);
            true
        }
        Command::Spin => p.try_gesture(Act::Spin),
        Command::Paw => p.try_gesture(Act::Paw),
    };
    // Still walking: it tries again in a moment (but not for ever).
    if done || now > due + 3.0 {
        println!("{}: asked to {command:?}, done: {done}", p.save.name);
        p.fun.order = None;
        if done {
            if moody {
                p.say(ctx, Line::ObeyLater);
            } else if p.rng.f32() < 0.5 {
                p.say(ctx, Line::Obey);
            }
        }
    }
}

/// At the catnip (PLAN §8 "Catnip"): cats that like it sniff, rub their
/// cheeks on it and roll about blissed; the rest stay in character.
fn nip(ground: &mut Playground, p: &mut Pet, ctx: &mut Ctx) {
    let now = ctx.world.now;
    let Some((stage, since, n)) = p.fun.nip else {
        return;
    };
    if p.press.is_some() || p.mind.asleep() {
        p.fun.nip = None;
        return;
    }
    let Some(sprig) = ground
        .things
        .iter()
        .find(|w| w.prop.thing == Thing::Catnip)
        .map(|w| w.prop)
    else {
        p.fun.nip = None;
        return;
    };
    let (fx, fy) = p.body.feet();
    let (w, h) = p.body.size;
    let likes = p.species.likes_catnip(p.save.id);
    let cat = matches!(p.species, Species::Munchkin | Species::Fold);
    // Done with it for now: a cat that likes it comes back in a while,
    // the rest leave this sprig be.
    let again = now + if likes { NIP_AGAIN } else { CATNIP_SECS };
    match stage {
        Nip::Going => {
            let dx = sprig.pos.0 - fx;
            // Out of reach (another window, too far): not worth it.
            if dx.abs() > REACH || (fy - sprig.pos.1).abs() > 0.3 * h {
                println!("{}: the catnip is out of reach", p.save.name);
                p.fun.nip = None;
                p.fun.nip_again = again;
                return;
            }
            if p.species == Species::Owl {
                p.body.facing = dx.signum();
                p.say(ctx, Line::Catnip);
                p.fun.nip = None;
                p.fun.nip_again = again;
                return;
            }
            // Beside it, so it shows.
            let x = sprig.pos.0 - dx.signum() * 0.18 * w;
            let heading = p.walk_to(x, 1.4, ctx.terrain);
            if heading == Heading::Blocked {
                println!("{}: the catnip is out of reach", p.save.name);
                p.fun.nip = None;
                p.fun.nip_again = again;
                return;
            }
            if heading == Heading::There {
                println!("{}: at the catnip (likes it: {likes})", p.save.name);
                p.body.facing = dx.signum();
                p.fun.nip = Some((Nip::At(0), now, 0));
            }
        }
        Nip::At(step) => {
            if p.anim.doing().is_some() {
                return;
            }
            let next = |p: &mut Pet, act: Act| p.try_gesture(act);
            match (p.species, likes, step) {
                (_, _, 0) => {
                    if next(p, sniff_act(p.species)) {
                        p.fun.nip = Some((Nip::At(1), now, n));
                    }
                }
                // Cats that like it: rub, roll, rub, roll… until it is gone.
                (_, true, _) => {
                    let act = if step % 2 == 1 { Act::Rub } else { Act::Bliss };
                    if next(p, act) {
                        if act == Act::Bliss && n == 0 {
                            p.say(ctx, Line::Catnip);
                            p.fun.effect(Fx::Hearts, now);
                        }
                        p.fun.nip = Some((Nip::At(step + 1), now, n + 1));
                    }
                }
                (_, false, _) if cat => {
                    p.say(ctx, Line::CatnipMeh);
                    p.fun.nip = Some((Nip::Leaving, now, n));
                }
                (Species::Shiba, ..) => {
                    p.say(ctx, Line::Catnip);
                    p.try_gesture(Act::Shake);
                    p.fun.nip = Some((Nip::Leaving, now, n));
                }
                (Species::Fox, ..) => {
                    if next(p, Act::Wave) {
                        p.say(ctx, Line::Catnip);
                        p.fun.nip = Some((Nip::Leaving, now, n));
                    }
                }
                (Species::Chick | Species::Duck, ..) => {
                    if step < 3 {
                        if next(p, Act::Peck) {
                            p.fun.nip = Some((Nip::At(step + 1), now, n));
                        }
                    } else {
                        p.say(ctx, Line::Catnip);
                        p.fun.nip = Some((Nip::Leaving, now, n));
                    }
                }
                _ => p.fun.nip = Some((Nip::Leaving, now, n)),
            }
        }
        Nip::Leaving => {
            if p.anim.doing().is_none() || now - since > 4.0 {
                p.fun.nip = None;
                p.fun.nip_again = again;
                if likes {
                    cheer(p, ctx);
                }
            }
        }
    }
}

/// The fox's fire, as the cats see it (see [`swat`]).
#[derive(Clone, Copy, Debug)]
struct Fire {
    fox: u64,
    x: f32,
    feet: f32,
    half: f32,
}

/// A cat near the fox calling up its fire runs over and bats at the orbs
/// hanging round it, now and then with a pounce, until the fire is gone.
fn swat(p: &mut Pet, fire: Option<Fire>, ctx: &mut Ctx) {
    let now = ctx.world.now;
    let cat = matches!(p.species, Species::Munchkin | Species::Fold);
    let Some(f) = fire else {
        if p.fun.swat.take().is_some() {
            cheer(p, ctx);
        }
        return;
    };
    if !cat || p.save.id == f.fox {
        return;
    }
    if p.fun.swat.is_none() {
        let near = (p.center_x() - f.x).abs() < SWAT_REACH;
        let level = (p.body.feet().1 - f.feet).abs() < 0.3 * p.body.size.1;
        if !near || !level || p.fun.busy(now) || !p.free_to_play(now) {
            return;
        }
        println!("{}: bats at the fox-fire", p.save.name);
        p.fun.swat = Some(f.fox);
    }
    if p.press.is_some() || p.mind.asleep() {
        p.fun.swat = None;
        return;
    }
    // Beside the fox, on its own side, where the orbs hang.
    let side = if p.center_x() < f.x { -1.0 } else { 1.0 };
    let spot = f.x + side * (f.half + 0.25 * p.body.size.0);
    let to_feet = p.body.feet().0 - p.center_x();
    if p.walk_to(spot + to_feet, 1.6, ctx.terrain) != Heading::Going {
        p.body.facing = -side;
        if p.anim.doing().is_none() && !p.anim.hopping() {
            if p.rng.f32() < 0.25 {
                p.anim.pounce();
            } else {
                p.try_gesture(Act::Paw);
            }
        }
    }
}

/// Catnip lying on the floor within reach: a pet free to play goes over
/// (not while it is still falling, nor straight back after a visit).
fn smell_nip(ground: &Playground, p: &mut Pet, now: f64) {
    if p.fun.nip.is_some()
        || now < p.fun.nip_again
        || p.fun.chase.is_some()
        || p.mind.asleep()
        || !p.free_to_play(now)
    {
        return;
    }
    let Some(sprig) = ground
        .things
        .iter()
        .find(|w| w.prop.thing == Thing::Catnip && w.grab.is_none())
        .map(|w| w.prop)
    else {
        return;
    };
    let landed = sprig.resting || sprig.vel.1 == 0.0;
    let (fx, fy) = p.body.feet();
    let h = p.body.size.1;
    if landed && (sprig.pos.0 - fx).abs() <= REACH && (fy - sprig.pos.1).abs() <= 0.3 * h {
        p.fun.nip = Some((Nip::Going, now, 0));
    }
}

fn sniff_act(species: Species) -> Act {
    match species {
        Species::Chick | Species::Duck | Species::Owl => Act::Peck,
        _ => Act::Sniff,
    }
}

/// Now and then, a pet with nothing on starts its own little scene
/// (one at a time on the desktop); true if one began.
fn maybe_scene(ground: &mut Playground, p: &mut Pet, ctx: &mut Ctx) -> bool {
    let now = ctx.world.now;
    if p.fun.next_scene == 0.0 {
        p.fun.next_scene = now + SCENE_GAP * (0.3 + f64::from(ground.roll()));
        return false;
    }
    if now < p.fun.next_scene
        || !p.free_to_play(now)
        || p.fun.busy(now)
        || p.playing
        || p.anim.expression != Expression::Neutral
        || p.body.motion != Motion::Idle
        || p.body.on_window()
    {
        return false;
    }
    p.fun.next_scene = now + SCENE_GAP * (1.0 + f64::from(ground.roll()));
    let scenes = p.species.scenes();
    let pick = scenes[(ground.roll() * scenes.len() as f32) as usize % scenes.len()];
    start_scene(ground, p, ctx, pick)
}

/// Start `scene` for `p` now (its thing put out beside it).
pub(super) fn start_scene(
    ground: &mut Playground,
    p: &mut Pet,
    ctx: &mut Ctx,
    scene: Scene,
) -> bool {
    let now = ctx.world.now;
    let s = p.fun.scale;
    let (fx, fy) = p.body.feet();
    let (w, _) = p.body.size;
    // The side with more room.
    let a = ctx.area;
    let side = if fx < a.x + a.w / 2.0 { 1.0 } else { -1.0 };
    let mut beside = fx + side * 1.1 * w;
    if let Some(thing) = scene
        .thing()
        .filter(|t| matches!(t, Thing::Pond | Thing::Sunbeam))
    {
        // Big things on the floor: nobody settled may be in them (an owl
        // asleep where a pond came out sat on its near rim); one awake and
        // free is welcome in the water (the pond is big now, and too
        // strict a rule left no room on a crowded desktop).
        let others: Vec<(f32, f32)> = ground
            .standing
            .iter()
            .filter(|o| o.0 != p.save.id && o.4 && (o.2 - fy).abs() < 30.0 * s)
            .map(|o| (o.1, o.3))
            .collect();
        let half = 0.5 * thing.size().0 * s;
        match free_spot(fx, side, w, half, (a.x, a.x + a.w), &others) {
            Some(x) => beside = x,
            None => {
                diag!("{}: no room for its {scene:?}", p.save.name);
                return false;
            }
        }
    }
    let mut prop = None;
    let mut spot = beside;
    if let Some(thing) = scene.thing() {
        let at = match scene {
            Scene::Perch => {
                // Out from the nearer edge of the screen, well up; the owl
                // lands far enough out for all of it to stay on the screen.
                let left = fx < a.x + a.w / 2.0;
                let bw = thing.size().0 * s;
                let out = (0.5 * w + 4.0).min(0.85 * bw);
                let (x, land) = if left {
                    (a.x + 0.5 * bw, a.x + out)
                } else {
                    (a.x + a.w - 0.5 * bw, a.x + a.w - out)
                };
                spot = land;
                (x, fy - (a.h * 0.45).min(420.0 * s))
            }
            Scene::Butterfly => (beside, fy - 90.0 * s),
            // The light comes down on it where it stands.
            Scene::Dance => {
                spot = fx;
                (fx, fy)
            }
            _ => (beside, fy),
        };
        let mut put = Prop::put(thing, at, s);
        put.resting = true;
        let until = now + scene.secs() + 1.0;
        // A pond comes with golden sand round it, laid first so it lies
        // under the water (Roger's eighteenth pass).
        if thing == Thing::Pond {
            let mut sand = Prop::put(Thing::Beach, at, s);
            sand.resting = true;
            ground.add(ctx.el, ctx.shared, sand, now, until);
        }
        let Some(id) = ground.add(ctx.el, ctx.shared, put, now, until) else {
            return false;
        };
        if scene == Scene::Perch {
            let left = at.0 < a.x + a.w / 2.0;
            if let Some(w) = ground.get_mut(id) {
                w.flip = left;
            }
            let half = 0.48 * thing.size().0 * s;
            ground.perch = Some((id, at.1, at.0 - half, at.0 + half));
            ground.terrain_dirty = true;
        }
        prop = Some(id);
    }
    p.fun.scene = Some(Run {
        scene,
        prop,
        phase: 0,
        since: now,
        until: now + scene.secs(),
        home: (fx, fy),
        spot,
        keep: false,
        count: 0,
    });
    println!("{}: starts its scene, {scene:?}", p.save.name);
    // A dance says its own line once the music starts.
    if scene != Scene::Dance {
        p.say(ctx, Line::Scene);
    }
    true
}

/// How much of a pet's window its body stands on, each side of its
/// middle, as a share of the window's width.
const FOOTPRINT: f32 = 0.3;

/// Where to put the middle of a thing `half` wide on the floor beside a
/// pet `w` wide with its feet at `fx`: the `side` with more room first,
/// a little further out if someone `others` (their middle, half width)
/// stands there, never past the edges `x`; none if every place is taken.
fn free_spot(
    fx: f32,
    side: f32,
    w: f32,
    half: f32,
    x: (f32, f32),
    others: &[(f32, f32)],
) -> Option<f32> {
    let margin = 0.1 * w;
    [side, -side]
        .into_iter()
        .flat_map(|side| [1.1, 1.6, 2.2, 3.0].map(|k| fx + side * k * w))
        .find(|&c| {
            c - half >= x.0
                && c + half <= x.1
                && others
                    .iter()
                    .all(|&(ox, oh)| (ox - c).abs() >= half + oh + margin)
        })
}

/// A scene under way, phase by phase.
fn scene(ground: &mut Playground, p: &mut Pet, ctx: &mut Ctx) {
    let now = ctx.world.now;
    let Some(mut run) = p.fun.scene else {
        return;
    };
    // Picked up, or fallen asleep: it ends (a touch or a scratch does
    // not; scratched hard enough, it laughs and drops its bone).
    let lifted = p.press.as_ref().is_some_and(|press| press.moved);
    let interrupted = lifted || p.mind.asleep() || p.in_skit;
    let prop = run.prop.and_then(|id| ground.get(id)).map(|w| w.prop);
    let end = |ground: &mut Playground, p: &mut Pet, run: &Run| {
        if let Some(id) = run.prop.filter(|_| !run.keep) {
            ground.remove(id);
        }
        if run.scene == Scene::Dance {
            p.fun.danced = now;
            if ground.tune.is_some_and(|(id, _)| id == p.save.id) {
                ground.tune = None;
            }
        }
        p.fun.carry = None;
        p.fun.scene = None;
        p.fun.hold_until = 0.0;
        p.stop();
    };
    if interrupted || (run.prop.is_some() && prop.is_none()) {
        if p.body.motion == Motion::Fly {
            p.body.motion = Motion::Fall;
            p.body.flight = None;
        }
        end(ground, p, &run);
        return;
    }
    let w = p.body.size.0;
    match (run.scene, run.phase) {
        // Off to the thing.
        (Scene::Perch, 0) => {
            // Once the branch is in the terrain, fly up to it.
            let Some((id, y, ..)) = ground.perch else {
                return end(ground, p, &run);
            };
            let ready = ctx
                .terrain
                .ledges()
                .iter()
                .any(|l| l.on == chonkgene_core::terrain::Support::Perch(id));
            if ready && p.body.motion != Motion::Fly {
                p.stop();
                p.body.fly_to(run.spot, y);
                run.phase = 1;
            }
        }
        (Scene::Perch, 1) => {
            if p.body.motion == Motion::Idle && p.body.flight.is_none() {
                // Perched: stays put, looking about, until it is time.
                p.stop();
                if now >= run.until {
                    p.body.fly_to(run.home.0, run.home.1);
                    run.phase = 2;
                }
            }
        }
        (Scene::Perch, _) => {
            if p.body.motion != Motion::Fly {
                return end(ground, p, &run);
            }
        }
        (Scene::Foxfire, 0) => {
            // Calling up its fire: its face glows all the while.
            run.phase = 1;
            p.glow = Some((now, run.until));
        }
        (Scene::Foxfire, _) => {
            // Swinging its tails in the fire, the bell ringing, now and
            // then (Roger: no spinning round).
            if now >= run.until {
                p.fun.scene = None;
                cheer(p, ctx);
                return;
            }
            if p.anim.doing().is_none() && ground.roll() < 0.02 && p.try_gesture(Act::Swish) {
                p.bell(ctx);
            }
        }
        (Scene::Butterfly, _) => {
            // The butterfly flutters about; the cat stalks and pounces as
            // at the grass stalk.
            let Some(id) = run.prop else {
                return end(ground, p, &run);
            };
            let t = (now - run.since) as f32;
            let s = p.fun.scale;
            let (hx, hy) = (run.home.0 + 1.1 * w, run.home.1 - 90.0 * s);
            let leaving = now >= run.until;
            let away = if leaving {
                (now - run.until) as f32
            } else {
                0.0
            };
            let at = (
                hx + 120.0 * s * (0.7 * t).sin() + 40.0 * s * (2.3 * t).sin() + 200.0 * s * away,
                hy + 35.0 * s * (1.1 * t).sin() - 160.0 * s * away,
            );
            if let Some(b) = ground.get_mut(id) {
                b.prop.pos = at;
            }
            if leaving {
                p.stop();
                p.anim.watch(None);
                if away > 2.0 {
                    end(ground, p, &run);
                    return;
                }
            } else {
                p.play(at, now, ctx.terrain);
            }
        }
        (Scene::Dance, phase) => {
            if dance(ground, p, ctx, &mut run, phase) {
                return end(ground, p, &run);
            }
        }
        (_, 0) => {
            let Some(at) = prop else {
                return end(ground, p, &run);
            };
            // Joining others in a pond: its own place in it.
            let to = if run.keep { run.spot } else { at.pos.0 };
            let heading = p.walk_to(to, 1.3, ctx.terrain);
            if heading == Heading::Blocked {
                return end(ground, p, &run);
            }
            if heading == Heading::There {
                run.phase = 1;
                run.since = now;
                match run.scene {
                    Scene::Bone => {
                        // Picks it up and lies down to chew it.
                        if let Some(id) = run.prop
                            && let Some(b) = ground.get_mut(id)
                        {
                            b.carrier = Some(p.save.id);
                        }
                        p.fun.carry = Some(Thing::Bone);
                        p.anim.posture = Posture::Lie;
                    }
                    Scene::Sunbeam => p.anim.posture = Posture::Lie,
                    _ => {}
                }
            }
        }
        (_, 1) => {
            if now >= run.until {
                run.phase = 2;
                p.fun.carry = None;
                p.fun.scene = Some(run);
                return;
            }
            p.fun.hold_until = now + 0.5;
            match run.scene {
                Scene::Bone => {
                    p.anim.posture = Posture::Lie;
                    if p.anim.doing().is_none() {
                        p.try_gesture(Act::Chew);
                    }
                }
                Scene::Seeds => {
                    p.fun.hold_until = 0.0;
                    if p.anim.doing().is_none() {
                        p.try_gesture(Act::Peck);
                    }
                }
                Scene::Sunbeam => {
                    p.anim.posture = Posture::Lie;
                }
                Scene::Pond => {
                    // Paddling slowly to and fro, well inside the water:
                    // over to one side, a stop, round, and back.
                    p.fun.hold_until = 0.0;
                    if let Some(pond) = prop {
                        // Each about its own place, so several fit.
                        let half = POND_PADDLE * pond.px().0 * if run.keep { 0.5 } else { 1.0 };
                        let x = run.spot + p.body.facing * half;
                        let near = 4.0 * p.fun.scale;
                        if p.body.head_for(x, 0.35, near, ctx.terrain) == Heading::There
                            && p.body.speed == 0.0
                        {
                            p.body.facing = -p.body.facing;
                        }
                    }
                }
                _ => {}
            }
        }
        (_, _) => {
            // Over: out of the water with a shake, or up from the sun.
            p.fun.hold_until = 0.0;
            if run.scene == Scene::Pond {
                let Some(pond) = prop else {
                    return end(ground, p, &run);
                };
                let out = pond.pos.0 + p.body.facing * (0.5 * pond.px().0 + 0.3 * w);
                if p.walk_to(out, 1.0, ctx.terrain) == Heading::Going {
                    return;
                }
                p.try_gesture(Act::Shake);
                p.fun.effect(Fx::Splash, now);
            }
            end(ground, p, &run);
            cheer(p, ctx);
            return;
        }
    }
    p.fun.scene = Some(run);
}

/// Water thrown in a water fight flies this long, seconds.
const SPRAY_SECS: f64 = 0.6;
/// A pet that went into a pond to join in stays this long, and goes in
/// again only this long after.
const BATH_SECS: f64 = 18.0;
const BATH_AGAIN: f64 = 40.0;

impl Playground {
    /// With a pond out and 非理性模式 on, pets near it come and get in now
    /// and then (Roger: room for more of them, a water fight), each to its
    /// own place in the water.
    fn pond_step(&mut self, pets: &mut [Pet], now: f64, dt: f32) {
        if !self.irrational {
            return;
        }
        let ponds: Vec<(u32, (f32, f32), f32)> = self
            .things
            .iter()
            .filter(|w| w.prop.thing == Thing::Pond)
            .map(|w| (w.id, w.prop.pos, w.prop.px().0))
            .collect();
        for (id, (px, py), width) in ponds {
            let mut inside = pets
                .iter()
                .filter(|p| p.fun.scene.is_some_and(|r| r.prop == Some(id)))
                .count();
            for p in pets.iter_mut() {
                if inside >= 5 {
                    break;
                }
                let rested = p.fun.bathed == 0.0 || now - p.fun.bathed >= BATH_AGAIN;
                if !p.free_to_play(now) || p.fun.busy(now) || p.playing || !rested {
                    continue;
                }
                let (fx, fy) = p.body.feet();
                let s = p.fun.scale;
                if (fy - py).abs() > 30.0 * s || (fx - px).abs() > 700.0 * s {
                    continue;
                }
                if self.roll() >= 0.25 * dt {
                    continue;
                }
                // Places along the water, apart, some further in.
                let slot = [-0.36, 0.36, -0.18, 0.18, 0.0][inside % 5] * width;
                p.fun.pond_deep = [0.30, 0.36, 0.24, 0.40, 0.33][inside % 5];
                diag!("{}: gets in the pond", p.save.name);
                p.fun.bathed = now;
                p.fun.scene = Some(Run {
                    scene: Scene::Pond,
                    prop: Some(id),
                    phase: 0,
                    since: now,
                    until: now + BATH_SECS,
                    home: (fx, fy),
                    spot: px + slot,
                    keep: true,
                    count: 0,
                });
                inside += 1;
            }
        }
    }
}

/// Two or more in the same pond throw water at each other (Roger: 打水仗):
/// one scoops it at the nearest other (paws, wings, the fox's tails), it
/// flies over in an arc, and the other starts, then shakes it off, laughs,
/// or throws some back.
fn water_fight(ground: &mut Playground, pets: &mut [Pet], ctx: &mut Ctx) {
    let now = ctx.world.now;
    // Water landing.
    let landed: Vec<(u64, f32)> = ground
        .hits
        .iter()
        .filter(|h| now >= h.1)
        .map(|h| (h.0, h.2))
        .collect();
    ground.hits.retain(|h| now < h.1);
    for (id, from) in landed {
        let Some(p) = pets.iter_mut().find(|p| p.save.id == id) else {
            continue;
        };
        if p.fun.pond.now.is_none() || p.press.is_some() {
            continue;
        }
        let (w, h) = p.renderer.size();
        let side = if from > p.center_x() { 0.9 } else { 0.1 };
        p.anim.flinch((side * w as f32, 0.35 * h as f32));
        p.fun.effect(Fx::Splash, now);
        let roll = ground.roll();
        if roll < 0.35 {
            // Laughing it off.
            p.anim.expression = Expression::Giggle;
            p.giggle_until = Some(now + 1.4);
            if ctx.sound_on {
                let pitch = p.pitch();
                ctx.sounds
                    .play(id, p.species, Sound::Laugh, now, ctx.volume, pitch);
            }
        } else if roll < 0.65 {
            p.try_gesture(Act::Shake);
            if ground.roll() < 0.5 {
                p.say(ctx, Line::Splashed);
            }
        } else {
            // Throwing some back, straight away.
            ground.splash_back = Some(id);
            ground.splash_at = ground.splash_at.min(now + 0.5);
        }
    }
    if now < ground.splash_at {
        return;
    }
    ground.splash_at = now + 1.3 + 1.4 * f64::from(ground.roll());
    // Who is in which pond, free to throw.
    let in_pond: Vec<(usize, (f32, f32))> = pets
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let pond = p.fun.pond.now?;
            let free = !p.mind.asleep() && p.press.is_none() && p.act.is_none() && !p.in_skit;
            free.then_some((i, pond))
        })
        .collect();
    let back = ground.splash_back.take();
    let thrower = back
        .and_then(|id| in_pond.iter().find(|(i, _)| pets[*i].save.id == id))
        .copied()
        .or_else(|| {
            // Anyone with company in its pond.
            let crowded: Vec<&(usize, (f32, f32))> = in_pond
                .iter()
                .filter(|(i, pond)| in_pond.iter().any(|(j, q)| j != i && q == pond))
                .collect();
            (!crowded.is_empty())
                .then(|| *crowded[(ground.roll() * crowded.len() as f32) as usize % crowded.len()])
        });
    let Some((i, pond)) = thrower else {
        return;
    };
    let x = pets[i].center_x();
    let Some(j) = in_pond
        .iter()
        .filter(|(j, q)| *j != i && *q == pond)
        .min_by(|a, b| {
            (pets[a.0].center_x() - x)
                .abs()
                .total_cmp(&(pets[b.0].center_x() - x).abs())
        })
        .map(|(j, _)| *j)
    else {
        return;
    };
    let to = pets[j].center_x();
    let (target, target_name) = (pets[j].save.id, pets[j].save.name.clone());
    let p = &mut pets[i];
    p.body.facing = if to >= x { 1.0 } else { -1.0 };
    let act = match p.species {
        Species::Fox => Act::Swish,
        Species::Chick | Species::Duck | Species::Owl => Act::Flap,
        _ => Act::Dig,
    };
    p.try_gesture(act);
    if ground.roll() < 0.3 {
        p.say(ctx, Line::Splash);
    }
    if ctx.sound_on {
        ctx.sounds
            .play(p.save.id, p.species, Sound::Splash, now, ctx.volume, 1.0);
    }
    diag!("{}: throws water at {target_name}", p.save.name);
    if let Some(w) = ground
        .things
        .iter_mut()
        .find(|w| w.prop.thing == Thing::Pond && w.prop.pos == pond)
    {
        w.sprays.push((x, to, now));
        if w.sprays.len() > 4 {
            w.sprays.remove(0);
        }
    }
    ground.hits.push((target, now + 0.85 * SPRAY_SECS, x));
}

/// The 肉肉舞 is over a while: it may dance again after this long.
const DANCE_AGAIN: f64 = 20.0;
/// A spotlight put down from the menu stays this long, seconds.
const STAGE_SECS: f64 = 45.0;
/// The tune's player id (its takes go A, B, ending in turn).
const TUNE_ID: u64 = u64::MAX - 2;

/// One step of the 肉肉舞 scene, `phase` 0 going into the light, 1
/// starting, 2 dancing; true when it is over.
fn dance(ground: &mut Playground, p: &mut Pet, ctx: &mut Ctx, run: &mut Run, phase: u8) -> bool {
    let now = ctx.world.now;
    match phase {
        0 => {
            let heading = p.walk_to(run.spot, 1.6, ctx.terrain);
            if heading == Heading::Blocked || now >= run.until {
                return true;
            }
            if heading == Heading::There {
                p.stop();
                run.phase = 1;
                run.since = now;
            }
            false
        }
        1 => {
            // Up on its feet to dance (lying about with a still mouse, it
            // stayed down; a hold keeps it lying: `max_hold`). The scene
            // alone keeps its own wandering away.
            p.fun.hold_until = 0.0;
            p.fun.lounge_until = 0.0;
            p.anim.posture = Posture::Free;
            if p.try_gesture(Act::Dance) {
                run.phase = 2;
                run.since = now;
                run.count = 0;
                run.until = now + f64::from(chonkgene_core::anim::DANCE_SECS) + 0.5;
                p.anim.expression = Expression::Happy;
                p.happy_until = now + f64::from(chonkgene_core::anim::DANCE_SECS);
                p.fun.effect(Fx::Notes, now);
                p.say(ctx, Line::Dance);
                diag!("{}: dances", p.save.name);
                // One tune for everyone in the light.
                if ground.tune.is_none_or(|(_, until)| now >= until) {
                    ground.tune = Some((p.save.id, run.until));
                    tune(ctx, 0);
                }
            } else if now - run.since > 3.0 {
                return true;
            }
            false
        }
        _ => {
            p.fun.hold_until = 0.0;
            p.anim.posture = Posture::Free;
            let t = (now - run.since) as f32;
            let beat = (t / chonkgene_core::anim::DANCE_BEAT) as u32;
            while run.count < beat {
                run.count += 1;
                let k = run.count;
                let lead = ground.tune.is_some_and(|(id, _)| id == p.save.id);
                if lead && (k == 4 || k == 8) {
                    tune(ctx, k / 4);
                }
                if k.is_multiple_of(3) && k < 12 {
                    p.fun.effect(Fx::Notes, now);
                }
                // 「登登！」
                if k == 12 {
                    p.fun.effect(Fx::Hearts, now);
                    p.fun.effect(Fx::Sparkles, now);
                    p.say(ctx, Line::DanceEnd);
                    p.bell(ctx);
                }
            }
            let done = p.anim.doing() != Some(Act::Dance) && t > 1.0;
            if done || now >= run.until {
                // Cut short, what it was doing instead says why.
                diag!(
                    "{}: done dancing after {t:.1} s (then {:?})",
                    p.save.name,
                    p.anim.doing()
                );
                cheer(p, ctx);
                return true;
            }
            false
        }
    }
}

/// Play phrase `k` (0 A, 1 B, 2 the ending) of the 肉肉舞's tune.
fn tune(ctx: &mut Ctx, k: u32) {
    if !ctx.sound_on {
        return;
    }
    // The takes play in turn, one more each time: set the one before, so
    // phrase k is take k + 1 whatever went before (a dance cut short).
    ctx.sounds
        .takes
        .insert((TUNE_ID, Sound::Tune), k % chonkgene_core::sound::TAKES);
    ctx.sounds.play(
        TUNE_ID,
        Species::Shiba,
        Sound::Tune,
        ctx.world.now,
        ctx.volume,
        1.0,
    );
}

/// `p` goes into the spotlight `stage` and dances, at screen x `x`.
pub(super) fn join_dance(p: &mut Pet, stage: u32, x: f32, now: f64) {
    let feet = p.body.feet();
    p.fun.scene = Some(Run {
        scene: Scene::Dance,
        prop: Some(stage),
        phase: 0,
        since: now,
        until: now + 8.0,
        home: feet,
        spot: x,
        keep: true,
        count: 0,
    });
}

impl Playground {
    /// Put a spotlight down beside `pet` (「放聚光燈」), or take the one
    /// down away; `pet` goes into it and dances.
    pub fn toggle_stage(
        &mut self,
        el: &ActiveEventLoop,
        shared: &Shared,
        pets: &mut [Pet],
        pet: u64,
        area: chonkgene_core::motion::Area,
        now: f64,
    ) {
        if let Some(id) = self.stage.take() {
            self.remove(id);
            return;
        }
        let Some(i) = pets.iter().position(|p| p.save.id == pet) else {
            return;
        };
        let (fx, fy) = pets[i].body.feet();
        let (w, s) = (pets[i].body.size.0, pets[i].fun.scale);
        let side = if fx < area.x + area.w / 2.0 {
            1.0
        } else {
            -1.0
        };
        let others: Vec<(f32, f32)> = pets
            .iter()
            .filter(|o| o.save.id != pet && (o.body.feet().1 - fy).abs() < 30.0 * s)
            .map(|o| (o.body.feet().0, FOOTPRINT * o.body.size.0))
            .collect();
        let half = 0.5 * Thing::Spotlight.size().0 * s;
        let x = free_spot(fx, side, w, half, (area.x, area.x + area.w), &others)
            .unwrap_or(fx + side * 1.1 * w);
        let mut put = Prop::put(Thing::Spotlight, (x, fy), s);
        put.resting = true;
        let Some(id) = self.add(el, shared, put, now, now + STAGE_SECS) else {
            return;
        };
        self.stage = Some(id);
        let p = &mut pets[i];
        if p.free_to_play(now) && !p.fun.busy(now) {
            join_dance(p, id, x, now);
        }
    }

    /// Pets walking into the spotlight put down join in; with 非理性模式
    /// on, those near by run over now and then. Several dance at once,
    /// side by side.
    fn stage_step(&mut self, pets: &mut [Pet], now: f64, dt: f32) {
        let Some(id) = self.stage else {
            return;
        };
        let Some(light) = self.get(id).map(|w| w.prop) else {
            self.stage = None;
            return;
        };
        let (sx, sy) = light.pos;
        let half = 0.42 * light.px().0;
        let mut dancers = pets
            .iter()
            .filter(|p| {
                p.fun
                    .scene
                    .is_some_and(|r| r.scene == Scene::Dance && r.prop == Some(id))
            })
            .count();
        for p in pets.iter_mut() {
            let rested = p.fun.danced == 0.0 || now - p.fun.danced >= DANCE_AGAIN;
            if !p.free_to_play(now) || p.fun.busy(now) || p.playing || !rested {
                continue;
            }
            let (fx, fy) = p.body.feet();
            let s = p.fun.scale;
            let level = (fy - sy).abs() < 30.0 * s;
            let inside = level && (fx - sx).abs() < half;
            let drawn =
                self.irrational && level && (fx - sx).abs() < 700.0 * s && self.roll() < 0.5 * dt;
            if inside || drawn {
                diag!(
                    "{}: joins the dance ({})",
                    p.save.name,
                    if inside { "walked in" } else { "ran over" }
                );
                // Side by side: the middle, then either side.
                let slot = [0.0, -1.0, 1.0, -2.0, 2.0][dancers % 5] * 0.4 * half;
                join_dance(p, id, sx + slot, now);
                dancers += 1;
            }
        }
    }
}

/// What you are doing, as the pets see it: read with the mouse still and
/// they lie about and see to an itch; type and they tap along, tip their
/// heads at a burst, a cat curls up by you and the Shiba nods off.
/// The cursor comes near this close to its head (pixels at 100%): it may
/// look; this close and moving this fast, a lively one may pounce at it.
const NOTICE_PX: f32 = 260.0;
const POUNCE_PX: f32 = 130.0;
const POUNCE_SPEED: f32 = 900.0;
/// Watching the cursor goes on this long at most (then it looks away and
/// leaves it a while), and this long after the cursor has gone.
const CURIOUS_SECS: (f64, f64) = (6.0, 3.0);
const CURIOUS_LINGER: f64 = 1.5;

/// How likely a pet is to notice the cursor coming near, each time it does
/// (PLAN §1.3 cursor curiosity; Roger's fourth pass, item 6: they hardly
/// ever looked at him): cats most, the owl stares at anything, the lively
/// more than the calm.
fn curiosity(species: Species, energy: f32) -> f32 {
    let base = match species {
        Species::Munchkin | Species::Fold => 0.75,
        Species::Owl => 0.8,
        Species::Fox | Species::Shiba => 0.6,
        Species::Chick | Species::Duck => 0.5,
    };
    (base + 0.3 * (energy - 0.5)).clamp(0.2, 0.95)
}

/// Cursor curiosity (PLAN §1.3): with the mouse moving near it, a pet at
/// rest may turn to look at the cursor, its head following it, for a few
/// seconds; a lively one sometimes pounces as it darts past (the owl and
/// the duckling only look). It looks away when the cursor goes, or tires
/// of it and leaves it a while.
fn curious(p: &mut Pet, ctx: &Ctx, activity: Activity, cursor: Option<(i32, i32)>, dt: f32) {
    let now = ctx.world.now;
    let s = p.fun.scale.max(0.1);
    let head = p.renderer.anchor();
    let at = cursor.map(|c| {
        (
            c.0 as f32 - p.placed.0 as f32,
            c.1 as f32 - p.placed.1 as f32,
        )
    });
    let speed = match (at, p.fun.cursor_was) {
        (Some(a), Some(b)) => (a.0 - b.0).hypot(a.1 - b.1) / dt.max(1e-3),
        _ => 0.0,
    };
    p.fun.cursor_was = at;
    let near = at.filter(|a| (a.0 - head.0).hypot(a.1 - head.1) < NOTICE_PX * s);
    let free = !p.mind.asleep()
        && !p.in_skit
        && p.act.is_none()
        && p.press.is_none()
        && !p.playing
        && !p.fun.busy(now)
        && p.body.motion == Motion::Idle
        && matches!(activity, Activity::Busy);
    let Some((since, until)) = p.fun.curious else {
        // Noticed, now and then: once per second of the cursor near.
        if free
            && near.is_some()
            && now >= p.fun.curious_rest
            && p.rng.f32() < curiosity(p.species, p.mind.personality.energy) * dt
        {
            p.fun.curious = Some((now, now + CURIOUS_LINGER));
            diag!("{}: looks at the cursor", p.save.name);
        }
        return;
    };
    let tired =
        now - since > CURIOUS_SECS.0 + CURIOUS_SECS.1 * f64::from(p.mind.personality.energy);
    if !free || tired || (near.is_none() && now >= until) {
        diag!(
            "{}: stops watching the cursor ({})",
            p.save.name,
            if tired {
                "tired of it"
            } else if free {
                "gone"
            } else {
                "busy"
            }
        );
        p.fun.curious = None;
        p.fun.curious_rest = now + 8.0 + 8.0 * f64::from(p.rng.f32());
        p.anim.watch(None);
        return;
    }
    if let Some(a) = near {
        p.fun.curious = Some((since, now + CURIOUS_LINGER));
        // Not off on a walk of its own while it watches.
        p.switch_at = p.switch_at.max(now + 1.0);
        let off = (
            p.body.pos.0 - p.placed.0 as f32,
            p.body.pos.1 - p.placed.1 as f32,
        );
        p.anim.watch(Some((a.0 - off.0, a.1 - off.1)));
        // Darting past close by: a lively one pounces at it.
        let pounces = !matches!(p.species, Species::Owl | Species::Duck);
        let close = (a.0 - head.0).hypot(a.1 - head.1) < POUNCE_PX * s;
        if pounces
            && close
            && speed > POUNCE_SPEED * s
            && now >= p.fun.cursor_pounce
            && p.rng.f32() < p.mind.personality.energy
        {
            p.fun.cursor_pounce = now + 6.0;
            p.anim.pounce();
            diag!("{}: pounces at the cursor", p.save.name);
        }
    }
}

fn react_to(
    p: &mut Pet,
    ctx: &mut Ctx,
    activity: Activity,
    typing_for: f64,
    cursor: Option<(i32, i32)>,
    dt: f32,
) {
    let now = ctx.world.now;
    if p.mind.asleep() || p.in_skit || p.act.is_some() || p.press.is_some() || p.playing {
        return;
    }
    match activity {
        Activity::Reading(_) => {
            if now >= p.fun.lounge_until {
                // Each pet in its own time: about one in seven a second.
                if p.body.motion != Motion::Idle || p.rng.f32() > 0.15 * dt {
                    return;
                }
                p.stop();
                p.fun.lounge_until = now + 20.0 + 20.0 * f64::from(p.rng.f32());
                p.fun.next_habit = now + 4.0;
                p.anim.posture = Posture::Lie;
            } else if now >= p.fun.next_habit {
                p.fun.next_habit = now + 5.0 + 5.0 * f64::from(p.rng.f32());
                let act = match p.species {
                    Species::Shiba => Act::Scratch,
                    Species::Munchkin | Species::Fold | Species::Fox => {
                        if p.rng.bool() {
                            Act::Groom
                        } else {
                            Act::Scratch
                        }
                    }
                    _ => Act::Preen,
                };
                p.anim.gesture(act);
            }
        }
        Activity::Typing { fast } => {
            if fast && now >= p.fun.next_tilt {
                p.fun.next_tilt = now + 8.0 + 6.0 * f64::from(p.rng.f32());
                p.try_gesture(Act::Tilt);
            } else if now >= p.fun.next_tap && p.anim.doing().is_none() {
                p.fun.next_tap = now + 5.0 + 5.0 * f64::from(p.rng.f32());
                if p.body.motion == Motion::Idle {
                    p.try_gesture(Act::Tap);
                }
            }
            let cat = matches!(p.species, Species::Munchkin | Species::Fold);
            if cat && typing_for > 30.0 && !p.fun.curled && p.fun.goto.is_none() {
                // Comes to curl up by you (the cursor is near what you type).
                if let Some(c) = cursor {
                    p.fun.curled = true;
                    p.fun.goto = Some(c.0 as f32);
                }
            }
            if p.species == Species::Shiba && typing_for > 120.0 && !p.fun.dozed {
                p.fun.dozed = true;
                let mut rng = p.rng.clone();
                let out = p.mind.put_to_sleep(&ctx.world, &mut rng);
                p.rng = rng;
                p.apply(ctx, out);
            }
        }
        Activity::Busy | Activity::Away => {
            // Back to the mouse: up they get.
            if now < p.fun.lounge_until && matches!(activity, Activity::Busy) {
                p.fun.lounge_until = 0.0;
            }
            if typing_for == 0.0 {
                p.fun.curled = false;
                p.fun.dozed = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cats and the owl are the most curious about the cursor, the lively
    /// more than the calm, and none never or always.
    #[test]
    fn curiosity_goes_by_species_and_liveliness() {
        assert!(curiosity(Species::Fold, 0.5) > curiosity(Species::Chick, 0.5));
        assert!(curiosity(Species::Owl, 0.5) > curiosity(Species::Shiba, 0.5));
        assert!(curiosity(Species::Shiba, 0.9) > curiosity(Species::Shiba, 0.1));
        for s in Species::ALL {
            for e in [0.0, 1.0] {
                let c = curiosity(s, e);
                assert!((0.2..=0.95).contains(&c), "{s:?} {e}: {c}");
            }
        }
    }

    /// A pond comes out where nobody stands: further out, the other side,
    /// or not at all (Roger's clip: an owl asleep on its near rim).
    #[test]
    fn a_pond_comes_out_where_nobody_stands() {
        let (w, half, x) = (150.0, 125.0, (0.0, 2000.0));
        // Nobody about: just beside it.
        assert_eq!(free_spot(800.0, 1.0, w, half, x, &[]), Some(965.0));
        // Someone sleeping there: further out on the same side.
        let owl = [(965.0, 45.0)];
        let at = free_spot(800.0, 1.0, w, half, x, &owl).unwrap();
        assert!(at > 965.0 && (at - 965.0).abs() >= half + 45.0);
        // That side full up to the edge: the other side.
        let crowd = [(965.0, 45.0), (1100.0, 45.0), (1300.0, 45.0)];
        let at = free_spot(800.0, 1.0, w, half, x, &crowd).unwrap();
        assert!(at < 800.0);
        // Hemmed in by the screen's edges: none.
        assert_eq!(free_spot(800.0, 1.0, w, half, (600.0, 1000.0), &[]), None);
    }

    /// A pet stepping about at a pond's edge stays in, splashes once, and
    /// is out only after the hold (Roger: the pond flickered).
    #[test]
    fn a_pet_at_the_ponds_edge_stays_in_and_splashes_once() {
        let pos = (500.0, 400.0);
        let (cx, _, rx, _) = props::pond_oval(pos, 1.0);
        // Between the in and out edges: not in yet, but kept in.
        let edge = (cx + 0.9 * rx, pos.1);
        assert!(!in_water(pos, 1.0, edge, false));
        assert!(in_water(pos, 1.0, edge, true));
        assert!(!in_water(pos, 1.0, (cx + 0.97 * rx, pos.1), true));

        let mut w = Wade::default();
        let mut splashes = 0;
        let mut t = 0.0;
        // In and out every other frame at 60 fps for two seconds.
        for k in 0..120 {
            let under = (k % 4 < 2).then_some(pos);
            splashes += u32::from(w.step(under, t));
            assert_eq!(w.now, Some(pos), "out at frame {k}");
            t += 1.0 / 60.0;
        }
        assert_eq!(splashes, 1);
        // Gone for good: out after the hold, and no splash straight back.
        while w.now.is_some() {
            w.step(None, t);
            t += 1.0 / 60.0;
        }
        assert!(t < 2.0 + POND_HOLD + 0.1);
        assert!(!w.step(Some(pos), t));
        w.now = None;
        assert!(w.step(Some(pos), t + SPLASH_AGAIN));
    }

    /// 躲貓貓: mostly only a hint shows round the edge; a peek leans out
    /// smoothly to most of the head and back, and never further.
    #[test]
    fn a_hiding_pet_shows_a_hint_and_peeks_out_now_and_then() {
        let h = Hide {
            edge: 0.0,
            side: -1.0,
            since: 0.0,
            until: HIDE_SECS,
            base: Some(-200.0),
            peek_at: 5.0,
            found: false,
        };
        assert_eq!(h.lean(4.9), HINT);
        assert_eq!(h.lean(5.0 + PEEK_SECS + 0.1), HINT);
        let middle = h.lean(5.0 + 0.5 * PEEK_SECS);
        assert!((middle - PEEK).abs() < 1e-4, "{middle}");
        let mut last = HINT;
        for k in 0..=40 {
            let lean = h.lean(5.0 + PEEK_SECS * f64::from(k) / 40.0);
            assert!((HINT..=PEEK).contains(&lean));
            assert!((lean - last).abs() < 0.08, "a jump to {lean}");
            last = lean;
        }
    }
}
