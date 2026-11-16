//! Split layouts for session tabs: which sessions are shown together, where
//! each pane goes, and how focus moves between them.

use ratatui::layout::{Constraint, Layout as Split, Rect};

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

    fn columns(self) -> usize {
        match self {
            Self::Grid => 2,
            other => other.panes(),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Columns2 => "2 columns",
            Self::Grid => "2x2 grid",
            Self::Columns3 => "3 columns",
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
    pub fn rects(self, area: Rect) -> Vec<Rect> {
        let cols = self.columns();
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
    pub fn neighbor(self, slot: usize, direction: char, shown: usize) -> Option<usize> {
        let cols = self.columns();
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
                let rects = layout.rects(area);
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
        assert_eq!(Layout::Single.rects(area), [area]);
        assert_eq!(
            Layout::Columns2.rects(area),
            [Rect::new(0, 1, 60, 30), Rect::new(60, 1, 60, 30)]
        );
        assert_eq!(
            Layout::Columns3.rects(area),
            [
                Rect::new(0, 1, 40, 30),
                Rect::new(40, 1, 40, 30),
                Rect::new(80, 1, 40, 30)
            ]
        );
        assert_eq!(
            Layout::Grid.rects(area),
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
            (g.neighbor(0, 'l', 4), g.neighbor(0, 'j', 4)),
            (Some(1), Some(2))
        );
        assert_eq!(
            (g.neighbor(3, 'h', 4), g.neighbor(3, 'k', 4)),
            (Some(2), Some(1))
        );
        // No wrapping at the edges.
        assert_eq!((g.neighbor(0, 'h', 4), g.neighbor(0, 'k', 4)), (None, None));
        assert_eq!((g.neighbor(1, 'l', 4), g.neighbor(2, 'j', 4)), (None, None));
        // Only three sessions: the fourth slot is empty and cannot take focus.
        assert_eq!((g.neighbor(2, 'l', 3), g.neighbor(1, 'j', 3)), (None, None));

        let c = Layout::Columns3;
        assert_eq!(
            (c.neighbor(1, 'h', 3), c.neighbor(1, 'l', 3)),
            (Some(0), Some(2))
        );
        assert_eq!((c.neighbor(1, 'j', 3), c.neighbor(1, 'k', 3)), (None, None));
        assert_eq!(c.neighbor(1, 'l', 2), None);
        assert_eq!(Layout::Single.neighbor(0, 'l', 5), None);
        assert_eq!(g.neighbor(0, 'x', 4), None);
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
