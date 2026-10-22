//! Windows: a layered window fed by `UpdateLayeredWindow`. Pixels with
//! alpha 0 let clicks through natively, so no hit-testing code is needed.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use tiny_skia::Pixmap;
use windows_sys::Win32::Foundation::{GetLastError, HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP,
    HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO, VK_LBUTTON, VK_RBUTTON,
};
use windows_sys::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, SPI_GETWORKAREA, SetWindowLongPtrW,
    SystemParametersInfoW, ULW_ALPHA, UpdateLayeredWindow, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Windows needs no shared connection; this keeps the API the same as X11.
pub struct Shared;

impl Shared {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn describe(&self) -> String {
        "Windows layered windows".into()
    }

    /// Work area of the primary monitor: x, y, width, height.
    pub fn work_area(&self) -> Option<(i32, i32, i32, i32)> {
        let mut r = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: SPI_GETWORKAREA writes one RECT to the pointer we pass.
        let ok =
            unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&raw mut r).cast::<c_void>(), 0) };
        (ok != 0).then(|| (r.left, r.top, r.right - r.left, r.bottom - r.top))
    }

    /// Is a mouse button held anywhere on screen? Polled, for closing menus
    /// on an outside click (no hooks, PLAN §6).
    pub fn button_down(&self) -> bool {
        // SAFETY: GetAsyncKeyState only reads key state; any virtual key is valid.
        let down = |vk: u16| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
        down(VK_LBUTTON) || down(VK_RBUTTON)
    }

    /// Seconds since the last keyboard or mouse input.
    pub fn idle_secs(&self) -> Option<f64> {
        let mut info = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: `info` is a properly sized LASTINPUTINFO.
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return None;
        }
        // SAFETY: no arguments; returns milliseconds since boot (wraps, hence wrapping_sub).
        let now = unsafe { GetTickCount() };
        Some(f64::from(now.wrapping_sub(info.dwTime)) / 1000.0)
    }

    /// Is a full-screen app, game or presentation in front?
    pub fn fullscreen_active(&self, _ours: &[u32]) -> bool {
        let mut state = 0;
        // SAFETY: writes one QUERY_USER_NOTIFICATION_STATE to `state`.
        if unsafe { SHQueryUserNotificationState(&mut state) } != 0 {
            return false;
        }
        matches!(
            state,
            QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE
        )
    }

    /// Global cursor position, polled (no input hooks, PLAN §6).
    pub fn cursor(&self) -> Option<(i32, i32)> {
        let mut p = POINT { x: 0, y: 0 };
        // SAFETY: GetCursorPos writes one POINT to the pointer we pass.
        let ok = unsafe { GetCursorPos(&mut p) };
        (ok != 0).then_some((p.x, p.y))
    }
}

pub struct Presenter {
    hwnd: HWND,
    screen_dc: HDC,
    mem_dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
}

impl Presenter {
    pub fn new(_shared: &Shared, window: &Window) -> Result<Self, String> {
        let hwnd = match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get() as HWND,
            other => return Err(format!("not a Win32 window: {other:?}")),
        };
        let size = window.inner_size();
        let (width, height) = (size.width as i32, size.height as i32);
        // SAFETY: plain Win32 calls on a window and DCs we own; every handle is
        // checked before use and released in Drop.
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let add = (WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | add);

            let screen_dc = GetDC(null_mut());
            let mem_dc = CreateCompatibleDC(screen_dc);
            if screen_dc.is_null() || mem_dc.is_null() {
                return Err("cannot create device contexts".into());
            }
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down rows, same order as tiny-skia
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..zeroed()
            };
            let mut bits: *mut c_void = null_mut();
            let bitmap = CreateDIBSection(mem_dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                DeleteDC(mem_dc);
                ReleaseDC(null_mut(), screen_dc);
                return Err("cannot create the DIB section".into());
            }
            let old = SelectObject(mem_dc, bitmap);
            Ok(Self {
                hwnd,
                screen_dc,
                mem_dc,
                bitmap,
                old,
                bits: bits.cast(),
                width,
                height,
            })
        }
    }

    pub fn describe(&self) -> String {
        "layered window, premultiplied BGRA".into()
    }

    /// Only meaningful on X11; Windows tells full-screen apps apart itself.
    pub fn id(&self) -> u32 {
        0
    }

    pub fn present(&mut self, _shared: &Shared, pm: &Pixmap) -> Result<(), String> {
        let len = (self.width * self.height * 4) as usize;
        if pm.data().len() != len {
            return Err("pixmap size does not match the window".into());
        }
        // SAFETY: `bits` points at the DIB section's width × height × 4 bytes,
        // which live until Drop.
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, len) };
        super::swizzle(pm.data(), dst);
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: all handles are valid for our lifetime; a null destination
        // point keeps the window where winit put it.
        let ok = unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                self.screen_dc,
                null(),
                &size,
                self.mem_dc,
                &src,
                0,
                &blend,
                ULW_ALPHA,
            )
        };
        if ok == 0 {
            // SAFETY: reads the calling thread's last-error value.
            return Err(format!("UpdateLayeredWindow failed (error {})", unsafe {
                GetLastError()
            }));
        }
        Ok(())
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        // SAFETY: releases exactly what `new` created, in reverse order.
        unsafe {
            SelectObject(self.mem_dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.mem_dc);
            ReleaseDC(null_mut(), self.screen_dc);
        }
    }
}
