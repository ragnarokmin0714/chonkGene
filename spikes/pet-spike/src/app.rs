//! S1 + S2 on a real desktop: transparent always-on-top pet windows with
//! per-pixel click-through, walking, lift / shake / throw, and live numbers.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

use tiny_skia::{Color, Pixmap};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId, WindowLevel};

use crate::ipc;
use crate::metrics::{self, Sampler};
use crate::pet::{Mode, Pet, SIZE};
use crate::present::{Presenter, Shared};

const WALK_SPEED: f64 = 60.0;
const GRAVITY: f64 = 2400.0;
const REPORT_EVERY: Duration = Duration::from_secs(5);
/// Direction changes within this window that count as a shake (PLAN §1.2).
const SHAKE_WINDOW: Duration = Duration::from_millis(1200);
const SHAKE_REVERSALS: usize = 4;

pub struct Options {
    pub pets: usize,
    pub x11_managed: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            pets: 6,
            x11_managed: false,
        }
    }
}

#[derive(Debug)]
pub enum UserEvent {
    Ipc(String),
}

pub fn run(opts: Options) -> Result<(), String> {
    #[allow(unused_mut)]
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    // v1 speaks X11 everywhere on Linux, XWayland included (PLAN §0).
    #[cfg(target_os = "linux")]
    winit::platform::x11::EventLoopBuilderExtX11::with_x11(&mut builder);
    let event_loop = builder.build().map_err(|e| e.to_string())?;
    let sock =
        ipc::serve(event_loop.create_proxy()).map_err(|e| format!("cannot start IPC: {e}"))?;
    let mut app = App::new(opts, sock);
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

/// x, y, width, height in physical pixels.
type Area = (i32, i32, i32, i32);

struct Press {
    /// Cursor offset inside the window when the button went down.
    grab: (f64, f64),
    moved: bool,
    last: (f64, f64),
    last_t: Instant,
    dir: i8,
    reversals: Vec<Instant>,
    shaken: bool,
}

struct PetWin {
    window: Window,
    presenter: Presenter,
    pm: Pixmap,
    pet: Pet,
    pos: (f64, f64),
    vel: (f64, f64),
    cursor: (f64, f64),
    press: Option<Press>,
    placed: (i32, i32),
    next: Instant,
    last: Instant,
    switch_at: Instant,
}

#[derive(Default)]
struct Stats {
    frames: u64,
    draw: Duration,
    present: Duration,
}

struct App {
    opts: Options,
    sock: String,
    shared: Option<Shared>,
    pets: Vec<PetWin>,
    area: Area,
    stats: Stats,
    sampler: Sampler,
    started: Instant,
    report_at: Instant,
    child: Option<Child>,
    rng: u32,
}

impl App {
    fn new(opts: Options, sock: String) -> Self {
        let now = Instant::now();
        Self {
            opts,
            sock,
            shared: None,
            pets: Vec::new(),
            area: (0, 0, 1920, 1080),
            stats: Stats::default(),
            sampler: Sampler::new(),
            started: now,
            report_at: now + REPORT_EVERY,
            child: None,
            rng: 0x9E37_79B9,
        }
    }

    fn spawn_pets(&mut self, event_loop: &ActiveEventLoop, shared: &Shared) -> Result<(), String> {
        let ground = ground(self.area);
        let span = (self.area.2 - SIZE as i32 - 80).max(1);
        let now = Instant::now();
        for i in 0..self.opts.pets {
            let x = self.area.0 + 40 + (i as i32 * 170) % span;
            let window = event_loop
                .create_window(self.attributes(x, ground as i32))
                .map_err(|e| e.to_string())?;
            let presenter = Presenter::new(shared, &window)?;
            if i == 0 {
                println!("window: {}", presenter.describe());
            }
            self.pets.push(PetWin {
                window,
                presenter,
                pm: Pixmap::new(SIZE, SIZE).ok_or("cannot allocate a pixmap")?,
                pet: Pet::new(i as u32),
                pos: (f64::from(x), ground),
                vel: (0.0, 0.0),
                cursor: (0.0, 0.0),
                press: None,
                placed: (x, ground as i32),
                next: now,
                last: now,
                switch_at: now + Duration::from_millis(1500 + 700 * i as u64),
            });
        }
        Ok(())
    }

    fn attributes(&self, x: i32, y: i32) -> winit::window::WindowAttributes {
        let attrs = Window::default_attributes()
            .with_title("chonkGene pet spike")
            .with_decorations(false)
            .with_resizable(false)
            .with_inner_size(PhysicalSize::new(SIZE, SIZE))
            .with_position(PhysicalPosition::new(x, y))
            .with_window_level(WindowLevel::AlwaysOnTop);
        #[cfg(target_os = "linux")]
        let attrs = {
            use winit::platform::x11::{WindowAttributesExtX11, WindowType};
            // An ARGB visual is only chosen for transparent windows.
            let attrs = attrs.with_transparent(true);
            if self.opts.x11_managed {
                attrs.with_x11_window_type(vec![WindowType::Utility])
            } else {
                attrs.with_override_redirect(true)
            }
        };
        #[cfg(windows)]
        let attrs = {
            use winit::platform::windows::WindowAttributesExtWindows;
            // Transparency comes from WS_EX_LAYERED in the presenter, not from winit.
            attrs.with_skip_taskbar(true)
        };
        attrs
    }

    fn on_mouse(&mut self, i: usize, state: ElementState, button: MouseButton) {
        let now = Instant::now();
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) => {
                let p = &mut self.pets[i];
                p.press = Some(Press {
                    grab: p.cursor,
                    moved: false,
                    last: p.pos,
                    last_t: now,
                    dir: 0,
                    reversals: Vec::new(),
                    shaken: false,
                });
                p.next = now;
            }
            (MouseButton::Left, ElementState::Released) => {
                let p = &mut self.pets[i];
                if let Some(press) = p.press.take() {
                    if press.moved {
                        p.pet.mode = Mode::Fall;
                    } else {
                        p.pet.poke();
                        println!("pet {i}: poke");
                    }
                    p.next = now;
                }
            }
            (MouseButton::Right, ElementState::Pressed) => self.open_settings(),
            _ => {}
        }
    }

    fn open_settings(&mut self) {
        if let Some(child) = &mut self.child
            && matches!(child.try_wait(), Ok(None))
        {
            println!("settings is already open");
            return;
        }
        let spawned = std::env::current_exe()
            .and_then(|exe| Command::new(exe).arg("--settings").arg(&self.sock).spawn());
        match spawned {
            Ok(child) => {
                println!("settings process started (pid {})", child.id());
                self.child = Some(child);
            }
            Err(e) => eprintln!("cannot start the settings process: {e}"),
        }
    }

    fn report(&mut self, now: Instant) {
        let secs = (now - self.started).as_secs_f64();
        let frames = self.stats.frames.max(1) as f64;
        println!(
            "[{secs:6.1}s] {} pets | {:5.1} frames/s | draw {:4.0} µs | present {:4.0} µs | CPU {:5.2}% of one core | RSS {:.1} MiB",
            self.pets.len(),
            self.stats.frames as f64 / REPORT_EVERY.as_secs_f64(),
            self.stats.draw.as_secs_f64() * 1e6 / frames,
            self.stats.present.as_secs_f64() * 1e6 / frames,
            self.sampler.cpu_percent(),
            metrics::rss_mib(),
        );
        self.stats = Stats::default();
        self.report_at = now + REPORT_EVERY;
    }
}

impl ApplicationHandler<UserEvent> for App {
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
        self.area = shared
            .work_area()
            .or_else(|| {
                event_loop.primary_monitor().map(|m| {
                    let (p, s) = (m.position(), m.size());
                    (p.x, p.y, s.width as i32, s.height as i32)
                })
            })
            .unwrap_or(self.area);
        println!("chonkGene Phase 0 pet spike: {} pets", self.opts.pets);
        println!("platform: {}", shared.describe());
        println!("work area: {:?} (x, y, width, height)", self.area);
        println!("click a belly to poke · drag to lift, shake while dragging, let go to throw");
        println!("right-click a pet to open the settings spike · Ctrl+C here to quit");
        if let Err(e) = self.spawn_pets(event_loop, &shared) {
            eprintln!("cannot create pet windows: {e}");
            event_loop.exit();
            return;
        }
        self.shared = Some(shared);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        let UserEvent::Ipc(msg) = event;
        println!("IPC <- {msg}");
        if let Some(name) = msg.strip_prefix("name ") {
            for p in &self.pets {
                p.window.set_title(name);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(i) = self.pets.iter().position(|p| p.window.id() == id) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => {
                self.pets[i].cursor = (position.x, position.y)
            }
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse(i, state, button),
            WindowEvent::RedrawRequested => {
                if let Some(shared) = &self.shared {
                    draw(&mut self.pets[i], shared, &mut self.stats);
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
        let cursor = if self.pets.iter().any(|p| p.press.is_some()) {
            shared.cursor()
        } else {
            None
        };
        for (i, p) in self.pets.iter_mut().enumerate() {
            if p.next > now {
                continue;
            }
            update(p, i, self.area, now, cursor, &mut self.rng);
            draw(p, shared, &mut self.stats);
            p.next = now + Duration::from_secs_f64(1.0 / f64::from(p.pet.fps()));
        }

        if let Some(child) = &mut self.child
            && let Ok(Some(status)) = child.try_wait()
        {
            println!("settings process ended ({status}); pets keep running");
            self.child = None;
        }
        if now >= self.report_at {
            self.report(now);
        }

        let wake = self
            .pets
            .iter()
            .map(|p| p.next)
            .chain([self.report_at])
            .min();
        event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

fn ground(area: Area) -> f64 {
    f64::from(area.1 + area.3 - SIZE as i32)
}

fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

fn update(
    p: &mut PetWin,
    i: usize,
    area: Area,
    now: Instant,
    cursor: Option<(i32, i32)>,
    rng: &mut u32,
) {
    let dt = (now - p.last).as_secs_f64().min(0.1);
    p.last = now;
    let ground = ground(area);
    let left = f64::from(area.0);
    let right = f64::from(area.0 + area.2 - SIZE as i32);

    if let Some(press) = p.press.as_mut() {
        if let Some((cx, cy)) = cursor {
            let target = (f64::from(cx) - press.grab.0, f64::from(cy) - press.grab.1);
            if !press.moved && (target.0 - p.pos.0).hypot(target.1 - p.pos.1) > 4.0 {
                press.moved = true;
                p.pet.mode = Mode::Held;
            }
            if press.moved {
                let step_dt = (now - press.last_t).as_secs_f64().max(1e-3);
                let step = (target.0 - press.last.0, target.1 - press.last.1);
                p.vel = (step.0 / step_dt, step.1 / step_dt);
                if step.0.abs() > 3.0 {
                    let dir = if step.0 > 0.0 { 1 } else { -1 };
                    if press.dir != 0 && dir != press.dir {
                        press.reversals.push(now);
                    }
                    press.dir = dir;
                }
                press.reversals.retain(|t| now - *t < SHAKE_WINDOW);
                if !press.shaken && press.reversals.len() >= SHAKE_REVERSALS {
                    press.shaken = true;
                    println!("pet {i}: shake detected");
                }
                press.last = target;
                press.last_t = now;
                p.pos = target;
                p.pet.sway = (-p.vel.0 * 0.0006).clamp(-0.35, 0.35) as f32;
            }
        }
    } else {
        match p.pet.mode {
            Mode::Walk => {
                p.pos.0 += WALK_SPEED * f64::from(p.pet.facing) * dt;
                if p.pos.0 < left {
                    p.pos.0 = left;
                    p.pet.facing = 1.0;
                } else if p.pos.0 > right {
                    p.pos.0 = right;
                    p.pet.facing = -1.0;
                }
                p.pos.1 = ground;
            }
            Mode::Fall | Mode::Held => {
                p.pet.mode = Mode::Fall;
                p.vel.1 += GRAVITY * dt;
                p.pos.0 += p.vel.0 * dt;
                p.pos.1 += p.vel.1 * dt;
                if p.pos.0 < left || p.pos.0 > right {
                    p.pos.0 = p.pos.0.clamp(left, right);
                    p.vel.0 = -p.vel.0 * 0.5;
                }
                if p.pos.1 >= ground {
                    p.pos.1 = ground;
                    if p.vel.1 > 250.0 {
                        p.vel = (p.vel.0 * 0.6, -p.vel.1 * 0.35);
                        p.pet.poke();
                    } else {
                        p.vel = (0.0, 0.0);
                        p.pet.mode = Mode::Idle;
                    }
                }
            }
            Mode::Idle => p.pos.1 = ground,
        }
        if matches!(p.pet.mode, Mode::Idle | Mode::Walk) && now >= p.switch_at {
            p.pet.mode = if p.pet.mode == Mode::Idle {
                Mode::Walk
            } else {
                Mode::Idle
            };
            if xorshift(rng).is_multiple_of(2) {
                p.pet.facing = -p.pet.facing;
            }
            p.switch_at = now + Duration::from_millis(3000 + u64::from(xorshift(rng) % 5000));
        }
    }

    p.pet.step(dt as f32);
    let placed = (p.pos.0.round() as i32, p.pos.1.round() as i32);
    if placed != p.placed {
        p.window
            .set_outer_position(PhysicalPosition::new(placed.0, placed.1));
        p.placed = placed;
    }
}

fn draw(p: &mut PetWin, shared: &Shared, stats: &mut Stats) {
    let t0 = Instant::now();
    p.pm.fill(Color::TRANSPARENT);
    p.pet.draw(&mut p.pm);
    let t1 = Instant::now();
    if let Err(e) = p.presenter.present(shared, &p.pm) {
        eprintln!("present failed: {e}");
    }
    stats.frames += 1;
    stats.draw += t1 - t0;
    stats.present += t1.elapsed();
}
