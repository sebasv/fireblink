#![cfg_attr(not(test), no_std)]

#[cfg(test)]
#[macro_use]
extern crate approx;

use core::iter::Iterator;

use libm::expf;
use smart_leds::RGB8;

pub const ROWS: usize = 10;
pub const COLS: usize = 15;
pub const N_LEDS: usize = ROWS * COLS;

/// How fast a dead cell's afterglow cools, as a fraction of 256 per frame.
// ponytail: ~0.8/frame → trails last ~15 frames; drop it for longer comet tails.
const HEAT_DECAY: u16 = 205;

#[derive(Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub color: RGB8,
    pub scale: f32,
}

impl Point {
    pub fn mv(&mut self) {
        self.x = wrap01(self.x + self.dx);
        self.y = wrap01(self.y + self.dy);
    }
}

/// Wrap into `[0, 1)`. `f32::rem_euclid` is std-only, so `%` (which keeps the
/// dividend's sign) is folded back by hand for negative deltas.
#[inline(always)]
fn wrap01(v: f32) -> f32 {
    let r = v % 1.;
    if r < 0. { r + 1. } else { r }
}

#[inline(always)]
fn pow(f: f32, i: usize) -> f32 {
    let mut out = 1.0;
    for _ in 0..i {
        out *= f;
    }
    out
}

pub struct Grid<'a> {
    points: &'a mut [Point],
    conway_state: [bool; N_LEDS],
    conway_heat: [u8; N_LEDS],
}

impl<'a> Grid<'a> {
    pub fn new(points: &'a mut [Point], conway_state: [bool; N_LEDS]) -> Grid<'a> {
        let mut conway_heat = [0u8; N_LEDS];
        for (heat, &alive) in conway_heat.iter_mut().zip(conway_state.iter()) {
            *heat = if alive { u8::MAX } else { 0 };
        }
        Grid {
            points,
            conway_state,
            conway_heat,
        }
    }
    pub fn render(&self) -> impl Iterator<Item = RGB8> + '_ {
        (0..N_LEDS).map(|ix| self.render_points_for_led(ix))
    }
    pub fn brightness(&self) -> u32 {
        self.render()
            .fold(0u32, |a, p| a + p.r as u32 + p.g as u32 + p.b as u32)
    }
    pub fn update(&mut self) {
        self.points.iter_mut().for_each(|p| p.mv());
        self.conway_state = conway_update(&self.conway_state);
        for (heat, &alive) in self.conway_heat.iter_mut().zip(self.conway_state.iter()) {
            *heat = if alive {
                u8::MAX
            } else {
                (*heat as u16 * HEAT_DECAY / 256) as u8
            };
        }
    }

    fn ix_to_grid(&self, ix: usize) -> (usize, usize) {
        let y = ix / COLS;
        let x = if y.is_multiple_of(2) {
            ix % COLS
        } else {
            COLS - (ix % COLS) - 1
        };
        (x, y)
    }

    /// map a snake to a grid:
    /// 1 2 3
    /// 6 5 4
    /// 7 8 9
    pub fn render_points_for_led(&self, ix: usize) -> RGB8 {
        let mut color: RGB8 = [0, 0, 0].into();
        let (x_u, y_u) = self.ix_to_grid(ix);
        let y = y_u as f32 / ROWS as f32;
        let x = x_u as f32 / COLS as f32;
        for point in &*self.points {
            let multiplier = smear(point.x, x, point.y, y, point.scale);
            color.g = u8::saturating_add(color.g, (point.color.g as f32 * multiplier) as u8);
            color.r = u8::saturating_add(color.r, (point.color.r as f32 * multiplier) as u8);
            color.b = u8::saturating_add(color.b, (point.color.b as f32 * multiplier) as u8);
        }
        let ember = ember(self.conway_heat[y_u * COLS + x_u]);
        color.r = u8::saturating_add(color.r, ember.r);
        color.g = u8::saturating_add(color.g, ember.g);
        color.b = u8::saturating_add(color.b, ember.b);
        color
    }
}

fn conway_update(state: &[bool; N_LEDS]) -> [bool; N_LEDS] {
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

/// Map a Conway cell's heat to a fire colour: white-hot when just alive,
/// cooling through orange and deep red to black as the afterglow fades.
// ponytail: channel ceilings kept low so a fully-lit board stays under the
// brightness budget; raise them if the panel can take it.
fn ember(heat: u8) -> RGB8 {
    let h = heat as u16;
    RGB8 {
        r: (h * 90 / 255) as u8,
        g: (h.saturating_sub(80) * 90 / 175) as u8,
        b: (h.saturating_sub(180) * 60 / 75) as u8,
    }
}

fn smear(x1: f32, x2: f32, y1: f32, y2: f32, scale: f32) -> f32 {
    let dist = |d: f32| d.min(1. - d);
    let dx = dist((x1 - x2).abs());
    let dy = dist((y1 - y2).abs());

    let distance = pow(dx, 2) + pow(dy, 2);

    expf(-distance / scale)

    // let beta = 0.5 + distance;
    // let beta_clipped = if beta < 1.0 { beta } else { 1.0 };
    // powi(beta_clipped, 4) * powi(1.0 - beta_clipped, 4) / 0.0625
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_pow() {
        assert_abs_diff_eq!(pow(2.0, 0), 1.0);
        assert_abs_diff_eq!(pow(2.0, 1), 2.0);
        assert_abs_diff_eq!(pow(2.0, 2), 4.0);
        assert_abs_diff_eq!(pow(2.0, 3), 8.0);
        assert_abs_diff_eq!(pow(0.5, 2), 0.25);
    }
    #[test]
    fn smear_ramps() {
        assert_abs_diff_eq!(smear(0.0, 0.0, 0.0, 0.0, 100.0), 1.0);
        let mid = smear(0.0, 0.1, 0.0, 0.0, 0.05);
        assert!(0.0 < mid);
        assert!(mid < 1.0);
        assert_abs_diff_eq!(smear(0.0, 0.5, 0.0, 0.0, 0.001), 0.0);
    }
    #[test]
    fn move_handled_boundary() {
        let mut point = Point {
            x: 0.9,
            y: 0.0,
            dx: 0.2,
            dy: -0.1,
            ..Point::default()
        };
        point.mv();
        assert_abs_diff_eq!(point.x, 0.1);
        assert_abs_diff_eq!(point.y, 0.9);
    }

    #[test]
    fn snake_grid_reverses_odd_rows() {
        let mut points = [];
        let grid = Grid::new(&mut points, [false; N_LEDS]);
        // even rows run left-to-right, odd rows right-to-left (15-wide snake).
        assert_eq!(grid.ix_to_grid(0), (0, 0));
        assert_eq!(grid.ix_to_grid(1), (1, 0));
        assert_eq!(grid.ix_to_grid(14), (14, 0));
        assert_eq!(grid.ix_to_grid(15), (14, 1));
        assert_eq!(grid.ix_to_grid(16), (13, 1));
        assert_eq!(grid.ix_to_grid(29), (0, 1));
        assert_eq!(grid.ix_to_grid(30), (0, 2));
    }

    #[test]
    fn brightness_peaks_at_the_point_and_fades_with_distance() {
        let mut points = [Point {
            x: 0.0,
            y: 0.0,
            color: [255, 255, 255].into(),
            scale: 0.01,
            ..Point::default()
        }];
        let grid = Grid::new(&mut points, [false; N_LEDS]);
        let near = grid.render_points_for_led(0).r;
        let far = grid.render_points_for_led(N_LEDS / 2).r;
        assert!(
            near > far,
            "LED at the point ({near}) should outshine a far LED ({far})"
        );
        assert_eq!(
            far, 0,
            "a point half the grid away should not light this LED"
        );
    }

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
    fn conway_heat_leaves_a_fading_ember() {
        let mut points = [];
        let mut grid = Grid::new(&mut points, board(&[(5, 5)]));
        assert_eq!(grid.conway_heat[5 * COLS + 5], u8::MAX);
        grid.update(); // lone cell dies, but its heat should linger and cool
        let h = grid.conway_heat[5 * COLS + 5];
        assert!(0 < h && h < u8::MAX, "dead cell should leave a fading ember, got {h}");
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
        // horizontal blinker straddling the column seam (…, 14, 0, 1) rotates
        // to vertical only if the left and right neighbours wrap around.
        let seam = board(&[(COLS - 1, 5), (0, 5), (1, 5)]);
        assert_eq!(live(&conway_update(&seam)), vec![(0, 4), (0, 5), (0, 6)]);
    }

    #[test]
    fn conway_folds_over_top_bottom_edge() {
        // vertical blinker straddling the row seam (…, 9, 0, 1) rotates to
        // horizontal only if the up and down neighbours wrap around.
        let seam = board(&[(7, ROWS - 1), (7, 0), (7, 1)]);
        assert_eq!(live(&conway_update(&seam)), vec![(6, 0), (7, 0), (8, 0)]);
    }

    #[test]
    fn conway_folds_over_corner_diagonally() {
        // the three cells diagonally around corner (0, 0) are only its
        // neighbours if both axes wrap; they should birth the corner.
        let corners = board(&[(COLS - 1, ROWS - 1), (0, ROWS - 1), (COLS - 1, 0)]);
        assert!(
            conway_update(&corners)[0],
            "corner (0,0) should be born from its three wrapped neighbours"
        );
    }
}
