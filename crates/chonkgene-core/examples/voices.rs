//! Writes every pet sound as a WAV file, to listen to them side by side.
//!
//! `cargo run --release -p chonkgene-core --example voices -- <out dir>`
//! writes `<species>-<sound>-<take>.wav` (mono, 16-bit, 44.1 kHz) and
//! prints each one's loudness (see `sound::loudness`) and peak.
//!
//! `... -- measure <file.wav>...` prints the loudness and peak of WAV files
//! of the same format (older sounds, recordings).

use chonkgene_core::sound::{RATE, Sound, TAKES, loudness, synth};
use chonkgene_core::species::Species;

fn wav(samples: &[i16]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&(RATE * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

/// The samples of a mono 16-bit WAV file, as floats.
fn read(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("read");
    let at = bytes
        .windows(4)
        .position(|w| w == b"data")
        .expect("no data chunk")
        + 8;
    bytes[at..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| f32::from(i16::from_le_bytes(*b)) / 32767.0)
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("measure") {
        for path in &args[1..] {
            let f = read(path);
            let peak = f.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            println!(
                "{path}\t{:.1} dB\tpeak {:.1} dBFS",
                loudness(&f),
                20.0 * peak.log10()
            );
        }
        return;
    }
    let out = args
        .first()
        .expect("usage: voices <out dir> | measure <file.wav>...");
    std::fs::create_dir_all(out).expect("out dir");
    for species in Species::ALL {
        for sound in Sound::ALL {
            for take in 0..TAKES {
                let s = synth(species, sound, take);
                let f: Vec<f32> = s.iter().map(|&x| f32::from(x) / 32767.0).collect();
                let peak = f.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                let name = format!("{}-{sound:?}-{take}", species.id()).to_lowercase();
                std::fs::write(format!("{out}/{name}.wav"), wav(&s)).expect("write");
                println!(
                    "{name}\t{:.1} dB\tpeak {:.1} dBFS\t{:.2} s",
                    loudness(&f),
                    20.0 * peak.log10(),
                    s.len() as f32 / RATE as f32
                );
            }
        }
    }
}
