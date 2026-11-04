//! Sound output (PLAN §1.5), with nothing to install and nothing to link:
//! Windows streams to `waveOut` (winmm, there on every Windows); Linux
//! loads ALSA (`libasound.so.2`, which PipeWire and PulseAudio also serve)
//! at run time and stays silent if it is missing.
//!
//! Several sounds play at once (PLAN §8 "Roger's seventh pass", item 2): a
//! thread mixes up to [`VOICES`] of them into 20 ms chunks through a soft
//! limiter and hands each chunk to the device, which paces it. With nothing
//! to play it writes nothing, and the device closes after a while.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::Duration;

/// The rate everything is mixed at.
pub const RATE: u32 = 44_100;
/// Sounds playing at once; a new one past this fades out the oldest.
pub const VOICES: usize = 4;
/// Samples a chunk: 20 ms.
pub const CHUNK: usize = (RATE / 50) as usize;
/// Close the device after this long without a sound.
const IDLE: Duration = Duration::from_secs(5);
/// A voice pushed out fades over this many samples (10 ms), not to click.
const FADE: f32 = 441.0;

pub struct Audio {
    tx: Sender<Job>,
}

/// A sound on its way to the mixer: its samples, volume applied, and how
/// far to step through them each output sample (pitch, and its rate
/// against [`RATE`]).
struct Job {
    samples: Vec<f32>,
    step: f32,
}

impl Audio {
    /// `None` if this machine has no usable sound output.
    pub fn new() -> Option<Self> {
        let (tx, rx) = channel();
        let device = imp::Device::load()?;
        thread::Builder::new()
            .name("chonkgene-audio".into())
            .spawn(move || pump(device, rx))
            .ok()?;
        Some(Self { tx })
    }

    /// Play mono 16-bit `samples` at `rate` Hz, scaled by `volume` (0.0–1.0),
    /// `pitch` times higher (and shorter), on top of whatever is playing.
    pub fn play(&self, samples: &[i16], rate: u32, volume: f32, pitch: f32) {
        let v = volume.clamp(0.0, 1.0) / f32::from(i16::MAX);
        let samples = samples.iter().map(|s| f32::from(*s) * v).collect();
        let step = pitch.clamp(0.5, 2.0) * rate as f32 / RATE as f32;
        let _ = self.tx.send(Job { samples, step });
    }
}

/// What the mixing thread needs of a sound device.
trait Out {
    fn is_open(&self) -> bool;
    fn open(&mut self) -> bool;
    /// Queue one chunk, waiting while the device's queue is full. False if
    /// the device has failed.
    fn write(&mut self, chunk: &[i16]) -> bool;
    fn close(&mut self);
}

/// The mixing thread: waits for a sound, then mixes and writes chunks
/// until every voice has played out.
fn pump(mut out: impl Out, rx: Receiver<Job>) {
    let mut mixer = Mixer::default();
    let mut chunk = vec![0i16; CHUNK];
    loop {
        if mixer.idle() {
            match rx.recv_timeout(IDLE) {
                Ok(job) => mixer.add(job.samples, job.step),
                Err(RecvTimeoutError::Timeout) => {
                    out.close();
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        for job in rx.try_iter() {
            mixer.add(job.samples, job.step);
        }
        if !out.is_open() && !out.open() {
            mixer.clear();
            continue;
        }
        mixer.fill(&mut chunk);
        if !out.write(&chunk) {
            out.close();
            mixer.clear();
        }
    }
    out.close();
}

struct Voice {
    samples: Vec<f32>,
    pos: f32,
    step: f32,
    /// Fading out (pushed out by a newer sound): the gain left.
    fade: Option<f32>,
}

/// Sounds playing at once, summed through a soft limiter.
#[derive(Default)]
pub struct Mixer {
    voices: Vec<Voice>,
}

impl Mixer {
    /// Start `samples` (−1 … 1), stepping `step` through them a sample.
    /// Past [`VOICES`], the oldest still playing fades out.
    pub fn add(&mut self, samples: Vec<f32>, step: f32) {
        if samples.len() < 2 {
            return;
        }
        let playing = self.voices.iter().filter(|v| v.fade.is_none()).count();
        if playing >= VOICES
            && let Some(oldest) = self.voices.iter_mut().find(|v| v.fade.is_none())
        {
            oldest.fade = Some(1.0);
        }
        self.voices.push(Voice {
            samples,
            pos: 0.0,
            step: step.max(0.01),
            fade: None,
        });
    }

    pub fn idle(&self) -> bool {
        self.voices.is_empty()
    }

    pub fn clear(&mut self) {
        self.voices.clear();
    }

    /// Mix the next `out.len()` samples (silence once every voice is done).
    pub fn fill(&mut self, out: &mut [i16]) {
        for o in out.iter_mut() {
            let mut sum = 0.0;
            for v in &mut self.voices {
                let i = v.pos as usize;
                let (Some(a), Some(b)) = (v.samples.get(i), v.samples.get(i + 1)) else {
                    continue;
                };
                let f = v.pos - i as f32;
                let mut x = a + (b - a) * f;
                if let Some(g) = &mut v.fade {
                    x *= g.max(0.0);
                    *g -= 1.0 / FADE;
                }
                sum += x;
                v.pos += v.step;
            }
            *o = (limit(sum) * f32::from(i16::MAX)) as i16;
        }
        self.voices
            .retain(|v| (v.pos as usize) + 1 < v.samples.len() && v.fade.is_none_or(|g| g > 0.0));
    }
}

/// The highest a mixed sample goes: −1 dBFS.
const PEAK: f32 = 0.891;

/// A soft knee, as the sounds' own (`chonkgene_core::sound::limit`):
/// untouched below 0.6 of full scale, bending toward [`PEAK`] above.
fn limit(x: f32) -> f32 {
    const KNEE: f32 = 0.6;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let room = PEAK - KNEE;
        x.signum() * (KNEE + room * ((a - KNEE) / room).tanh())
    }
}

#[cfg(windows)]
mod imp {
    use std::ptr::{addr_of, null_mut};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Media::Audio::{
        CALLBACK_NULL, HWAVEOUT, WAVE_FORMAT_PCM, WAVE_MAPPER, WAVEFORMATEX, WAVEHDR, WHDR_INQUEUE,
        waveOutClose, waveOutOpen, waveOutPrepareHeader, waveOutReset, waveOutUnprepareHeader,
        waveOutWrite,
    };
    use windows_sys::Win32::Media::MMSYSERR_NOERROR;

    use super::{CHUNK, RATE};

    /// Chunks queued at once: 80 ms.
    const BUFFERS: usize = 4;
    /// A queued chunk not done after this long: the device is gone.
    const STUCK: Duration = Duration::from_secs(2);

    /// A `waveOut` stream with [`BUFFERS`] chunks it reuses in turn.
    pub struct Device {
        wave: HWAVEOUT,
        /// The chunks and their headers, on the heap and never resized, so
        /// the driver's pointers to them stay put.
        bufs: Vec<Box<[i16]>>,
        hdrs: Box<[WAVEHDR]>,
        next: usize,
    }

    impl Device {
        pub fn load() -> Option<Self> {
            Some(Self {
                wave: null_mut(),
                bufs: (0..BUFFERS)
                    .map(|_| vec![0i16; CHUNK].into_boxed_slice())
                    .collect(),
                hdrs: (0..BUFFERS).map(|_| WAVEHDR::default()).collect(),
                next: 0,
            })
        }

        /// The header's flags, which the driver sets as it plays.
        fn flags(hdr: *const WAVEHDR) -> u32 {
            // SAFETY: `hdr` points to one of our headers, alive and in place
            // for as long as the device; the struct is packed, so read
            // unaligned.
            unsafe { addr_of!((*hdr).dwFlags).read_unaligned() }
        }
    }

    // SAFETY: the device moves once, into the mixing thread, before it is
    // opened; the handle and headers are only ever used from that thread.
    unsafe impl Send for Device {}

    impl super::Out for Device {
        fn is_open(&self) -> bool {
            !self.wave.is_null()
        }

        fn open(&mut self) -> bool {
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: 1,
                nSamplesPerSec: RATE,
                nAvgBytesPerSec: RATE * 2,
                nBlockAlign: 2,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            let mut wave = null_mut();
            // SAFETY: `wave` is a valid out-pointer and `format` a complete
            // PCM format; no callback (we poll the headers).
            let err = unsafe { waveOutOpen(&mut wave, WAVE_MAPPER, &format, 0, 0, CALLBACK_NULL) };
            if err != MMSYSERR_NOERROR || wave.is_null() {
                return false;
            }
            self.wave = wave;
            for (buf, hdr) in self.bufs.iter_mut().zip(self.hdrs.iter_mut()) {
                *hdr = WAVEHDR {
                    lpData: buf.as_mut_ptr().cast(),
                    dwBufferLength: (buf.len() * 2) as u32,
                    ..WAVEHDR::default()
                };
                // SAFETY: the header points at its own boxed chunk, both
                // kept until the header is unprepared in `close`.
                unsafe { waveOutPrepareHeader(wave, hdr, size_of::<WAVEHDR>() as u32) };
            }
            self.next = 0;
            true
        }

        fn write(&mut self, chunk: &[i16]) -> bool {
            let k = self.next;
            let hdr: *mut WAVEHDR = &mut self.hdrs[k];
            // The oldest chunk is the next one free: wait for it to play.
            let since = Instant::now();
            while Self::flags(hdr) & WHDR_INQUEUE != 0 {
                if since.elapsed() > STUCK {
                    return false;
                }
                sleep(Duration::from_millis(2));
            }
            let n = chunk.len().min(self.bufs[k].len());
            self.bufs[k][..n].copy_from_slice(&chunk[..n]);
            // SAFETY: a prepared header of ours that is no longer queued;
            // its chunk stays put until it plays out or `close` resets it.
            let err = unsafe { waveOutWrite(self.wave, hdr, size_of::<WAVEHDR>() as u32) };
            self.next = (k + 1) % BUFFERS;
            err == MMSYSERR_NOERROR
        }

        fn close(&mut self) {
            if self.wave.is_null() {
                return;
            }
            // SAFETY: reset returns every queued header to us at once; then
            // each is unprepared and the handle we opened is closed.
            unsafe {
                waveOutReset(self.wave);
                for hdr in self.hdrs.iter_mut() {
                    waveOutUnprepareHeader(self.wave, hdr, size_of::<WAVEHDR>() as u32);
                }
                waveOutClose(self.wave);
            }
            self.wave = null_mut();
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};

    use libloading::{Library, Symbol};

    use super::RATE;

    const STREAM_PLAYBACK: c_int = 0;
    const FORMAT_S16_LE: c_int = 2;
    const ACCESS_RW_INTERLEAVED: c_int = 3;

    type Open = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
    type SetParams =
        unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
    type WriteI = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
    type Recover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
    type Close = unsafe extern "C" fn(*mut c_void) -> c_int;

    /// ALSA's default PCM, opened while there is something to play.
    pub struct Device {
        // Keeps the function pointers below valid.
        _lib: Library,
        open: Open,
        set_params: SetParams,
        writei: WriteI,
        recover: Recover,
        close: Close,
        pcm: *mut c_void,
    }

    // SAFETY: the device moves once, into the mixing thread, before it is
    // opened; the library and the handle are only ever used from there.
    unsafe impl Send for Device {}

    impl Device {
        pub fn load() -> Option<Self> {
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
                let close = get(b"snd_pcm_close\0")?;
                Some(Self {
                    open: std::mem::transmute::<*const (), Open>(open),
                    set_params: std::mem::transmute::<*const (), SetParams>(set_params),
                    writei: std::mem::transmute::<*const (), WriteI>(writei),
                    recover: std::mem::transmute::<*const (), Recover>(recover),
                    close: std::mem::transmute::<*const (), Close>(close),
                    _lib: lib,
                    pcm: std::ptr::null_mut(),
                })
            }
        }
    }

    impl super::Out for Device {
        fn is_open(&self) -> bool {
            !self.pcm.is_null()
        }

        fn open(&mut self) -> bool {
            let mut pcm: *mut c_void = std::ptr::null_mut();
            // SAFETY: `pcm` is a valid out-pointer and the name is NUL-terminated.
            let err = unsafe { (self.open)(&mut pcm, c"default".as_ptr(), STREAM_PLAYBACK, 0) };
            if err < 0 || pcm.is_null() {
                return false;
            }
            // SAFETY: `pcm` was just opened; 60 ms latency, resampling allowed.
            let err = unsafe {
                (self.set_params)(
                    pcm,
                    FORMAT_S16_LE,
                    ACCESS_RW_INTERLEAVED,
                    1,
                    RATE,
                    1,
                    60_000,
                )
            };
            if err < 0 {
                // SAFETY: closing the handle we opened.
                unsafe { (self.close)(pcm) };
                return false;
            }
            self.pcm = pcm;
            true
        }

        fn write(&mut self, chunk: &[i16]) -> bool {
            let mut rest = chunk;
            while !rest.is_empty() {
                // SAFETY: `rest` holds `rest.len()` mono frames of S16 samples
                // and `pcm` is our open handle.
                let n =
                    unsafe { (self.writei)(self.pcm, rest.as_ptr().cast(), rest.len() as c_ulong) };
                if n < 0 {
                    // An underrun after a quiet spell, or a suspend: recover.
                    // SAFETY: recovering our open handle, quietly.
                    if unsafe { (self.recover)(self.pcm, n as c_int, 1) } < 0 {
                        return false;
                    }
                } else {
                    rest = &rest[(n as usize).min(rest.len())..];
                }
            }
            true
        }

        fn close(&mut self) {
            if !self.pcm.is_null() {
                // SAFETY: closing the handle we opened.
                unsafe { (self.close)(self.pcm) };
                self.pcm = std::ptr::null_mut();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * (i as f32 * 0.05).sin()).collect()
    }

    fn run(m: &mut Mixer, n: usize) -> Vec<i16> {
        let mut out = vec![0i16; n];
        m.fill(&mut out);
        out
    }

    #[test]
    fn sounds_play_together_and_the_sum_never_clips() {
        let mut m = Mixer::default();
        for _ in 0..VOICES {
            m.add(tone(4000, 0.9), 1.0);
        }
        let out = run(&mut m, 2000);
        let peak = out.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(f32::from(peak) <= PEAK * 32767.0 + 1.0, "{peak}");
        // Four in step are louder than one.
        let mut one = Mixer::default();
        one.add(tone(4000, 0.9), 1.0);
        let single = run(&mut one, 2000);
        let rms = |v: &[i16]| {
            (v.iter().map(|&s| f32::from(s).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
        };
        assert!(rms(&out) > 1.2 * rms(&single));
    }

    #[test]
    fn a_quiet_mix_passes_untouched() {
        let mut m = Mixer::default();
        m.add(vec![0.25; 100], 1.0);
        m.add(vec![0.25; 100], 1.0);
        let out = run(&mut m, 10);
        assert!(out.iter().all(|&s| s == (0.5 * 32767.0) as i16), "{out:?}");
    }

    #[test]
    fn a_voice_plays_out_and_the_mixer_goes_idle() {
        let mut m = Mixer::default();
        m.add(vec![0.5; 1000], 1.0);
        run(&mut m, 500);
        assert!(!m.idle());
        let out = run(&mut m, 600);
        assert!(m.idle());
        // Silence after the end.
        assert!(out[550..].iter().all(|&s| s == 0));
    }

    #[test]
    fn pitch_steps_faster_through_a_sound() {
        let mut m = Mixer::default();
        m.add(vec![0.5; 1000], 2.0);
        run(&mut m, 520);
        assert!(m.idle(), "an octave up plays in half the time");
    }

    #[test]
    fn a_fifth_sound_fades_out_the_oldest_without_a_click() {
        let mut m = Mixer::default();
        m.add(vec![0.2; 10_000], 1.0);
        for _ in 1..VOICES {
            m.add(vec![0.0; 10_000], 1.0);
        }
        m.add(vec![0.0; 10_000], 1.0);
        let out = run(&mut m, 1000);
        // The first fades from 0.2 to nothing over 10 ms, no step.
        let steps = out.windows(2).map(|w| (w[0] - w[1]).abs()).max().unwrap();
        assert!(steps < 100, "a step of {steps}");
        assert_eq!(out[999], 0);
        assert_eq!(m.voices.len(), VOICES);
    }
}
