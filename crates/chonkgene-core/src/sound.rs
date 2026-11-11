//! The pets' sounds, synthesised (PLAN §1.5, §8 "Roger's seventh pass",
//! item 2): each call is a voice (a band-limited pulse with breath noise)
//! shaped by formant filters, with its own pitch and vowel contours, so a
//! Shiba barks 汪, a cat glides through 喵 and a duckling buzzes 呱. Every
//! sound has a few takes, and each is normalised to its class's loudness,
//! measured roughly as ears hear it (presence weighted), not by its peak.

use crate::species::Species;

pub const RATE: u32 = 44_100;
/// Takes of each sound: repeats are never the same.
pub const TAKES: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sound {
    /// The species' voice: 汪, 喵, 咕, 啾, 呱.
    Voice,
    Poke,
    Laugh,
    Sigh,
    Grumpy,
    Snore,
    Land,
    Eat,
    /// A cat's purr while patted (others hum contentedly).
    Purr,
    /// The Shiba panting, tongue out.
    Pant,
    /// The fox's bell.
    Bell,
    /// Wings flapping.
    Flutter,
    /// Glad to see you: hello, welcome back, made up.
    Greet,
    /// The 肉肉舞's little tune: a two-second phrase, each take the next
    /// one (A, B, then the 「登登！」 ending), played one after another.
    Tune,
}

impl Sound {
    pub const ALL: [Sound; 14] = [
        Sound::Voice,
        Sound::Poke,
        Sound::Laugh,
        Sound::Sigh,
        Sound::Grumpy,
        Sound::Snore,
        Sound::Land,
        Sound::Eat,
        Sound::Purr,
        Sound::Pant,
        Sound::Bell,
        Sound::Flutter,
        Sound::Greet,
        Sound::Tune,
    ];

    pub fn class(self) -> Class {
        match self {
            Sound::Voice | Sound::Laugh | Sound::Grumpy | Sound::Greet => Class::Call,
            Sound::Poke | Sound::Land | Sound::Bell | Sound::Flutter | Sound::Tune => Class::Knock,
            Sound::Sigh | Sound::Snore | Sound::Eat | Sound::Purr | Sound::Pant => Class::Soft,
        }
    }
}

/// How loud a sound is meant to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    /// Calls: the voice, laughing, grumbling, greeting.
    Call,
    /// Pokes, landings, the bell, wings.
    Knock,
    /// Breathing, snoring, eating, purring: quiet, but clearly heard.
    Soft,
}

impl Class {
    /// Target loudness, dB below full scale (see [`loudness`]).
    pub fn target(self) -> f32 {
        match self {
            Class::Call => -8.0,
            Class::Knock => -12.0,
            Class::Soft => -17.0,
        }
    }
}

/// The highest a sample goes after the limiter: −1 dBFS.
pub const PEAK: f32 = 0.891;

const TAU: f32 = std::f32::consts::TAU;

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Linear through `(t, value)` points, held flat past either end.
fn curve(points: &[(f32, f32)], t: f32) -> f32 {
    match points {
        [] => 0.0,
        [(_, v)] => *v,
        _ => {
            if t <= points[0].0 {
                return points[0].1;
            }
            for w in points.windows(2) {
                let ((t0, v0), (t1, v1)) = (w[0], w[1]);
                if t <= t1 {
                    return v0 + (v1 - v0) * ((t - t0) / (t1 - t0).max(1e-6));
                }
            }
            points[points.len() - 1].1
        }
    }
}

/// A resonant band-pass (RBJ, constant 0 dB peak), its centre and width
/// changeable every sample.
#[derive(Clone, Copy, Default)]
struct Band {
    x: [f32; 2],
    y: [f32; 2],
}

impl Band {
    fn run(&mut self, input: f32, hz: f32, q: f32) -> f32 {
        let w = TAU * hz.clamp(40.0, 0.45 * RATE as f32) / RATE as f32;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b2) = (alpha / a0, -alpha / a0);
        let (a1, a2) = (-2.0 * w.cos() / a0, (1.0 - alpha) / a0);
        let out = b0 * input + b2 * self.x[1] - a1 * self.y[0] - a2 * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [out, self.y[0]];
        out
    }
}

/// One-pole low-pass, for smoothing noise and envelopes.
#[derive(Clone, Copy, Default)]
struct Low(f32);

impl Low {
    fn run(&mut self, x: f32, hz: f32) -> f32 {
        let k = 1.0 - (-TAU * hz / RATE as f32).exp();
        self.0 += (x - self.0) * k;
        self.0
    }
}

/// A small xorshift for noise and takes.
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9) | 1)
    }
    fn f(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
    /// −1 … 1.
    fn s(&mut self) -> f32 {
        2.0 * self.f() - 1.0
    }
}

/// A syllable of voice: what its pitch, vowel and loudness do over its
/// length (`t` from 0 to 1 along each curve).
struct Syllable<'a> {
    at: f32,
    secs: f32,
    /// Hz along the syllable.
    pitch: &'a [(f32, f32)],
    /// Formant centres (Hz) along the syllable, up to three.
    formants: [&'a [(f32, f32)]; 3],
    /// Each formant's gain and sharpness.
    gains: [f32; 3],
    q: [f32; 3],
    /// Breath noise against the voice, 0 … 1, along the syllable.
    breath: &'a [(f32, f32)],
    /// Brightness: how slowly the pulse's harmonics fall off (0 dull … 1
    /// buzzing).
    buzz: f32,
    /// Amplitude: attack and release, seconds.
    attack: f32,
    release: f32,
    /// A rough, growly flutter of the voice (Hz, depth).
    rough: (f32, f32),
    gain: f32,
}

impl Default for Syllable<'_> {
    fn default() -> Self {
        Self {
            at: 0.0,
            secs: 0.2,
            pitch: &[(0.0, 400.0)],
            formants: [&[(0.0, 800.0)], &[(0.0, 1600.0)], &[(0.0, 3000.0)]],
            gains: [1.0, 0.6, 0.25],
            q: [5.0, 7.0, 8.0],
            breath: &[(0.0, 0.1)],
            buzz: 0.6,
            attack: 0.02,
            release: 0.08,
            rough: (0.0, 0.0),
            gain: 1.0,
        }
    }
}

struct Buf {
    s: Vec<f32>,
    rng: Rng,
    /// A pitch factor for the whole sound (a take's own), and vibrato depth.
    pitch: f32,
}

impl Buf {
    fn new(secs: f32, take: u32, pitch: f32) -> Self {
        Self {
            s: vec![0.0; (secs * RATE as f32) as usize],
            rng: Rng::new(take.wrapping_add(17)),
            pitch,
        }
    }

    fn voice(&mut self, y: &Syllable) {
        let start = (y.at * RATE as f32) as usize;
        let n = (y.secs * RATE as f32) as usize;
        let mut bands = [Band::default(); 3];
        let mut breath_low = Low::default();
        let mut phase = 0.0f32;
        let mut wobble = Low::default();
        // Harmonics fall off as 1/k^tilt: a buzz keeps its highs.
        let tilt = 2.0 - 1.3 * y.buzz.clamp(0.0, 1.0);
        for i in 0..n {
            let Some(_) = self.s.get(start + i) else {
                break;
            };
            let secs = i as f32 / RATE as f32;
            let t = i as f32 / n as f32;
            // A slow random wobble and a little vibrato keep it alive.
            let jitter = wobble.run(self.rng.s(), 12.0) * 0.06;
            let vib = 1.0 + 0.012 * (TAU * 5.5 * secs).sin();
            let f0 = curve(y.pitch, t) * self.pitch * vib * (1.0 + jitter);
            phase = (phase + f0 / RATE as f32).fract();
            // The pulse: harmonics up to 7 kHz.
            let top = ((7000.0 / f0.max(40.0)) as usize).clamp(1, 40);
            let mut pulse = 0.0;
            for k in 1..=top {
                pulse += (TAU * k as f32 * phase).sin() / (k as f32).powf(tilt);
            }
            if y.rough.1 > 0.0 {
                pulse *= 1.0 - y.rough.1 * (0.5 + 0.5 * (TAU * y.rough.0 * secs).sin());
            }
            let noise = breath_low.run(self.rng.s(), 6000.0) * 2.0;
            let b = curve(y.breath, t).clamp(0.0, 1.0);
            let src = pulse * (1.0 - b) + noise * b * 1.5;
            let mut out = 0.0;
            for (k, band) in bands.iter_mut().enumerate() {
                out += band.run(src, curve(y.formants[k], t), y.q[k]) * y.gains[k];
            }
            // A little of the raw pulse for body under the formants.
            out += 0.08 * pulse * (1.0 - b);
            let attack = ease(secs / y.attack.max(1e-3));
            let release = ease((y.secs - secs) / y.release.max(1e-3));
            self.s[start + i] += out * attack * release * y.gain;
        }
    }

    /// Filtered noise through a band (Hz, sharpness), shaped by `env`
    /// (0 … 1 along it).
    fn noise(&mut self, at: f32, secs: f32, hz: f32, q: f32, env: &dyn Fn(f32) -> f32, gain: f32) {
        let start = (at * RATE as f32) as usize;
        let n = (secs * RATE as f32) as usize;
        let mut band = Band::default();
        for i in 0..n {
            let Some(out) = self.s.get_mut(start + i) else {
                break;
            };
            let t = i as f32 / n as f32;
            *out += band.run(self.rng.s(), hz, q) * env(t) * gain;
        }
    }

    /// A sine gliding from `f0` to `f1`, with a quick attack and a decay.
    fn tone(&mut self, at: f32, secs: f32, f0: f32, f1: f32, gain: f32) {
        let start = (at * RATE as f32) as usize;
        let n = (secs * RATE as f32) as usize;
        let mut phase = 0.0f32;
        for i in 0..n {
            let Some(out) = self.s.get_mut(start + i) else {
                break;
            };
            let t = i as f32 / n as f32;
            phase = (phase + f0 * (f1 / f0).powf(t) / RATE as f32).fract();
            let env = ease(t / 0.04) * (1.0 - t).powi(2);
            *out += (TAU * phase).sin() * env * gain;
        }
    }

    /// A struck bell: inharmonic partials ringing down.
    fn bell(&mut self, at: f32, hz: f32, gain: f32) {
        let start = (at * RATE as f32) as usize;
        let n = (0.45 * RATE as f32) as usize;
        let partials = [
            (1.0, 1.0, 0.18),
            (2.32, 0.55, 0.1),
            (4.25, 0.3, 0.06),
            (6.63, 0.15, 0.04),
        ];
        for i in 0..n {
            let Some(out) = self.s.get_mut(start + i) else {
                break;
            };
            let secs = i as f32 / RATE as f32;
            let mut v = 0.0;
            for (ratio, amp, decay) in partials {
                v += (TAU * hz * ratio * secs).sin() * amp * (-secs / decay).exp();
            }
            *out += v * gain * ease(secs / 0.002);
        }
    }

    /// Mix in a recorded clip at `at` seconds, `up` times higher (and
    /// shorter) besides the take's own pitch, only the first `keep` of it
    /// (0 … 1, the cut eased out), at `gain`. Returns how long it lasts.
    fn clip(&mut self, at: f32, clip: Clip, up: f32, keep: f32, gain: f32) -> f32 {
        let src = clip.samples();
        let step = up * self.pitch;
        let used = (src.len() as f32 * keep.clamp(0.05, 1.0)) as usize;
        let n = (used as f32 / step) as usize;
        let start = (at * RATE as f32) as usize;
        // A cut short of the end fades over its last 15 ms.
        let fade = if keep < 1.0 { 0.015 * RATE as f32 } else { 0.0 };
        for i in 0..n {
            let Some(out) = self.s.get_mut(start + i) else {
                break;
            };
            let x = i as f32 * step;
            let k = x as usize;
            let f = x - k as f32;
            let a = src.get(k).copied().unwrap_or(0.0);
            let b = src.get(k + 1).copied().unwrap_or(0.0);
            let tail = if fade > 0.0 {
                ease((n - i) as f32 / fade)
            } else {
                1.0
            };
            *out += (a + (b - a) * f) * gain * tail;
        }
        n as f32 / RATE as f32
    }

    /// Cut 30 ms after `end` seconds, then [`Buf::finish`].
    fn end(mut self, end: f32, class: Class) -> Vec<i16> {
        let n = ((end + 0.03) * RATE as f32) as usize;
        self.s.truncate(n.min(self.s.len()));
        self.finish(class)
    }

    /// Normalise to `class`'s loudness, then soft-limit the peaks to
    /// [`PEAK`].
    fn finish(self, class: Class) -> Vec<i16> {
        // The limiter takes a little off loud peaks: measure again after it
        // and make up the difference, twice.
        let mut gain = 1.0;
        for _ in 0..3 {
            let out: Vec<f32> = self.s.iter().map(|x| limit(x * gain)).collect();
            let now = loudness(&out);
            if !now.is_finite() {
                break;
            }
            gain *= 10f32.powf((class.target() - now) / 20.0);
        }
        self.s
            .iter()
            .map(|x| (limit(x * gain) * f32::from(i16::MAX)) as i16)
            .collect()
    }
}

/// A soft knee: untouched below 0.6 of full scale, then bending smoothly
/// toward [`PEAK`], never past it.
pub fn limit(x: f32) -> f32 {
    const KNEE: f32 = 0.6;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let room = PEAK - KNEE;
        x.signum() * (KNEE + room * ((a - KNEE) / room).tanh())
    }
}

/// How loud `samples` sound, dB below full scale: the RMS of the louder
/// 50 ms blocks (quiet gaps do not count) after a presence weighting (bass
/// below 100 Hz counts less, 1.5–5 kHz more), roughly as ears hear it.
pub fn loudness(samples: &[f32]) -> f32 {
    let mut low = Low::default();
    let mut band = Band::default();
    let weighted: Vec<f32> = samples
        .iter()
        .map(|&x| {
            let bass = low.run(x, 100.0);
            let presence = band.run(x, 3000.0, 0.7);
            (x - 0.7 * bass) + 0.6 * presence
        })
        .collect();
    let block = (0.05 * RATE as f32) as usize;
    let mut power: Vec<f32> = weighted
        .chunks(block)
        .map(|c| c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32)
        .collect();
    let top = power.iter().copied().fold(0.0f32, f32::max);
    // Blocks within 20 dB of the loudest.
    power.retain(|p| *p > top * 0.01);
    let mean = power.iter().sum::<f32>() / power.len().max(1) as f32;
    10.0 * mean.max(1e-12).log10()
}

/// A recorded clip (fifteenth pass, item 5): real animals, all CC0, picked
/// by Roger by ear and cut by `tools/soundbake` from its list
/// `sounds.json`; where each comes from is in `licenses/sounds.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Clip {
    ShibaBark,
    ShibaWhimper,
    ShibaGrowl,
    FoxYip,
    FoldMeow,
    MunchkinMeow,
    CatPurr,
    CatTrill,
    CatGrumble,
    OwlHoot,
    ChickPeep,
    DucklingPeeps,
    Snore,
}

impl Clip {
    fn wav(self) -> &'static [u8] {
        match self {
            Clip::ShibaBark => include_bytes!("../../../assets/sounds/shiba-bark.wav"),
            Clip::ShibaWhimper => include_bytes!("../../../assets/sounds/shiba-whimper.wav"),
            Clip::ShibaGrowl => include_bytes!("../../../assets/sounds/shiba-growl.wav"),
            Clip::FoxYip => include_bytes!("../../../assets/sounds/fox-yip.wav"),
            Clip::FoldMeow => include_bytes!("../../../assets/sounds/fold-meow.wav"),
            Clip::MunchkinMeow => include_bytes!("../../../assets/sounds/munchkin-meow.wav"),
            Clip::CatPurr => include_bytes!("../../../assets/sounds/cat-purr.wav"),
            Clip::CatTrill => include_bytes!("../../../assets/sounds/cat-trill.wav"),
            Clip::CatGrumble => include_bytes!("../../../assets/sounds/cat-grumble.wav"),
            Clip::OwlHoot => include_bytes!("../../../assets/sounds/owl-hoot.wav"),
            Clip::ChickPeep => include_bytes!("../../../assets/sounds/chick-peep.wav"),
            Clip::DucklingPeeps => include_bytes!("../../../assets/sounds/duckling-peeps.wav"),
            Clip::Snore => include_bytes!("../../../assets/sounds/snore.wav"),
        }
    }

    /// The clip's samples, −1 … 1 (mono 16-bit WAV at [`RATE`]).
    fn samples(self) -> Vec<f32> {
        let wav = self.wav();
        let at = wav
            .windows(4)
            .position(|w| w == b"data")
            .map_or(wav.len(), |p| p + 8);
        wav[at..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) / 32767.0)
            .collect()
    }
}

/// `sound` in `species`' voice from the recorded clips, take `take`, or
/// `None` where the sound is still synthesised (pokes, landings, eating,
/// panting, the bell, wings; the birds' and cats' sighs). Returns how long
/// it lasts.
fn recorded(b: &mut Buf, species: Species, sound: Sound, take: u32) -> Option<f32> {
    use Species::*;
    // The species' own call, and how many of it make one "voice".
    let call = match species {
        Shiba => Clip::ShibaBark,
        Fox => Clip::FoxYip,
        Fold => Clip::FoldMeow,
        Munchkin => Clip::MunchkinMeow,
        Owl => Clip::OwlHoot,
        Chick => Clip::ChickPeep,
        Duck => Clip::DucklingPeeps,
    };
    let cat = matches!(species, Fold | Munchkin);
    // The Munchkin's trill a little higher than the Fold's.
    let small = if species == Munchkin { 1.08 } else { 1.0 };
    // Calls in a row, with uneven gaps (an even rate sounds like a
    // machine), each `up` and `keep` its own.
    let row = |b: &mut Buf, clip: Clip, calls: &[(f32, f32, f32)], gaps: &[f32]| {
        let mut at = 0.0;
        for (k, &(up, keep, gain)) in calls.iter().enumerate() {
            at += b.clip(at, clip, up, keep, gain);
            if k + 1 < calls.len() {
                at += gaps[k % gaps.len()];
            }
        }
        at
    };
    Some(match sound {
        Sound::Voice => match species {
            // One bark or two, a few yips or peeps, as the synthesis did.
            Shiba if take == 1 => row(b, call, &[(1.0, 1.0, 1.0), (0.97, 0.8, 0.85)], &[0.07]),
            Fox => {
                let n = 2 + take as usize % 2;
                row(
                    b,
                    call,
                    &[(1.0, 1.0, 1.0), (1.04, 1.0, 0.9), (0.97, 1.0, 0.85)][..n],
                    &[0.05, 0.07],
                )
            }
            Chick => {
                let n = 2 + take as usize % 2;
                row(
                    b,
                    call,
                    &[(1.0, 1.0, 1.0), (1.06, 1.0, 0.9), (0.98, 1.0, 0.85)][..n],
                    &[0.04, 0.06],
                )
            }
            _ => b.clip(0.0, call, 1.0, 1.0, 1.0),
        },
        Sound::Greet => match species {
            // The Shiba coaxes; cats trill; the rest call twice, going up.
            Shiba => b.clip(0.0, Clip::ShibaWhimper, 1.06, 1.0, 1.0),
            _ if cat => b.clip(0.0, Clip::CatTrill, small, 1.0, 1.0),
            Owl => b.clip(0.0, call, 1.12, 1.0, 1.0),
            Duck => b.clip(0.0, call, 1.1, 1.0, 1.0),
            _ => row(b, call, &[(1.0, 0.8, 0.85), (1.15, 1.0, 1.0)], &[0.06]),
        },
        Sound::Laugh => {
            // Quick, each a little lower and softer; long calls cut short.
            let (clip, keep, base) = match species {
                Shiba => (call, 0.6, 1.08),
                _ if cat => (Clip::CatTrill, 0.7, small),
                Owl => (call, 0.3, 1.1),
                Duck => (call, 0.55, 1.12),
                _ => (call, 1.0, 1.1),
            };
            let n = if species == Duck {
                3
            } else {
                4 + take as usize % 2
            };
            let calls: Vec<_> = (0..n)
                .map(|k| (base - 0.05 * k as f32, keep, 1.0 - 0.1 * k as f32))
                .collect();
            row(b, clip, &calls, &[0.05, 0.09, 0.04, 0.11])
        }
        Sound::Grumpy => match species {
            Shiba => b.clip(0.0, Clip::ShibaGrowl, 1.0, 1.0, 1.0),
            Fox => b.clip(0.0, Clip::ShibaGrowl, 1.3, 1.0, 1.0),
            _ if cat => b.clip(0.0, Clip::CatGrumble, small, 1.0, 1.0),
            // Birds: their own call, lower, short and sharp, twice or thrice.
            Owl => row(b, call, &[(0.85, 0.3, 1.0), (0.8, 0.3, 0.9)], &[0.08]),
            Duck => row(b, call, &[(0.9, 0.4, 1.0), (0.86, 0.4, 0.9)], &[0.06]),
            _ => row(
                b,
                call,
                &[(0.88, 1.0, 1.0), (0.86, 1.0, 0.95), (0.84, 1.0, 0.9)],
                &[0.03],
            ),
        },
        Sound::Sigh => match species {
            Shiba => b.clip(0.0, Clip::ShibaWhimper, 0.92, 1.0, 1.0),
            Fox => b.clip(0.0, Clip::ShibaWhimper, 1.15, 1.0, 1.0),
            _ => return None,
        },
        Sound::Snore => b.clip(0.0, Clip::Snore, 1.0, 1.0, 1.0),
        Sound::Purr if cat => b.clip(0.0, Clip::CatPurr, 1.0, 1.0, 1.0),
        _ => return None,
    })
}

/// The voice each species calls with (see [`call`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Bark,
    Yip,
    Meow,
    Hoot,
    Chirp,
    Quack,
}

fn kind_of(s: Species) -> (Kind, f32) {
    match s {
        Species::Shiba => (Kind::Bark, 1.0),
        Species::Fox => (Kind::Yip, 1.0),
        Species::Munchkin => (Kind::Meow, 1.08),
        Species::Fold => (Kind::Meow, 1.0),
        Species::Owl => (Kind::Hoot, 1.0),
        Species::Chick => (Kind::Chirp, 1.0),
        Species::Duck => (Kind::Quack, 1.0),
    }
}

/// One call of `kind` at `at` seconds, `len` times its usual length, its
/// pitch times `up` (a laugh's quick ones go up, a grumble's down), `gain`.
fn call(b: &mut Buf, kind: Kind, at: f32, len: f32, up: f32, gain: f32) -> f32 {
    match kind {
        // 汪: a noisy burst at the onset, the pitch jumping up and
        // falling, the vowel opening from "w" to "a" and closing.
        Kind::Bark => {
            let secs = 0.2 * len;
            let p = [(0.0, 470.0 * up), (0.18, 600.0 * up), (1.0, 380.0 * up)];
            b.voice(&Syllable {
                at,
                secs,
                pitch: &p,
                formants: [
                    &[(0.0, 500.0), (0.3, 1000.0), (1.0, 700.0)],
                    &[(0.0, 1000.0), (0.3, 1500.0), (1.0, 1100.0)],
                    &[(0.0, 2600.0)],
                ],
                gains: [1.0, 0.7, 0.35],
                q: [4.0, 6.0, 7.0],
                breath: &[(0.0, 0.7), (0.15, 0.2), (1.0, 0.35)],
                buzz: 0.75,
                attack: 0.006,
                release: 0.07 * len,
                rough: (70.0, 0.25),
                gain,
            });
            secs
        }
        // A fox's yip: higher, shorter, sharper.
        Kind::Yip => {
            let secs = 0.13 * len;
            let p = [(0.0, 760.0 * up), (0.25, 980.0 * up), (1.0, 700.0 * up)];
            b.voice(&Syllable {
                at,
                secs,
                pitch: &p,
                formants: [
                    &[(0.0, 800.0), (0.4, 1200.0), (1.0, 900.0)],
                    &[(0.0, 1900.0), (1.0, 2300.0)],
                    &[(0.0, 3400.0)],
                ],
                gains: [1.0, 0.8, 0.4],
                q: [4.0, 6.0, 7.0],
                breath: &[(0.0, 0.5), (0.2, 0.15)],
                buzz: 0.7,
                attack: 0.005,
                release: 0.05 * len,
                gain,
                ..Syllable::default()
            });
            secs
        }
        // 喵: in through a nasal "m", up to "i", open to "a", closing on
        // "u"; the pitch rises then falls.
        Kind::Meow => {
            let secs = 0.55 * len;
            let p = [(0.0, 520.0 * up), (0.35, 760.0 * up), (1.0, 440.0 * up)];
            b.voice(&Syllable {
                at,
                secs,
                pitch: &p,
                formants: [
                    &[(0.0, 320.0), (0.15, 450.0), (0.45, 1000.0), (1.0, 500.0)],
                    &[(0.0, 1300.0), (0.15, 2500.0), (0.45, 1600.0), (1.0, 950.0)],
                    &[(0.0, 2800.0), (0.45, 3200.0), (1.0, 2700.0)],
                ],
                gains: [1.0, 0.75, 0.3],
                q: [5.0, 7.0, 8.0],
                breath: &[(0.0, 0.1), (0.8, 0.15), (1.0, 0.35)],
                buzz: 0.55,
                attack: 0.06,
                release: 0.15 * len,
                gain,
                ..Syllable::default()
            });
            secs
        }
        // 咕: low, round and breathy.
        Kind::Hoot => {
            let secs = 0.3 * len;
            let p = [(0.0, 360.0 * up), (0.3, 400.0 * up), (1.0, 330.0 * up)];
            b.voice(&Syllable {
                at,
                secs,
                pitch: &p,
                formants: [&[(0.0, 400.0)], &[(0.0, 750.0)], &[(0.0, 2400.0)]],
                gains: [1.0, 0.5, 0.08],
                q: [3.0, 5.0, 6.0],
                breath: &[(0.0, 0.45), (0.3, 0.3), (1.0, 0.45)],
                buzz: 0.15,
                attack: 0.05,
                release: 0.12 * len,
                gain,
                ..Syllable::default()
            });
            secs
        }
        // 啾: a quick, clear sweep up and down.
        Kind::Chirp => {
            let secs = 0.09 * len;
            let p = [(0.0, 2600.0 * up), (0.45, 4200.0 * up), (1.0, 3100.0 * up)];
            b.voice(&Syllable {
                at,
                secs,
                pitch: &p,
                formants: [
                    &[(0.0, 2800.0), (0.45, 4200.0), (1.0, 3200.0)],
                    &[(0.0, 6000.0)],
                    &[(0.0, 8000.0)],
                ],
                gains: [1.0, 0.15, 0.05],
                q: [2.0, 3.0, 3.0],
                breath: &[(0.0, 0.05)],
                buzz: 0.0,
                attack: 0.008,
                release: 0.03 * len,
                gain,
                ..Syllable::default()
            });
            secs
        }
        // 呱: a nasal buzz, flat and a little rough, the pitch sagging.
        Kind::Quack => {
            let secs = 0.26 * len;
            let p = [(0.0, 440.0 * up), (0.3, 470.0 * up), (1.0, 360.0 * up)];
            b.voice(&Syllable {
                at,
                secs,
                pitch: &p,
                formants: [
                    &[(0.0, 900.0), (0.3, 1150.0), (1.0, 950.0)],
                    &[(0.0, 2300.0), (1.0, 2600.0)],
                    &[(0.0, 3600.0)],
                ],
                gains: [1.0, 0.9, 0.45],
                q: [9.0, 9.0, 8.0],
                breath: &[(0.0, 0.15)],
                buzz: 1.0,
                attack: 0.01,
                release: 0.06 * len,
                rough: (55.0, 0.35),
                gain,
            });
            secs
        }
    }
}

/// How many times a species calls in a row for its voice, and the gap.
fn repeats(kind: Kind, take: u32) -> (u32, f32) {
    match kind {
        Kind::Bark => (1 + take % 2, 0.09),
        Kind::Yip => (2 + take % 2, 0.06),
        Kind::Meow => (1, 0.0),
        Kind::Hoot => (2, 0.12),
        Kind::Chirp => (2 + take % 2, 0.05),
        Kind::Quack => (1 + (take + 1) % 2, 0.08),
    }
}

/// The longest a sound lasts, seconds: the snore, a whole breath.
pub const LONGEST: f32 = 2.6;

/// The 肉肉舞's tune, phrase `take` of three (the app plays takes 1, 2, 0
/// in turn), four beats at 120 a minute each: kagura (Roger: like a
/// miko's 神樂舞, a little more of the old about it). A bamboo flute
/// (神樂笛) breathy and sliding up into its notes in the old 呂 scale
/// (D E G A B, no half steps), the small drum (締太鼓) on the beat, the
/// big drum (大太鼓) on the first, the miko's bells (神樂鈴) shaken on the
/// off beats, and a quiet 笙 chord held under it all. The last phrase ends
/// 「登登！」: two strokes of the big drum with the bells and a high cry
/// of the flute, on the dance's eleventh and twelfth beats (the second as
/// it strikes its pose). Returns its length.
fn tune(b: &mut Buf, take: u32) -> f32 {
    const BEAT: f32 = 0.5;
    let hz = |m: u8| 440.0 * 2f32.powf((f32::from(m) - 69.0) / 12.0);
    // (MIDI note, beats), one after another.
    let flute: &[(u8, f32)] = match take {
        1 => &[
            (81, 1.0),
            (83, 0.5),
            (81, 0.5),
            (79, 1.0),
            (76, 0.5),
            (79, 0.5),
        ],
        2 => &[
            (86, 1.0),
            (83, 0.5),
            (81, 0.5),
            (79, 0.5),
            (81, 0.5),
            (76, 1.0),
        ],
        _ => &[(74, 0.5), (76, 0.5), (81, 1.5)],
    };
    // The 笙's chord under it (D, E, A, B: open, old).
    let end = if take == 0 { 2.55 } else { 2.0 };
    for m in [74, 76, 81, 83] {
        sho(&mut b.s, 0.0, end, hz(m), 0.026);
    }
    let mut at = 0.0;
    for &(m, beats) in flute {
        let len = beats * BEAT;
        kagura_bue(b, at, len, hz(m), 0.3);
        at += len;
    }
    // The small drum's pattern, beats in and how hard, not every beat the
    // same (Gemini, listening: square, march-like); the bells shaken in a
    // roll on the second and fourth. Played by hand: each stroke a little
    // early or late, harder or softer (a strict grid read as an arcade
    // game).
    let last = if take == 0 { 2.0 } else { 4.0 };
    let shime_at = [
        (0.0, 0.45),
        (1.0, 0.35),
        (1.75, 0.22),
        (2.5, 0.38),
        (3.0, 0.3),
        (3.5, 0.2),
    ];
    taiko(&mut b.s, 0.0, 0.9);
    for (beat, g) in shime_at.into_iter().filter(|(beat, _)| *beat < last) {
        let (dt, k) = (0.012 * b.rng.s(), 1.0 + 0.15 * b.rng.s());
        shime(b, beat * BEAT + dt.max(0.0), g * k);
    }
    for beat in [1.0, 3.0].into_iter().filter(|beat| *beat < last) {
        let (dt, k) = (0.01 * b.rng.s(), 1.0 + 0.15 * b.rng.s());
        suzu(b, beat * BEAT + dt.max(0.0), 0.24 * k);
    }
    if take == 0 {
        // 「登登！」
        for (t, g) in [(1.5, 0.75), (2.0, 1.0)] {
            taiko(&mut b.s, t, g);
            shime(b, t, 0.5 * g);
            suzu(b, t, 0.5 * g);
        }
        // The flute's cry over the last stroke (an octave's leap up, but
        // not so high it is shrill).
        kagura_bue(b, 2.0, 0.5, hz(83), 0.24);
    }
    // In a wooden hall: a short warm reverb over it all.
    hall(&mut b.s, 0.22);
    // Faded out where it is cut (see `Buf::end`): into the next phrase,
    // which starts as this one's last 30 ms play, or the ending's last
    // ring; a cut with the reverb still going clicked.
    let fade = if take == 0 { 0.13 } else { 0.03 };
    let (from, to) = (end + 0.03 - fade, end + 0.03);
    for (i, v) in b.s.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        if t > from {
            *v *= 1.0 - ease((t - from) / (to - from));
        }
    }
    end
}

/// A small room's reverb (Schroeder: four combs, two all-passes), mixed in
/// at `wet`.
fn hall(s: &mut [f32], wet: f32) {
    let ms = |m: f32| (m * RATE as f32 / 1000.0) as usize;
    let combs = [
        (ms(29.7), 0.77),
        (ms(37.1), 0.75),
        (ms(41.1), 0.73),
        (ms(43.7), 0.71),
    ];
    let mut out = vec![0.0f32; s.len()];
    for (delay, feedback) in combs {
        let mut buf = vec![0.0f32; delay];
        let mut low = Low::default();
        for (i, o) in out.iter_mut().enumerate() {
            let k = i % delay;
            let y = buf[k];
            // Damped: wood takes the highs out of each echo.
            buf[k] = s[i] + feedback * low.run(y, 3500.0);
            *o += 0.25 * y;
        }
    }
    for delay in [ms(5.0), ms(1.7)] {
        let mut buf = vec![0.0f32; delay];
        for (i, o) in out.iter_mut().enumerate() {
            let k = i % delay;
            let y = buf[k];
            let x = *o;
            buf[k] = x + 0.5 * y;
            *o = y - 0.5 * buf[k];
        }
    }
    for (d, w) in s.iter_mut().zip(out) {
        *d += wet * w;
    }
}

/// The kagura flute: a soft, slightly hollow tone sliding up into its note
/// from a little below, breath noise in it, a slow vibrato coming in on
/// longer notes.
fn kagura_bue(b: &mut Buf, at: f32, len: f32, hz: f32, gain: f32) {
    let start = (at * RATE as f32) as usize;
    let n = ((len + 0.06) * RATE as f32) as usize;
    let mut phase = 0.0f32;
    let mut band = Band::default();
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let noise = b.rng.s();
        let Some(out) = b.s.get_mut(start + i) else {
            break;
        };
        // Up into the note from two semitones below, quick at first and
        // settling (a finger lifted, not a synthesiser's glide); the end
        // falling away a little as the breath goes.
        let rise = 1.0 - (1.0 - (t / 0.08).min(1.0)).powi(3);
        let fall = ease((t - len + 0.08) / 0.08);
        let semis = -2.0 * (1.0 - rise) - 0.6 * fall;
        let vib = 1.0 + 0.01 * ease((t - 0.2) / 0.3) * (TAU * 5.2 * t).sin();
        let f = hz * 2f32.powf(semis / 12.0) * vib;
        phase += TAU * f / RATE as f32;
        let tone = phase.sin()
            + 0.3 * (2.0 * phase).sin()
            + 0.12 * (3.0 * phase).sin()
            + 0.06 * (4.0 * phase).sin();
        // Air across the mouth hole: noise round the note and above it.
        let breath = band.run(noise, 1.4 * f, 1.0) * 1.6;
        let env = ease(t / 0.03) * (1.0 - ease((t - len) / 0.06));
        // A puff of breath at the start of each note.
        let puff = 1.0 + 1.5 * (-t / 0.05).exp();
        *out += (0.85 * tone + breath * puff) * env * gain;
    }
}

/// The 笙's chord: reedy, coming in slowly and holding.
fn sho(s: &mut [f32], at: f32, len: f32, hz: f32, gain: f32) {
    let start = (at * RATE as f32) as usize;
    let n = (len * RATE as f32) as usize;
    for i in 0..n {
        let Some(out) = s.get_mut(start + i) else {
            break;
        };
        let t = i as f32 / RATE as f32;
        // Two reeds a hair apart, beating slowly; few overtones (it read
        // as a buzzy organ).
        let w = TAU * hz * t;
        let w2 = TAU * hz * 1.003 * t;
        let tone = 0.5 * (w.sin() + w2.sin()) + 0.12 * (3.0 * w).sin() + 0.04 * (5.0 * w).sin();
        *out += tone * gain * ease(t / 0.4) * (1.0 - ease((t - len + 0.15) / 0.15));
    }
}

/// The big drum: a deep 「咚」 falling into its pitch.
fn taiko(s: &mut [f32], at: f32, gain: f32) {
    let start = (at * RATE as f32) as usize;
    let n = (0.6 * RATE as f32) as usize;
    let mut phase = 0.0f32;
    for i in 0..n {
        let Some(out) = s.get_mut(start + i) else {
            break;
        };
        let t = i as f32 / RATE as f32;
        let f = 78.0 * (1.0 + 0.8 * (-t / 0.025).exp());
        phase += TAU * f / RATE as f32;
        *out += phase.sin() * gain * ease(t / 0.002) * (-t / 0.22).exp();
    }
}

/// The small, tight drum: a high 「天」 with a slap of skin on it.
fn shime(b: &mut Buf, at: f32, gain: f32) {
    let start = (at * RATE as f32) as usize;
    let n = (0.18 * RATE as f32) as usize;
    let mut phase = 0.0f32;
    let mut band = Band::default();
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let noise = b.rng.s();
        let Some(out) = b.s.get_mut(start + i) else {
            break;
        };
        let f = 420.0 * (1.0 + 0.3 * (-t / 0.01).exp());
        phase += TAU * f / RATE as f32;
        let slap = band.run(noise, 1700.0, 1.0) * (-t / 0.014).exp();
        // A skin struck, not a click: three milliseconds in.
        *out += (phase.sin() * (-t / 0.06).exp() + 0.8 * slap) * gain * ease(t / 0.003);
    }
}

/// The miko's bells (神樂鈴, tiers of small bells on one handle) shaken
/// once: a quick roll of strikes dying away over a seventh of a second,
/// 「沙啦啦」, not a sleigh bell's even jingle; low and brassy.
fn suzu(b: &mut Buf, at: f32, gain: f32) {
    for k in 0..7 {
        let kf = k as f32;
        let t = at + 0.02 * kf + 0.008 * b.rng.f();
        // Its highest partial (6.6 times) stays under half the sample rate.
        let hz = 1500.0 + 600.0 * b.rng.f();
        let hand = 0.8 + 0.4 * b.rng.f();
        b.bell(t, hz, gain * (1.0 - 0.12 * kf) * hand);
    }
}

/// Mono 16-bit samples at [`RATE`] for `sound` in `species`' voice, take
/// `take` (any number; they repeat every [`TAKES`]): a recorded clip where
/// there is one (see [`Clip`]), else synthesised.
pub fn synth(species: Species, sound: Sound, take: u32) -> Vec<i16> {
    let take = take % TAKES;
    // The same for every species: music, not a voice.
    if sound == Sound::Tune {
        let mut b = Buf::new(LONGEST, take, 1.0);
        let end = tune(&mut b, take);
        return b.end(end, sound.class());
    }
    // Each take a little higher or lower.
    let wobble = [1.0, 1.05, 0.95][take as usize];
    let mut b = Buf::new(LONGEST, take, wobble);
    if let Some(end) = recorded(&mut b, species, sound, take) {
        return b.end(end, sound.class());
    }
    let (kind, own) = kind_of(species);
    let pitch = own * wobble;
    let mut b = Buf::new(2.0, take, pitch);
    let end = match sound {
        Sound::Voice => {
            let (n, gap) = repeats(kind, take);
            let mut at = 0.0;
            for k in 0..n {
                let last = k + 1 == n;
                at += call(&mut b, kind, at, 1.0, if last { 0.96 } else { 1.0 }, 1.0) + gap;
            }
            at
        }
        Sound::Greet => {
            // Glad: a short call, then a longer one going up.
            let first = call(&mut b, kind, 0.0, 0.7, 1.05, 0.85);
            let at = first + 0.08;
            at + call(&mut b, kind, at, 1.2, 1.22, 1.0)
        }
        Sound::Laugh => {
            // Quick bursts, each a little lower, in the voice.
            let mut at = 0.0;
            for k in 0..5 {
                let len = if kind == Kind::Meow { 0.32 } else { 0.6 };
                let up = 1.25 - 0.06 * k as f32;
                at += call(&mut b, kind, at, len, up, 1.0 - 0.1 * k as f32) + 0.05;
            }
            at
        }
        Sound::Grumpy => {
            // Short and low, with a growl in it.
            let secs = 0.32;
            let (f0, f1) = match kind {
                Kind::Chirp => (1800.0, 1400.0),
                Kind::Yip | Kind::Meow => (300.0, 240.0),
                _ => (200.0, 160.0),
            };
            let p = [(0.0, f0), (0.4, f0 * 1.1), (1.0, f1)];
            b.voice(&Syllable {
                secs,
                pitch: &p,
                formants: [&[(0.0, 650.0)], &[(0.0, 1300.0)], &[(0.0, 2600.0)]],
                gains: [1.0, 0.6, 0.3],
                breath: &[(0.0, 0.35)],
                buzz: 0.85,
                attack: 0.02,
                release: 0.1,
                rough: (32.0, 0.6),
                ..Syllable::default()
            });
            secs
        }
        Sound::Sigh => {
            // A long breath out, a soft "hmm" fading in it.
            b.noise(
                0.0,
                0.8,
                900.0,
                0.6,
                &|t| ease(t / 0.15) * (1.0 - t).powf(1.4),
                1.0,
            );
            let p = [(0.0, 260.0), (1.0, 190.0)];
            b.voice(&Syllable {
                at: 0.05,
                secs: 0.55,
                pitch: &p,
                formants: [&[(0.0, 450.0)], &[(0.0, 900.0)], &[(0.0, 2400.0)]],
                gains: [1.0, 0.4, 0.1],
                breath: &[(0.0, 0.6)],
                buzz: 0.3,
                attack: 0.1,
                release: 0.3,
                gain: 0.4,
                ..Syllable::default()
            });
            0.8
        }
        Sound::Snore => {
            // A slow, rumbling breath in.
            let p = [(0.0, 75.0), (0.5, 88.0), (1.0, 70.0)];
            b.voice(&Syllable {
                secs: 1.0,
                pitch: &p,
                formants: [&[(0.0, 300.0)], &[(0.0, 700.0)], &[(0.0, 1800.0)]],
                gains: [1.0, 0.5, 0.2],
                breath: &[(0.0, 0.5)],
                buzz: 0.7,
                attack: 0.3,
                release: 0.35,
                rough: (22.0, 0.5),
                ..Syllable::default()
            });
            1.0
        }
        Sound::Poke => {
            // Boop: a soft, quick drop with a tiny click.
            b.tone(0.0, 0.13, 760.0 * pitch, 330.0 * pitch, 1.0);
            b.tone(0.0, 0.1, 1520.0 * pitch, 660.0 * pitch, 0.25);
            b.noise(0.0, 0.012, 3000.0, 1.0, &|t| 1.0 - t, 0.3);
            0.14
        }
        Sound::Land => {
            // A soft thump and a patter of paws.
            b.tone(0.0, 0.16, 150.0, 60.0, 1.0);
            for k in 0..3 {
                let at = 0.02 + 0.045 * k as f32;
                b.noise(at, 0.03, 1800.0, 1.5, &|t| (1.0 - t).powi(2), 0.35);
            }
            0.18
        }
        Sound::Eat => {
            // Nom, nom: crunchy bites.
            for k in 0..3 {
                let at = 0.11 * k as f32;
                b.noise(at, 0.05, 2600.0, 1.2, &|t| (1.0 - t).powf(1.5), 1.0);
                b.tone(at, 0.06, 220.0, 150.0, 0.4);
            }
            0.34
        }
        Sound::Purr => {
            // About 26 throbs a second, low and buzzing, in and out.
            let p = [(0.0, 26.0)];
            b.voice(&Syllable {
                secs: 1.6,
                pitch: &p,
                formants: [&[(0.0, 180.0)], &[(0.0, 450.0)], &[(0.0, 1200.0)]],
                gains: [1.0, 0.7, 0.3],
                q: [2.0, 3.0, 3.0],
                breath: &[(0.0, 0.35)],
                buzz: 1.0,
                attack: 0.2,
                release: 0.3,
                rough: (0.6, 0.5),
                ..Syllable::default()
            });
            1.6
        }
        Sound::Pant => {
            // Hah-hah-hah: quick breaths, out louder than in.
            for k in 0..8 {
                let at = 0.155 * k as f32;
                let out = k % 2 == 0;
                let hz = if out { 1500.0 } else { 1100.0 };
                let g = if out { 1.0 } else { 0.55 };
                b.noise(at, 0.12, hz, 1.2, &|t| ease(t / 0.2) * (1.0 - t), g);
            }
            1.25
        }
        // Made above, before any species' voice.
        Sound::Tune => 0.0,
        Sound::Bell => {
            // A small bell (suzu) shaken: a few strikes, one after another.
            let hz = 2350.0;
            for (k, g) in [1.0, 0.7, 0.5, 0.35].into_iter().enumerate() {
                let at = 0.07 * k as f32 + 0.01 * b.rng.f();
                let detune = 1.0 + 0.01 * b.rng.s();
                b.bell(at, hz * detune, g);
            }
            0.7
        }
        Sound::Flutter => {
            // Wing beats: soft whooshes.
            for k in 0..5 {
                let at = 0.075 * k as f32;
                b.noise(
                    at,
                    0.065,
                    700.0,
                    0.8,
                    &|t| (std::f32::consts::PI * t).sin(),
                    1.0 - 0.1 * k as f32,
                );
            }
            0.42
        }
    };
    b.end(end, sound.class())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floats(v: &[i16]) -> Vec<f32> {
        v.iter()
            .map(|&s| f32::from(s) / f32::from(i16::MAX))
            .collect()
    }

    #[test]
    fn every_sound_is_short_at_its_loudness_and_never_clips() {
        for s in Species::ALL {
            for sound in Sound::ALL {
                for take in 0..TAKES {
                    let samples = synth(s, sound, take);
                    assert!(!samples.is_empty());
                    assert!(
                        samples.len() as f32 <= RATE as f32 * LONGEST,
                        "{s:?} {sound:?} is too long"
                    );
                    let peak = samples.iter().map(|v| v.unsigned_abs()).max().unwrap();
                    assert!(
                        f32::from(peak) <= PEAK * f32::from(i16::MAX) + 1.0,
                        "{s:?} {sound:?} clips"
                    );
                    let loud = loudness(&floats(&samples));
                    let want = sound.class().target();
                    assert!(
                        (loud - want).abs() < 2.0,
                        "{s:?} {sound:?} take {take}: {loud:.1} dB, meant {want} dB"
                    );
                }
            }
        }
    }

    #[test]
    fn calls_are_louder_than_soft_sounds() {
        let voice = loudness(&floats(&synth(Species::Shiba, Sound::Voice, 0)));
        let snore = loudness(&floats(&synth(Species::Shiba, Sound::Snore, 0)));
        assert!(voice - snore > 6.0, "{voice} vs {snore}");
    }

    #[test]
    fn takes_and_species_sound_different() {
        for s in Species::ALL {
            let takes: Vec<_> = (0..TAKES).map(|t| synth(s, Sound::Voice, t)).collect();
            assert_ne!(takes[0], takes[1], "{s:?}");
            assert_ne!(takes[1], takes[2], "{s:?}");
        }
        assert_ne!(
            synth(Species::Chick, Sound::Voice, 0),
            synth(Species::Duck, Sound::Voice, 0)
        );
    }

    #[test]
    fn the_limiter_bends_peaks_but_leaves_quiet_samples_alone() {
        assert_eq!(limit(0.3), 0.3);
        assert_eq!(limit(-0.5), -0.5);
        for x in [0.7, 1.0, 3.0, 100.0] {
            let y = limit(x);
            assert!(y > 0.6 && y <= PEAK, "{x} → {y}");
            assert_eq!(limit(-x), -y);
        }
        assert!(limit(1.0) > limit(0.8));
    }

    #[test]
    fn every_recorded_clip_is_there_and_fits() {
        use Clip::*;
        for clip in [
            ShibaBark,
            ShibaWhimper,
            ShibaGrowl,
            FoxYip,
            FoldMeow,
            MunchkinMeow,
            CatPurr,
            CatTrill,
            CatGrumble,
            OwlHoot,
            ChickPeep,
            DucklingPeeps,
            Snore,
        ] {
            let s = clip.samples();
            let secs = s.len() as f32 / RATE as f32;
            assert!((0.15..=LONGEST).contains(&secs), "{clip:?}: {secs} s");
            // Baked to peak at −1 dBFS (tools/soundbake).
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!((peak - PEAK).abs() < 0.01, "{clip:?}: peak {peak}");
        }
    }

    #[test]
    fn the_calls_are_recorded_and_the_knocks_synthesised() {
        let mut b = Buf::new(LONGEST, 0, 1.0);
        for s in Species::ALL {
            for sound in [
                Sound::Voice,
                Sound::Greet,
                Sound::Laugh,
                Sound::Grumpy,
                Sound::Snore,
            ] {
                assert!(recorded(&mut b, s, sound, 0).is_some(), "{s:?} {sound:?}");
            }
            for sound in [
                Sound::Poke,
                Sound::Land,
                Sound::Eat,
                Sound::Bell,
                Sound::Flutter,
                Sound::Pant,
            ] {
                assert!(recorded(&mut b, s, sound, 0).is_none(), "{s:?} {sound:?}");
            }
        }
        // Each cat its own meow.
        assert_ne!(
            synth(Species::Fold, Sound::Voice, 0),
            synth(Species::Munchkin, Sound::Voice, 0)
        );
    }
}
