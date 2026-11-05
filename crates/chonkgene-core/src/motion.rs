//! Moving around the desktop (PLAN §1.2, §1.3): walking along ledges (the
//! bottom of the work area, the tops of windows), falling off their ends,
//! being held, thrown, carried by a moving window, and hopping up onto a
//! window.

use crate::terrain::{Ledge, Support, Terrain};

const GRAVITY: f32 = 2400.0;
const WALK_SPEED: f32 = 60.0;
/// Seconds a walk takes to get going or to come to a stop at the usual
/// pace (quicker at a faster one). The legs ramp the same way (chonkgene-
/// render), so a planted paw stays put while the pet slows down.
pub const WALK_RAMP: f32 = 0.5;

/// How fast the pet goes over the ground, against full pace, a ramp `speed`
/// (0 standing … 1 full pace) into a walk: the legs both step slower and
/// swing shorter as they slow down, so ground speed falls faster than the
/// ramp.
pub fn gait(speed: f32) -> f32 {
    let s = speed.clamp(0.0, 1.0);
    s * (0.4 + 0.6 * s)
}
/// Landing slower than this stops instead of bouncing.
const BOUNCE_MIN: f32 = 250.0;
const BOUNCE_KEEP: f32 = 0.35;
const WALL_KEEP: f32 = 0.5;
/// A supporting ledge may move this far between frames and still be the
/// one under the feet (a window being dragged).
const STAY_ON: f32 = 6.0;
/// A window yanked faster than this (pixels a second) throws its pet off.
const SHAKE_OFF: f32 = 1500.0;
/// How far above a ledge a hop up peaks, pixels.
const HOP_CLEARANCE: f32 = 40.0;

/// A work area, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    Idle,
    Walk,
    Held,
    Fall,
    /// Flying somewhere (a bird to its perch and back down): see
    /// [`Body::fly_to`].
    Fly,
}

/// A flight from one place to another: the feet from `from` to `to`
/// (screen pixels) over `secs`, arcing up by `arc` pixels on the way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub secs: f32,
    pub t: f32,
    pub arc: f32,
}

/// Flying, pixels a second (wings beating, not falling).
const FLY_SPEED: f32 = 320.0;

/// Where a pet window is and how it moves. `pos` is the window's top-left;
/// `size` is the window size; the feet are `foot` pixels above its bottom
/// edge (the contact shadow goes below them).
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: (f32, f32),
    pub vel: (f32, f32),
    pub size: (f32, f32),
    pub foot: f32,
    pub motion: Motion,
    /// 1.0 faces right.
    pub facing: f32,
    /// Walking speed multiplier (zoomies run faster).
    pub pace: f32,
    /// How far into a walk, 0 standing … 1 full pace: walking ramps it up,
    /// stopping ramps it down, and the pet keeps going until it is 0.
    pub speed: f32,
    /// What the feet are on; none in the air.
    pub on: Option<Support>,
    /// Under way while flying.
    pub flight: Option<Flight>,
}

/// Something worth a squash: the speed of a landing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landing {
    pub speed: f32,
}

impl Body {
    /// Standing on the floor under `x` (the window's left edge).
    pub fn on_ground(terrain: &Terrain, x: f32, size: (f32, f32), foot: f32) -> Self {
        let mut b = Self {
            pos: (x, 0.0),
            vel: (0.0, 0.0),
            size,
            foot,
            motion: Motion::Idle,
            facing: 1.0,
            pace: 1.0,
            speed: 0.0,
            on: None,
            flight: None,
        };
        if let Some(l) = terrain.floor_under(b.feet().0) {
            b.pos.0 = b.pos.0.clamp(l.x0 - size.0 / 2.0, l.x1 - size.0 / 2.0);
            b.stand(&l);
        }
        b
    }

    /// The middle of the feet, in screen pixels.
    pub fn feet(&self) -> (f32, f32) {
        (
            self.pos.0 + self.size.0 / 2.0,
            self.pos.1 + self.size.1 - self.foot,
        )
    }

    fn stand(&mut self, l: &Ledge) {
        self.pos.1 = l.y - self.size.1 + self.foot;
        self.on = Some(l.on);
    }

    /// Is it standing on a window (rather than a floor or nothing)?
    pub fn on_window(&self) -> bool {
        matches!(self.on, Some(Support::Window(_)))
    }

    /// Follow the cursor while held; `vel` is kept for the throw.
    pub fn hold_at(&mut self, pos: (f32, f32), dt: f32) {
        if dt > 0.0 {
            self.vel = ((pos.0 - self.pos.0) / dt, (pos.1 - self.pos.1) / dt);
        }
        self.pos = pos;
        self.motion = Motion::Held;
        self.speed = 0.0;
        self.on = None;
    }

    pub fn release(&mut self) {
        self.motion = Motion::Fall;
        self.speed = 0.0;
        self.on = None;
    }

    /// Hop up onto a ledge at height `y`, landing with the feet at `x`.
    pub fn leap(&mut self, x: f32, y: f32) {
        let (fx, fy) = self.feet();
        let rise = (fy - y).max(0.0) + HOP_CLEARANCE;
        let up = (2.0 * GRAVITY * rise).sqrt();
        let secs = up / GRAVITY + (2.0 * HOP_CLEARANCE / GRAVITY).sqrt();
        self.vel = ((x - fx) / secs, -up);
        if self.vel.0 != 0.0 {
            self.facing = self.vel.0.signum();
        }
        self.motion = Motion::Fall;
        self.speed = 0.0;
        self.on = None;
    }

    /// Fly so the feet come down at `(x, y)` (a perch, or the floor),
    /// arcing up over the way there.
    pub fn fly_to(&mut self, x: f32, y: f32) {
        let from = self.feet();
        let d = (x - from.0).hypot(y - from.1);
        let secs = (d / FLY_SPEED).clamp(0.8, 4.0);
        self.flight = Some(Flight {
            from,
            to: (x, y),
            secs,
            t: 0.0,
            arc: (0.25 * d).clamp(40.0, 160.0),
        });
        if x != from.0 {
            self.facing = (x - from.0).signum();
        }
        self.motion = Motion::Fly;
        self.speed = 0.0;
        self.on = None;
    }

    /// Walking or slowing to a stop: the window is still going.
    pub fn moving_on(&self) -> bool {
        matches!(self.motion, Motion::Idle | Motion::Walk) && self.speed > 0.0
    }

    /// The desktop changed over `dt` seconds: a pet on a window that moved
    /// moves with it, and one yanked hard is thrown off.
    pub fn carry(&mut self, old: &Terrain, new: &Terrain, dt: f32) {
        let Some(Support::Window(id)) = self.on else {
            return;
        };
        let (Some(a), Some(b)) = (old.window(id), new.window(id)) else {
            return;
        };
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        self.pos.0 += dx;
        self.pos.1 += dy;
        if dt > 0.0 && dx.hypot(dy) / dt > SHAKE_OFF {
            self.vel = (0.4 * dx / dt, (0.4 * dy / dt).min(0.0));
            self.motion = Motion::Fall;
            self.on = None;
        }
    }

    pub fn step(&mut self, dt: f32, terrain: &Terrain) -> Option<Landing> {
        let (left, right) = terrain.bounds();
        let right = (right - self.size.0).max(left);
        match self.motion {
            Motion::Held => None,
            Motion::Fly => {
                let Some(f) = self.flight.as_mut() else {
                    self.motion = Motion::Fall;
                    return None;
                };
                f.t += dt;
                let u = (f.t / f.secs).min(1.0);
                // Eased along, over an arc that comes in level to land.
                let e = u * u * (3.0 - 2.0 * u);
                let up = f.arc * (std::f32::consts::PI * u).sin();
                let (x, y) = (
                    f.from.0 + (f.to.0 - f.from.0) * e,
                    f.from.1 + (f.to.1 - f.from.1) * e - up,
                );
                let (to, done) = (f.to, u >= 1.0);
                self.pos = (x - self.size.0 / 2.0, y - self.size.1 + self.foot);
                self.vel = (0.0, 0.0);
                if !done {
                    return None;
                }
                self.flight = None;
                match terrain.ledge_level(to.0, to.1) {
                    Some(l) => {
                        self.stand(&l);
                        self.motion = Motion::Idle;
                        Some(Landing { speed: 300.0 })
                    }
                    // Its perch went away under it: down it goes.
                    None => {
                        self.motion = Motion::Fall;
                        None
                    }
                }
            }
            Motion::Idle | Motion::Walk => {
                // Getting going and slowing down take a moment (quicker at
                // a faster pace), rather than the window jumping to full
                // speed or stopping dead.
                let goal = if self.motion == Motion::Walk {
                    1.0
                } else {
                    0.0
                };
                let ramp = dt * self.pace.max(0.2) / WALK_RAMP;
                self.speed += (goal - self.speed).clamp(-ramp, ramp);
                let vx = WALK_SPEED * self.pace * gait(self.speed) * self.facing;
                if self.speed > 0.0 {
                    let was = self.pos.0;
                    self.pos.0 += vx * dt;
                    if self.pos.0 <= left {
                        self.pos.0 = left;
                        self.facing = 1.0;
                    } else if self.pos.0 >= right {
                        self.pos.0 = right;
                        self.facing = -1.0;
                    } else if !terrain.over_screen(self.feet().0) {
                        // A gap between monitors: turn back.
                        self.pos.0 = was;
                        self.facing = -self.facing;
                    }
                }
                let (x, y) = self.feet();
                let under = self
                    .on
                    .and_then(|s| terrain.ledge_on(s, x))
                    .filter(|l| (l.y - y).abs() <= STAY_ON)
                    .or_else(|| terrain.ledge_level(x, y));
                match under {
                    Some(l) => self.stand(&l),
                    None => {
                        // Walked off the end, or the window went away.
                        self.vel = (vx, 0.0);
                        self.motion = Motion::Fall;
                        self.speed = 0.0;
                        self.on = None;
                    }
                }
                None
            }
            Motion::Fall => {
                let (_, y0) = self.feet();
                self.vel.1 += GRAVITY * dt;
                self.pos.0 += self.vel.0 * dt;
                self.pos.1 += self.vel.1 * dt;
                if self.pos.0 < left || self.pos.0 > right {
                    self.pos.0 = self.pos.0.clamp(left, right);
                    self.vel.0 = -self.vel.0 * WALL_KEEP;
                }
                let (x, y1) = self.feet();
                let landing = if self.vel.1 > 0.0 {
                    terrain.landing(x, y0, y1).or_else(|| {
                        // Fell past everything (a gap between monitors):
                        // back onto the nearest floor.
                        terrain.floor_under(x).filter(|l| y1 > l.y).inspect(|l| {
                            self.pos.0 = self
                                .pos
                                .0
                                .clamp(l.x0 - self.size.0 / 2.0, l.x1 - self.size.0 / 2.0);
                        })
                    })
                } else {
                    None
                };
                let l = landing?;
                self.stand(&l);
                let speed = self.vel.1;
                if speed > BOUNCE_MIN {
                    self.vel = (self.vel.0 * 0.6, -speed * BOUNCE_KEEP);
                    self.on = None;
                } else {
                    self.vel = (0.0, 0.0);
                    self.motion = Motion::Idle;
                }
                Some(Landing { speed })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::Win;

    const AREA: Area = Area {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1032.0,
    };
    const SIZE: (f32, f32) = (220.0, 196.0);
    const FOOT: f32 = 8.0;

    fn flat() -> Terrain {
        Terrain::flat(AREA)
    }

    fn run(b: &mut Body, t: &Terrain, secs: f32) -> usize {
        let mut landings = 0;
        for _ in 0..(secs * 60.0) as usize {
            if b.step(1.0 / 60.0, t).is_some() {
                landings += 1;
            }
        }
        landings
    }

    #[test]
    fn a_bird_flies_up_to_its_perch_and_stands_on_it() {
        use crate::terrain::Support;
        let t = flat().with_perch(1, 500.0, 1200.0, 1360.0);
        let mut b = Body::on_ground(&t, 300.0, SIZE, FOOT);
        b.fly_to(1280.0, 500.0);
        let mut highest = f32::MAX;
        let mut landed = 0;
        for _ in 0..(5 * 60) {
            if b.step(1.0 / 60.0, &t).is_some() {
                landed += 1;
            }
            highest = highest.min(b.feet().1);
        }
        assert_eq!(landed, 1);
        assert_eq!(b.motion, Motion::Idle);
        assert_eq!(b.on, Some(Support::Perch(1)));
        assert!((b.feet().0 - 1280.0).abs() < 1.0 && (b.feet().1 - 500.0).abs() < 0.5);
        assert!(highest < 480.0, "never rose above the perch: {highest}");
        // Taken away from under it, it falls back to the floor.
        let floor = flat();
        run(&mut b, &floor, 3.0);
        assert_eq!(b.feet().1, 1032.0);
    }

    #[test]
    fn a_thrown_pet_lands_and_settles_inside_the_area() {
        let t = flat();
        let mut b = Body::on_ground(&t, 800.0, SIZE, FOOT);
        b.hold_at((900.0, 300.0), 0.0);
        b.hold_at((1300.0, 200.0), 1.0 / 60.0);
        b.release();
        let mut landings = 0;
        for _ in 0..600 {
            if b.step(1.0 / 60.0, &t).is_some() {
                landings += 1;
            }
            assert!(b.pos.0 >= AREA.x && b.pos.0 + SIZE.0 <= AREA.x + AREA.w);
        }
        assert!(landings >= 2, "should bounce at least once");
        assert_eq!(b.motion, Motion::Idle);
        assert_eq!(b.feet().1, AREA.h);
        assert_eq!(b.on, Some(Support::Floor(0)));
    }

    /// Roger saw pets walking along stop dead: a walk gets going and comes
    /// to a stop over a moment, never jumping in speed.
    #[test]
    fn a_walk_eases_in_and_out_instead_of_stopping_dead() {
        let t = flat();
        let mut b = Body::on_ground(&t, 800.0, SIZE, FOOT);
        b.motion = Motion::Walk;
        let dt = 1.0 / 60.0;
        let mut xs = vec![b.pos.0];
        for f in 0..180 {
            if f == 90 {
                b.motion = Motion::Idle;
            }
            b.step(dt, &t);
            xs.push(b.pos.0);
        }
        let v: Vec<f32> = xs.windows(2).map(|w| (w[1] - w[0]) / dt).collect();
        let jump = v
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(jump < 8.0, "speed jumped {jump:.1} px/s in a frame");
        assert!((v[80] - WALK_SPEED).abs() < 0.1, "full pace {}", v[80]);
        // It carries on a little after being told to stop, then stands.
        assert!(v[92] > 0.5 * WALK_SPEED, "{}", v[92]);
        assert_eq!(v[90 + (60.0 * WALK_RAMP) as usize + 1], 0.0);
        assert_eq!(b.speed, 0.0);
    }

    #[test]
    fn walking_turns_around_at_the_edges() {
        let t = flat();
        let mut b = Body::on_ground(&t, 10.0, SIZE, FOOT);
        b.motion = Motion::Walk;
        b.facing = -1.0;
        b.speed = 1.0;
        b.step(1.0, &t);
        assert_eq!(b.pos.0, 0.0);
        assert_eq!(b.facing, 1.0);
    }

    #[test]
    fn a_pet_dropped_over_a_window_lands_on_top_of_it() {
        let t = Terrain::new(
            vec![AREA],
            &[Win {
                id: 7,
                x: 600.0,
                y: 600.0,
                w: 600.0,
                h: 300.0,
            }],
            200.0,
        );
        let mut b = Body::on_ground(&t, 700.0, SIZE, FOOT);
        b.hold_at((700.0, 100.0), 0.0);
        b.release();
        run(&mut b, &t, 3.0);
        assert_eq!(b.on, Some(Support::Window(7)));
        assert_eq!(b.feet().1, 600.0);
    }

    #[test]
    fn walking_off_the_end_of_a_window_falls_to_the_floor() {
        let t = Terrain::new(
            vec![AREA],
            &[Win {
                id: 7,
                x: 600.0,
                y: 600.0,
                w: 400.0,
                h: 300.0,
            }],
            200.0,
        );
        let mut b = Body::on_ground(&t, 700.0, SIZE, FOOT);
        b.hold_at((700.0, 200.0), 0.0);
        b.release();
        run(&mut b, &t, 2.0);
        assert_eq!(b.on, Some(Support::Window(7)));
        b.motion = Motion::Walk;
        b.facing = 1.0;
        let landings = run(&mut b, &t, 8.0);
        assert!(landings >= 1);
        assert_eq!(b.on, Some(Support::Floor(0)));
        assert!(b.feet().0 > 1000.0, "went off the right end");
    }

    #[test]
    fn a_pet_on_a_window_rides_along_when_it_moves() {
        let before = Terrain::new(
            vec![AREA],
            &[Win {
                id: 7,
                x: 600.0,
                y: 600.0,
                w: 600.0,
                h: 300.0,
            }],
            200.0,
        );
        let after = Terrain::new(
            vec![AREA],
            &[Win {
                id: 7,
                x: 500.0,
                y: 500.0,
                w: 600.0,
                h: 300.0,
            }],
            200.0,
        );
        let mut b = Body::on_ground(&before, 700.0, SIZE, FOOT);
        b.hold_at((700.0, 200.0), 0.0);
        b.release();
        run(&mut b, &before, 2.0);
        let x = b.feet().0;
        b.carry(&before, &after, 0.1);
        b.step(1.0 / 60.0, &after);
        assert_eq!(b.on, Some(Support::Window(7)));
        assert_eq!(b.feet(), (x - 100.0, 500.0));
    }

    #[test]
    fn a_closed_window_drops_its_pet() {
        let with = Terrain::new(
            vec![AREA],
            &[Win {
                id: 7,
                x: 600.0,
                y: 600.0,
                w: 600.0,
                h: 300.0,
            }],
            200.0,
        );
        let without = flat();
        let mut b = Body::on_ground(&with, 700.0, SIZE, FOOT);
        b.hold_at((700.0, 200.0), 0.0);
        b.release();
        run(&mut b, &with, 2.0);
        b.carry(&with, &without, 0.1);
        run(&mut b, &without, 2.0);
        assert_eq!(b.on, Some(Support::Floor(0)));
    }

    #[test]
    fn a_window_yanked_hard_throws_its_pet_off() {
        let before = Terrain::new(
            vec![AREA],
            &[Win {
                id: 7,
                x: 600.0,
                y: 600.0,
                w: 600.0,
                h: 300.0,
            }],
            200.0,
        );
        let after = Terrain::new(
            vec![AREA],
            &[Win {
                id: 7,
                x: 1000.0,
                y: 600.0,
                w: 600.0,
                h: 300.0,
            }],
            200.0,
        );
        let mut b = Body::on_ground(&before, 700.0, SIZE, FOOT);
        b.hold_at((700.0, 200.0), 0.0);
        b.release();
        run(&mut b, &before, 2.0);
        b.carry(&before, &after, 0.1);
        assert_eq!(b.motion, Motion::Fall);
        assert!(b.vel.0 > 0.0, "flung the way the window went");
    }

    #[test]
    fn a_hop_up_lands_on_the_window() {
        let t = Terrain::new(
            vec![AREA],
            &[Win {
                id: 7,
                x: 700.0,
                y: 880.0,
                w: 500.0,
                h: 200.0,
            }],
            200.0,
        );
        let mut b = Body::on_ground(&t, 500.0, SIZE, FOOT);
        let (fx, fy) = b.feet();
        let (l, x) = t.ledge_above(fx, fy, 40.0, 260.0, 300.0).unwrap();
        b.leap(x, l.y);
        run(&mut b, &t, 2.0);
        assert_eq!(b.on, Some(Support::Window(7)));
        // It lands on the spot and bounces once, sliding a little on.
        assert!((b.feet().0 - x).abs() < 24.0, "{} vs {x}", b.feet().0);
    }

    #[test]
    fn walking_crosses_to_the_next_monitor() {
        let right = Area { x: 1920.0, ..AREA };
        let t = Terrain::new(vec![AREA, right], &[], 0.0);
        let mut b = Body::on_ground(&t, 1800.0, SIZE, FOOT);
        b.motion = Motion::Walk;
        b.facing = 1.0;
        run(&mut b, &t, 6.0);
        assert!(b.feet().0 > 1920.0);
        assert_eq!(b.on, Some(Support::Floor(1)));
        assert_eq!(b.motion, Motion::Walk);
    }
}
