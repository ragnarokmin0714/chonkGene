//! OS integration for chonkGene (docs/PLAN.md §3.2, §3.5): transparent,
//! always-on-top pet windows that let clicks through their transparent
//! pixels, plus the work area and cursor position.
//!
//! Windows: layered windows fed by `UpdateLayeredWindow`. Linux: X11 (also
//! under XWayland) with an ARGB visual and an XShape input region.

#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub use x11::{Presenter, Shared};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{Presenter, Shared};

#[cfg(not(any(target_os = "linux", windows)))]
compile_error!("chonkGene supports Linux and Windows");

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::EventLoopBuilder;
use winit::window::{Window, WindowAttributes, WindowLevel};

/// v1 speaks X11 on Linux, XWayland included (PLAN §0).
pub fn configure_event_loop<T>(builder: &mut EventLoopBuilder<T>) {
    #[cfg(target_os = "linux")]
    winit::platform::x11::EventLoopBuilderExtX11::with_x11(builder);
    #[cfg(not(target_os = "linux"))]
    let _ = builder;
}

/// Attributes for a frameless, always-on-top pet window of `size` pixels.
pub fn pet_window(size: (u32, u32), pos: (i32, i32), title: &str) -> WindowAttributes {
    let attrs = Window::default_attributes()
        .with_title(title)
        .with_decorations(false)
        .with_resizable(false)
        .with_inner_size(PhysicalSize::new(size.0, size.1))
        .with_position(PhysicalPosition::new(pos.0, pos.1))
        .with_window_level(WindowLevel::AlwaysOnTop);
    #[cfg(target_os = "linux")]
    let attrs = {
        use winit::platform::x11::WindowAttributesExtX11;
        // An ARGB visual is only chosen for transparent windows; override-
        // redirect keeps the window manager from framing or moving it.
        attrs.with_transparent(true).with_override_redirect(true)
    };
    #[cfg(windows)]
    let attrs = {
        use winit::platform::windows::WindowAttributesExtWindows;
        // Transparency comes from WS_EX_LAYERED in the presenter.
        attrs.with_skip_taskbar(true)
    };
    attrs
}

/// tiny-skia stores premultiplied RGBA; both platforms take premultiplied BGRA.
fn swizzle(src: &[u8], dst: &mut [u8]) {
    let (src, _) = src.as_chunks::<4>();
    let (dst, _) = dst.as_chunks_mut::<4>();
    for (s, d) in src.iter().zip(dst) {
        *d = [s[2], s[1], s[0], s[3]];
    }
}
