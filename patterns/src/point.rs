//! The drifting light field: soft coloured blobs that move and wrap.

use crate::{COLS, ROWS, palette};
use libm::{cosf, expf, sinf, sqrtf};
use smart_leds::RGB8;

/// Hue-cycled point colours are dimmed to this fraction of full brightness so a
/// blob stays an accent, not a floodlight.
// ponytail: raise the divisor to dim hue points further.
const HUE_DIM: u8 = 4;

#[derive(Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub color: RGB8,
    pub scale: f32,
    /// Radians the velocity rotates per frame; `0` = straight drift, small
    /// values trace orbits and arcs.
    pub turn: f32,
    /// Current hue, advanced by `hue_rate` each frame.
    pub hue: u8,
    /// Hue steps per frame; `0` keeps the static `color`.
    pub hue_rate: u8,
}

impl Point {
    pub fn mv(&mut self) {
        if self.turn != 0.0 {
            let (s, c) = (sinf(self.turn), cosf(self.turn));
            (self.dx, self.dy) = (self.dx * c - self.dy * s, self.dx * s + self.dy * c);
        }
        self.x = wrap01(self.x + self.dx);
        self.y = wrap01(self.y + self.dy);
        if self.hue_rate != 0 {
            self.hue = self.hue.wrapping_add(self.hue_rate);
            let c = palette::wheel(self.hue);
            self.color = RGB8 {
                r: c.r / HUE_DIM,
                g: c.g / HUE_DIM,
                b: c.b / HUE_DIM,
            };
        }
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

/// Equal-mass elastic collisions inside a box (walls at 0 and 1). A hit swaps
/// the velocity components along the line of centres and nudges the pair apart
/// so they don't stick; walls reflect the velocity. Collision midpoints are
/// written to `hits` (up to its length) for the caller to spark; returns how
/// many were recorded. Uses plain (non-wrapping) distance — the box confines
/// particles, so there's no seam to wrap across.
pub(crate) fn collide(points: &mut [Point], radius: f32, hits: &mut [(f32, f32)]) -> usize {
    let mut n_hits = 0;
    let min_d = 2.0 * radius;
    let n = points.len();
    for i in 0..n {
        let (head, tail) = points.split_at_mut(i + 1);
        let a = &mut head[i];
        for b in tail.iter_mut() {
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let d2 = dx * dx + dy * dy;
            if d2 >= min_d * min_d || d2 == 0.0 {
                continue;
            }
            let dist = sqrtf(d2);
            let (nx, ny) = (dx / dist, dy / dist);
            let vrel = (b.dx - a.dx) * nx + (b.dy - a.dy) * ny;
            if vrel < 0.0 {
                // approaching: exchange the normal velocity components
                a.dx += vrel * nx;
                a.dy += vrel * ny;
                b.dx -= vrel * nx;
                b.dy -= vrel * ny;
            }
            if n_hits < hits.len() {
                hits[n_hits] = ((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                n_hits += 1;
            }
            // separate along the normal so they clear next frame
            let push = (min_d - dist) * 0.5;
            a.x -= nx * push;
            a.y -= ny * push;
            b.x += nx * push;
            b.y += ny * push;
        }
    }
    for p in points.iter_mut() {
        if (p.x + p.dx) < 0.0 || (p.x + p.dx) > 1.0 {
            p.dx = -p.dx;
        }
        if (p.y + p.dy) < 0.0 || (p.y + p.dy) > 1.0 {
            p.dy = -p.dy;
        }
    }
    n_hits
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
    fn collision_swaps_velocities_head_on() {
        let mut pts = [
            Point {
                x: 0.40,
                y: 0.5,
                dx: 0.02,
                dy: 0.0,
                ..Point::default()
            },
            Point {
                x: 0.44,
                y: 0.5,
                ..Point::default()
            },
        ];
        let mut hits = [(0.0, 0.0); 4];
        let n = collide(&mut pts, 0.05, &mut hits); // min_d 0.10 > gap 0.04 → collide
        assert_abs_diff_eq!(pts[0].dx, 0.0, epsilon = 1e-6);
        assert!(pts[1].dx > 0.0, "the struck particle takes over the motion");
        assert_eq!(n, 1, "one collision recorded");
    }

    #[test]
    fn collision_reflects_off_the_wall() {
        let mut pts = [Point {
            x: 0.98,
            y: 0.5,
            dx: 0.05,
            dy: 0.0,
            ..Point::default()
        }];
        let mut hits = [(0.0, 0.0); 1];
        collide(&mut pts, 0.05, &mut hits);
        assert!(
            pts[0].dx < 0.0,
            "a particle heading into the wall bounces back"
        );
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

    #[test]
    fn turn_rotates_the_velocity() {
        let mut point = Point {
            dx: 0.01,
            dy: 0.0,
            turn: core::f32::consts::FRAC_PI_2,
            ..Point::default()
        };
        point.mv();
        // a quarter turn sends +x velocity to +y
        assert_abs_diff_eq!(point.dx, 0.0, epsilon = 1e-6);
        assert_abs_diff_eq!(point.dy, 0.01, epsilon = 1e-6);
    }

    #[test]
    fn hue_rate_cycles_the_colour_and_zero_keeps_it_static() {
        let mut cycling = Point {
            hue_rate: 10,
            ..Point::default()
        };
        cycling.mv();
        assert_eq!(cycling.hue, 10);
        assert!(cycling.color.r > 0, "hue cycling should light the colour");

        let mut fixed = Point {
            color: RGB8 { r: 3, g: 5, b: 7 },
            ..Point::default()
        };
        fixed.mv();
        assert_eq!(
            fixed.color,
            RGB8 { r: 3, g: 5, b: 7 },
            "static colour preserved"
        );
    }
}
