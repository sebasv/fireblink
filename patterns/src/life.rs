//! Board evolution rules and the heat (afterglow) buffer.

use crate::{COLS, N_LEDS, ROWS};

/// How the board evolves each frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rule {
    /// Conway's Game of Life, B3/S23.
    Conway,
    /// Spreading fire: burning cells ignite neighbours, then burn out;
    /// occasional lightning keeps the board alive.
    Wildfire,
}

/// How fast a dead cell's afterglow cools, as a fraction of 256 per frame.
// ponytail: ~0.8/frame → trails last ~15 frames; drop it for longer comet tails.
pub(crate) const BASE_DECAY: u16 = 205;
/// Slowest cooling, reached at full audio level (louder music = longer tails).
pub(crate) const MAX_DECAY: u16 = 250;

/// Chance (out of `LIGHTNING`) that an idle cell spontaneously ignites in
/// wildfire mode. Higher denominator = rarer sparks.
const LIGHTNING: u32 = 4096;

pub(crate) fn step(rule: Rule, state: &[bool; N_LEDS], rng: &mut u32) -> [bool; N_LEDS] {
    match rule {
        Rule::Conway => conway_update(state),
        Rule::Wildfire => wildfire_update(state, rng),
    }
}

/// Hard-set heat from a board: live cells full, everything else dark. Used to
/// (re)seed, clearing any lingering afterglow.
pub(crate) fn seed_heat(heat: &mut [u8; N_LEDS], state: &[bool; N_LEDS]) {
    for (h, &alive) in heat.iter_mut().zip(state.iter()) {
        *h = if alive { u8::MAX } else { 0 };
    }
}

/// Live cells map to full heat; dead cells cool by `decay / 256` per frame.
pub(crate) fn decay_heat(heat: &mut [u8; N_LEDS], next: &[bool; N_LEDS], decay: u16) {
    for (h, &alive) in heat.iter_mut().zip(next.iter()) {
        *h = if alive {
            u8::MAX
        } else {
            (*h as u16 * decay / 256) as u8
        };
    }
}

/// Flare every cell's heat up — a beat pulse across the whole board.
pub(crate) fn flare(heat: &mut [u8; N_LEDS], amount: u8) {
    for h in heat.iter_mut() {
        *h = h.saturating_add(amount);
    }
}

/// Fraction of cells that changed between two boards, scaled to `0..=255`.
pub(crate) fn churn(a: &[bool; N_LEDS], b: &[bool; N_LEDS]) -> u8 {
    let changed = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    (changed * 255 / N_LEDS) as u8
}

/// xorshift32 — good enough randomness for lightning strikes on a hobby panel.
pub(crate) fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

pub(crate) fn conway_update(state: &[bool; N_LEDS]) -> [bool; N_LEDS] {
    let mut buf = [false; N_LEDS];
    for i in 0..ROWS {
        for j in 0..COLS {
            let l = (COLS + j - 1) % COLS;
            let r = (COLS + j + 1) % COLS;
            let u = (ROWS + i - 1) % ROWS;
            let d = (ROWS + i + 1) % ROWS;
            let n_neighbors = state[u * COLS + l] as u8
                + state[u * COLS + j] as u8
                + state[u * COLS + r] as u8
                + state[i * COLS + l] as u8
                + state[i * COLS + r] as u8
                + state[d * COLS + l] as u8
                + state[d * COLS + j] as u8
                + state[d * COLS + r] as u8;
            buf[i * COLS + j] = if state[i * COLS + j] {
                (n_neighbors == 2) | (n_neighbors == 3)
            } else {
                n_neighbors == 3
            };
        }
    }
    buf
}

fn wildfire_update(state: &[bool; N_LEDS], rng: &mut u32) -> [bool; N_LEDS] {
    let mut buf = [false; N_LEDS];
    for i in 0..ROWS {
        for j in 0..COLS {
            let idx = i * COLS + j;
            if state[idx] {
                continue; // burning → burnt out next frame
            }
            let l = (COLS + j - 1) % COLS;
            let r = (COLS + j + 1) % COLS;
            let u = (ROWS + i - 1) % ROWS;
            let d = (ROWS + i + 1) % ROWS;
            let burning = state[u * COLS + j]
                || state[d * COLS + j]
                || state[i * COLS + l]
                || state[i * COLS + r];
            buf[idx] = burning || xorshift(rng).is_multiple_of(LIGHTNING);
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(cells: &[(usize, usize)]) -> [bool; N_LEDS] {
        let mut state = [false; N_LEDS];
        for &(x, y) in cells {
            state[y * COLS + x] = true;
        }
        state
    }

    fn live(state: &[bool; N_LEDS]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for y in 0..ROWS {
            for x in 0..COLS {
                if state[y * COLS + x] {
                    out.push((x, y));
                }
            }
        }
        out
    }

    #[test]
    fn conway_empty_stays_empty() {
        assert!(live(&conway_update(&[false; N_LEDS])).is_empty());
    }

    #[test]
    fn conway_block_is_stable() {
        let block = board(&[(1, 1), (2, 1), (1, 2), (2, 2)]);
        assert_eq!(live(&conway_update(&block)), live(&block));
    }

    #[test]
    fn conway_blinker_oscillates_with_period_two() {
        let vertical = board(&[(7, 4), (7, 5), (7, 6)]);
        let horizontal = conway_update(&vertical);
        assert_eq!(live(&horizontal), vec![(6, 5), (7, 5), (8, 5)]);
        assert_eq!(live(&conway_update(&horizontal)), live(&vertical));
    }

    #[test]
    fn conway_folds_over_left_right_edge() {
        let seam = board(&[(COLS - 1, 5), (0, 5), (1, 5)]);
        assert_eq!(live(&conway_update(&seam)), vec![(0, 4), (0, 5), (0, 6)]);
    }

    #[test]
    fn conway_folds_over_top_bottom_edge() {
        let seam = board(&[(7, ROWS - 1), (7, 0), (7, 1)]);
        assert_eq!(live(&conway_update(&seam)), vec![(6, 0), (7, 0), (8, 0)]);
    }

    #[test]
    fn conway_folds_over_corner_diagonally() {
        let corners = board(&[(COLS - 1, ROWS - 1), (0, ROWS - 1), (COLS - 1, 0)]);
        assert!(
            conway_update(&corners)[0],
            "corner (0,0) should be born from its three wrapped neighbours"
        );
    }

    #[test]
    fn decay_heat_leaves_a_fading_ember() {
        let mut heat = [0u8; N_LEDS];
        heat[5] = u8::MAX;
        decay_heat(&mut heat, &[false; N_LEDS], BASE_DECAY); // cell 5 is now dead
        assert!(0 < heat[5] && heat[5] < u8::MAX, "dead cell should linger");
    }

    #[test]
    fn wildfire_burns_out_and_spreads() {
        let mut rng = 0x1234_5678;
        let spark = board(&[(7, 5)]);
        let next = wildfire_update(&spark, &mut rng);
        assert!(!next[5 * COLS + 7], "the burning cell should burn out");
        // its four orthogonal neighbours should catch
        assert!(
            next[5 * COLS + 6] && next[5 * COLS + 8] && next[4 * COLS + 7] && next[6 * COLS + 7]
        );
    }
}
