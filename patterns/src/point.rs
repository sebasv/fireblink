//! The drifting light field: soft coloured blobs that move and wrap.

use crate::{COLS, ROWS, palette};
use libm::{cosf, expf, sinf};
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
    /// How much loudness inflates the blob: effective scale is
    /// `scale * (1 + pulse * level)`. `0` = ignore audio.
    pub pulse: f32,
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

/// Sum every point's contribution at grid cell `(x_u, y_u)`. `level` is the
/// current audio loudness, which blooms points whose `pulse` is non-zero.
pub(crate) fn field(points: &[Point], x_u: usize, y_u: usize, level: u8) -> RGB8 {
    let x = x_u as f32 / COLS as f32;
    let y = y_u as f32 / ROWS as f32;
    let lf = level as f32 / 255.0;
    let mut c = RGB8::default();
    for p in points {
        let eff_scale = p.scale * (1.0 + p.pulse * lf);
        let m = smear(p.x, x, p.y, y, eff_scale);
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

    #[test]
    fn pulse_blooms_the_blob_with_loudness() {
        let pts = [Point {
            x: 0.0,
            y: 0.0,
            scale: 0.01,
            pulse: 1.0,
            color: RGB8 {
                r: 255,
                g: 255,
                b: 255,
            },
            ..Point::default()
        }];
        // at a cell away from the point, a louder level widens the blob → brighter
        let quiet = field(&pts, 3, 0, 0).r;
        let loud = field(&pts, 3, 0, 255).r;
        assert!(
            loud > quiet,
            "loudness should bloom the blob ({quiet} -> {loud})"
        );
    }
}
