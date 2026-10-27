//! What pets stand on (PLAN §1.3): the bottom of each monitor's work area,
//! and the top edges of windows where no other window covers them. Pets walk
//! along these ledges, fall off their ends, land on the highest one below
//! and ride along when a window moves.
//!
//! Pure: the platform layer lists monitors and windows; everything here is
//! in physical pixels, y down.

use crate::motion::Area;

/// A window as the platform sees it, in front-to-back order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Win {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// What a ledge belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// The bottom of monitor `n`'s work area.
    Floor(usize),
    /// The top edge of a window.
    Window(u64),
}

/// A stretch of edge a pet can stand on: feet at `y`, from `x0` to `x1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ledge {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
    pub on: Support,
}

impl Ledge {
    pub fn holds(&self, x: f32) -> bool {
        x >= self.x0 && x <= self.x1
    }
}

/// Ledges narrower than this are not worth standing on.
const MIN_LEDGE: f32 = 24.0;
/// Two ledges this close in height are one walkable surface.
const SAME_HEIGHT: f32 = 2.0;

#[derive(Clone, Debug, Default)]
pub struct Terrain {
    floors: Vec<Area>,
    windows: Vec<Win>,
    ledges: Vec<Ledge>,
}

impl Terrain {
    /// Just the floor of one work area (no windows).
    pub fn flat(area: Area) -> Self {
        Self::new(vec![area], &[], 0.0)
    }

    /// `floors`: each monitor's work area. `windows`: front to back, the
    /// pets' own excluded. A window's top edge counts where nothing in front
    /// covers it and there is `headroom` above it inside a work area (a
    /// maximised window's top is not a ledge).
    pub fn new(floors: Vec<Area>, windows: &[Win], headroom: f32) -> Self {
        let mut ledges: Vec<Ledge> = floors
            .iter()
            .enumerate()
            .map(|(i, a)| Ledge {
                y: a.y + a.h,
                x0: a.x,
                x1: a.x + a.w,
                on: Support::Floor(i),
            })
            .collect();
        for (i, w) in windows.iter().enumerate() {
            let y = w.y;
            // Where the edge is inside a work area, with room above it.
            let mut spans: Vec<(f32, f32)> = floors
                .iter()
                .filter(|a| y - headroom >= a.y && y < a.y + a.h)
                .map(|a| (w.x.max(a.x), (w.x + w.w).min(a.x + a.w)))
                .filter(|(a, b)| b > a)
                .collect();
            // Minus what windows in front cover at that height.
            for front in &windows[..i] {
                if front.y <= y && front.y + front.h > y {
                    spans = cut(&spans, front.x, front.x + front.w);
                }
            }
            ledges.extend(
                spans
                    .into_iter()
                    .filter(|(a, b)| b - a >= MIN_LEDGE)
                    .map(|(x0, x1)| Ledge {
                        y,
                        x0,
                        x1,
                        on: Support::Window(w.id),
                    }),
            );
        }
        Self {
            floors,
            windows: windows.to_vec(),
            ledges,
        }
    }

    pub fn ledges(&self) -> &[Ledge] {
        &self.ledges
    }

    pub fn window(&self, id: u64) -> Option<Win> {
        self.windows.iter().find(|w| w.id == id).copied()
    }

    /// Left and right of everything walkable.
    pub fn bounds(&self) -> (f32, f32) {
        let l = self.floors.iter().map(|a| a.x).fold(f32::MAX, f32::min);
        let r = self
            .floors
            .iter()
            .map(|a| a.x + a.w)
            .fold(f32::MIN, f32::max);
        if l <= r { (l, r) } else { (0.0, 0.0) }
    }

    /// Is `x` over some monitor?
    pub fn over_screen(&self, x: f32) -> bool {
        self.floors.iter().any(|a| x >= a.x && x <= a.x + a.w)
    }

    /// The ledge belonging to `on` that holds `x`, if any.
    pub fn ledge_on(&self, on: Support, x: f32) -> Option<Ledge> {
        self.ledges
            .iter()
            .find(|l| l.on == on && l.holds(x))
            .copied()
    }

    /// Any ledge at height `y` (give or take a couple of pixels) holding
    /// `x`: walking from one window onto the next at the same height, or
    /// across monitors.
    pub fn ledge_level(&self, x: f32, y: f32) -> Option<Ledge> {
        self.ledges
            .iter()
            .filter(|l| l.holds(x) && (l.y - y).abs() <= SAME_HEIGHT)
            .min_by(|a, b| (a.y - y).abs().total_cmp(&(b.y - y).abs()))
            .copied()
    }

    /// The first ledge feet at `x` cross falling from `y0` down to `y1`.
    pub fn landing(&self, x: f32, y0: f32, y1: f32) -> Option<Ledge> {
        self.ledges
            .iter()
            .filter(|l| l.holds(x) && l.y >= y0 && l.y <= y1)
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .copied()
    }

    /// The floor under `x`, or the nearest one: where a pet ends up if it
    /// somehow falls past everything.
    pub fn floor_under(&self, x: f32) -> Option<Ledge> {
        self.ledges
            .iter()
            .filter(|l| matches!(l.on, Support::Floor(_)))
            .min_by(|a, b| {
                let d = |l: &Ledge| (l.x0 - x).max(x - l.x1).max(0.0);
                d(a).total_cmp(&d(b))
            })
            .copied()
    }

    /// The nearest ledge above feet at (`x`, `y`) that a pet could hop up
    /// onto: between `low` and `high` pixels higher, within `reach` sideways.
    /// Returns the ledge and the x to land at.
    pub fn ledge_above(
        &self,
        x: f32,
        y: f32,
        low: f32,
        high: f32,
        reach: f32,
    ) -> Option<(Ledge, f32)> {
        self.ledges
            .iter()
            .filter(|l| matches!(l.on, Support::Window(_)))
            .filter(|l| y - l.y >= low && y - l.y <= high)
            .filter_map(|l| {
                // Land a little way in from the near end.
                let margin = ((l.x1 - l.x0) / 2.0).min(40.0);
                let at = x.clamp(l.x0 + margin, l.x1 - margin);
                ((at - x).abs() <= reach).then_some((*l, at))
            })
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
    }
}

/// `spans` minus the interval `[a, b]`.
fn cut(spans: &[(f32, f32)], a: f32, b: f32) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for &(x0, x1) in spans {
        if b <= x0 || a >= x1 {
            out.push((x0, x1));
            continue;
        }
        if a > x0 {
            out.push((x0, a));
        }
        if b < x1 {
            out.push((b, x1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Area = Area {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1032.0,
    };

    fn win(id: u64, x: f32, y: f32, w: f32, h: f32) -> Win {
        Win { id, x, y, w, h }
    }

    #[test]
    fn a_window_top_is_a_ledge_where_nothing_covers_it() {
        // Window 2 behind window 1, which covers its middle.
        let t = Terrain::new(
            vec![SCREEN],
            &[
                win(1, 600.0, 300.0, 400.0, 500.0),
                win(2, 400.0, 400.0, 800.0, 300.0),
            ],
            200.0,
        );
        let on2: Vec<(f32, f32)> = t
            .ledges()
            .iter()
            .filter(|l| l.on == Support::Window(2))
            .map(|l| (l.x0, l.x1))
            .collect();
        assert_eq!(on2, vec![(400.0, 600.0), (1000.0, 1200.0)]);
        assert!(t.ledge_on(Support::Window(1), 800.0).is_some());
        assert!(t.ledge_on(Support::Floor(0), 10.0).is_some());
    }

    #[test]
    fn a_maximised_window_top_is_no_ledge() {
        let t = Terrain::new(vec![SCREEN], &[win(1, 0.0, 0.0, 1920.0, 1032.0)], 200.0);
        assert!(t.ledge_on(Support::Window(1), 900.0).is_none());
    }

    #[test]
    fn falling_lands_on_the_first_ledge_below() {
        let t = Terrain::new(
            vec![SCREEN],
            &[
                win(1, 100.0, 500.0, 400.0, 300.0),
                win(2, 100.0, 700.0, 400.0, 200.0),
            ],
            200.0,
        );
        let l = t.landing(300.0, 200.0, 2000.0).unwrap();
        assert_eq!(l.on, Support::Window(1));
        // Already below window 1: window 2 is covered there, so the floor.
        let l = t.landing(300.0, 600.0, 2000.0).unwrap();
        assert_eq!(l.on, Support::Floor(0));
    }

    #[test]
    fn two_monitors_side_by_side_are_one_walk() {
        let right = Area {
            x: 1920.0,
            ..SCREEN
        };
        let t = Terrain::new(vec![SCREEN, right], &[], 0.0);
        assert_eq!(t.bounds(), (0.0, 3840.0));
        let l = t.ledge_level(2500.0, 1032.0).unwrap();
        assert_eq!(l.on, Support::Floor(1));
    }

    #[test]
    fn hopping_up_finds_a_low_window_nearby() {
        let t = Terrain::new(vec![SCREEN], &[win(1, 700.0, 880.0, 500.0, 200.0)], 200.0);
        let (l, x) = t.ledge_above(650.0, 1032.0, 40.0, 260.0, 200.0).unwrap();
        assert_eq!(l.on, Support::Window(1));
        assert!(x > 700.0 && x < 800.0, "{x}");
        assert!(t.ledge_above(100.0, 1032.0, 40.0, 260.0, 200.0).is_none());
    }
}
