//! Board evolution rules and the heat (afterglow) buffer.

use crate::{COLS, N_LEDS, ROWS, Seed};
use libm::{logf, sqrtf};

/// How the board evolves each frame. Rule-specific tuning rides along in the
/// variant, so a rule is only ever handed the knobs it actually uses — and a
/// rule fully describes a channel, which is what lets the two channels run
/// different rules.
#[derive(Clone, Copy, PartialEq)]
pub enum Rule {
    /// Conway's Game of Life, B3/S23, started from `seed`.
    Conway { seed: Seed },
    /// Spreading fire on an excitable medium: fronts travel and can't
    /// back-propagate into the hot ash they leave, so they curl into rings and
    /// spirals. `lightning` is the reciprocal spark chance per idle cell
    /// (higher = rarer); `refractory` is the heat below which burnt ash may
    /// catch again (higher = shorter refractory, thicker fronts).
    Wildfire { lightning: u32, refractory: u8 },
    /// Rain on a pool: drops land as a Poisson process (exponential inter-arrival
    /// times) and each ripples outward as an expanding ring. `drops_per_frame`
    /// is the expected arrival rate.
    Raindrops { drops_per_frame: f32 },
}

impl Rule {
    /// Conway seeded with a methuselah worth watching evolve.
    pub const DEFAULT_CONWAY: Rule = Rule::Conway { seed: Seed::Acorn };
    /// Wildfire with a sensible spark rate and refractory window.
    pub const DEFAULT_WILDFIRE: Rule = Rule::Wildfire {
        lightning: 4096,
        refractory: 40,
    };
    /// Gentle rain, ~1 drop every 7 frames.
    pub const DEFAULT_RAINDROPS: Rule = Rule::Raindrops {
        drops_per_frame: 0.15,
    };
}

/// How fast a dead cell's afterglow cools, as a fraction of 256 per frame.
// ponytail: ~0.8/frame → trails last ~15 frames; drop it for longer comet tails.
const HEAT_DECAY: u16 = 205;

/// Cells a ripple's radius grows per frame.
const RIPPLE_SPEED: f32 = 1.0;
/// Retire a ripple once it has expanded past the (toroidal) board.
const MAX_RADIUS: f32 = 9.5;
/// Half-thickness of a lit ring, in cells.
const RING_HALF_WIDTH: f32 = 0.7;
/// Concurrent ripples tracked per board.
const MAX_RIPPLES: usize = 12;

pub(crate) fn step(
    rule: Rule,
    state: &[bool; N_LEDS],
    heat: &[u8; N_LEDS],
    rng: &mut u32,
    pond: &mut Pond,
) -> [bool; N_LEDS] {
    match rule {
        Rule::Conway { .. } => conway_update(state),
        Rule::Wildfire {
            lightning,
            refractory,
        } => wildfire_update(state, heat, rng, lightning, refractory),
        Rule::Raindrops { drops_per_frame } => pond.advance(rng, drops_per_frame),
    }
}

/// Hard-set heat from a board: live cells full, everything else dark. Used to
/// (re)seed, clearing any lingering afterglow.
pub(crate) fn seed_heat(heat: &mut [u8; N_LEDS], state: &[bool; N_LEDS]) {
    for (h, &alive) in heat.iter_mut().zip(state.iter()) {
        *h = if alive { u8::MAX } else { 0 };
    }
}

/// Live cells map to full heat; dead cells cool toward zero.
pub(crate) fn decay_heat(heat: &mut [u8; N_LEDS], next: &[bool; N_LEDS]) {
    for (h, &alive) in heat.iter_mut().zip(next.iter()) {
        *h = if alive {
            u8::MAX
        } else {
            (*h as u16 * HEAT_DECAY / 256) as u8
        };
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

/// One step of an excitable medium: an active cell falls quiescent, and a cell
/// cooled below `refractory` catches from any orthogonally-active neighbour.
/// Lightning ignition is layered on top by the caller.
fn ripple_front(state: &[bool; N_LEDS], heat: &[u8; N_LEDS], refractory: u8) -> [bool; N_LEDS] {
    let mut buf = [false; N_LEDS];
    for i in 0..ROWS {
        for j in 0..COLS {
            let idx = i * COLS + j;
            if state[idx] || heat[idx] >= refractory {
                continue; // active cell burns out; hot wake can't catch yet
            }
            let l = (COLS + j - 1) % COLS;
            let r = (COLS + j + 1) % COLS;
            let u = (ROWS + i - 1) % ROWS;
            let d = (ROWS + i + 1) % ROWS;
            buf[idx] = state[u * COLS + j]
                || state[d * COLS + j]
                || state[i * COLS + l]
                || state[i * COLS + r];
        }
    }
    buf
}

fn wildfire_update(
    state: &[bool; N_LEDS],
    heat: &[u8; N_LEDS],
    rng: &mut u32,
    lightning: u32,
    refractory: u8,
) -> [bool; N_LEDS] {
    let mut buf = ripple_front(state, heat, refractory);
    for idx in 0..N_LEDS {
        if !state[idx] && heat[idx] < refractory && xorshift(rng).is_multiple_of(lightning) {
            buf[idx] = true; // lightning sparks a fresh front
        }
    }
    buf
}

#[derive(Clone, Copy, Default)]
struct Ripple {
    x: f32,
    y: f32,
    radius: f32,
    active: bool,
}

/// A pool of expanding rings. Rings are drawn from each drop's radius, so
/// overlapping ripples pass through one another instead of annihilating the way
/// excitable-medium fronts do.
#[derive(Clone, Copy)]
pub(crate) struct Pond {
    ripples: [Ripple; MAX_RIPPLES],
    clock: f32,
}

impl Pond {
    pub(crate) fn new() -> Self {
        Pond {
            ripples: [Ripple::default(); MAX_RIPPLES],
            clock: 0.0,
        }
    }

    pub(crate) fn advance(&mut self, rng: &mut u32, drops_per_frame: f32) -> [bool; N_LEDS] {
        for rp in self.ripples.iter_mut() {
            if rp.active {
                rp.radius += RIPPLE_SPEED;
                if rp.radius > MAX_RADIUS {
                    rp.active = false;
                }
            }
        }
        if drops_per_frame > 0.0 {
            self.clock -= 1.0;
            // Poisson arrivals: land every drop whose time has come this frame.
            while self.clock <= 0.0 {
                self.spawn(rng);
                self.clock += exp_gap(rng, drops_per_frame);
            }
        }
        let mut buf = [false; N_LEDS];
        for i in 0..ROWS {
            for j in 0..COLS {
                buf[i * COLS + j] = self
                    .ripples
                    .iter()
                    .any(|rp| rp.active && ring_hit(rp, j, i));
            }
        }
        buf
    }

    fn spawn(&mut self, rng: &mut u32) {
        let cell = (xorshift(rng) as usize) % N_LEDS;
        let slot = self.free_slot();
        self.ripples[slot] = Ripple {
            x: (cell % COLS) as f32,
            y: (cell / COLS) as f32,
            radius: 0.0,
            active: true,
        };
    }

    /// A free slot, else the most-expanded ripple (nearest to retiring).
    fn free_slot(&self) -> usize {
        let mut worst = 0;
        for (k, rp) in self.ripples.iter().enumerate() {
            if !rp.active {
                return k;
            }
            if rp.radius > self.ripples[worst].radius {
                worst = k;
            }
        }
        worst
    }
}

/// Whether cell `(x, y)` sits on `rp`'s ring this frame, by toroidal distance.
fn ring_hit(rp: &Ripple, x: usize, y: usize) -> bool {
    let axis = |a: f32, b: f32, span: f32| {
        let d = (a - b).abs();
        d.min(span - d)
    };
    let dx = axis(rp.x, x as f32, COLS as f32);
    let dy = axis(rp.y, y as f32, ROWS as f32);
    (sqrtf(dx * dx + dy * dy) - rp.radius).abs() < RING_HALF_WIDTH
}

/// A sample from Exp(rate): frames until the next drop. `u` is drawn in
/// `(0, 1]` so `logf` stays finite and the gap is non-negative.
fn exp_gap(rng: &mut u32, rate: f32) -> f32 {
    let u = ((xorshift(rng) >> 8) + 1) as f32 / 16_777_217.0;
    -logf(u) / rate
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
        decay_heat(&mut heat, &[false; N_LEDS]); // cell 5 is now dead
        assert!(0 < heat[5] && heat[5] < u8::MAX, "dead cell should linger");
    }

    #[test]
    fn wildfire_burns_out_and_spreads() {
        let mut rng = 0x1234_5678;
        let spark = board(&[(7, 5)]);
        let next = wildfire_update(&spark, &[0u8; N_LEDS], &mut rng, 4096, 40);
        assert!(!next[5 * COLS + 7], "the burning cell should burn out");
        // its four orthogonal neighbours should catch
        assert!(
            next[5 * COLS + 6] && next[5 * COLS + 8] && next[4 * COLS + 7] && next[6 * COLS + 7]
        );
    }

    #[test]
    fn wildfire_refractory_ash_does_not_reignite() {
        let mut rng = 0x1234_5678;
        let spark = board(&[(7, 5)]);
        let mut heat = [0u8; N_LEDS];
        heat[5 * COLS + 6] = 40; // left neighbour is still-hot ash, at the threshold
        let next = wildfire_update(&spark, &heat, &mut rng, 4096, 40);
        assert!(
            !next[5 * COLS + 6],
            "hot ash must stay dark until it cools below the refractory threshold"
        );
        assert!(
            next[5 * COLS + 8],
            "cool fuel on the other side still catches"
        );
    }

    #[test]
    fn ripples_pass_through_each_other() {
        let mut pond = Pond::new();
        // two rings closing on each other along row 5
        pond.ripples[0] = Ripple {
            x: 3.0,
            y: 5.0,
            radius: 3.0,
            active: true,
        };
        pond.ripples[1] = Ripple {
            x: 9.0,
            y: 5.0,
            radius: 3.0,
            active: true,
        };
        let mut rng = 1;
        let board = pond.advance(&mut rng, 0.0); // grow to radius 4, no new drops
        // each ring keeps expanding through the meeting point rather than dying at it
        assert!(
            board[5 * COLS + 7],
            "left ring reaches x=7, past the right origin"
        );
        assert!(
            board[5 * COLS + 5],
            "right ring reaches x=5, past the left origin"
        );
    }

    #[test]
    fn raindrops_land_on_a_calm_pool() {
        let mut pond = Pond::new();
        let mut rng = 0x1234_5678;
        let board = pond.advance(&mut rng, 5.0); // high rate → a drop is essentially certain
        assert!(
            board.iter().any(|&c| c),
            "a drop lands and shows as a point"
        );
    }
}
