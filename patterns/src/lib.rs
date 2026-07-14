#![cfg_attr(not(test), no_std)]

#[cfg(test)]
#[macro_use]
extern crate approx;

use libm::expf;
use smart_leds::RGB8;

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
    pub fn mv(self) -> Point {
        Point {
            x: wrap01(self.x + self.dx),
            y: wrap01(self.y + self.dy),
            ..self
        }
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

pub struct Grid {
    cols: usize,
    rows: usize,
    n_leds: usize,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Grid {
        Grid {
            cols,
            rows,
            n_leds: cols * rows,
        }
    }
    pub fn render(&self) -> &[RGB8] {
        (0..grid.n_leds).map(|ix| grid.render_points_for_led(ix, &points))
    }

    fn ix_to_grid(&self, ix: usize) -> (usize, usize) {
        let y = ix / self.cols;
        let x = if y.is_multiple_of(2) {
            ix % self.cols
        } else {
            self.cols - (ix % self.cols) - 1
        };
        (x, y)
    }

    /// map a snake to a grid:
    /// 1 2 3
    /// 6 5 4
    /// 7 8 9
    pub fn render_points_for_led(&self, ix: usize, points: &[Point]) -> RGB8 {
        let mut color: RGB8 = [0, 0, 0].into();
        let (x_u, y_u) = self.ix_to_grid(ix);
        let y = y_u as f32 / self.rows as f32;
        let x = x_u as f32 / self.cols as f32;
        for point in points {
            let multiplier = smear(point.x, x, point.y, y, point.scale);
            color.g = u8::saturating_add(color.g, (point.color.g as f32 * multiplier) as u8);
            color.r = u8::saturating_add(color.r, (point.color.r as f32 * multiplier) as u8);
            color.b = u8::saturating_add(color.b, (point.color.b as f32 * multiplier) as u8);
        }
        color
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
        assert_abs_diff_eq!(smear(0.0, 0.0, 0.0, 0.0), 1.0);
        let mid = smear(0.0, 0.005, 0.0, 0.0, 100.0);
        assert!(0.0 < mid);
        assert!(mid < 1.0);
        assert_abs_diff_eq!(smear(0.0, 0.01, 0.0, 0.0), 0.0);
    }
    #[test]
    fn move_handled_boundary() {
        let point = Point {
            x: 0.9,
            y: 0.0,
            dx: 0.2,
            dy: -0.1,
            color: [0, 0, 0].into(),
            scale: 100.0,
        }
        .mv();
        assert_abs_diff_eq!(point.x, 0.1);
        assert_abs_diff_eq!(point.y, 0.9);
    }

    #[test]
    fn snake_grid_reverses_odd_rows() {
        let grid = Grid::new(3, 3);
        // self.cols == 1 so x is always 0; the interesting axis is y == row.
        assert_eq!(grid.ix_to_grid(0), (0, 0));
        assert_eq!(grid.ix_to_grid(1), (1, 0));
        assert_eq!(grid.ix_to_grid(2), (2, 0));
        assert_eq!(grid.ix_to_grid(3), (2, 1));
        assert_eq!(grid.ix_to_grid(4), (1, 1));
        assert_eq!(grid.ix_to_grid(5), (0, 1));
        assert_eq!(grid.ix_to_grid(6), (0, 2));
        assert_eq!(grid.ix_to_grid(7), (1, 2));
        assert_eq!(grid.ix_to_grid(8), (2, 2));
    }

    #[test]
    fn brightness_peaks_at_the_point_and_fades_with_distance() {
        let grid = Grid::new(3, 3);
        let points = [Point {
            x: 0.0,
            y: 0.0,
            color: [255, 255, 255].into(),
            ..Point::default()
        }];
        let near = grid.render_points_for_led(0, &points).r;
        let far = grid.render_points_for_led(grid.rows / 2, &points).r;
        assert!(
            near > far,
            "LED at the point ({near}) should outshine a far LED ({far})"
        );
        assert_eq!(
            far, 0,
            "a point half the grid away should not light this LED"
        );
    }
}
