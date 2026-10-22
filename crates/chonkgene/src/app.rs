//! The pet process: one winit event loop owning every pet window (PLAN §3.2).

use std::time::{Duration, Instant};

use chonkgene_core::anim::{Animator, Expression};
use chonkgene_core::motion::{Area, Body, Motion};
use chonkgene_core::shake::ShakeDetector;
use chonkgene_core::tickle::{Reaction, Tickle};
use chonkgene_core::{Personality, Zone};
use chonkgene_platform::{Presenter, Shared};
use chonkgene_render::{BoneMap, Renderer};
use chonkgene_rig::{Pose, Rig, builtin};
use cpu_time::ProcessTime;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const STATS_EVERY: Duration = Duration::from_secs(5);
/// A press that moves further than this (pixels) is a lift, not a poke.
const DRAG_START: f32 = 4.0;
const HAPPY_SECS: f64 = 3.0;

pub struct Options {
    pub pets: usize,
    pub scale: f32,
    pub chonk: f32,
    pub coat: String,
    pub stats: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            pets: 1,
            scale: 1.0,
            chonk: 0.33,
            coat: "red".into(),
            stats: false,
        }
    }
}

pub fn run(opts: Options) -> Result<(), String> {
    let rig = Rig::load(builtin::SHIBA).map_err(|e| e.to_string())?;
    let coat = rig
        .coat(&opts.coat)
        .ok_or_else(|| format!("unknown coat {}", opts.coat))?;
    let mut builder = EventLoop::builder();
    chonkgene_platform::configure_event_loop(&mut builder);
    let event_loop = builder.build().map_err(|e| e.to_string())?;
    let mut app = App::new(opts, rig, coat);
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

struct Press {
    /// Cursor offset inside the window when the button went down.
    grab: (f32, f32),
    moved: bool,
    last: Instant,
}

struct Pet {
    window: Window,
    presenter: Presenter,
    renderer: Renderer,
    pose: Pose,
    body: Body,
    anim: Animator,
    tickle: Tickle,
    personality: Personality,
    shake: ShakeDetector,
    rng: fastrand::Rng,
    cursor: (f32, f32),
    press: Option<Press>,
    placed: (i32, i32),
    next: Instant,
    last: Instant,
    switch_at: f64,
    happy_until: f64,
}

struct App {
    opts: Options,
    rig: Rig,
    bones: BoneMap,
    coat: usize,
    shared: Option<Shared>,
    area: Area,
    pets: Vec<Pet>,
    start: Instant,
    stats_at: Instant,
    stats_cpu: ProcessTime,
    frames: u64,
}

impl App {
    fn new(opts: Options, rig: Rig, coat: usize) -> Self {
        let now = Instant::now();
        Self {
            opts,
            bones: BoneMap::new(&rig),
            rig,
            coat,
            shared: None,
            area: Area {
                x: 0.0,
                y: 0.0,
                w: 1920.0,
                h: 1080.0,
            },
            pets: Vec::new(),
            start: now,
            stats_at: now + STATS_EVERY,
            stats_cpu: ProcessTime::now(),
            frames: 0,
        }
    }

    fn secs(&self, at: Instant) -> f64 {
        (at - self.start).as_secs_f64()
    }

    fn spawn(&mut self, event_loop: &ActiveEventLoop, shared: &Shared) -> Result<(), String> {
        let size = Renderer::pixel_size(&self.rig, self.opts.scale);
        let size_f = (size.0 as f32, size.1 as f32);
        let now = Instant::now();
        for i in 0..self.opts.pets {
            let span = (self.area.w - size_f.0).max(1.0);
            let x = self.area.x + (self.area.w * 0.6 + i as f32 * size_f.0 * 0.8) % span;
            let body = Body::on_ground(&self.area, x, size_f);
            let pos = (body.pos.0.round() as i32, body.pos.1.round() as i32);
            let window = event_loop
                .create_window(chonkgene_platform::pet_window(size, pos, "肉肉賴桌"))
                .map_err(|e| e.to_string())?;
            let presenter = Presenter::new(shared, &window)?;
            if i == 0 {
                println!("window: {}", presenter.describe());
            }
            let renderer = Renderer::new(&self.rig, self.opts.scale, self.opts.chonk, self.coat)
                .ok_or("cannot allocate the pet's pixmap")?;
            let seed = 0x5EED + i as u64;
            self.pets.push(Pet {
                window,
                presenter,
                renderer,
                pose: self.rig.rest_pose(),
                body,
                anim: Animator::new(seed, self.opts.chonk),
                tickle: Tickle::default(),
                personality: Personality::SHIBA,
                shake: ShakeDetector::default(),
                rng: fastrand::Rng::with_seed(seed),
                cursor: (0.0, 0.0),
                press: None,
                placed: pos,
                next: now,
                last: now,
                switch_at: self.secs(now) + 2.0 + i as f64,
                happy_until: 0.0,
            });
        }
        Ok(())
    }

    fn on_mouse(&mut self, i: usize, state: ElementState, button: MouseButton) {
        let now = Instant::now();
        let t = self.secs(now);
        let p = &mut self.pets[i];
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => {
                p.press = Some(Press {
                    grab: p.cursor,
                    moved: false,
                    last: now,
                });
                p.shake = ShakeDetector::default();
            }
            (MouseButton::Left, ElementState::Released) => {
                let Some(press) = p.press.take() else {
                    return;
                };
                if press.moved {
                    p.body.release();
                } else {
                    let zone = p
                        .renderer
                        .hit(&self.rig, p.cursor.0, p.cursor.1)
                        .and_then(Zone::from_name)
                        .unwrap_or(Zone::Belly);
                    p.anim.poke(zone);
                    let reaction = p.tickle.poke(t, zone, &p.personality, &mut p.rng);
                    println!("pet {i}: poke {zone:?} → {reaction:?}");
                    react(p, reaction, t);
                }
                p.next = now;
            }
            (MouseButton::Right, ElementState::Pressed) => {
                println!("pet {i}: the right-click menu is not built yet");
            }
            _ => {}
        }
    }

    fn report(&mut self, now: Instant) {
        let wall = STATS_EVERY.as_secs_f64();
        let cpu = self.stats_cpu.elapsed().as_secs_f64() / wall * 100.0;
        let rss = memory_stats::memory_stats()
            .map(|m| m.physical_mem as f64 / 1_048_576.0)
            .unwrap_or(f64::NAN);
        println!(
            "[{:6.1}s] {} pets | {:5.1} frames/s | CPU {cpu:5.2}% of one core | RSS {rss:.1} MiB",
            self.secs(now),
            self.pets.len(),
            self.frames as f64 / wall,
        );
        self.frames = 0;
        self.stats_cpu = ProcessTime::now();
        self.stats_at = now + STATS_EVERY;
    }
}

fn react(p: &mut Pet, reaction: Reaction, t: f64) {
    p.anim.expression = match reaction {
        Reaction::Small | Reaction::Ignored => p.anim.expression,
        Reaction::Laugh => Expression::Giggle,
        Reaction::FedUp => Expression::FedUp,
        Reaction::Overdone => Expression::Sulk,
        Reaction::CalmedDown => Expression::Neutral,
        Reaction::MadeUp => {
            p.happy_until = t + HAPPY_SECS;
            Expression::Happy
        }
    };
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.shared.is_some() {
            return;
        }
        let shared = match Shared::new() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                event_loop.exit();
                return;
            }
        };
        let (x, y, w, h) = shared
            .work_area()
            .or_else(|| {
                event_loop.primary_monitor().map(|m| {
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
        println!("肉肉賴桌 Phase 1 prototype · {}", shared.describe());
        println!(
            "click to poke (belly, cheeks, paws, head, bottom) · drag to lift · shake while dragging · let go to throw"
        );
        println!("Ctrl+C here to quit");
        if let Err(e) = self.spawn(event_loop, &shared) {
            eprintln!("cannot create the pet window: {e}");
            event_loop.exit();
            return;
        }
        self.shared = Some(shared);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(i) = self.pets.iter().position(|p| p.window.id() == id) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                self.pets[i].cursor = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse(i, state, button),
            WindowEvent::RedrawRequested => {
                if let Some(shared) = &self.shared {
                    let p = &mut self.pets[i];
                    let frame = p.renderer.render(&self.rig, &p.pose);
                    if let Err(e) = p.presenter.present(shared, frame) {
                        eprintln!("present failed: {e}");
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(shared) = &self.shared else {
            return;
        };
        let now = Instant::now();
        let t = (now - self.start).as_secs_f64();
        let cursor = if self.pets.iter().any(|p| p.press.is_some()) {
            shared.cursor()
        } else {
            None
        };
        for (i, p) in self.pets.iter_mut().enumerate() {
            if p.next > now {
                continue;
            }
            let dt = (now - p.last).as_secs_f32().min(0.1);
            p.last = now;
            step(p, i, t, dt, now, cursor, &self.area);

            self.bones.apply(&p.anim.params(), &mut p.pose);
            let frame = p.renderer.render(&self.rig, &p.pose);
            if let Err(e) = p.presenter.present(shared, frame) {
                eprintln!("present failed: {e}");
            }
            self.frames += 1;
            let moving =
                p.press.as_ref().is_some_and(|pr| pr.moved) || p.body.motion == Motion::Fall;
            p.next = now + Duration::from_secs_f64(1.0 / f64::from(p.anim.fps(moving)));
        }
        if self.opts.stats && now >= self.stats_at {
            self.report(now);
        }
        let wake = self
            .pets
            .iter()
            .map(|p| p.next)
            .chain(self.opts.stats.then_some(self.stats_at))
            .min();
        event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

fn step(
    p: &mut Pet,
    i: usize,
    t: f64,
    dt: f32,
    now: Instant,
    cursor: Option<(i32, i32)>,
    area: &Area,
) {
    if let Some(press) = p.press.as_mut() {
        if let Some((cx, cy)) = cursor {
            let target = (cx as f32 - press.grab.0, cy as f32 - press.grab.1);
            let dist = (target.0 - p.body.pos.0).hypot(target.1 - p.body.pos.1);
            if !press.moved && dist > DRAG_START {
                press.moved = true;
            }
            if press.moved {
                let held_dt = (now - press.last).as_secs_f32();
                press.last = now;
                p.body.hold_at(target, held_dt);
                p.anim.sway = (-p.body.vel.0 * 0.03).clamp(-20.0, 20.0);
                if p.shake.feed(target.0, t) {
                    // Sleep comes in Phase 2; for now a shake just makes it dizzy.
                    println!("pet {i}: shaken!");
                    p.anim.poke(Zone::Belly);
                    p.anim.expression = Expression::FedUp;
                }
            }
        }
    } else {
        if let Some(landing) = p.body.step(dt, area) {
            p.anim.land(landing.speed);
        }
        if matches!(p.body.motion, Motion::Idle | Motion::Walk) && t >= p.switch_at {
            p.body.motion = if p.body.motion == Motion::Idle {
                Motion::Walk
            } else {
                Motion::Idle
            };
            if p.rng.bool() {
                p.body.facing = -p.body.facing;
            }
            p.switch_at = t + 3.0 + f64::from(p.rng.f32()) * 5.0;
        }
    }

    if let Some(reaction) = p.tickle.update(t, &p.personality) {
        println!("pet {i}: {reaction:?}");
        react(p, reaction, t);
    }
    if p.anim.expression == Expression::Happy && t >= p.happy_until {
        p.anim.expression = Expression::Neutral;
    }
    p.anim.walking = p.body.motion == Motion::Walk;
    p.anim.facing = p.body.facing;
    p.anim.step(dt);

    let placed = (p.body.pos.0.round() as i32, p.body.pos.1.round() as i32);
    if placed != p.placed {
        p.window
            .set_outer_position(PhysicalPosition::new(placed.0, placed.1));
        p.placed = placed;
    }
}
