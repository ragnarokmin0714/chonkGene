//! The pet's right-click menu (PLAN §1.2), drawn like the mockup: a cream
//! card, a header with name, species, affinity and state, and icon rows.

use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform,
};

use crate::text::Face;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Heart,
    Snack,
    Pencil,
    Palette,
    Moon,
    Sun,
    Hide,
    Sliders,
    Exit,
    /// A hand waving goodbye.
    Bye,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub label: String,
    pub hint: Option<String>,
    pub icon: Icon,
    /// Draw a separator line above this item.
    pub separated: bool,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub name: String,
    pub species: String,
    pub level: u32,
    /// 0.0–1.0 towards the next level.
    pub progress: f32,
    /// 「睡覺中」, 「有起床氣」, …
    pub status: Option<String>,
    /// Use the grumpy (pink) chip colour.
    pub grumpy: bool,
    /// Colour of the avatar dot: main and light.
    pub avatar: ([u8; 4], [u8; 4]),
    pub items: Vec<Item>,
}

/// A drawn menu and where each item is, in its own pixels.
pub struct Drawn {
    pub pixmap: Pixmap,
    pub rows: Vec<Rect>,
}

const W: f32 = 248.0;
const PAD: f32 = 8.0;
const HEAD: f32 = 66.0;
const ROW: f32 = 44.0;
const SEP: f32 = 13.0;
const RADIUS: f32 = 20.0;
const SHADOW: f32 = 8.0;

const BG: [u8; 4] = [0xFF, 0xFD, 0xF9, 255];
const BORDER: [u8; 4] = [0xF0, 0xDF, 0xCC, 255];
const HOVER: [u8; 4] = [0xFF, 0xEE, 0xDB, 255];
const INK: [u8; 4] = [0x4B, 0x35, 0x28, 255];
const MUTED: [u8; 4] = [0x7A, 0x62, 0x53, 255];
const ACCENT: [u8; 4] = [0xD9, 0x77, 0x2F, 255];
const SEP_LINE: [u8; 4] = [0xF3, 0xE6, 0xD8, 255];
const TRACK: [u8; 4] = [0xFF, 0xE3, 0xCC, 255];
const FILL: [u8; 4] = [0xE8, 0x89, 0x4A, 255];
const CHIP: [u8; 4] = [0xEE, 0xF0, 0xF8, 255];
const CHIP_GRUMPY: [u8; 4] = [0xFD, 0xE2, 0xE2, 255];

fn paint(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], c[3]);
    p.anti_alias = true;
    p
}

pub(crate) fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

fn height(model: &Model) -> f32 {
    let seps = model.items.iter().filter(|i| i.separated).count() as f32;
    PAD * 2.0 + HEAD + ROW * model.items.len() as f32 + SEP * seps
}

/// Size in pixels at `scale`, shadow included.
pub fn size(model: &Model, scale: f32) -> (u32, u32) {
    (
        ((W + 2.0 * SHADOW) * scale).ceil() as u32,
        ((height(model) + 2.0 * SHADOW) * scale).ceil() as u32,
    )
}

pub fn draw(face: &mut Face, model: &Model, hover: Option<usize>, scale: f32) -> Option<Drawn> {
    let (pw, ph) = size(model, scale);
    let mut pm = Pixmap::new(pw, ph)?;
    let s = scale;
    let ts = Transform::from_scale(s, s).pre_translate(SHADOW, SHADOW);
    let h = height(model);

    // Soft shadow: a few expanding translucent layers.
    for (grow, alpha) in [(6.0, 8u8), (3.0, 12), (1.0, 18)] {
        if let Some(p) = rounded(
            -grow,
            -grow + 4.0,
            W + 2.0 * grow,
            h + 2.0 * grow,
            RADIUS + grow,
        ) {
            pm.fill_path(
                &p,
                &paint([0x4B, 0x35, 0x28, alpha]),
                FillRule::Winding,
                ts,
                None,
            );
        }
    }
    let card = rounded(0.0, 0.0, W, h, RADIUS)?;
    pm.fill_path(&card, &paint(BG), FillRule::Winding, ts, None);
    pm.stroke_path(
        &card,
        &paint(BORDER),
        &Stroke {
            width: 1.5,
            ..Stroke::default()
        },
        ts,
        None,
    );

    // Header: avatar, name, species, affinity, state.
    let (main, light) = model.avatar;
    if let Some(dot) = PathBuilder::from_circle(PAD + 26.0, PAD + 24.0, 16.0) {
        pm.fill_path(&dot, &paint(main), FillRule::Winding, ts, None);
    }
    if let Some(belly) =
        PathBuilder::from_oval(Rect::from_xywh(PAD + 16.0, PAD + 29.0, 20.0, 10.0)?)
    {
        pm.fill_path(&belly, &paint(light), FillRule::Winding, ts, None);
    }
    let tx = PAD + 52.0;
    let text =
        |pm: &mut Pixmap, face: &mut Face, t: &str, x: f32, y: f32, size: f32, c: [u8; 4]| {
            face.draw(
                pm,
                t,
                (SHADOW + x) * s,
                (SHADOW + y) * s,
                size * s,
                &paint(c),
            );
        };
    text(&mut pm, face, &model.name, tx, PAD + 22.0, 16.0, INK);
    let name_w = face.measure(&model.name, 16.0);
    text(
        &mut pm,
        face,
        &model.species,
        tx + name_w + 6.0,
        PAD + 22.0,
        12.0,
        MUTED,
    );
    let level = format!("親密度 Lv.{}", model.level);
    text(&mut pm, face, &level, tx, PAD + 44.0, 12.0, MUTED);
    let meter_x = tx + face.measure(&level, 12.0) + 8.0;
    let meter_w = (W - PAD - 12.0 - meter_x).max(20.0);
    if let Some(track) = rounded(meter_x, PAD + 36.0, meter_w, 8.0, 4.0) {
        pm.fill_path(&track, &paint(TRACK), FillRule::Winding, ts, None);
    }
    let filled = meter_w * model.progress.clamp(0.0, 1.0);
    if filled > 1.0
        && let Some(bar) = rounded(meter_x, PAD + 36.0, filled, 8.0, 4.0)
    {
        pm.fill_path(&bar, &paint(FILL), FillRule::Winding, ts, None);
    }
    if let Some(status) = &model.status {
        let w = face.measure(status, 12.0) + 18.0;
        let x = W - PAD - 6.0 - w;
        if let Some(chip) = rounded(x, PAD + 9.0, w, 22.0, 11.0) {
            let bg = if model.grumpy { CHIP_GRUMPY } else { CHIP };
            pm.fill_path(&chip, &paint(bg), FillRule::Winding, ts, None);
        }
        text(&mut pm, face, status, x + 9.0, PAD + 25.0, 12.0, INK);
    }
    let mut dash = PathBuilder::new();
    dash.move_to(PAD + 10.0, PAD + HEAD - 6.0);
    dash.line_to(W - PAD - 10.0, PAD + HEAD - 6.0);
    if let Some(dash) = dash.finish() {
        let stroke = Stroke {
            width: 1.5,
            dash: tiny_skia::StrokeDash::new(vec![4.0, 4.0], 0.0),
            ..Stroke::default()
        };
        pm.stroke_path(&dash, &paint(BORDER), &stroke, ts, None);
    }

    // Rows.
    let mut rows = Vec::with_capacity(model.items.len());
    let mut y = PAD + HEAD;
    for (i, item) in model.items.iter().enumerate() {
        if item.separated {
            let mut line = PathBuilder::new();
            line.move_to(PAD + 8.0, y + SEP / 2.0);
            line.line_to(W - PAD - 8.0, y + SEP / 2.0);
            if let Some(line) = line.finish() {
                pm.stroke_path(
                    &line,
                    &paint(SEP_LINE),
                    &Stroke {
                        width: 1.5,
                        ..Stroke::default()
                    },
                    ts,
                    None,
                );
            }
            y += SEP;
        }
        let row = Rect::from_xywh(PAD, y, W - 2.0 * PAD, ROW)?;
        let bg = if hover == Some(i) {
            if let Some(hl) = rounded(row.x(), row.y(), row.width(), row.height(), 13.0) {
                pm.fill_path(&hl, &paint(HOVER), FillRule::Winding, ts, None);
            }
            HOVER
        } else {
            BG
        };
        icon(
            &mut pm,
            item.icon,
            ts.pre_translate(PAD + 12.0, y + 12.0)
                .pre_scale(20.0 / 24.0, 20.0 / 24.0),
            bg,
        );
        text(&mut pm, face, &item.label, PAD + 44.0, y + 28.0, 15.0, INK);
        if let Some(hint) = &item.hint {
            let w = face.measure(hint, 12.0);
            text(
                &mut pm,
                face,
                hint,
                W - PAD - 12.0 - w,
                y + 27.0,
                12.0,
                MUTED,
            );
        }
        // Rows in the drawn pixmap's pixels, for hit testing.
        rows.push(Rect::from_xywh(
            (SHADOW + row.x()) * s,
            (SHADOW + row.y()) * s,
            row.width() * s,
            row.height() * s,
        )?);
        y += ROW;
    }
    Some(Drawn { pixmap: pm, rows })
}

type Cmds<'a> = &'a [(char, &'a [f32])];

/// A 24 × 24 line icon in the accent colour; `bg` is the row behind it.
fn icon(pm: &mut Pixmap, icon: Icon, ts: Transform, bg: [u8; 4]) {
    let line = Stroke {
        width: 2.0,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    let ink = paint(ACCENT);
    let stroke = |pm: &mut Pixmap, d: Cmds| {
        let mut pb = PathBuilder::new();
        for (cmd, a) in d {
            match cmd {
                'M' => pb.move_to(a[0], a[1]),
                'L' => pb.line_to(a[0], a[1]),
                'C' => pb.cubic_to(a[0], a[1], a[2], a[3], a[4], a[5]),
                'Z' => pb.close(),
                _ => {}
            }
        }
        if let Some(p) = pb.finish() {
            pm.stroke_path(&p, &ink, &line, ts, None);
        }
    };
    let ring = |pm: &mut Pixmap, x: f32, y: f32, r: f32| {
        if let Some(c) = PathBuilder::from_circle(x, y, r) {
            pm.stroke_path(&c, &ink, &line, ts, None);
        }
    };
    let dot = |pm: &mut Pixmap, x: f32, y: f32, r: f32, c: [u8; 4]| {
        if let Some(p) = PathBuilder::from_circle(x, y, r) {
            pm.fill_path(&p, &paint(c), FillRule::Winding, ts, None);
        }
    };
    match icon {
        Icon::Heart => stroke(
            pm,
            &[
                ('M', &[12.0, 20.0]),
                ('C', &[5.0, 15.5, 3.0, 12.0, 3.0, 9.0]),
                ('C', &[3.0, 6.0, 5.5, 4.0, 8.0, 4.0]),
                ('C', &[10.0, 4.0, 11.3, 5.2, 12.0, 6.5]),
                ('C', &[12.7, 5.2, 14.0, 4.0, 16.0, 4.0]),
                ('C', &[18.5, 4.0, 21.0, 6.0, 21.0, 9.0]),
                ('C', &[21.0, 12.0, 19.0, 15.5, 12.0, 20.0]),
                ('Z', &[]),
            ],
        ),
        Icon::Snack => {
            stroke(pm, &[('M', &[6.0, 12.0]), ('L', &[18.0, 12.0])]);
            for (x, y) in [(5.0, 9.5), (5.0, 14.5), (19.0, 9.5), (19.0, 14.5)] {
                ring(pm, x, y, 2.6);
            }
        }
        Icon::Pencil => {
            stroke(
                pm,
                &[
                    ('M', &[4.0, 20.0]),
                    ('L', &[8.0, 20.0]),
                    ('L', &[19.0, 9.0]),
                    ('L', &[15.0, 5.0]),
                    ('L', &[4.0, 16.0]),
                    ('Z', &[]),
                ],
            );
            stroke(pm, &[('M', &[13.5, 6.5]), ('L', &[17.5, 10.5])]);
        }
        Icon::Palette => {
            ring(pm, 12.0, 12.0, 8.5);
            for (x, y) in [(8.0, 10.0), (11.0, 7.0), (15.5, 8.0), (9.5, 14.5)] {
                dot(pm, x, y, 1.2, ACCENT);
            }
        }
        Icon::Moon => {
            dot(pm, 11.0, 13.0, 7.5, ACCENT);
            dot(pm, 15.5, 9.0, 6.5, bg);
        }
        Icon::Sun => {
            ring(pm, 12.0, 12.0, 4.0);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (sx, sy) = (12.0 + 7.0 * a.cos(), 12.0 + 7.0 * a.sin());
                let (ex, ey) = (12.0 + 9.5 * a.cos(), 12.0 + 9.5 * a.sin());
                stroke(pm, &[('M', &[sx, sy]), ('L', &[ex, ey])]);
            }
        }
        Icon::Hide => {
            stroke(
                pm,
                &[
                    ('M', &[2.5, 12.0]),
                    ('C', &[6.0, 6.0, 18.0, 6.0, 21.5, 12.0]),
                    ('C', &[18.0, 18.0, 6.0, 18.0, 2.5, 12.0]),
                    ('Z', &[]),
                ],
            );
            stroke(pm, &[('M', &[4.0, 4.0]), ('L', &[20.0, 20.0])]);
        }
        Icon::Sliders => {
            stroke(pm, &[('M', &[4.0, 7.0]), ('L', &[14.0, 7.0])]);
            stroke(pm, &[('M', &[18.0, 7.0]), ('L', &[20.0, 7.0])]);
            stroke(pm, &[('M', &[4.0, 17.0]), ('L', &[8.0, 17.0])]);
            stroke(pm, &[('M', &[12.0, 17.0]), ('L', &[20.0, 17.0])]);
            ring(pm, 16.0, 7.0, 2.0);
            ring(pm, 10.0, 17.0, 2.0);
        }
        Icon::Exit => {
            stroke(
                pm,
                &[
                    ('M', &[10.0, 4.0]),
                    ('L', &[5.0, 4.0]),
                    ('L', &[5.0, 20.0]),
                    ('L', &[10.0, 20.0]),
                ],
            );
            stroke(
                pm,
                &[
                    ('M', &[14.0, 8.0]),
                    ('L', &[18.0, 12.0]),
                    ('L', &[14.0, 16.0]),
                ],
            );
            stroke(pm, &[('M', &[18.0, 12.0]), ('L', &[9.0, 12.0])]);
        }
        Icon::Bye => {
            // A palm with four fingers up, tilted as it waves, and two
            // little motion marks beside it.
            stroke(
                pm,
                &[
                    ('M', &[7.0, 13.0]),
                    ('C', &[6.0, 17.0, 8.5, 20.5, 12.5, 20.0]),
                    ('C', &[16.0, 19.5, 17.5, 17.0, 17.0, 13.5]),
                    ('L', &[17.5, 9.5]),
                ],
            );
            for (x0, y0, x1, y1) in [
                (8.0, 13.0, 7.0, 6.0),
                (10.5, 12.0, 10.0, 4.0),
                (13.0, 12.0, 13.5, 4.5),
                (15.5, 12.5, 16.5, 6.5),
            ] {
                stroke(pm, &[('M', &[x0, y0]), ('L', &[x1, y1])]);
            }
            stroke(
                pm,
                &[
                    ('M', &[20.0, 5.0]),
                    ('C', &[21.5, 6.5, 21.5, 8.5, 20.5, 10.0]),
                ],
            );
            stroke(
                pm,
                &[
                    ('M', &[3.5, 15.0]),
                    ('C', &[2.5, 16.5, 2.8, 18.5, 4.0, 19.5]),
                ],
            );
        }
    }
}

/// The row under pixel `(x, y)` of a drawn menu.
pub fn row_at(drawn: &Drawn, x: f32, y: f32) -> Option<usize> {
    drawn
        .rows
        .iter()
        .position(|r| x >= r.left() && x < r.right() && y >= r.top() && y < r.bottom())
}
