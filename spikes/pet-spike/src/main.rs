//! Phase 0 spikes for chonkGene (docs/PLAN.md §8).
//!
//! One executable covers:
//! - S1/S2: transparent, always-on-top pet windows with per-pixel click-through,
//!   per-frame moves and live CPU/memory numbers (default mode);
//! - S2 headless: drawing cost without any window (`--bench`);
//! - S3: the settings UI as a child process talking over IPC (`--settings`,
//!   started by right-clicking a pet).

mod app;
mod bench;
mod ipc;
mod metrics;
mod pet;
mod present;
mod settings;

use std::process::ExitCode;

const USAGE: &str = "\
usage: pet-spike [--pets N] [--x11-managed]
       pet-spike --bench [--pets N] [--secs S]

  --pets N        pets on screen (default 6; the bench also runs 4 × N)
  --x11-managed   Linux: use a WM-managed utility window instead of override-redirect
  --bench         headless drawing benchmark, no windows
  --secs S        bench duration per pet count (default 10)";

enum Mode {
    Pets,
    Bench,
    Settings(String),
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = app::Options::default();
    let mut mode = Mode::Pets;
    let mut secs = 10.0_f32;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--pets" => match it.next().and_then(|v| v.parse().ok()) {
                Some(n) if n > 0 => opts.pets = n,
                _ => return usage_error("--pets needs a positive number"),
            },
            "--secs" => match it.next().and_then(|v| v.parse().ok()) {
                Some(s) if s > 0.0 => secs = s,
                _ => return usage_error("--secs needs a positive number"),
            },
            "--bench" => mode = Mode::Bench,
            "--x11-managed" => opts.x11_managed = true,
            "--settings" => match it.next() {
                Some(sock) => mode = Mode::Settings(sock.clone()),
                None => return usage_error("--settings needs a socket name"),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => return usage_error(&format!("unknown argument {other}")),
        }
    }

    let result = match mode {
        Mode::Pets => app::run(opts),
        Mode::Bench => {
            bench::run(opts.pets, secs);
            Ok(())
        }
        Mode::Settings(sock) => settings::run(&sock),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("{msg}\n\n{USAGE}");
    ExitCode::from(2)
}
