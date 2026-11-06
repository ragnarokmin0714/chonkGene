//! chonkGene (肉肉賴桌): chubby desktop pets that lounge on your desktop.
//!
//! One executable (PLAN §3.2): the pet process by default, the settings
//! window with `--settings`, a GPU check with `--gpu-check`.

// Release builds on Windows are GUI programs: no console window pops up.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod brand;
mod diag;
mod ipc;
mod lines;
mod settings;
mod store;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: chonkgene [--data DIR] [--stats] [--gpu-check]
                 [--menu-entry | --no-menu-entry]

  --data DIR    keep settings and pets in DIR instead of the usual place
  --stats       print CPU and memory every 5 seconds
  --gpu-check   measure the GPU renderer and write a report next to the
                program (chonkGene-gpu-check), then open that folder
  --menu-entry  Linux: put 肉肉賴桌 with its icon in the desktop's app
                menu (moving the folder later is fine: it follows)
  --no-menu-entry  take it out of the app menu again";

fn main() -> ExitCode {
    let mut data: Option<PathBuf> = None;
    let mut stats = false;
    let mut settings = false;
    let mut pet: Option<u64> = None;
    let mut gpu_check = false;
    let mut menu: Option<bool> = None;
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
            "--gpu-check" => gpu_check = true,
            "--menu-entry" => menu = Some(true),
            "--no-menu-entry" => menu = Some(false),
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
    let result = if let Some(add) = menu {
        menu_entry(add)
    } else if gpu_check {
        check_gpu()
    } else if settings {
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

/// `--gpu-check`: release builds on Windows have no console, so the report
/// goes into a folder next to the program, which then opens.
fn check_gpu() -> Result<(), String> {
    let out = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("chonkGene-gpu-check")))
        .unwrap_or_else(|| PathBuf::from("chonkGene-gpu-check"));
    let report = chonkgene_render::gpu_check::run(&out)?;
    print!("{report}");
    println!("\n→ {}", out.display());
    settings::open_folder(&out);
    Ok(())
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("{msg}\n\n{USAGE}");
    ExitCode::from(2)
}

/// `--menu-entry`, `--no-menu-entry`: the Linux app-menu entry.
fn menu_entry(add: bool) -> Result<(), String> {
    use chonkgene_platform::menu_entry;
    if add {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        menu_entry::add(&exe, brand::png_256())?;
        println!("肉肉賴桌 is in the app menu now.");
    } else {
        menu_entry::remove()?;
        println!("肉肉賴桌 is out of the app menu.");
    }
    Ok(())
}
