//! chonkGene (肉肉賴桌): chubby desktop pets that lounge on your desktop.
//!
//! One executable (PLAN §3.2): the pet process by default, the settings
//! window with `--settings`.

// Release builds on Windows are GUI programs: no console window pops up.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod ipc;
mod lines;
mod settings;
mod store;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: chonkgene [--data DIR] [--stats]

  --data DIR    keep settings and pets in DIR instead of the usual place
  --stats       print CPU and memory every 5 seconds";

fn main() -> ExitCode {
    let mut data: Option<PathBuf> = None;
    let mut stats = false;
    let mut settings = false;
    let mut pet: Option<u64> = None;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--data" => match it.next() {
                Some(dir) => data = Some(PathBuf::from(dir)),
                None => return usage("--data needs a folder"),
            },
            "--stats" => stats = true,
            "--settings" => settings = true,
            "--pet" => match it.next().and_then(|v| v.parse().ok()) {
                Some(id) => pet = Some(id),
                None => return usage("--pet needs a pet id"),
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => return usage(&format!("unknown argument {other}")),
        }
    }
    let result = if settings {
        settings::run(data, pet)
    } else {
        app::run(app::Options { data, stats })
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("{msg}\n\n{USAGE}");
    ExitCode::from(2)
}
