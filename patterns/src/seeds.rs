//! Catalogue of Conway starting patterns. See `IDEAS.md` for shapes.

use crate::{COLS, N_LEDS, ROWS};

/// A named starting pattern. Cell coordinates are `(x, y)` on the 15×10 board.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Seed {
    Empty,
    Glider,
    Lwss,
    RPentomino,
    Acorn,
    Pentadecathlon,
    Beacon,
    Toad,
}

impl Seed {
    /// Cyclable patterns for the seed control — `Empty` is deliberately excluded
    /// so the button never lands on a blank board.
    pub const ALL: [Seed; 7] = [
        Seed::Glider,
        Seed::Lwss,
        Seed::RPentomino,
        Seed::Acorn,
        Seed::Pentadecathlon,
        Seed::Beacon,
        Seed::Toad,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    pub fn next(self) -> Seed {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Seed {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    const fn cells(self) -> &'static [(usize, usize)] {
        match self {
            Seed::Empty => &[],
            Seed::Glider => &[(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)],
            Seed::Lwss => &[
                (5, 3),
                (8, 3),
                (9, 4),
                (5, 5),
                (9, 5),
                (6, 6),
                (7, 6),
                (8, 6),
                (9, 6),
            ],
            Seed::RPentomino => &[(8, 4), (9, 4), (7, 5), (8, 5), (8, 6)],
            Seed::Acorn => &[(5, 3), (7, 4), (4, 5), (5, 5), (8, 5), (9, 5), (10, 5)],
            Seed::Pentadecathlon => &[
                (2, 4),
                (7, 4),
                (0, 5),
                (1, 5),
                (3, 5),
                (4, 5),
                (5, 5),
                (6, 5),
                (8, 5),
                (9, 5),
                (2, 6),
                (7, 6),
            ],
            Seed::Beacon => &[
                (5, 3),
                (6, 3),
                (5, 4),
                (6, 4),
                (7, 5),
                (8, 5),
                (7, 6),
                (8, 6),
            ],
            Seed::Toad => &[(6, 4), (7, 4), (8, 4), (5, 5), (6, 5), (7, 5)],
        }
    }
}

/// Build a board from a seed, shifted by `(ox, oy)` cells (wraps toroidally).
pub fn board(seed: Seed, ox: usize, oy: usize) -> [bool; N_LEDS] {
    let mut state = [false; N_LEDS];
    for &(x, y) in seed.cells() {
        state[((y + oy) % ROWS) * COLS + (x + ox) % COLS] = true;
    }
    state
}
