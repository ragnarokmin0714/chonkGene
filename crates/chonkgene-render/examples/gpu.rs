//! The GPU renderer against the CPU one, and what it costs:
//! `cargo run --release -p chonkgene-render --example gpu -- <out dir>`
//! (the same check as `chonkgene --gpu-check`; see `gpu_check`).

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "gpu-out".into());
    match chonkgene_render::gpu_check::run(std::path::Path::new(&out)) {
        Ok(report) => print!("{report}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
