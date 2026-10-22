use std::time::Instant;

use cpu_time::ProcessTime;

/// Process CPU use between samples, as a percentage of one core.
pub struct Sampler {
    wall: Instant,
    cpu: ProcessTime,
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            wall: Instant::now(),
            cpu: ProcessTime::now(),
        }
    }

    pub fn cpu_percent(&mut self) -> f64 {
        let wall = self.wall.elapsed().as_secs_f64();
        let cpu = self.cpu.elapsed().as_secs_f64();
        self.wall = Instant::now();
        self.cpu = ProcessTime::now();
        if wall > 0.0 { cpu / wall * 100.0 } else { 0.0 }
    }
}

/// Resident memory of this process in MiB, or NaN where unsupported.
pub fn rss_mib() -> f64 {
    memory_stats::memory_stats()
        .map(|m| m.physical_mem as f64 / 1_048_576.0)
        .unwrap_or(f64::NAN)
}
