//! S2 without windows: what drawing the pets costs, so the number can be
//! compared across machines (CI runners included) without a desktop.

use std::thread;
use std::time::{Duration, Instant};

use tiny_skia::{Color, Pixmap};

use crate::metrics::{self, Sampler};
use crate::pet::{Mode, Pet, SIZE};
use crate::present::swizzle;

pub fn run(pets: usize, secs: f32) {
    println!("chonkGene Phase 0 bench: {SIZE} px pets, {secs} s per run");
    for n in [pets, pets * 4] {
        measure(n, secs);
    }
}

fn measure(n: usize, secs: f32) {
    let rss_before = metrics::rss_mib();
    let mut pixmaps: Vec<Pixmap> = (0..n)
        .map(|_| Pixmap::new(SIZE, SIZE).expect("pixmap"))
        .collect();
    let mut pets: Vec<Pet> = (0..n).map(|i| Pet::new(i as u32)).collect();
    let mut bgra = vec![0u8; (SIZE * SIZE * 4) as usize];
    for (i, pet) in pets.iter_mut().enumerate() {
        // Mix like a real desktop: one in six walking, the rest idle; pokes add 60 fps bursts.
        pet.mode = if i % 6 == 0 { Mode::Walk } else { Mode::Idle };
    }

    // Throughput: how long one frame takes, flat out.
    let half = Duration::from_secs_f32(secs / 2.0);
    let start = Instant::now();
    let (mut frames, mut draw, mut convert) = (0u64, Duration::ZERO, Duration::ZERO);
    while start.elapsed() < half {
        for (pet, pm) in pets.iter_mut().zip(&mut pixmaps) {
            if frames % 40 == 0 {
                pet.poke();
            }
            let t0 = Instant::now();
            pet.step(1.0 / 30.0);
            pm.fill(Color::TRANSPARENT);
            pet.draw(pm);
            let t1 = Instant::now();
            swizzle(pm.data(), &mut bgra);
            std::hint::black_box(&mut bgra);
            draw += t1 - t0;
            convert += t1.elapsed();
            frames += 1;
        }
    }
    let per = |d: Duration| d.as_secs_f64() * 1e6 / frames as f64;

    // Paced: every pet at the frame rate it asks for, with real sleeping in between.
    let mut sampler = Sampler::new();
    let start = Instant::now();
    let mut next = vec![start; n];
    let mut last = vec![start; n];
    let mut paced_frames = 0u64;
    loop {
        let now = Instant::now();
        if now - start >= half {
            break;
        }
        for i in 0..n {
            if next[i] > now {
                continue;
            }
            if i % 6 == 1 && (now - start).as_millis() % 3000 < 20 {
                pets[i].poke();
            }
            pets[i].step((now - last[i]).as_secs_f32());
            pixmaps[i].fill(Color::TRANSPARENT);
            pets[i].draw(&mut pixmaps[i]);
            swizzle(pixmaps[i].data(), &mut bgra);
            std::hint::black_box(&mut bgra);
            last[i] = now;
            next[i] = now + Duration::from_secs_f64(1.0 / f64::from(pets[i].fps()));
            paced_frames += 1;
        }
        if let Some(wake) = next.iter().min() {
            thread::sleep(wake.saturating_duration_since(Instant::now()));
        }
    }
    let cpu = sampler.cpu_percent();
    let rss = metrics::rss_mib();

    let fps = paced_frames as f64 / half.as_secs_f64();
    // Drawing cost at full clock speed; the measured figure also includes
    // wake-ups and frequency scaling, which vary a lot between machines.
    let estimate = fps * (per(draw) + per(convert)) / 1e4;
    println!(
        "{n:>3} pets | draw {:>4.0} µs + convert {:>3.0} µs per frame | {fps:>5.1} frames/s | CPU {estimate:>5.2}% of one core at full clock, {cpu:>5.2}% measured | RSS {rss:.1} MiB (+{:.1})",
        per(draw),
        per(convert),
        rss - rss_before,
    );
}
