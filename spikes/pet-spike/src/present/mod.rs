//! Pushing a pet's pixels straight to its window, bypassing the GPU (PLAN §3.2).

#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub use x11::{Presenter, Shared};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{Presenter, Shared};

#[cfg(not(any(target_os = "linux", windows)))]
compile_error!("the spike supports Linux and Windows only");

/// tiny-skia stores premultiplied RGBA; both platforms take premultiplied BGRA.
pub fn swizzle(src: &[u8], dst: &mut [u8]) {
    let (src, _) = src.as_chunks::<4>();
    let (dst, _) = dst.as_chunks_mut::<4>();
    for (s, d) in src.iter().zip(dst) {
        *d = [s[2], s[1], s[0], s[3]];
    }
}
