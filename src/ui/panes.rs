//! Split layouts for session tabs: which sessions are shown together, where
//! each pane goes, and how focus moves between them.

use ratatui::layout::{Constraint, Layout as Split, Rect};

use crate::config::Orientation;

/// Columns needed to put two panels side by side.
pub const SIDE_BY_SIDE: u16 = 80;
/// Rows needed to put one panel above another.
pub const STACKED: u16 = 30;

/// How the screen is arranged. Worked out from the terminal size on every
/// frame, so a resize or a rotated screen re-flows at once.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Shape {
    /// Landscape: panels side by side.
    #[default]
    Wide,
    /// Portrait: panels stacked, each using the full width.
    Tall,
    /// Too small for two panels either way: one at a time.
    Compact,
}

impl Shape {
    /// Portrait when the terminal has fewer columns than rows, landscape
    /// otherwise, unless the config says which. Either falls back to what
    /// actually fits: a landscape window under 80 columns stacks if it has
    /// the rows, and a window with neither the columns nor the rows shows
    /// one panel at a time.
    pub fn of(width: u16, height: u16, prefer: Orientation) -> Self {
        let portrait = match prefer {
            Orientation::Auto => width < height,
            Orientation::Portrait => true,
            Orientation::Landscape => false,
        };
        match (portrait, width >= SIDE_BY_SIDE, height >= STACKED) {
            (true, _, true) | (false, false, true) => Self::Tall,
            (false, true, _) | (true, true, false) => Self::Wide,
            (_, false, false) => Self::Compact,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Layout {
    #[default]
    Single,
    Columns2,
    Grid,
    Columns3,
}

impl Layout {
    /// Number of pane slots.
    pub fn panes(self) -> usize {
        match self {
            Self::Single => 1,
            Self::Columns2 => 2,
            Self::Columns3 => 3,
            Self::Grid => 4,
        }
    }

    /// Panes per row. Stacked (portrait) every pane gets the full width.
    fn columns(self, stacked: bool) -> usize {
        match self {
            _ if stacked => 1,
            Self::Grid => 2,
            other => other.panes(),
        }
    }

    pub fn name(self, stacked: bool) -> &'static str {
        match (self, stacked) {
            (Self::Single, _) => "single",
            (Self::Columns2, false) => "2 columns",
            (Self::Grid, false) => "2x2 grid",
            (Self::Columns3, false) => "3 columns",
            (Self::Columns2, true) => "2 rows",
            (Self::Columns3, true) => "3 rows",
            (Self::Grid, true) => "4 rows",
        }
    }

    /// The order the layout key cycles through.
    pub fn next(self) -> Self {
        match self {
            Self::Single => Self::Columns2,
            Self::Columns2 => Self::Grid,
            Self::Grid => Self::Columns3,
            Self::Columns3 => Self::Single,
        }
    }

    /// The smallest layout that shows `sessions` at once.
    pub fn fit(sessions: usize) -> Self {
        match sessions {
            0 | 1 => Self::Single,
            2 => Self::Columns2,
            3 => Self::Columns3,
            _ => Self::Grid,
        }
    }

    /// One rectangle per slot, left to right, top to bottom. They tile `area`.
    pub fn rects(self, area: Rect, stacked: bool) -> Vec<Rect> {
        let cols = self.columns(stacked);
        let rows = self.panes() / cols;
        let even = |n: usize| vec![Constraint::Ratio(1, n as u32); n];
        let rows = Split::vertical(even(rows)).split(area);
        let cells = rows
            .iter()
            .flat_map(|row| Split::horizontal(even(cols)).split(*row).to_vec());
        cells.collect()
    }

    /// The slot one step from `slot` in a vim direction (`h` `j` `k` `l`),
    /// if it exists and one of the `shown` sessions is in it.
    pub fn neighbor(
        self,
        slot: usize,
        direction: char,
        shown: usize,
        stacked: bool,
    ) -> Option<usize> {
        let cols = self.columns(stacked);
        let to = match direction {
            'h' if !slot.is_multiple_of(cols) => slot - 1,
            'l' if slot % cols + 1 < cols => slot + 1,
            'k' if slot >= cols => slot - cols,
            'j' => slot + cols,
            _ => return None,
        };
        (to < shown.min(self.panes())).then_some(to)
    }
}

/// Index of the first session of the window of `size` sessions shown
/// together. The `previous` window stays put while the active session (at
/// index `at` of `sessions`) is inside it, otherwise it slides just far
/// enough; it never runs past the end.
pub fn window(previous: usize, at: usize, size: usize, sessions: usize) -> usize {
    let start = if at < previous {
        at
    } else if at >= previous + size {
        at + 1 - size
    } else {
        previous
    };
    start.min(sessions.saturating_sub(size))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Layout; 4] = [
        Layout::Single,
        Layout::Columns2,
        Layout::Grid,
        Layout::Columns3,
    ];

    #[test]
    fn panes_tile_the_area_without_gaps_or_overlap() {
        for area in [Rect::new(0, 1, 120, 30), Rect::new(3, 2, 101, 27)] {
            for layout in ALL {
                let rects = layout.rects(area, false);
                assert_eq!(rects.len(), layout.panes(), "{layout:?}");
                let covered: u32 = rects
                    .iter()
                    .map(|r| u32::from(r.width) * u32::from(r.height))
                    .sum();
                assert_eq!(
                    covered,
                    u32::from(area.width) * u32::from(area.height),
                    "{layout:?}"
                );
                for (i, a) in rects.iter().enumerate() {
                    assert_eq!(
                        a.intersection(area),
                        *a,
                        "{layout:?} pane {i} leaves the area"
                    );
                    for b in &rects[i + 1..] {
                        assert!(
                            a.intersection(*b).is_empty(),
                            "{layout:?}: {a} overlaps {b}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn geometry_of_each_preset() {
        let area = Rect::new(0, 1, 120, 30);
        assert_eq!(Layout::Single.rects(area, false), [area]);
        assert_eq!(
            Layout::Columns2.rects(area, false),
            [Rect::new(0, 1, 60, 30), Rect::new(60, 1, 60, 30)]
        );
        assert_eq!(
            Layout::Columns3.rects(area, false),
            [
                Rect::new(0, 1, 40, 30),
                Rect::new(40, 1, 40, 30),
                Rect::new(80, 1, 40, 30)
            ]
        );
        assert_eq!(
            Layout::Grid.rects(area, false),
            [
                Rect::new(0, 1, 60, 15),
                Rect::new(60, 1, 60, 15),
                Rect::new(0, 16, 60, 15),
                Rect::new(60, 16, 60, 15)
            ]
        );
    }

    #[test]
    fn focus_moves_like_vim_windows() {
        let g = Layout::Grid;
        // 0 1
        // 2 3
        assert_eq!(
            (g.neighbor(0, 'l', 4, false), g.neighbor(0, 'j', 4, false)),
            (Some(1), Some(2))
        );
        assert_eq!(
            (g.neighbor(3, 'h', 4, false), g.neighbor(3, 'k', 4, false)),
            (Some(2), Some(1))
        );
        // No wrapping at the edges.
        assert_eq!(
            (g.neighbor(0, 'h', 4, false), g.neighbor(0, 'k', 4, false)),
            (None, None)
        );
        assert_eq!(
            (g.neighbor(1, 'l', 4, false), g.neighbor(2, 'j', 4, false)),
            (None, None)
        );
        // Only three sessions: the fourth slot is empty and cannot take focus.
        assert_eq!(
            (g.neighbor(2, 'l', 3, false), g.neighbor(1, 'j', 3, false)),
            (None, None)
        );

        let c = Layout::Columns3;
        assert_eq!(
            (c.neighbor(1, 'h', 3, false), c.neighbor(1, 'l', 3, false)),
            (Some(0), Some(2))
        );
        assert_eq!(
            (c.neighbor(1, 'j', 3, false), c.neighbor(1, 'k', 3, false)),
            (None, None)
        );
        assert_eq!(c.neighbor(1, 'l', 2, false), None);
        assert_eq!(Layout::Single.neighbor(0, 'l', 5, false), None);
        assert_eq!(g.neighbor(0, 'x', 4, false), None);
    }

    #[test]
    fn shape_follows_the_terminal_size() {
        use Orientation::*;
        let shape = |w, h| Shape::of(w, h, Auto);
        // Landscape, from the smallest supported size up.
        assert_eq!(shape(80, 24), Shape::Wide);
        assert_eq!(shape(120, 30), Shape::Wide);
        assert_eq!(shape(200, 50), Shape::Wide);
        // Portrait: a phone, a rotated monitor.
        assert_eq!(shape(60, 50), Shape::Tall);
        assert_eq!(shape(45, 90), Shape::Tall);
        assert_eq!(shape(100, 120), Shape::Tall);
        // Landscape by the numbers, but too narrow for two columns: stack if there are rows.
        assert_eq!(shape(70, 40), Shape::Tall);
        assert_eq!(shape(79, 30), Shape::Tall);
        // Small windows: one panel at a time.
        assert_eq!(shape(60, 20), Shape::Compact);
        assert_eq!(shape(79, 29), Shape::Compact);
        assert_eq!(shape(20, 25), Shape::Compact);
        for tiny in [(0, 0), (1, 1), (10, 3)] {
            assert_eq!(shape(tiny.0, tiny.1), Shape::Compact, "{tiny:?}");
        }
        // The config can force an orientation, as long as it fits.
        assert_eq!(Shape::of(200, 50, Portrait), Shape::Tall);
        assert_eq!(Shape::of(100, 120, Landscape), Shape::Wide);
        assert_eq!(Shape::of(120, 24, Portrait), Shape::Wide);
        assert_eq!(Shape::of(60, 50, Landscape), Shape::Tall);
        assert_eq!(Shape::of(60, 20, Portrait), Shape::Compact);
    }

    #[test]
    fn stacked_panes_use_the_full_width() {
        let area = Rect::new(0, 1, 60, 45);
        for layout in ALL {
            let rects = layout.rects(area, true);
            assert_eq!(rects.len(), layout.panes(), "{layout:?}");
            assert!(
                rects.iter().all(|r| r.width == 60 && r.x == 0),
                "{layout:?}"
            );
            let rows: u16 = rects.iter().map(|r| r.height).sum();
            assert_eq!(rows, 45, "{layout:?}");
            assert!(
                rects.windows(2).all(|w| w[0].bottom() == w[1].y),
                "{layout:?}"
            );
        }
        assert_eq!(
            Layout::Columns3.rects(area, true),
            [
                Rect::new(0, 1, 60, 15),
                Rect::new(0, 16, 60, 15),
                Rect::new(0, 31, 60, 15)
            ]
        );
        // Focus then moves with j / k only.
        let c = Layout::Columns3;
        assert_eq!(
            (c.neighbor(0, 'j', 3, true), c.neighbor(2, 'k', 3, true)),
            (Some(1), Some(1))
        );
        assert_eq!(
            (c.neighbor(1, 'h', 3, true), c.neighbor(1, 'l', 3, true)),
            (None, None)
        );
        assert_eq!(
            (c.neighbor(2, 'j', 3, true), c.neighbor(0, 'k', 3, true)),
            (None, None)
        );
        assert_eq!(Layout::Grid.neighbor(1, 'j', 4, true), Some(2));
        assert_eq!((c.name(false), c.name(true)), ("3 columns", "3 rows"));
    }

    #[test]
    fn layouts_cycle_and_fit() {
        let mut seen = vec![Layout::Single];
        while seen.len() < 5 {
            seen.push(seen[seen.len() - 1].next());
        }
        assert_eq!(seen[..4], ALL);
        assert_eq!(seen[4], Layout::Single);
        let fit: Vec<_> = (0..=6).map(Layout::fit).collect();
        use Layout::*;
        assert_eq!(fit, [Single, Single, Columns2, Columns3, Grid, Grid, Grid]);
    }

    #[test]
    fn window_follows_the_active_session() {
        // Seven sessions, three shown: nothing moves while focus stays inside.
        assert_eq!(window(0, 0, 3, 7), 0);
        assert_eq!(window(0, 2, 3, 7), 0);
        // Stepping past either edge slides by one.
        assert_eq!(window(0, 3, 3, 7), 1);
        assert_eq!(window(2, 1, 3, 7), 1);
        // Jumping far away puts the active session at the near edge.
        assert_eq!(window(0, 6, 3, 7), 4);
        assert_eq!(window(4, 0, 3, 7), 0);
        // A group opened at index 2 is shown from its first session.
        assert_eq!(window(2, 2, 3, 5), 2);
        // Sessions closed: the window backs up instead of showing empty panes.
        assert_eq!(window(4, 3, 3, 4), 1);
        assert_eq!(window(4, 0, 3, 2), 0);
        assert_eq!(window(0, 0, 4, 0), 0);
    }
}
