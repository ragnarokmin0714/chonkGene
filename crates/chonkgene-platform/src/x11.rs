//! X11 (and XWayland): ARGB visual + PutImage, with an XShape input region
//! cut to the pet's silhouette so clicks elsewhere fall through.

use std::borrow::Cow;

use tiny_skia::Pixmap;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::image::{BitsPerPixel, Image, ImageOrder, ScanlinePad};
use x11rb::protocol::Event;
use x11rb::protocol::shape::{self, ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{AtomEnum, ClipOrdering, ConnectionExt as _, CreateGCAux, Rectangle};
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
        Ok(Self { conn, root, shape })
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

    /// Global cursor position, polled (no input hooks, PLAN §6).
    pub fn cursor(&self) -> Option<(i32, i32)> {
        let r = self.conn.query_pointer(self.root).ok()?.reply().ok()?;
        Some((i32::from(r.root_x), i32::from(r.root_y)))
    }
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

    pub fn describe(&self) -> String {
        if self.depth == 32 {
            "depth 32 (per-pixel alpha available)".into()
        } else {
            format!("depth {} (NO alpha: is a compositor running?)", self.depth)
        }
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
