//! The drifting light field: soft coloured blobs that move and wrap.

use crate::{COLS, ROWS};
use libm::{expf, sqrtf};
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
    pub fn mv(&mut self) {
        self.x = wrap01(self.x + self.dx);
        self.y = wrap01(self.y + self.dy);
    }
}

/// Sum every point's contribution at grid cell `(x_u, y_u)`.
pub(crate) fn field(points: &[Point], x_u: usize, y_u: usize) -> RGB8 {
    let x = x_u as f32 / COLS as f32;
    let y = y_u as f32 / ROWS as f32;
    let mut c = RGB8::default();
    for p in points {
        let m = smear(p.x, x, p.y, y, p.scale);
        c.r = c.r.saturating_add((p.color.r as f32 * m) as u8);
        c.g = c.g.saturating_add((p.color.g as f32 * m) as u8);
        c.b = c.b.saturating_add((p.color.b as f32 * m) as u8);
    }
    c
}

/// Wrap into `[0, 1)`. `f32::rem_euclid` is std-only, so `%` (which keeps the
/// dividend's sign) is folded back by hand for negative deltas.
#[inline(always)]
fn wrap01(v: f32) -> f32 {
    let r = v % 1.;
    if r < 0. { r + 1. } else { r }
}

/// Shortest signed distance on the unit torus, in `(-0.5, 0.5]`.
#[inline(always)]
fn wrap_delta(d: f32) -> f32 {
    if d > 0.5 {
        d - 1.
    } else if d < -0.5 {
        d + 1.
    } else {
        d
    }
}

/// Accelerate every particle toward `well` under Plummer-softened gravity, then
/// leave the position step to `mv` — that ordering is semi-implicit (symplectic)
/// Euler, which keeps orbits bound instead of spiralling out. `softening` caps
/// the force near the centre so a close pass can't blow up to infinity.
pub(crate) fn gravitate(points: &mut [Point], well: (f32, f32), g: f32, softening: f32) {
    let s2 = softening * softening;
    for p in points.iter_mut() {
        let dx = wrap_delta(well.0 - p.x);
        let dy = wrap_delta(well.1 - p.y);
        let r2 = dx * dx + dy * dy;
        // a = g * d / (|d|² + s²)^{3/2}; the ^1.5 is (r²+s²)·√(r²+s²).
        let denom = (r2 + s2) * sqrtf(r2 + s2);
        let a = g / denom;
        p.dx += a * dx;
        p.dy += a * dy;
    }
}

#[inline(always)]
fn pow(f: f32, i: usize) -> f32 {
    let mut out = 1.0;
    for _ in 0..i {
        out *= f;
    }
    out
}

/// Gaussian falloff between two toroidal grid positions.
pub(crate) fn smear(x1: f32, x2: f32, y1: f32, y2: f32, scale: f32) -> f32 {
    let dist = |d: f32| d.min(1. - d);
    let dx = dist((x1 - x2).abs());
    let dy = dist((y1 - y2).abs());
    let distance = pow(dx, 2) + pow(dy, 2);
    expf(-distance / scale)
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
    fn gravity_pulls_toward_the_well() {
        let mut pts = [Point {
            x: 0.2,
            y: 0.5,
            ..Point::default()
        }];
        gravitate(&mut pts, (0.5, 0.5), 0.001, 0.1);
        assert!(pts[0].dx > 0.0, "should accelerate toward the well in +x");
        assert_abs_diff_eq!(pts[0].dy, 0.0);
    }

    #[test]
    fn gravity_is_bounded_at_the_well_centre() {
        let mut pts = [Point {
            x: 0.5,
            y: 0.5,
            ..Point::default()
        }];
        gravitate(&mut pts, (0.5, 0.5), 0.001, 0.1);
        assert!(
            pts[0].dx.is_finite() && pts[0].dy.is_finite(),
            "softening must keep the force finite at the centre"
        );
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
}
