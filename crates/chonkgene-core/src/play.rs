//! Things to play with and come to (PLAN §8, Roger's fourth pass, items 2
//! and 4, and "Catnip"): a ball or a frisbee thrown across the desktop, a
//! sprig of catnip put down, the things of a species' scene; how each
//! species goes for a thrown toy (dogs and the fox fetch, cats bat it
//! about, birds chase a few steps, the owl watches); and the commands of
//! the pet menu (坐下, 趴下, 過來, 轉圈, 握手) with how long a pet takes to
//! do as it is asked.

use crate::species::Species;
use crate::terrain::Terrain;

/// Pixels a second², as pets fall (motion.rs).
const GRAVITY: f32 = 2400.0;
/// A ball bounces back up with this share of its speed, and stops bouncing
/// below this speed (pixels a second).
const BALL_KEEP: f32 = 0.55;
const BALL_STILL: f32 = 140.0;
/// Rolling, a ball slows by this share a second (exponential), and stops
/// below this speed.
const ROLL_DRAG: f32 = 1.1;
const ROLL_STILL: f32 = 10.0;
/// A frisbee is held up by the air in proportion to its speed: at this
/// speed (pixels a second) and over, nearly all of its weight.
const GLIDE_SPEED: f32 = 650.0;
const GLIDE_MOST: f32 = 0.86;
/// How quickly air slows a frisbee, and the floor a landed one.
const AIR_DRAG: f32 = 0.45;
const SLIDE_DRAG: f32 = 7.0;
/// Bouncing off the edges of the desktop keeps this share of the speed.
const WALL_KEEP: f32 = 0.6;

/// A thing on the desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Thing {
    /// Thrown from the menu or flicked: it bounces and rolls.
    Ball,
    /// Thrown: it glides, then slides to a stop.
    Frisbee,
    /// A sprig put down for the cats (PLAN §8 "Catnip"); used up after a
    /// while.
    Catnip,
    /// The Shiba's scene: a bone to chew.
    Bone,
    /// The chick's scene: seeds scattered to peck at.
    Seeds,
    /// The owl's scene: a branch to fly to and perch on.
    Branch,
    /// The duckling's scene: a little pond to swim in.
    Pond,
    /// A cat's scene: a patch of sun to lie in.
    Sunbeam,
    /// A cat's scene: a butterfly to chase.
    Butterfly,
    /// A stage spotlight: whoever is in its pool dances the 肉肉舞.
    Spotlight,
    /// Golden sand round a pond, to walk and run on (Roger's eighteenth
    /// pass): it comes and goes with the pond, lying under it.
    Beach,
}

impl Thing {
    /// The size of its picture at 100%, pixels; the thing rests with the
    /// middle of its bottom edge on the floor.
    pub fn size(self) -> (f32, f32) {
        match self {
            Thing::Ball => (39.0, 39.0),
            Thing::Frisbee => (66.0, 24.0),
            Thing::Catnip => (40.0, 34.0),
            Thing::Bone => (46.0, 24.0),
            Thing::Seeds => (70.0, 14.0),
            Thing::Branch => (260.0, 60.0),
            Thing::Pond => (640.0, 136.0),
            Thing::Beach => (1180.0, 150.0),
            Thing::Sunbeam => (400.0, 420.0),
            Thing::Butterfly => (28.0, 24.0),
            Thing::Spotlight => (240.0, 380.0),
        }
    }

    /// Thrown or put down, it falls and settles on what is below it; the
    /// rest (the scene's branch, pond, sunbeam, butterfly) stay where they
    /// are put.
    pub fn falls(self) -> bool {
        matches!(
            self,
            Thing::Ball | Thing::Frisbee | Thing::Catnip | Thing::Bone | Thing::Seeds
        )
    }

    /// Can a pet pick it up and carry it in its mouth?
    pub fn carried(self) -> bool {
        matches!(self, Thing::Ball | Thing::Frisbee | Thing::Bone)
    }

    /// Can you pick it up with the mouse (PLAN §8 "Roger's tenth pass",
    /// item 1): toys to throw, catnip to move where the cats are (Roger's
    /// thirteenth pass)? The rest let every click through.
    pub fn grabbed(self) -> bool {
        matches!(self, Thing::Ball | Thing::Frisbee | Thing::Catnip)
    }
}

/// A thing and where it is: `pos` is the middle of its bottom, screen
/// pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prop {
    pub thing: Thing,
    pub pos: (f32, f32),
    pub vel: (f32, f32),
    /// How far it has turned (radians): a ball rolling, a frisbee spinning.
    pub spin: f32,
    pub scale: f32,
    /// On the ground and still.
    pub resting: bool,
    /// Seconds since it was thrown or put down.
    pub age: f32,
}

impl Prop {
    /// Put down at `at` (screen pixels, where its bottom goes): it drops
    /// onto whatever is below.
    pub fn put(thing: Thing, at: (f32, f32), scale: f32) -> Self {
        Self {
            thing,
            pos: at,
            vel: (0.0, 0.0),
            spin: 0.0,
            scale,
            resting: !thing.falls(),
            age: 0.0,
        }
    }

    /// Thrown from `from` (screen pixels) toward `side` (−1 left, 1 right):
    /// a ball high and far, a frisbee flat and gliding.
    pub fn thrown(thing: Thing, from: (f32, f32), side: f32, scale: f32) -> Self {
        let side = if side < 0.0 { -1.0 } else { 1.0 };
        let s = scale.max(0.5);
        let vel = match thing {
            Thing::Frisbee => (side * 720.0 * s, -260.0 * s),
            _ => (side * 520.0 * s, -760.0 * s),
        };
        Self {
            vel,
            ..Self::put(thing, from, scale)
        }
    }

    /// Its picture's size, pixels.
    pub fn px(&self) -> (f32, f32) {
        let (w, h) = self.thing.size();
        (w * self.scale, h * self.scale)
    }

    /// In the air: thrown and not down yet.
    pub fn flying(&self) -> bool {
        self.thing.falls() && !self.resting && self.vel.1 != 0.0
    }

    /// Knocked by a paw: off it goes this fast (pixels a second).
    pub fn bat(&mut self, vel: (f32, f32)) {
        self.vel = vel;
        self.resting = false;
    }

    /// `dt` seconds on: it falls, glides, bounces, rolls. The speed it hit
    /// the ground at, if it did (for a sound).
    pub fn step(&mut self, dt: f32, terrain: &Terrain) -> Option<f32> {
        self.age += dt;
        if self.resting || !self.thing.falls() {
            return None;
        }
        let (left, right) = terrain.bounds();
        let half = 0.5 * self.px().0;
        let (left, right) = (left + half, (right - half).max(left + half));
        let speed = self.vel.0.abs();
        let lift = if self.thing == Thing::Frisbee {
            GLIDE_MOST * (speed / GLIDE_SPEED).min(1.0)
        } else {
            0.0
        };
        // Rolling or sliding along a ledge (a landing that does not bounce
        // leaves it exactly still up and down).
        let on_ground = self.vel.1 == 0.0 && terrain.ledge_level(self.pos.0, self.pos.1).is_some();
        let mut landed = None;
        if on_ground {
            let drag = if self.thing == Thing::Ball {
                ROLL_DRAG
            } else {
                SLIDE_DRAG
            };
            self.vel.0 *= (-drag * dt).exp();
            self.pos.0 += self.vel.0 * dt;
            if self.vel.0.abs() < ROLL_STILL * self.scale {
                self.vel.0 = 0.0;
                self.resting = true;
            }
            // Rolled off the end of a window: down it goes.
            if terrain.ledge_level(self.pos.0, self.pos.1).is_none() {
                self.resting = false;
                self.vel.1 = 1.0;
            }
        } else {
            let y0 = self.pos.1;
            self.vel.1 += GRAVITY * (1.0 - lift) * dt;
            if self.thing == Thing::Frisbee {
                self.vel.0 *= (-AIR_DRAG * dt).exp();
            }
            self.pos.0 += self.vel.0 * dt;
            self.pos.1 += self.vel.1 * dt;
            let fell = if self.vel.1 > 0.0 {
                terrain.landing(self.pos.0, y0, self.pos.1).or_else(|| {
                    // Past everything (between monitors): the nearest floor.
                    terrain.floor_under(self.pos.0).filter(|l| self.pos.1 > l.y)
                })
            } else {
                None
            };
            if let Some(l) = fell {
                self.pos.1 = l.y;
                let hit = self.vel.1;
                landed = Some(hit);
                if self.thing == Thing::Ball && hit > BALL_STILL * self.scale {
                    self.vel.1 = -hit * BALL_KEEP;
                    self.vel.0 *= 0.85;
                } else {
                    self.vel.1 = 0.0;
                    if self.thing != Thing::Ball && self.thing != Thing::Frisbee {
                        self.vel.0 = 0.0;
                        self.resting = true;
                    }
                }
            }
        }
        if self.pos.0 < left || self.pos.0 > right {
            self.pos.0 = self.pos.0.clamp(left, right);
            self.vel.0 = -self.vel.0 * WALL_KEEP;
        }
        // A ball rolls as it goes; a frisbee spins all the while it flies.
        let r = 0.5 * self.px().1.max(1.0);
        self.spin += match self.thing {
            Thing::Ball => self.vel.0 * dt / r,
            Thing::Frisbee if !on_ground => 14.0 * dt,
            _ => 0.0,
        };
        landed
    }
}

/// How a species goes for a thrown toy. A ball every one of them brings
/// back: dogs and the fox nose it, cats bat it with a paw, the chick and
/// the duckling peck it along, the owl flies it back in its beak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fetch {
    /// Runs for it and brings it back (dogs, the fox): a frisbee leapt for
    /// and caught in the mouth.
    Retrieve,
    /// Runs for a frisbee and bats it about a few times, then loses
    /// interest (cats).
    Bat,
    /// Chases a frisbee a few steps and gives up (chick, duckling).
    Chase,
    /// Only watches a frisbee go (the owl).
    Watch,
}

impl Species {
    pub fn fetch(self) -> Fetch {
        match self {
            Species::Shiba | Species::Fox => Fetch::Retrieve,
            Species::Munchkin | Species::Fold => Fetch::Bat,
            Species::Chick | Species::Duck => Fetch::Chase,
            Species::Owl => Fetch::Watch,
        }
    }
}

/// What a pet going for a toy does next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Order {
    /// Stand and watch it.
    Watch,
    /// Run to `x` (screen pixels, its feet) at this pace.
    Run { x: f32, pace: f32 },
    /// Jump for it and catch it, or pick it up off the floor: it is now in
    /// the pet's mouth.
    Grab,
    /// Leap up `height` pixels to catch it in the air, coming down with
    /// the feet at screen x `x`.
    Leap { x: f32, height: f32 },
    /// In the air after a leap: nothing to do but wait for it.
    Wait,
    /// Push it on toward `side`, at `speed` pixels a second (a ball brought
    /// back to you: with the nose, a paw, a beak, as the species does).
    Nose { side: f32, speed: f32 },
    /// Nosed all the way back to where it was thrown from.
    Brought,
    /// Put it down (it is back where it was thrown from).
    Drop,
    /// Knock it on with a paw toward `side`.
    Bat { side: f32 },
    /// A peck at it.
    Peck,
    /// Had enough.
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Going,
    Carrying,
}

/// A pet going after a thrown toy.
#[derive(Clone, Debug)]
pub struct Chase {
    how: Fetch,
    phase: Phase,
    /// When it set off, and when it gives up.
    since: f64,
    until: f64,
    /// Where it was thrown from: a retriever brings it back here.
    home: f32,
    /// How many times a cat has batted it, and how many it will.
    bats: u8,
    most_bats: u8,
    /// When it may bat, peck or nose it again.
    next: f64,
    /// Up in the air after a leap for it until then, and how many times
    /// it has leapt.
    aloft_until: f64,
    leaps: u8,
    /// How far its feet can walk, left and right (screen x), if known.
    reach: Option<(f32, f32)>,
    /// Still on the move (slowing to a stop): it cannot push the ball yet.
    moving: bool,
}

/// The longest any pet keeps after a toy, seconds.
const CHASE_SECS: f64 = 25.0;
/// Birds give up after this long, seconds (and up to as long again).
const BIRD_CHASE: f64 = 2.0;
/// The owl watches this long.
const WATCH_SECS: f64 = 3.5;
/// A retriever leaps at most this many times for one throw.
const MOST_LEAPS: u8 = 3;
/// Where a pet's mouth is, as a share of its window's height above its
/// feet, and how near (shares of the window) a frisbee must come to it to
/// be caught.
const MOUTH_UP: f32 = 0.55;
const CATCH: (f32, f32) = (0.45, 0.4);

impl Chase {
    /// Set off after `thing` thrown from `home` (screen x); `roll` (0 … 1)
    /// varies how long a cat keeps at a frisbee and a bird keeps running.
    /// A ball every species brings back (Roger, 2026-11-06), so it keeps
    /// at it as long as it takes.
    pub fn new(how: Fetch, thing: Thing, home: f32, now: f64, roll: f32) -> Self {
        let until = now
            + match how {
                _ if thing == Thing::Ball => CHASE_SECS,
                Fetch::Retrieve | Fetch::Bat => CHASE_SECS,
                Fetch::Chase => BIRD_CHASE * (1.0 + f64::from(roll)),
                Fetch::Watch => WATCH_SECS,
            };
        Self {
            how,
            phase: Phase::Going,
            since: now,
            until,
            home,
            bats: 0,
            most_bats: 2 + (roll * 3.0) as u8,
            next: now,
            aloft_until: 0.0,
            leaps: 0,
            reach: None,
            moving: false,
        }
    }

    pub fn carrying(&self) -> bool {
        self.phase == Phase::Carrying
    }

    /// How far the pet's feet can walk, left and right (screen x), so it
    /// never sets off for a spot it cannot reach (set before each
    /// [`Chase::tick`]).
    pub fn set_reach(&mut self, reach: (f32, f32)) {
        self.reach = Some(reach);
    }

    /// Whether the pet is still moving (set before each [`Chase::tick`]):
    /// it pushes the ball only once it has stopped by it.
    pub fn set_moving(&mut self, moving: bool) {
        self.moving = moving;
    }

    /// What to do now, with the pet's feet at `feet`, its window `size`
    /// (pixels) and the toy at `prop` (whose it is if `taken`: another pet
    /// got there first).
    pub fn tick(
        &mut self,
        now: f64,
        feet: (f32, f32),
        size: (f32, f32),
        prop: &Prop,
        taken: bool,
    ) -> Order {
        let (w, h) = size;
        if now >= self.until {
            return Order::Done;
        }
        if self.phase == Phase::Carrying {
            // Caught in the air: down on its feet first.
            if now < self.aloft_until {
                return Order::Wait;
            }
            let dx = self.home - feet.0;
            return if dx.abs() < 0.25 * w {
                Order::Drop
            } else {
                Order::Run {
                    x: self.home,
                    pace: 1.6,
                }
            };
        }
        if taken {
            return Order::Done;
        }
        let dx = prop.pos.0 - feet.0;
        // Up on a window out of reach, or gone far below: not worth it
        // (unless it is only up in the air after a leap for it).
        let above = feet.1 - prop.pos.1;
        let aloft = now < self.aloft_until;
        // The owl fetches a ball on the wing: up or down is no matter.
        let flier = self.how == Fetch::Watch && prop.thing == Thing::Ball;
        if !aloft && !flier && !prop.flying() && (above > 1.2 * h || above < -0.3 * h) {
            return if now - self.since > 1.5 {
                Order::Done
            } else {
                Order::Watch
            };
        }
        // Near enough to pick it up, or as near as it can get (a toy
        // against the edge of the screens).
        let reachable = self
            .reach
            .map_or(prop.pos.0, |(lo, hi)| prop.pos.0.clamp(lo, hi));
        let near = (reachable - feet.0).abs() < 0.3 * w;
        match self.how {
            // The owl flies over, picks the ball up in its beak and flies
            // it back.
            Fetch::Watch if prop.thing == Thing::Ball => {
                // It watches the ball come to rest, then swoops on it.
                let rolling = prop.vel.0.abs() > 40.0 * prop.scale.max(0.5);
                if self.phase == Phase::Going && (prop.flying() || rolling) {
                    return Order::Watch;
                }
                if near && !prop.flying() {
                    self.phase = Phase::Carrying;
                    Order::Grab
                } else {
                    Order::Run {
                        x: reachable,
                        pace: 2.0,
                    }
                }
            }
            Fetch::Watch => Order::Watch,
            // Everyone else pushes a ball home its own way: nose, paw, beak.
            _ if prop.thing == Thing::Ball => self.nose(now, feet, size, prop),
            Fetch::Retrieve => {
                let above = feet.1 - prop.pos.1;
                // Up in the air after a leap: snapped up as it comes by
                // the mouth.
                if aloft {
                    let caught = dx.abs() < CATCH.0 * w
                        && (above - MOUTH_UP * h).abs() < CATCH.1 * h
                        && prop.flying();
                    return if caught {
                        self.phase = Phase::Carrying;
                        Order::Grab
                    } else {
                        Order::Wait
                    };
                }
                // Gliding by within a leap: up after it, the top of the
                // leap where it will be by then (so it comes down twice as
                // far on).
                if prop.flying()
                    && self.leaps < MOST_LEAPS
                    && dx.abs() < 1.1 * w
                    && (0.75 * h..2.2 * h).contains(&above)
                {
                    self.leaps += 1;
                    let rise = (above - MOUTH_UP * h).clamp(0.25 * h, 1.5 * h);
                    let secs = (2.0 * rise / GRAVITY).sqrt();
                    self.aloft_until = now + 2.0 * f64::from(secs);
                    let meet = prop.pos.0 + prop.vel.0 * secs;
                    let mut x = feet.0 + 2.0 * (meet - feet.0).clamp(-0.6 * w, 0.6 * w);
                    if let Some((lo, hi)) = self.reach {
                        x = x.clamp(lo, hi);
                    }
                    return Order::Leap { x, height: rise };
                }
                // Low enough to snap up from the floor, or lying there.
                if near && (above < 1.0 * h || !prop.flying()) {
                    self.phase = Phase::Carrying;
                    Order::Grab
                } else {
                    Order::Run {
                        x: prop.pos.0,
                        pace: 2.2,
                    }
                }
            }
            Fetch::Bat => {
                if near && !prop.flying() {
                    if self.bats >= self.most_bats {
                        return Order::Done;
                    }
                    if now < self.next {
                        return Order::Watch;
                    }
                    self.bats += 1;
                    self.next = now + 1.1;
                    // On, away from the cat.
                    Order::Bat {
                        side: if dx < 0.0 { -1.0 } else { 1.0 },
                    }
                } else {
                    Order::Run {
                        x: prop.pos.0,
                        pace: 1.8,
                    }
                }
            }
            Fetch::Chase => {
                if near && !prop.flying() && now >= self.next {
                    self.next = now + 0.8;
                    Order::Peck
                } else {
                    Order::Run {
                        x: prop.pos.0,
                        pace: 1.5,
                    }
                }
            }
        }
    }
}

impl Chase {
    /// A ball nosed back home: run round behind it (the side away from
    /// home), then bump it on toward home with the nose, again and again,
    /// until it is back.
    fn nose(&mut self, now: f64, feet: (f32, f32), size: (f32, f32), prop: &Prop) -> Order {
        let w = size.0;
        let to_home = self.home - prop.pos.0;
        if to_home.abs() < 0.4 * w && !prop.flying() {
            return Order::Brought;
        }
        let side = if to_home < 0.0 { -1.0 } else { 1.0 };
        // Its feet just behind the ball, the nose at it (or as near as it
        // can get, the ball against the edge of the screens).
        let mut behind = prop.pos.0 - side * 0.3 * w;
        if let Some((lo, hi)) = self.reach {
            behind = behind.clamp(lo, hi);
        }
        let dx = behind - feet.0;
        let rolling = prop.vel.0.abs() > 160.0 * prop.scale.max(0.5);
        // Close enough: further than a walk's stopping distance, which a
        // pet stood still does not set off again for.
        if dx.abs() < 0.3 * w && !prop.flying() && !rolling {
            // Stopping by it first (a push while still sliding along never
            // happens, and it slid on past the ball).
            if now < self.next || self.moving {
                return Order::Watch;
            }
            // A bird's beak pushes it on less far, and pecks more often.
            let (strength, gap) = match self.how {
                Fetch::Chase => (0.5, 0.7),
                _ => (1.0, 0.9),
            };
            self.next = now + gap;
            // Rolls about speed / ROLL_DRAG before it stops: most of the
            // way home, never far past it.
            let speed = strength
                * (0.8 * ROLL_DRAG * to_home.abs()).clamp(160.0, 520.0)
                * prop.scale.max(0.5);
            return Order::Nose { side, speed };
        }
        Order::Run {
            x: behind,
            pace: 2.0,
        }
    }
}

/// A species' own little scene now and then (PLAN §8, Roger's fourth
/// pass, item 4): its things appear only while it plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scene {
    /// The owl flies up to a branch and perches a while.
    Perch,
    /// The duckling swims in a little pond.
    Pond,
    /// The chick pecks at scattered seeds.
    Seeds,
    /// A cat lies in a patch of sun.
    Sunbeam,
    /// A cat chases a butterfly.
    Butterfly,
    /// The Shiba lies chewing a bone.
    Bone,
    /// The fox chases its tails round in fox-fire.
    Foxfire,
    /// 肉肉舞 in a spotlight (any species; asked for, or a spotlight put
    /// down).
    Dance,
}

impl Scene {
    /// How long it plays, seconds.
    pub fn secs(self) -> f64 {
        match self {
            Scene::Perch => 40.0,
            Scene::Pond => 45.0,
            Scene::Seeds => 18.0,
            Scene::Sunbeam => 40.0,
            Scene::Butterfly => 20.0,
            Scene::Bone => 25.0,
            Scene::Foxfire => 10.0,
            // The light comes on, the dance, the light goes out.
            Scene::Dance => f64::from(crate::anim::DANCE_SECS) + 2.0,
        }
    }

    /// The thing it brings out (the fox's fire is drawn round the fox).
    pub fn thing(self) -> Option<Thing> {
        match self {
            Scene::Perch => Some(Thing::Branch),
            Scene::Pond => Some(Thing::Pond),
            Scene::Seeds => Some(Thing::Seeds),
            Scene::Sunbeam => Some(Thing::Sunbeam),
            Scene::Butterfly => Some(Thing::Butterfly),
            Scene::Bone => Some(Thing::Bone),
            Scene::Foxfire => None,
            Scene::Dance => Some(Thing::Spotlight),
        }
    }
}

/// Between one pet's scenes, seconds (and up to as long again).
pub const SCENE_GAP: f64 = 150.0;

impl Species {
    /// Its scenes.
    pub fn scenes(self) -> &'static [Scene] {
        match self {
            Species::Shiba => &[Scene::Bone],
            Species::Munchkin | Species::Fold => &[Scene::Sunbeam, Scene::Butterfly],
            Species::Fox => &[Scene::Foxfire],
            Species::Chick => &[Scene::Seeds],
            Species::Duck => &[Scene::Pond],
            Species::Owl => &[Scene::Perch],
        }
    }

    /// Cats care for catnip, mostly: about one in three does not (PLAN §8
    /// "Catnip"), fixed per pet by its id, as if rolled when adopted.
    pub fn likes_catnip(self, pet: u64) -> bool {
        matches!(self, Species::Munchkin | Species::Fold)
            && pet.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 61 > 2
    }
}

/// Asked from the pet menu's 「叫牠…」 (PLAN §8, Roger's fourth pass,
/// item 2). Every pet does them from the start: affinity only makes it
/// quicker about it, and a bad mood makes it take its time, never refuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// 坐下: sits and stays a while.
    Sit,
    /// 趴下: lies down (awake) a while.
    Lie,
    /// 過來: comes over to where the cursor is.
    Come,
    /// 轉圈: turns round on the spot.
    Spin,
    /// 握手: holds out a paw (a wing).
    Paw,
}

impl Command {
    pub const ALL: [Command; 5] = [
        Command::Sit,
        Command::Lie,
        Command::Come,
        Command::Spin,
        Command::Paw,
    ];
}

/// How long a pet takes to do as it is asked, seconds: at affinity level 1
/// it thinks about it a moment, by level 5 it is at once; in a bad mood
/// (grumpy, sulking, fed up) it takes a good few seconds more, `roll`
/// (0 … 1) saying how many.
pub fn obey_delay(level: u32, moody: bool, roll: f32) -> f64 {
    let keen = (f64::from(level.saturating_sub(1)) / 4.0).min(1.0);
    let base = 0.25 + 1.0 * (1.0 - keen) + 0.4 * f64::from(roll);
    if moody {
        base + 3.0 + 4.0 * f64::from(roll)
    } else {
        base
    }
}

/// How long a command's pose is held, seconds: sitting and lying stay a
/// while, the rest are gestures with their own length.
pub fn stay_secs(command: Command) -> f64 {
    match command {
        Command::Sit => 7.0,
        Command::Lie => 10.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::Area;

    const AREA: Area = Area {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1032.0,
    };

    fn run(p: &mut Prop, t: &Terrain, secs: f32) -> usize {
        let mut hits = 0;
        for _ in 0..(secs * 60.0) as usize {
            if p.step(1.0 / 60.0, t).is_some() {
                hits += 1;
            }
        }
        hits
    }

    #[test]
    fn a_thrown_ball_bounces_rolls_and_stops_on_the_floor() {
        let t = Terrain::flat(AREA);
        let mut b = Prop::thrown(Thing::Ball, (600.0, 700.0), 1.0, 1.0);
        let hits = run(&mut b, &t, 12.0);
        assert!(hits >= 3, "bounced only {hits} times");
        assert!(b.resting, "still rolling at {:?}", b.vel);
        assert_eq!(b.pos.1, 1032.0);
        assert!(b.pos.0 > 900.0, "only got to {}", b.pos.0);
        assert!(b.pos.0 <= 1920.0);
        assert!(b.spin > 3.0, "rolled without turning");
    }

    #[test]
    fn a_frisbee_glides_further_than_it_would_fall() {
        let t = Terrain::flat(AREA);
        let mut f = Prop::thrown(Thing::Frisbee, (200.0, 700.0), 1.0, 1.0);
        let mut airtime = 0.0;
        for _ in 0..600 {
            if f.flying() {
                airtime += 1.0 / 60.0;
            }
            f.step(1.0 / 60.0, &t);
        }
        // A stone thrown so would drop 332 px in under 0.65 s.
        assert!(airtime > 1.0, "down after {airtime:.2} s");
        assert!(f.resting && f.pos.1 == 1032.0);
        assert!(f.pos.0 > 900.0, "only got to {}", f.pos.0);
    }

    #[test]
    fn a_ball_bounces_back_off_the_edge_of_the_desktop() {
        let t = Terrain::flat(AREA);
        let mut b = Prop::thrown(Thing::Ball, (1800.0, 700.0), 1.0, 1.0);
        run(&mut b, &t, 12.0);
        let half = 13.0;
        assert!(b.pos.0 <= 1920.0 - half && b.pos.0 >= half);
        assert!(b.resting);
    }

    #[test]
    fn catnip_put_down_drops_onto_the_window_below() {
        use crate::terrain::Win;
        let win = Win {
            id: 7,
            x: 400.0,
            y: 600.0,
            w: 600.0,
            h: 300.0,
        };
        let t = Terrain::new(vec![AREA], &[win], 100.0);
        let mut c = Prop::put(Thing::Catnip, (700.0, 300.0), 1.0);
        let hits = run(&mut c, &t, 2.0);
        assert_eq!(hits, 1);
        assert!(c.resting);
        assert_eq!(c.pos.1, 600.0);
    }

    #[test]
    fn a_retriever_leaps_for_a_frisbee_catches_it_in_the_air_and_brings_it_back() {
        let t = Terrain::flat(AREA);
        let floor = 1032.0;
        let mut disc = Prop::thrown(Thing::Frisbee, (500.0, 880.0), 1.0, 1.0);
        let size = (260.0, 190.0);
        let mut feet = (700.0, floor);
        // Up and down after a leap: (speed up, speed along).
        let mut air: Option<(f32, f32)> = None;
        let mut c = Chase::new(Fetch::Retrieve, Thing::Frisbee, 500.0, 0.0, 0.5);
        let (mut leapt, mut caught_up, mut grabbed, mut dropped) = (false, false, false, false);
        let mut now = 0.0;
        let dt: f32 = 1.0 / 60.0;
        for _ in 0..(20 * 60) {
            now += f64::from(dt);
            if !grabbed {
                disc.step(dt, &t);
            }
            if let Some((vy, vx)) = &mut air {
                *vy += GRAVITY * dt;
                feet.0 += *vx * dt;
                feet.1 = (feet.1 + *vy * dt).min(floor);
                if feet.1 >= floor {
                    air = None;
                }
            }
            match c.tick(now, feet, size, &disc, false) {
                Order::Run { x, pace } if air.is_none() => {
                    feet.0 += (x - feet.0).signum() * 60.0 * pace * dt
                }
                Order::Leap { x, height } => {
                    leapt = true;
                    let up = (2.0 * GRAVITY * height).sqrt();
                    air = Some((-up, (x - feet.0) / (2.0 * up / GRAVITY)));
                }
                Order::Grab => {
                    grabbed = true;
                    caught_up = feet.1 < floor - 10.0;
                }
                Order::Drop => {
                    dropped = true;
                    break;
                }
                Order::Done => break,
                _ => {}
            }
        }
        assert!(leapt, "never leapt for it");
        assert!(
            grabbed && caught_up,
            "grabbed {grabbed}, in the air {caught_up}"
        );
        assert!(dropped);
        assert!((feet.0 - 500.0).abs() < 0.25 * size.0);
    }

    #[test]
    fn a_retriever_noses_a_ball_back_home_without_picking_it_up() {
        let t = Terrain::flat(AREA);
        let mut ball = Prop::thrown(Thing::Ball, (500.0, 900.0), 1.0, 1.0);
        let size = (260.0, 190.0);
        let mut feet = (450.0, 1032.0);
        let mut c = Chase::new(Fetch::Retrieve, Thing::Ball, 500.0, 0.0, 0.5);
        let (mut noses, mut brought, mut grabbed) = (0, false, false);
        let mut now = 0.0;
        let mut far = 0.0f32;
        for _ in 0..(25 * 60) {
            now += 1.0 / 60.0;
            ball.step(1.0 / 60.0, &t);
            far = far.max(ball.pos.0 - 500.0);
            match c.tick(now, feet, size, &ball, false) {
                Order::Run { x, pace } => feet.0 += (x - feet.0).signum() * 60.0 * pace / 60.0,
                Order::Nose { side, speed } => {
                    noses += 1;
                    assert_eq!(side, -1.0, "nosed away from home");
                    ball.bat((side * speed, -40.0));
                }
                Order::Grab => grabbed = true,
                Order::Brought => {
                    brought = true;
                    break;
                }
                Order::Done => break,
                _ => {}
            }
        }
        assert!(far > 200.0, "the ball only went {far} px");
        assert!(!grabbed, "a ball is nosed, not carried");
        assert!(
            brought && noses >= 1,
            "brought {brought} after {noses} noses"
        );
        assert!((ball.pos.0 - 500.0).abs() < 0.4 * size.0);
    }

    #[test]
    fn a_ball_against_the_edge_of_the_screens_is_still_nosed_back() {
        let t = Terrain::flat(AREA);
        let (left, right) = t.bounds();
        let size = (260.0, 190.0);
        let floor = 1032.0;
        // Hard against the right edge, home far to the left: it cannot get
        // round behind the ball.
        let mut ball = Prop::put(Thing::Ball, (right - 20.0, floor), 1.0);
        ball.resting = true;
        let reach = (left + 0.5 * size.0 + 1.0, right - 0.5 * size.0 - 1.0);
        let mut feet = (right - 600.0, floor);
        let mut c = Chase::new(Fetch::Retrieve, Thing::Ball, left + 400.0, 0.0, 0.5);
        let mut nosed = false;
        let mut now = 0.0;
        for _ in 0..(8 * 60) {
            now += 1.0 / 60.0;
            ball.step(1.0 / 60.0, &t);
            c.set_reach(reach);
            match c.tick(now, feet, size, &ball, false) {
                Order::Run { x, pace } => {
                    assert!(x <= reach.1, "sent past the edge to {x}");
                    feet.0 += (x - feet.0).signum() * 60.0 * pace / 60.0;
                }
                Order::Nose { side, speed } => {
                    nosed = true;
                    ball.bat((side * speed, -40.0));
                    break;
                }
                Order::Done => break,
                _ => {}
            }
        }
        assert!(nosed, "never got to nose it");
        assert!(ball.vel.0 < 0.0);
    }

    #[test]
    fn a_cat_bats_a_frisbee_a_few_times_then_loses_interest() {
        let disc = Prop {
            resting: true,
            ..Prop::put(Thing::Frisbee, (500.0, 1032.0), 1.0)
        };
        let mut c = Chase::new(Fetch::Bat, Thing::Frisbee, 0.0, 0.0, 0.0);
        let mut bats = 0;
        let mut now = 0.0;
        let order = loop {
            now += 0.1;
            match c.tick(now, (510.0, 1032.0), (200.0, 180.0), &disc, false) {
                Order::Bat { side } => {
                    assert_eq!(side, -1.0, "batted it toward itself");
                    bats += 1;
                }
                Order::Done => break Order::Done,
                Order::Run { .. } => panic!("ran when it was right there"),
                _ => {}
            }
            assert!(now < 20.0, "never lost interest");
        };
        assert_eq!(order, Order::Done);
        assert!((2..=4).contains(&bats), "{bats} bats");
    }

    #[test]
    fn birds_give_up_a_frisbee_soon_and_the_owl_only_watches_one() {
        let disc = Prop::put(Thing::Frisbee, (1500.0, 1032.0), 1.0);
        let mut chick = Chase::new(Fetch::Chase, Thing::Frisbee, 0.0, 0.0, 1.0);
        assert!(matches!(
            chick.tick(1.0, (200.0, 1032.0), (180.0, 180.0), &disc, false),
            Order::Run { .. }
        ));
        assert_eq!(
            chick.tick(4.1, (300.0, 1032.0), (180.0, 180.0), &disc, false),
            Order::Done
        );
        let mut owl = Chase::new(Fetch::Watch, Thing::Frisbee, 0.0, 0.0, 0.0);
        assert_eq!(
            owl.tick(1.0, (200.0, 1032.0), (180.0, 180.0), &disc, false),
            Order::Watch
        );
    }

    /// Roger (2026-11-06): every species brings a ball back, its own way.
    #[test]
    fn every_species_brings_a_ball_back() {
        let t = Terrain::flat(AREA);
        for how in [Fetch::Retrieve, Fetch::Bat, Fetch::Chase, Fetch::Watch] {
            let mut ball = Prop::thrown(Thing::Ball, (500.0, 900.0), 1.0, 1.0);
            let size = (220.0, 180.0);
            let mut feet = (450.0, 1032.0);
            let mut c = Chase::new(how, Thing::Ball, 500.0, 0.0, 0.0);
            let (mut carried, mut back) = (false, false);
            let mut now = 0.0;
            for _ in 0..(25 * 60) {
                now += 1.0 / 60.0;
                if !carried {
                    ball.step(1.0 / 60.0, &t);
                }
                match c.tick(now, feet, size, &ball, false) {
                    Order::Run { x, pace } => {
                        feet.0 += (x - feet.0).signum() * 60.0 * pace / 60.0;
                        if carried {
                            ball.pos.0 = feet.0;
                        }
                    }
                    Order::Nose { side, speed } => ball.bat((side * speed, -40.0)),
                    Order::Grab => carried = true,
                    Order::Drop | Order::Brought => {
                        back = true;
                        break;
                    }
                    Order::Done => break,
                    _ => {}
                }
            }
            assert!(back, "{how:?} never brought it back");
            let at = if carried { feet.0 } else { ball.pos.0 };
            assert!((at - 500.0).abs() < 0.4 * size.0, "{how:?} left it at {at}");
            assert_eq!(carried, how == Fetch::Watch, "{how:?} carried {carried}");
        }
    }

    #[test]
    fn whoever_gets_there_second_gives_up() {
        let ball = Prop::put(Thing::Ball, (500.0, 1032.0), 1.0);
        let mut c = Chase::new(Fetch::Retrieve, Thing::Ball, 0.0, 0.0, 0.0);
        assert_eq!(
            c.tick(1.0, (400.0, 1032.0), (260.0, 190.0), &ball, true),
            Order::Done
        );
    }

    #[test]
    fn about_a_third_of_cats_do_not_care_for_catnip() {
        let cats = (1..=300u64)
            .filter(|&id| Species::Fold.likes_catnip(id))
            .count();
        assert!((170..=230).contains(&cats), "{cats} of 300 like it");
        assert!(!(1..=50u64).any(|id| Species::Shiba.likes_catnip(id)));
    }

    #[test]
    fn every_species_has_a_scene_of_its_own() {
        for s in Species::ALL {
            assert!(!s.scenes().is_empty(), "{s:?}");
        }
    }

    #[test]
    fn a_pet_always_obeys_sooner_or_later_and_sooner_when_it_likes_you() {
        for roll in [0.0, 0.5, 1.0] {
            let new = obey_delay(1, false, roll);
            let close = obey_delay(5, false, roll);
            let moody = obey_delay(5, true, roll);
            assert!(close < new, "level 5 no quicker than level 1");
            assert!(close < 0.7, "{close:.2} s at level 5");
            assert!(moody > close + 2.5 && moody < 9.0, "{moody:.2} s moody");
        }
    }
}
