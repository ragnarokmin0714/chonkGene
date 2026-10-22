//! Placeholder sounds, synthesised (PLAN §1.5, §10): short, soft and
//! different per species until recorded sounds replace them.

use crate::species::Species;

pub const RATE: u32 = 44_100;

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
}

/// Base pitch (Hz) and timbre of each species' voice.
fn voice_of(s: Species) -> (f32, Timbre) {
    match s {
        Species::Shiba => (560.0, Timbre::Bark),
        Species::Munchkin => (760.0, Timbre::Meow),
        Species::Fold => (680.0, Timbre::Meow),
        Species::Owl => (360.0, Timbre::Hoot),
        Species::Chick => (1900.0, Timbre::Chirp),
        Species::Duck => (430.0, Timbre::Quack),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Timbre {
    Bark,
    Meow,
    Hoot,
    Chirp,
    Quack,
}

struct Buf {
    samples: Vec<f32>,
}

impl Buf {
    fn new(secs: f32) -> Self {
        Self {
            samples: vec![0.0; (secs * RATE as f32) as usize],
        }
    }

    /// Add a voiced syllable starting at `at` seconds.
    fn syllable(&mut self, at: f32, dur: f32, f_from: f32, f_to: f32, timbre: Timbre, gain: f32) {
        let start = (at * RATE as f32) as usize;
        let n = (dur * RATE as f32) as usize;
        let mut phase = 0.0f32;
        for i in 0..n {
            let Some(out) = self.samples.get_mut(start + i) else {
                break;
            };
            let t = i as f32 / n as f32;
            // Meows rise then fall; everything else glides.
            let f = match timbre {
                Timbre::Meow => {
                    let peak = f_from * 1.25;
                    if t < 0.35 {
                        f_from + (peak - f_from) * (t / 0.35)
                    } else {
                        peak + (f_to - peak) * ((t - 0.35) / 0.65)
                    }
                }
                _ => f_from * (f_to / f_from).powf(t),
            };
            let vibrato =
                1.0 + 0.012 * (i as f32 / RATE as f32 * std::f32::consts::TAU * 6.0).sin();
            phase += f * vibrato / RATE as f32;
            let p = phase * std::f32::consts::TAU;
            let wave = match timbre {
                Timbre::Bark => p.sin() + 0.45 * (2.0 * p).sin() + 0.2 * (3.0 * p).sin(),
                Timbre::Meow => p.sin() + 0.3 * (2.0 * p).sin() + 0.12 * (4.0 * p).sin(),
                Timbre::Hoot => p.sin() + 0.08 * (2.0 * p).sin(),
                Timbre::Chirp => p.sin() + 0.15 * (2.0 * p).sin(),
                // Odd harmonics give the nasal 呱.
                Timbre::Quack => {
                    p.sin() + 0.6 * (3.0 * p).sin() + 0.35 * (5.0 * p).sin() + 0.2 * (7.0 * p).sin()
                }
            };
            let attack = (t / 0.08).min(1.0);
            let release = ((1.0 - t) / 0.35).min(1.0);
            *out += wave * attack * release * gain;
        }
    }

    fn noise(&mut self, at: f32, dur: f32, gain: f32, smooth: f32, rng: &mut fastrand::Rng) {
        let start = (at * RATE as f32) as usize;
        let n = (dur * RATE as f32) as usize;
        let mut low = 0.0f32;
        for i in 0..n {
            let Some(out) = self.samples.get_mut(start + i) else {
                break;
            };
            let t = i as f32 / n as f32;
            low += (rng.f32() * 2.0 - 1.0 - low) * smooth;
            let env = (t / 0.1).min(1.0) * (1.0 - t).powf(1.5);
            *out += low * env * gain;
        }
    }

    fn tone(&mut self, at: f32, dur: f32, f_from: f32, f_to: f32, gain: f32) {
        let start = (at * RATE as f32) as usize;
        let n = (dur * RATE as f32) as usize;
        let mut phase = 0.0f32;
        for i in 0..n {
            let Some(out) = self.samples.get_mut(start + i) else {
                break;
            };
            let t = i as f32 / n as f32;
            phase += (f_from * (f_to / f_from).powf(t)) / RATE as f32;
            let env = (t / 0.05).min(1.0) * (1.0 - t).powi(2);
            *out += (phase * std::f32::consts::TAU).sin() * env * gain;
        }
    }

    /// Normalise to a gentle peak and convert.
    fn finish(self, peak: f32) -> Vec<i16> {
        let max = self
            .samples
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()))
            .max(1e-6);
        self.samples
            .iter()
            .map(|s| (s / max * peak * f32::from(i16::MAX)) as i16)
            .collect()
    }
}

/// Mono 16-bit samples at [`RATE`] for `sound` in `species`' voice.
pub fn synth(species: Species, sound: Sound, seed: u64) -> Vec<i16> {
    let mut rng = fastrand::Rng::with_seed(seed);
    let (f0, timbre) = voice_of(species);
    let short = if timbre == Timbre::Chirp { 0.6 } else { 1.0 };
    let (buf, peak) = match sound {
        Sound::Voice => {
            let mut b = Buf::new(0.45);
            b.syllable(0.0, 0.32 * short, f0, f0 * 0.82, timbre, 1.0);
            if timbre == Timbre::Chirp {
                b.syllable(0.2, 0.18, f0 * 1.1, f0 * 0.95, timbre, 0.8);
            }
            (b, 0.55)
        }
        Sound::Poke => {
            let mut b = Buf::new(0.14);
            b.tone(0.0, 0.12, 320.0, 210.0, 1.0);
            (b, 0.45)
        }
        Sound::Laugh => {
            let mut b = Buf::new(0.8);
            for k in 0..4 {
                let f = f0 * (1.25 - 0.06 * k as f32);
                b.syllable(
                    0.17 * k as f32,
                    0.12 * short,
                    f,
                    f * 0.9,
                    timbre,
                    1.0 - 0.1 * k as f32,
                );
            }
            (b, 0.5)
        }
        Sound::Sigh => {
            let mut b = Buf::new(0.75);
            b.noise(0.0, 0.7, 0.8, 0.08, &mut rng);
            b.syllable(0.05, 0.5, f0 * 0.7, f0 * 0.5, timbre, 0.25);
            (b, 0.35)
        }
        Sound::Grumpy => {
            let mut b = Buf::new(0.3);
            b.syllable(0.0, 0.24 * short.max(0.8), f0 * 1.3, f0 * 1.7, timbre, 1.0);
            (b, 0.5)
        }
        Sound::Snore => {
            let mut b = Buf::new(1.0);
            b.tone(0.0, 0.9, 95.0, 80.0, 0.7);
            b.noise(0.05, 0.85, 0.4, 0.03, &mut rng);
            (b, 0.25)
        }
        Sound::Land => {
            let mut b = Buf::new(0.16);
            b.tone(0.0, 0.14, 140.0, 55.0, 1.0);
            (b, 0.5)
        }
        Sound::Eat => {
            let mut b = Buf::new(0.32);
            for k in 0..3 {
                b.noise(0.1 * k as f32, 0.035, 1.0, 0.6, &mut rng);
            }
            (b, 0.35)
        }
    };
    buf.finish(peak)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_short_audible_and_not_clipping() {
        for s in Species::ALL {
            for sound in [
                Sound::Voice,
                Sound::Poke,
                Sound::Laugh,
                Sound::Sigh,
                Sound::Grumpy,
                Sound::Snore,
                Sound::Land,
                Sound::Eat,
            ] {
                let samples = synth(s, sound, 1);
                assert!(!samples.is_empty());
                assert!(
                    samples.len() <= RATE as usize * 2,
                    "{s:?} {sound:?} is too long"
                );
                let peak = samples.iter().map(|v| v.unsigned_abs()).max().unwrap();
                assert!(peak > 3000, "{s:?} {sound:?} is silent");
                assert!(peak < 25_000, "{s:?} {sound:?} is too loud");
            }
        }
    }

    #[test]
    fn species_sound_different() {
        let a = synth(Species::Chick, Sound::Voice, 1);
        let b = synth(Species::Duck, Sound::Voice, 1);
        assert_ne!(a, b);
    }
}
