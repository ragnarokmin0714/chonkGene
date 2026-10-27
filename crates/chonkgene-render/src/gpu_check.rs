//! What the GPU renderer costs on this machine, and whether it draws what
//! the CPU does: `chonkgene --gpu-check` (a downloaded build, no Rust
//! needed) and `cargo run --release -p chonkgene-render --example gpu`.
//!
//! Writes into `out`: the report, every species drawn by the GPU and the
//! CPU side by side, and frames of a real 3D turn, a hop (the shadow stays
//! on the floor) and lying down.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use chonkgene_rig::{Layer, Look, PoseId, Scene, builtin};
use tiny_skia::{IntSize, Pixmap, PixmapPaint, Transform};

use chonkgene_core::anim::Animator;

use crate::gpu::{Frame, Gpu};
use crate::sprites::{Variant, render_layer};
use crate::{GpuRenderer, KEYS};

const SPECIES: [&str; 6] = ["shiba", "munchkin", "fold", "owl", "chick", "duck"];

/// Run the check, write its files into `out` and return the report (zh-TW,
/// as Roger reads it).
pub fn run(out: &Path) -> Result<String, String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut report = String::new();
    let gpu = match Gpu::new() {
        Ok(g) => g,
        Err(e) => {
            let _ = writeln!(report, "找不到可用的 GPU：{e}");
            let _ = writeln!(report, "（這台電腦會用預先算好的圖片方式。）");
            return Ok(report);
        }
    };
    let _ = writeln!(report, "顯示卡：{}", gpu.name());
    if gpu.is_software() {
        let _ = writeln!(
            report,
            "這是用 CPU 模擬的 GPU：這台電腦會用預先算好的圖片方式，下面的數字僅供參考。"
        );
    }
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let save = |pm: &Pixmap, name: &str| {
        pm.save_png(out.join(name))
            .map_err(|e| format!("{name}: {e}"))
    };

    // The GPU against the CPU, every species.
    let _ = writeln!(report, "\nGPU 和 CPU 畫出來的差異（越小越好）：");
    let mut canvas = gpu.canvas(1.0);
    let (w, h) = canvas.size();
    let mut pm = Pixmap::new(w, h).ok_or("pixmap")?;
    for id in SPECIES {
        let model = builtin::load(id).map_err(|e| e.to_string())?;
        let key = KEYS[0];
        let scene = Scene::new(&model, 0, 0.33, key.pose, key.look, Layer::All);
        gpu.render(&mut canvas, &scene, &Frame::default(), true, &mut pm)?;
        let variant = Variant {
            coat: 0,
            chonk: 0.33,
            scale: 1.0,
        };
        let s = render_layer(&model, variant, key, Layer::All, threads);
        let cpu = Pixmap::from_vec(s.rgba, IntSize::from_wh(s.width, s.height).ok_or("size")?)
            .ok_or("cpu pixmap")?;
        let (mean, off) = compare(&pm, &cpu);
        let _ = writeln!(report, "  {id}: 平均差 {mean:.2}，{off:.2}% 的像素差超過 8");
        save(&side_by_side(&pm, &cpu), &format!("{id}-gpu-cpu.png"))?;
    }

    // Frames: a real turn, a hop, lying down.
    let model = builtin::load("shiba").map_err(|e| e.to_string())?;
    let sit = model.pose(PoseId::Sit).clone();
    let walk = model.pose(PoseId::WalkA).clone();
    let turned = walk.mirrored();
    let loaf = model.pose(PoseId::Loaf).clone();
    for i in 0..=12 {
        let pose = walk.blend(&turned, i as f32 / 12.0);
        let scene = Scene::posed(&model, 0, 0.33, &pose, Look::Open, Layer::All);
        gpu.render(&mut canvas, &scene, &Frame::default(), true, &mut pm)?;
        save(&pm, &format!("turn-{i:02}.png"))?;
    }
    for (i, lift) in [0.0, 10.0, 20.0, 28.0].into_iter().enumerate() {
        let scene = Scene::new(&model, 0, 0.33, PoseId::Sit, Look::Happy, Layer::All);
        let frame = Frame {
            lift,
            squash: if lift == 0.0 { 0.06 } else { -0.04 },
            ..Frame::default()
        };
        gpu.render(&mut canvas, &scene, &frame, true, &mut pm)?;
        save(&pm, &format!("hop-{i}.png"))?;
    }
    for i in 0..=6 {
        let pose = sit.blend(&loaf, i as f32 / 6.0);
        let scene = Scene::posed(&model, 0, 0.33, &pose, Look::Shut, Layer::All);
        gpu.render(&mut canvas, &scene, &Frame::default(), true, &mut pm)?;
        save(&pm, &format!("lie-{i}.png"))?;
    }

    // Cost: a steady run of turning frames at each size.
    let _ = writeln!(report, "\n每幀成本（畫 + 傳回，持續轉身）：");
    for scale in [1.0, 1.5, 2.0] {
        let mut canvas = gpu.canvas(scale);
        let (w, h) = canvas.size();
        let mut pm = Pixmap::new(w, h).ok_or("pixmap")?;
        let (mut times, mut gpu_times) = (Vec::new(), Vec::new());
        for i in 0..60 {
            let t = (i as f32 / 30.0 * std::f32::consts::PI).sin() * 0.5 + 0.5;
            let pose = walk.blend(&turned, t);
            let scene = Scene::posed(&model, 0, 0.33, &pose, Look::Open, Layer::All);
            let (started, before) = (Instant::now(), canvas.gpu_seconds());
            gpu.render(&mut canvas, &scene, &Frame::default(), true, &mut pm)?;
            times.push(started.elapsed());
            gpu_times.push(Duration::from_secs_f64(canvas.gpu_seconds() - before));
        }
        // The first frames include warming up the driver.
        times.drain(..5);
        gpu_times.drain(..5);
        times.sort();
        gpu_times.sort();
        let median = times[times.len() / 2];
        let gpu_median = gpu_times[gpu_times.len() / 2];
        let measured = if gpu.times_frames() {
            format!("，其中 GPU 實際畫 {}", ms(gpu_median))
        } else {
            String::new()
        };
        let busy = if gpu.times_frames() {
            gpu_median
        } else {
            median
        };
        let _ = writeln!(
            report,
            "  {:.0}%（{w}×{h}）：中位數 {}{measured}；一隻持續動作（30 fps）約佔這張顯示卡 {:.1}%",
            scale * 100.0,
            ms(median),
            busy.as_secs_f64() * 30.0 * 100.0
        );
    }
    if !gpu.times_frames() {
        let _ = writeln!(
            report,
            "\n（這張卡不能計時：「約佔」把等畫面傳回來的時間也算進去了，實際更少。）"
        );
    }

    // What pets actually send: most frames come from the cache. Without
    // the budget, to see what they would ask for.
    let _ = writeln!(report, "\n實際使用（模擬，100% 大小，不設上限）：");
    let model = std::sync::Arc::new(model);
    let gpu: &'static Gpu = Box::leak(Box::new(gpu));
    let budget = gpu.budget();
    gpu.set_budget(1.0);
    for (label, walking, secs) in [("閒著的寵物", false, 60.0), ("一直走路的寵物", true, 20.0)]
    {
        let Some(mut r) = GpuRenderer::new(gpu, std::sync::Arc::clone(&model), 1.0, 0.33, 0) else {
            continue;
        };
        let mut anim = Animator::new(7, 0.33, 0.35);
        anim.pops = false;
        anim.walking = walking;
        let (mut t, mut frames) = (0.0f32, 0u32);
        let started = Instant::now();
        while t < secs {
            let mut fps = anim.fps(false);
            if r.busy() {
                fps = fps.max(30);
            }
            let dt = 1.0 / fps as f32;
            anim.step(dt);
            t += dt;
            r.render(&anim.params(), anim.expression);
            r.finish();
            frames += 1;
        }
        let busy = started.elapsed().as_secs_f64();
        let sent = r.frames_sent() as f64 / f64::from(secs);
        let share = if gpu.times_frames() {
            format!(
                "GPU 實際忙 {:.1}%",
                r.gpu_seconds() / f64::from(secs) * 100.0
            )
        } else {
            format!(
                "GPU 約忙 {:.1}%（上限，這張卡不能計時）",
                busy / f64::from(secs) * 100.0
            )
        };
        let _ = writeln!(
            report,
            "  {label}：每秒 {:.1} 幀，其中 {sent:.1} 幀交給 GPU；{share}",
            f64::from(frames) / f64::from(secs),
        );
    }
    gpu.set_budget(budget);
    let _ = writeln!(
        report,
        "\n寵物的 GPU 用量上限：{:.0}%（超過時動作的幀數會自動降低）",
        budget * 100.0
    );
    std::fs::write(out.join("report.txt"), &report).map_err(|e| format!("report: {e}"))?;
    Ok(report)
}

fn ms(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1e3)
}

/// Mean absolute channel difference, and the share of pixels off by more
/// than 8 in any channel.
fn compare(a: &Pixmap, b: &Pixmap) -> (f64, f64) {
    let (mut sum, mut off) = (0u64, 0usize);
    for (p, q) in a
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.data().as_chunks::<4>().0)
    {
        let d = [0, 1, 2, 3].map(|i| p[i].abs_diff(q[i]));
        sum += d.iter().map(|&x| u64::from(x)).sum::<u64>();
        if d.iter().any(|&x| x > 8) {
            off += 1;
        }
    }
    let px = (a.data().len() / 4).max(1);
    (sum as f64 / (px * 4) as f64, off as f64 * 100.0 / px as f64)
}

fn side_by_side(a: &Pixmap, b: &Pixmap) -> Pixmap {
    let Some(mut out) = Pixmap::new(a.width() * 2, a.height()) else {
        return a.clone();
    };
    let paint = PixmapPaint::default();
    out.draw_pixmap(0, 0, a.as_ref(), &paint, Transform::identity(), None);
    out.draw_pixmap(
        a.width() as i32,
        0,
        b.as_ref(),
        &paint,
        Transform::identity(),
        None,
    );
    out
}
