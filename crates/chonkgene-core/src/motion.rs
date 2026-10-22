//! Moving around the desktop (PLAN §1.2, §1.3): walking along the bottom of
//! the work area, being held, and falling after a throw.

const GRAVITY: f32 = 2400.0;
const WALK_SPEED: f32 = 60.0;
/// Landing slower than this stops instead of bouncing.
const BOUNCE_MIN: f32 = 250.0;
const BOUNCE_KEEP: f32 = 0.35;
const WALL_KEEP: f32 = 0.5;

/// The work area, in physical pixels.
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
}

/// Where a pet window is and how it moves. `pos` is the window's top-left;
/// `size` is the window size; the feet are at the bottom edge.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: (f32, f32),
    pub vel: (f32, f32),
    pub size: (f32, f32),
    pub motion: Motion,
    /// 1.0 faces right.
    pub facing: f32,
    /// Walking speed multiplier (zoomies run faster).
    pub pace: f32,
}

/// Something worth a squash: the speed of a landing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Landing {
    pub speed: f32,
}

impl Body {
    pub fn on_ground(area: &Area, x: f32, size: (f32, f32)) -> Self {
        let mut b = Self {
            pos: (x, 0.0),
            vel: (0.0, 0.0),
            size,
            motion: Motion::Idle,
            facing: 1.0,
            pace: 1.0,
        };
        b.pos.1 = b.ground(area);
        b
    }

    fn ground(&self, area: &Area) -> f32 {
        area.y + area.h - self.size.1
    }

    fn bounds(&self, area: &Area) -> (f32, f32) {
        (area.x, (area.x + area.w - self.size.0).max(area.x))
    }

    /// Follow the cursor while held; `vel` is kept for the throw.
    pub fn hold_at(&mut self, pos: (f32, f32), dt: f32) {
        if dt > 0.0 {
            self.vel = ((pos.0 - self.pos.0) / dt, (pos.1 - self.pos.1) / dt);
        }
        self.pos = pos;
        self.motion = Motion::Held;
    }

    pub fn release(&mut self) {
        self.motion = Motion::Fall;
    }

    pub fn step(&mut self, dt: f32, area: &Area) -> Option<Landing> {
        let (left, right) = self.bounds(area);
        let ground = self.ground(area);
        match self.motion {
            Motion::Held => None,
            Motion::Idle => {
                self.pos.1 = ground;
                None
            }
            Motion::Walk => {
                self.pos.0 += WALK_SPEED * self.pace * self.facing * dt;
                if self.pos.0 <= left {
                    self.pos.0 = left;
                    self.facing = 1.0;
                } else if self.pos.0 >= right {
                    self.pos.0 = right;
                    self.facing = -1.0;
                }
                self.pos.1 = ground;
                None
            }
            Motion::Fall => {
                self.vel.1 += GRAVITY * dt;
                self.pos.0 += self.vel.0 * dt;
                self.pos.1 += self.vel.1 * dt;
                if self.pos.0 < left || self.pos.0 > right {
                    self.pos.0 = self.pos.0.clamp(left, right);
                    self.vel.0 = -self.vel.0 * WALL_KEEP;
                }
                if self.pos.1 < ground {
                    return None;
                }
                self.pos.1 = ground;
                let speed = self.vel.1;
                if speed > BOUNCE_MIN {
                    self.vel = (self.vel.0 * 0.6, -speed * BOUNCE_KEEP);
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

    const AREA: Area = Area {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1032.0,
    };
    const SIZE: (f32, f32) = (220.0, 196.0);

    #[test]
    fn a_thrown_pet_lands_and_settles_inside_the_area() {
        let mut b = Body::on_ground(&AREA, 800.0, SIZE);
        b.hold_at((900.0, 300.0), 0.0);
        b.hold_at((1300.0, 200.0), 1.0 / 60.0);
        b.release();
        let mut landings = 0;
        for _ in 0..600 {
            if b.step(1.0 / 60.0, &AREA).is_some() {
                landings += 1;
            }
            assert!(b.pos.0 >= AREA.x && b.pos.0 + SIZE.0 <= AREA.x + AREA.w);
        }
        assert!(landings >= 2, "should bounce at least once");
        assert_eq!(b.motion, Motion::Idle);
        assert_eq!(b.pos.1, AREA.h - SIZE.1);
    }

    #[test]
    fn walking_turns_around_at_the_edges() {
        let mut b = Body::on_ground(&AREA, 10.0, SIZE);
        b.motion = Motion::Walk;
        b.facing = -1.0;
        b.step(1.0, &AREA);
        assert_eq!(b.pos.0, 0.0);
        assert_eq!(b.facing, 1.0);
    }
}
