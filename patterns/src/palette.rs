//! Heat → colour mapping and the colour-math modifiers used by the renderer.

use smart_leds::RGB8;

/// Channel ceiling for every palette, so a fully-lit board stays under the
/// brightness budget.
// ponytail: raise if the panel/PSU can take a brighter board.
const MAX: u16 = 90;

/// Colour scheme mapping a cell's heat (and position) to a colour.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Palette {
    /// White-hot → orange → deep red → black.
    Fire,
    /// White → cyan → blue → black.
    Ice,
    /// White → green → dark green → black.
    Toxic,
    /// Hue rotates with a cell's age.
    RainbowByAge,
    /// Hue fixed by grid position; heat drives brightness.
    SpatialRainbow,
}

/// How the two channels are combined into one colour.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// Sum the channels' palette colours in RGB. Bright, but overlaps saturate
    /// toward white and lose which channel was where.
    Add,
    /// Put the channel *balance* on the hue wheel and the *total* on brightness,
    /// so overlaps stay distinct instead of washing out. Ignores the per-channel
    /// palettes (the hue comes from the balance, not from Fire/Ice).
    HueBalance,
}

pub(crate) fn color(p: Palette, heat: u8, x: usize, y: usize) -> RGB8 {
    let h = heat as u16;
    match p {
        Palette::Fire => RGB8 {
            r: (h * MAX / 255) as u8,
            g: (h.saturating_sub(80) * MAX / 175) as u8,
            b: (h.saturating_sub(180) * 60 / 75) as u8,
        },
        Palette::Ice => RGB8 {
            r: (h.saturating_sub(180) * 60 / 75) as u8,
            g: (h.saturating_sub(80) * MAX / 175) as u8,
            b: (h * MAX / 255) as u8,
        },
        Palette::Toxic => RGB8 {
            r: (h.saturating_sub(80) * 80 / 175) as u8,
            g: (h * MAX / 255) as u8,
            b: (h.saturating_sub(200) * 40 / 55) as u8,
        },
        Palette::RainbowByAge => dim(wheel(255u8.saturating_sub(heat)), heat),
        Palette::SpatialRainbow => dim(wheel(((x * 17 + y * 29) & 0xff) as u8), heat),
    }
}

/// Redistribute `ember`'s total brightness across `ambient`'s hue, so flames
/// pick up the drifting point field's colour without losing energy. Neutral
/// (grey) when there is no field nearby. (viz idea #1)
pub(crate) fn tint(ember: RGB8, ambient: RGB8) -> RGB8 {
    let e = ember.r as u16 + ember.g as u16 + ember.b as u16;
    let (ar, ag, ab) = (
        ambient.r as u16 + 1,
        ambient.g as u16 + 1,
        ambient.b as u16 + 1,
    );
    let a = ar + ag + ab;
    RGB8 {
        r: (e * ar / a).min(255) as u8,
        g: (e * ag / a).min(255) as u8,
        b: (e * ab / a).min(255) as u8,
    }
}

/// Scale a colour by `factor / 255`. (viz idea #2: activity-reactive intensity)
pub(crate) fn scale(c: RGB8, factor: u8) -> RGB8 {
    let s = |v: u8| (v as u16 * factor as u16 / 255) as u8;
    RGB8 {
        r: s(c.r),
        g: s(c.g),
        b: s(c.b),
    }
}

/// Full-brightness colour wheel, `h` running once around the hues.
pub(crate) fn wheel(h: u8) -> RGB8 {
    let f = (h % 43) * 6;
    match h / 43 {
        0 => RGB8 { r: 255, g: f, b: 0 },
        1 => RGB8 {
            r: 255 - f,
            g: 255,
            b: 0,
        },
        2 => RGB8 { r: 0, g: 255, b: f },
        3 => RGB8 {
            r: 0,
            g: 255 - f,
            b: 255,
        },
        4 => RGB8 { r: f, g: 0, b: 255 },
        _ => RGB8 {
            r: 255,
            g: 0,
            b: 255 - f,
        },
    }
}

/// Scale a full-brightness colour by heat and the palette ceiling.
fn dim(c: RGB8, heat: u8) -> RGB8 {
    let s = |v: u8| (v as u32 * heat as u32 * MAX as u32 / (255 * 255)) as u8;
    RGB8 {
        r: s(c.r),
        g: s(c.g),
        b: s(c.b),
    }
}

/// Combine two channel heats on the hue wheel: the balance `heat_a - heat_b`
/// picks the hue — A at the warm (red) end, B at the cool (blue) end, an even
/// mix in the green middle — and the total picks the brightness. Never
/// saturates to white; the "difference" between the channels stays visible.
pub(crate) fn hue_balance(heat_a: u8, heat_b: u8) -> RGB8 {
    let diff = heat_a as i16 - heat_b as i16; // -255..=255, positive = A dominant
    // (255 - diff) in 0..=510 → hue 0 (red, A) .. 85 (green) .. 170 (blue, B),
    // an arc that never wraps back onto red, so the two ends stay distinct.
    let hue = ((255 - diff) as u32 * 170 / 510) as u8;
    let total = (heat_a as u16 + heat_b as u16).min(255) as u8;
    dim(wheel(hue), total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_respect_the_brightness_ceiling() {
        for p in [
            Palette::Fire,
            Palette::Ice,
            Palette::Toxic,
            Palette::RainbowByAge,
            Palette::SpatialRainbow,
        ] {
            for heat in 0..=255u16 {
                let c = color(p, heat as u8, 3, 7);
                for ch in [c.r, c.g, c.b] {
                    assert!(
                        ch as u16 <= MAX,
                        "channel {ch} over ceiling for heat {heat}"
                    );
                }
            }
        }
    }

    #[test]
    fn hue_balance_leans_warm_for_a_and_cool_for_b() {
        let a = hue_balance(255, 0); // A dominant → warm
        let b = hue_balance(0, 255); // B dominant → cool
        assert!(a.r > a.b, "A-dominant should lean red");
        assert!(b.b > b.r, "B-dominant should lean blue");
        // an even overlap stays a definite colour, never white
        let mid = hue_balance(255, 255);
        assert!(
            !(mid.r == mid.g && mid.g == mid.b),
            "a balanced overlap should keep a hue, not wash to grey/white"
        );
    }

    #[test]
    fn tint_preserves_energy_and_survives_a_dark_field() {
        let ember = RGB8 { r: 60, g: 30, b: 0 };
        let energy = ember.r as u16 + ember.g as u16 + ember.b as u16;
        let blued = tint(ember, RGB8 { r: 0, g: 0, b: 200 });
        assert!(blued.b > blued.r && blued.b > blued.g, "should lean blue");
        // dark field must not extinguish the flame
        let neutral = tint(ember, RGB8::default());
        let kept = neutral.r as u16 + neutral.g as u16 + neutral.b as u16;
        assert!(kept + 3 >= energy, "energy preserved within rounding");
    }
}
