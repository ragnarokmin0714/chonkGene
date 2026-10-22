//! chonkGene (肉肉賴桌): chubby desktop pets.
//!
//! Phase 1 prototype (docs/PLAN.md §8): one Shiba to judge the feel of
//! poking, lifting, shaking and throwing on a real desktop.

mod app;
mod ipc;
mod lines;
mod store;

use std::process::ExitCode;

const USAGE: &str = "\
usage: chonkgene [--pets N] [--scale S] [--chonk C] [--coat NAME] [--stats]

  --pets N      how many Shibas (default 1)
  --scale S     size, 0.5–2.0 (default 1.0)
  --chonk C     body value, 0.0 標準 … 0.33 略肥 … 1.0 麻糬 (default 0.33)
  --coat NAME   red, black, sesame or white (default red)
  --stats       print CPU and memory every 5 seconds";

fn main() -> ExitCode {
    let mut opts = app::Options::default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        let parsed: Result<(), String> = match arg.as_str() {
            "--pets" => value("--pets").and_then(|v| match v.parse() {
                Ok(n) if n > 0 => {
                    opts.pets = n;
                    Ok(())
                }
                _ => Err("--pets needs a positive number".into()),
            }),
            "--scale" => value("--scale").and_then(|v| match v.parse::<f32>() {
                Ok(s) if (0.5..=2.0).contains(&s) => {
                    opts.scale = s;
                    Ok(())
                }
                _ => Err("--scale must be between 0.5 and 2.0".into()),
            }),
            "--chonk" => value("--chonk").and_then(|v| match v.parse::<f32>() {
                Ok(c) if (0.0..=1.0).contains(&c) => {
                    opts.chonk = c;
                    Ok(())
                }
                _ => Err("--chonk must be between 0.0 and 1.0".into()),
            }),
            "--coat" => value("--coat").map(|v| opts.coat = v.clone()),
            "--stats" => {
                opts.stats = true;
                Ok(())
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => Err(format!("unknown argument {other}")),
        };
        if let Err(e) = parsed {
            eprintln!("{e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    }
    match app::run(opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
