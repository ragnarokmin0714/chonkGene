//! X11 (and XWayland): ARGB visual + PutImage, with an XShape input region
//! cut to the pet's silhouette so clicks elsewhere fall through.

use std::borrow::Cow;

use tiny_skia::Pixmap;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::image::{BitsPerPixel, Image, ImageOrder, ScanlinePad};
use x11rb::protocol::Event;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::screensaver::{self, ConnectionExt as _};
use x11rb::protocol::shape::{self, ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{
    AtomEnum, ClipOrdering, ConnectionExt as _, CreateGCAux, KeyButMask, MapState, Rectangle,
};
use x11rb::rust_connection::RustConnection;

/// Alpha below this lets clicks through.
const HIT_ALPHA: u8 = 32;
/// Re-cut the input region every this many frames.
const SHAPE_EVERY: u32 = 4;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// One X connection shared by every pet window.
pub struct Shared {
    conn: RustConnection,
    root: u32,
    shape: bool,
    screensaver: bool,
    active_window: u32,
    wm_state: u32,
    fullscreen: u32,
    terrain: TerrainAtoms,
}

/// Atoms for reading where windows are (PLAN §1.3).
struct TerrainAtoms {
    stacking: u32,
    hidden: u32,
    window_type: u32,
    /// Window types pets may stand on.
    standable: [u32; 3],
    desktop: u32,
    current_desktop: u32,
    frame_extents: u32,
    gtk_frame_extents: u32,
}

fn intern(conn: &RustConnection, name: &[u8]) -> u32 {
    conn.intern_atom(false, name)
        .ok()
        .and_then(|c| c.reply().ok())
        .map_or(0, |r| r.atom)
}

impl Shared {
    pub fn new() -> Result<Self, String> {
        let (conn, screen) =
            x11rb::connect(None).map_err(|e| format!("cannot connect to X11: {e}"))?;
        let root = conn.setup().roots[screen].root;
        let shape = conn
            .extension_information(shape::X11_EXTENSION_NAME)
            .map_err(err)?
            .is_some();
        let screensaver = conn
            .extension_information(screensaver::X11_EXTENSION_NAME)
            .map_err(err)?
            .is_some();
        let active_window = intern(&conn, b"_NET_ACTIVE_WINDOW");
        let wm_state = intern(&conn, b"_NET_WM_STATE");
        let fullscreen = intern(&conn, b"_NET_WM_STATE_FULLSCREEN");
        let terrain = TerrainAtoms {
            stacking: intern(&conn, b"_NET_CLIENT_LIST_STACKING"),
            hidden: intern(&conn, b"_NET_WM_STATE_HIDDEN"),
            window_type: intern(&conn, b"_NET_WM_WINDOW_TYPE"),
            standable: [
                intern(&conn, b"_NET_WM_WINDOW_TYPE_NORMAL"),
                intern(&conn, b"_NET_WM_WINDOW_TYPE_DIALOG"),
                intern(&conn, b"_NET_WM_WINDOW_TYPE_UTILITY"),
            ],
            desktop: intern(&conn, b"_NET_WM_DESKTOP"),
            current_desktop: intern(&conn, b"_NET_CURRENT_DESKTOP"),
            frame_extents: intern(&conn, b"_NET_FRAME_EXTENTS"),
            gtk_frame_extents: intern(&conn, b"_GTK_FRAME_EXTENTS"),
        };
        Ok(Self {
            conn,
            root,
            shape,
            screensaver,
            active_window,
            wm_state,
            fullscreen,
            terrain,
        })
    }

    pub fn describe(&self) -> String {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".into());
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "?".into());
        format!(
            "X11 server \"{}\", SHAPE {}, session {session}, desktop {desktop}",
            String::from_utf8_lossy(&self.conn.setup().vendor),
            if self.shape {
                "yes"
            } else {
                "MISSING (no click-through)"
            },
        )
    }

    /// `_NET_WORKAREA` of the first desktop: x, y, width, height.
    pub fn work_area(&self) -> Option<(i32, i32, i32, i32)> {
        let atom = self
            .conn
            .intern_atom(true, b"_NET_WORKAREA")
            .ok()?
            .reply()
            .ok()?
            .atom;
        if atom == u32::from(AtomEnum::NONE) {
            return None;
        }
        let reply = self
            .conn
            .get_property(false, self.root, atom, AtomEnum::CARDINAL, 0, 4)
            .ok()?
            .reply()
            .ok()?;
        let v: Vec<u32> = reply.value32()?.collect();
        (v.len() == 4).then(|| (v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32))
    }

    /// Every monitor's work area (RandR monitors cut to `_NET_WORKAREA`),
    /// or the one work area when RandR has nothing to say.
    pub fn work_areas(&self) -> Vec<(i32, i32, i32, i32)> {
        let whole = self.work_area();
        let monitors = self
            .conn
            .randr_get_monitors(self.root, true)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.monitors)
            .unwrap_or_default();
        let mut areas: Vec<(i32, i32, i32, i32)> = monitors
            .iter()
            .map(|m| {
                let r = (
                    i32::from(m.x),
                    i32::from(m.y),
                    i32::from(m.width),
                    i32::from(m.height),
                );
                whole.map_or(Some(r), |w| intersect(r, w))
            })
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default();
        if areas.is_empty() {
            areas.extend(whole);
        }
        areas
    }

    /// Windows on the current desktop that pets may stand on, front to
    /// back, as they are seen (frame included, client-side shadows not).
    /// `ours` are the pets' windows. Requests go out together and the
    /// replies are read after, so a refresh costs about one round trip.
    pub fn windows(&self, ours: &[u32]) -> Vec<crate::WinRect> {
        let a = &self.terrain;
        let prop32 = |win: u32, atom: u32, ty: AtomEnum, len: u32| {
            self.conn.get_property(false, win, atom, ty, 0, len).ok()
        };
        let read = |c: Option<
            x11rb::cookie::Cookie<'_, RustConnection, x11rb::protocol::xproto::GetPropertyReply>,
        >|
         -> Vec<u32> {
            c.and_then(|c| c.reply().ok())
                .and_then(|r| r.value32().map(Iterator::collect))
                .unwrap_or_default()
        };
        let mut stacking = read(prop32(self.root, a.stacking, AtomEnum::WINDOW, 4096));
        let managed = !stacking.is_empty();
        if !managed {
            // No EWMH window manager (a minimal one, or none): the server's
            // own stacking order of top-level windows, bottom first.
            stacking = self
                .conn
                .query_tree(self.root)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| r.children)
                .unwrap_or_default();
        }
        let current = read(prop32(self.root, a.current_desktop, AtomEnum::CARDINAL, 1))
            .first()
            .copied();
        let wins: Vec<u32> = stacking
            .into_iter()
            .rev()
            .filter(|w| !ours.contains(w))
            .collect();
        // Every request first…
        let asked: Vec<_> = wins
            .iter()
            .map(|&w| {
                (
                    w,
                    self.conn.get_window_attributes(w).ok(),
                    prop32(w, self.wm_state, AtomEnum::ATOM, 32),
                    prop32(w, a.window_type, AtomEnum::ATOM, 32),
                    prop32(w, a.desktop, AtomEnum::CARDINAL, 1),
                    self.conn.get_geometry(w).ok(),
                    self.conn.translate_coordinates(w, self.root, 0, 0).ok(),
                    prop32(w, a.frame_extents, AtomEnum::CARDINAL, 4),
                    prop32(w, a.gtk_frame_extents, AtomEnum::CARDINAL, 4),
                )
            })
            .collect();
        // …then every reply.
        let mut out = Vec::new();
        for (w, attrs, state, kind, desktop, geometry, at, frame, gtk) in asked {
            let attrs = attrs.and_then(|c| c.reply().ok());
            // Unmanaged, override-redirect windows are menus and popups.
            let viewable = attrs.as_ref().is_some_and(|r| {
                r.map_state == MapState::VIEWABLE && (managed || !r.override_redirect)
            });
            let hidden = read(state).contains(&a.hidden);
            let kind = read(kind);
            let standable = kind.is_empty() || kind.iter().any(|k| a.standable.contains(k));
            let desktop = read(desktop).first().copied();
            let here = match (desktop, current) {
                (Some(d), Some(c)) => d == c || d == u32::MAX,
                _ => true,
            };
            let geometry = geometry.and_then(|c| c.reply().ok());
            let at = at.and_then(|c| c.reply().ok());
            let (frame, gtk) = (read(frame), read(gtk));
            let (Some(g), Some(at)) = (geometry, at) else {
                continue;
            };
            if !viewable || hidden || !standable || !here {
                continue;
            }
            let (mut x, mut y) = (i32::from(at.dst_x), i32::from(at.dst_y));
            let (mut wd, mut ht) = (i32::from(g.width), i32::from(g.height));
            // Decorations drawn by the window manager count…
            if let [l, r, t, b] = frame[..] {
                let [l, r, t, b] = [l, r, t, b].map(|v| v as i32);
                (x, y, wd, ht) = (x - l, y - t, wd + l + r, ht + t + b);
            }
            // …invisible client-side shadows do not.
            if let [l, r, t, b] = gtk[..] {
                let [l, r, t, b] = [l, r, t, b].map(|v| v as i32);
                (x, y, wd, ht) = (x + l, y + t, wd - l - r, ht - t - b);
            }
            if wd > 0 && ht > 0 {
                out.push(crate::WinRect {
                    id: u64::from(w),
                    x,
                    y,
                    w: wd,
                    h: ht,
                });
            }
        }
        out
    }

    /// Global cursor position, polled (no input hooks, PLAN §6).
    pub fn cursor(&self) -> Option<(i32, i32)> {
        let r = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((i32::from(r.root_x), i32::from(r.root_y)))
    }

    /// Is a mouse button held anywhere on screen? Polled, for closing menus
    /// on an outside click without grabbing the pointer.
    pub fn button_down(&self) -> bool {
        self.conn
            .query_pointer(self.root)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| {
                let buttons =
                    u16::from(KeyButMask::BUTTON1 | KeyButMask::BUTTON2 | KeyButMask::BUTTON3);
                u16::from(r.mask) & buttons != 0
            })
    }

    /// Seconds since the last keyboard or mouse input (XScreenSaver; under
    /// XWayland this only sees X11 input).
    pub fn idle_secs(&self) -> Option<f64> {
        if !self.screensaver {
            return None;
        }
        let r = self
            .conn
            .screensaver_query_info(self.root)
            .ok()?
            .reply()
            .ok()?;
        Some(f64::from(r.ms_since_user_input) / 1000.0)
    }

    /// Is the active window full screen? `ours` are the pet windows, which
    /// never count.
    pub fn fullscreen_active(&self, ours: &[u32]) -> bool {
        if self.active_window == 0 || self.fullscreen == 0 {
            return false;
        }
        let active = self
            .conn
            .get_property(false, self.root, self.active_window, AtomEnum::WINDOW, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()));
        let Some(win) = active.filter(|w| *w != 0 && !ours.contains(w)) else {
            return false;
        };
        self.conn
            .get_property(false, win, self.wm_state, AtomEnum::ATOM, 0, 32)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|mut v| v.any(|a| a == self.fullscreen)))
            .unwrap_or(false)
    }
}

/// The overlap of two x, y, w, h rectangles.
fn intersect(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> Option<(i32, i32, i32, i32)> {
    let (x0, y0) = (a.0.max(b.0), a.1.max(b.1));
    let (x1, y1) = ((a.0 + a.2).min(b.0 + b.2), (a.1 + a.3).min(b.1 + b.3));
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

pub struct Presenter {
    win: u32,
    gc: u32,
    depth: u8,
    buf: Vec<u8>,
    frame: u32,
}

impl Presenter {
    pub fn new(shared: &Shared, window: &Window) -> Result<Self, String> {
        let win = match window.window_handle().map_err(err)?.as_raw() {
            RawWindowHandle::Xlib(h) => h.window as u32,
            RawWindowHandle::Xcb(h) => h.window.get(),
            other => return Err(format!("not an X11 window: {other:?}")),
        };
        let conn = &shared.conn;
        let depth = conn
            .get_geometry(win)
            .map_err(err)?
            .reply()
            .map_err(err)?
            .depth;
        let gc = conn.generate_id().map_err(err)?;
        conn.create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0))
            .map_err(err)?;
        let size = window.inner_size();
        Ok(Self {
            win,
            gc,
            depth,
            buf: vec![0; (size.width * size.height * 4) as usize],
            frame: 0,
        })
    }

    /// The X11 window id, for telling our windows apart from others.
    pub fn id(&self) -> u32 {
        self.win
    }

    pub fn describe(&self) -> String {
        if self.depth == 32 {
            "depth 32 (per-pixel alpha available)".into()
        } else {
            format!("depth {} (NO alpha: is a compositor running?)", self.depth)
        }
    }

    /// Nothing to do on X11: the window keeps its visual and shape.
    pub fn restyle(&self) -> bool {
        false
    }

    pub fn present(&mut self, shared: &Shared, pm: &Pixmap) -> Result<(), String> {
        let conn = &shared.conn;
        super::swizzle(pm.data(), &mut self.buf);
        let image = Image::new(
            pm.width() as u16,
            pm.height() as u16,
            ScanlinePad::Pad32,
            self.depth,
            BitsPerPixel::B32,
            ImageOrder::LsbFirst,
            Cow::Borrowed(&self.buf),
        )
        .map_err(err)?;
        image.put(conn, self.win, self.gc, 0, 0).map_err(err)?;
        if shared.shape && self.frame.is_multiple_of(SHAPE_EVERY) {
            conn.shape_rectangles(
                SO::SET,
                SK::INPUT,
                ClipOrdering::UNSORTED,
                self.win,
                0,
                0,
                &silhouette(pm),
            )
            .map_err(err)?;
        }
        self.frame = self.frame.wrapping_add(1);
        conn.flush().map_err(err)?;
        while let Ok(Some(event)) = conn.poll_for_event() {
            if let Event::Error(e) = event {
                eprintln!("X11 error: {e:?}");
            }
        }
        Ok(())
    }
}

/// One rectangle per horizontal run of opaque-enough pixels.
fn silhouette(pm: &Pixmap) -> Vec<Rectangle> {
    let w = pm.width() as usize;
    let mut rects = Vec::new();
    for (y, row) in pm.data().chunks_exact(w * 4).enumerate() {
        let mut start = None;
        for x in 0..=w {
            let solid = x < w && row[x * 4 + 3] >= HIT_ALPHA;
            match (solid, start) {
                (true, None) => start = Some(x),
                (false, Some(s)) => {
                    rects.push(Rectangle {
                        x: s as i16,
                        y: y as i16,
                        width: (x - s) as u16,
                        height: 1,
                    });
                    start = None;
                }
                _ => {}
            }
        }
    }
    rects
}
