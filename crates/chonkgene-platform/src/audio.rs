//! Sound output (PLAN §1.5), with nothing to install and nothing to link:
//! Windows plays an in-memory WAV with `PlaySound`; Linux loads ALSA
//! (`libasound.so.2`, which PipeWire and PulseAudio also serve) at run time
//! and stays silent if it is missing.
//!
//! One sound at a time: a new one replaces whatever is playing.

pub struct Audio {
    imp: imp::Imp,
}

impl Audio {
    /// `None` if this machine has no usable sound output.
    pub fn new() -> Option<Self> {
        imp::Imp::new().map(|imp| Self { imp })
    }

    /// Play mono 16-bit `samples` at `rate` Hz, scaled by `volume` (0.0–1.0).
    pub fn play(&self, samples: &[i16], rate: u32, volume: f32) {
        let v = volume.clamp(0.0, 1.0);
        let scaled: Vec<i16> = samples.iter().map(|s| (f32::from(*s) * v) as i16).collect();
        self.imp.play(scaled, rate);
    }
}

#[cfg(windows)]
mod imp {
    use std::ptr::null_mut;
    use std::sync::Mutex;

    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

    pub struct Imp {
        /// The WAV being played must outlive the asynchronous playback.
        playing: Mutex<Vec<u8>>,
    }

    fn wav(samples: &[i16], rate: u32) -> Vec<u8> {
        let data = (samples.len() * 2) as u32;
        let mut w = Vec::with_capacity(44 + data as usize);
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&1u16.to_le_bytes()); // mono
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data.to_le_bytes());
        for s in samples {
            w.extend_from_slice(&s.to_le_bytes());
        }
        w
    }

    impl Imp {
        pub fn new() -> Option<Self> {
            Some(Self {
                playing: Mutex::new(Vec::new()),
            })
        }

        pub fn play(&self, samples: Vec<i16>, rate: u32) {
            let next = wav(&samples, rate);
            let Ok(mut playing) = self.playing.lock() else {
                return;
            };
            // SAFETY: `next` is a complete WAV image; PlaySound stops the
            // previous sound before it returns, so the old buffer can then
            // be dropped, and `next` is kept alive in `playing`.
            unsafe {
                PlaySoundW(
                    next.as_ptr().cast(),
                    null_mut(),
                    SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
                );
            }
            *playing = next;
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::thread;
    use std::time::Duration;

    use libloading::{Library, Symbol};

    const STREAM_PLAYBACK: c_int = 0;
    const FORMAT_S16_LE: c_int = 2;
    const ACCESS_RW_INTERLEAVED: c_int = 3;
    /// Close the device after this long without a sound.
    const IDLE: Duration = Duration::from_secs(5);

    type Open = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
    type SetParams =
        unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
    type WriteI = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
    type Recover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
    type Drain = unsafe extern "C" fn(*mut c_void) -> c_int;
    type Close = unsafe extern "C" fn(*mut c_void) -> c_int;

    pub struct Imp {
        tx: Sender<(Vec<i16>, u32)>,
    }

    struct Alsa {
        // Keeps the function pointers below valid.
        _lib: Library,
        open: Open,
        set_params: SetParams,
        writei: WriteI,
        recover: Recover,
        drain: Drain,
        close: Close,
    }

    impl Alsa {
        fn load() -> Option<Self> {
            // SAFETY: loading the system ALSA library runs its initialisers,
            // which have no preconditions; each symbol is looked up with the
            // C signature documented in alsa-lib's pcm.h.
            unsafe {
                let lib = Library::new("libasound.so.2").ok()?;
                let get = |name: &[u8]| -> Option<*const ()> {
                    let s: Symbol<*const ()> = lib.get(name).ok()?;
                    Some(*s)
                };
                let open = get(b"snd_pcm_open\0")?;
                let set_params = get(b"snd_pcm_set_params\0")?;
                let writei = get(b"snd_pcm_writei\0")?;
                let recover = get(b"snd_pcm_recover\0")?;
                let drain = get(b"snd_pcm_drain\0")?;
                let close = get(b"snd_pcm_close\0")?;
                Some(Self {
                    open: std::mem::transmute::<*const (), Open>(open),
                    set_params: std::mem::transmute::<*const (), SetParams>(set_params),
                    writei: std::mem::transmute::<*const (), WriteI>(writei),
                    recover: std::mem::transmute::<*const (), Recover>(recover),
                    drain: std::mem::transmute::<*const (), Drain>(drain),
                    close: std::mem::transmute::<*const (), Close>(close),
                    _lib: lib,
                })
            }
        }

        fn open_pcm(&self, rate: u32) -> Option<*mut c_void> {
            let mut pcm: *mut c_void = std::ptr::null_mut();
            // SAFETY: `pcm` is a valid out-pointer and the name is NUL-terminated.
            let err = unsafe { (self.open)(&mut pcm, c"default".as_ptr(), STREAM_PLAYBACK, 0) };
            if err < 0 || pcm.is_null() {
                return None;
            }
            // SAFETY: `pcm` was just opened; 100 ms latency, resampling allowed.
            let err = unsafe {
                (self.set_params)(
                    pcm,
                    FORMAT_S16_LE,
                    ACCESS_RW_INTERLEAVED,
                    1,
                    rate,
                    1,
                    100_000,
                )
            };
            if err < 0 {
                // SAFETY: closing the handle we opened.
                unsafe { (self.close)(pcm) };
                return None;
            }
            Some(pcm)
        }

        fn write(&self, pcm: *mut c_void, samples: &[i16]) {
            let mut rest = samples;
            while !rest.is_empty() {
                // SAFETY: `rest` holds `rest.len()` mono frames of S16 samples.
                let n = unsafe { (self.writei)(pcm, rest.as_ptr().cast(), rest.len() as c_ulong) };
                if n < 0 {
                    // SAFETY: recovering the handle from an underrun or suspend.
                    if unsafe { (self.recover)(pcm, n as c_int, 1) } < 0 {
                        return;
                    }
                } else {
                    rest = &rest[(n as usize).min(rest.len())..];
                }
            }
        }
    }

    fn worker(alsa: Alsa, rx: Receiver<(Vec<i16>, u32)>) {
        let mut pcm: Option<(*mut c_void, u32)> = None;
        loop {
            let next = match rx.recv_timeout(IDLE) {
                Ok(job) => job,
                Err(RecvTimeoutError::Timeout) => {
                    if let Some((p, _)) = pcm.take() {
                        // SAFETY: closing the handle we opened.
                        unsafe { (alsa.close)(p) };
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            };
            // Only the newest pending sound plays.
            let (samples, rate) = rx.try_iter().last().unwrap_or(next);
            if pcm.is_some_and(|(_, r)| r != rate)
                && let Some((p, _)) = pcm.take()
            {
                // SAFETY: closing the handle we opened.
                unsafe { (alsa.close)(p) };
            }
            if pcm.is_none() {
                pcm = alsa.open_pcm(rate).map(|p| (p, rate));
            }
            if let Some((p, _)) = pcm {
                alsa.write(p, &samples);
                // SAFETY: wait for this sound to finish on our open handle.
                unsafe { (alsa.drain)(p) };
            }
        }
        if let Some((p, _)) = pcm {
            // SAFETY: closing the handle we opened.
            unsafe { (alsa.close)(p) };
        }
    }

    impl Imp {
        pub fn new() -> Option<Self> {
            let alsa = Alsa::load()?;
            let (tx, rx) = mpsc::channel();
            thread::Builder::new()
                .name("chonkgene-audio".into())
                .spawn(move || worker(alsa, rx))
                .ok()?;
            Some(Self { tx })
        }

        pub fn play(&self, samples: Vec<i16>, rate: u32) {
            let _ = self.tx.send((samples, rate));
        }
    }
}
