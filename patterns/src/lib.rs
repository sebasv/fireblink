#![cfg_attr(not(test), no_std)]

#[cfg(test)]
#[macro_use]
extern crate approx;

mod life;
mod palette;
mod point;
mod seeds;

pub use life::Rule;
pub use palette::Palette;
pub use point::Point;
pub use seeds::Seed;

use smart_leds::RGB8;

pub const ROWS: usize = 10;
pub const COLS: usize = 15;
pub const N_LEDS: usize = ROWS * COLS;

/// Non-zero xorshift seed for wildfire lightning.
const RNG_SEED: u32 = 0x9E37_79B9;
/// Second channel is seeded half a board away from the first. (viz idea #5)
const CHANNEL_B_SHIFT: (usize, usize) = (COLS / 2, ROWS / 2);

/// Everything a control surface can flip at runtime.
#[derive(Clone, Copy)]
pub struct Config {
    pub seed: Seed,
    pub palette: Palette,
    /// Palette for the second board's ember contribution. (viz idea #5)
    pub palette_b: Palette,
    pub rule: Rule,
    /// Flames pick up the drifting point field's hue. (viz idea #1)
    pub tint_by_field: bool,
    /// Palette brightness tracks how much the board is churning. (viz idea #2)
    pub reactive: bool,
    /// Run a second Life board on the blue channel. (viz idea #5)
    pub two_channel: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            seed: Seed::Glider,
            palette: Palette::Fire,
            palette_b: Palette::Ice,
            rule: Rule::Conway,
            tint_by_field: false,
            reactive: false,
            two_channel: false,
        }
    }
}

pub struct Grid<'a> {
    points: &'a mut [Point],
    config: Config,
    rng: u32,
    state_a: [bool; N_LEDS],
    heat_a: [u8; N_LEDS],
    state_b: [bool; N_LEDS],
    heat_b: [u8; N_LEDS],
    activity: u8,
    /// Expanding-ripple state for each board under `Rule::Raindrops`.
    pond_a: life::Pond,
    pond_b: life::Pond,
}

pub struct GridBuilder<'a> {
    points: &'a mut [Point],
    config: Config,
}

impl<'a> GridBuilder<'a> {
    pub fn seed(mut self, seed: Seed) -> Self {
        self.config.seed = seed;
        self
    }
    pub fn palette(mut self, palette: Palette) -> Self {
        self.config.palette = palette;
        self
    }
    pub fn palette_b(mut self, palette: Palette) -> Self {
        self.config.palette_b = palette;
        self
    }
    pub fn rule(mut self, rule: Rule) -> Self {
        self.config.rule = rule;
        self
    }
    pub fn tint_by_field(mut self, on: bool) -> Self {
        self.config.tint_by_field = on;
        self
    }
    pub fn reactive(mut self, on: bool) -> Self {
        self.config.reactive = on;
        self
    }
    pub fn two_channel(mut self, on: bool) -> Self {
        self.config.two_channel = on;
        self
    }
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }
    pub fn build(self) -> Grid<'a> {
        let state_a = seeds::board(self.config.seed, 0, 0);
        let state_b = seeds::board(self.config.seed, CHANNEL_B_SHIFT.0, CHANNEL_B_SHIFT.1);
        let mut heat_a = [0u8; N_LEDS];
        let mut heat_b = [0u8; N_LEDS];
        life::seed_heat(&mut heat_a, &state_a);
        life::seed_heat(&mut heat_b, &state_b);
        Grid {
            points: self.points,
            config: self.config,
            rng: RNG_SEED,
            state_a,
            heat_a,
            state_b,
            heat_b,
            activity: 0,
            pond_a: life::Pond::new(),
            pond_b: life::Pond::new(),
        }
    }
}

impl<'a> Grid<'a> {
    pub fn builder(points: &'a mut [Point]) -> GridBuilder<'a> {
        GridBuilder {
            points,
            config: Config::default(),
        }
    }

    pub fn render(&self) -> impl Iterator<Item = RGB8> + '_ {
        (0..N_LEDS).map(|ix| self.render_led(ix))
    }

    pub fn brightness(&self) -> u32 {
        self.render()
            .fold(0u32, |a, p| a + p.r as u32 + p.g as u32 + p.b as u32)
    }

    pub fn update(&mut self) {
        self.points.iter_mut().for_each(Point::mv);

        let current = self.state_a;
        let next = life::step(
            self.config.rule,
            &current,
            &self.heat_a,
            &mut self.rng,
            &mut self.pond_a,
        );
        self.activity = life::churn(&current, &next);
        life::decay_heat(&mut self.heat_a, &next);
        self.state_a = next;

        if self.config.two_channel {
            let current = self.state_b;
            let next = life::step(
                self.config.rule,
                &current,
                &self.heat_b,
                &mut self.rng,
                &mut self.pond_b,
            );
            life::decay_heat(&mut self.heat_b, &next);
            self.state_b = next;
        }
    }

    /// Reseed both boards — for a "next pattern" control.
    pub fn reseed(&mut self, seed: Seed) {
        self.config.seed = seed;
        self.state_a = seeds::board(seed, 0, 0);
        self.state_b = seeds::board(seed, CHANNEL_B_SHIFT.0, CHANNEL_B_SHIFT.1);
        life::seed_heat(&mut self.heat_a, &self.state_a);
        life::seed_heat(&mut self.heat_b, &self.state_b);
    }

    /// Swap visual config — for palette/rule/mode controls. Does not reseed.
    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn config(&self) -> Config {
        self.config
    }

    /// Fraction of channel-A cells that changed on the last `update`, `0..=255`.
    /// Zero means the board has died or frozen — a cue to reseed.
    pub fn activity(&self) -> u8 {
        self.activity
    }

    /// FNV-1a hash of both boards. A repeat within the last few frames means
    /// the board has fallen into a short cycle (dead, still life, blinker …) —
    /// the cue that the interesting transient is over. (see `main` reseed loop)
    pub fn fingerprint(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &c in self.state_a.iter().chain(self.state_b.iter()) {
            h = (h ^ c as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// map a snake to a grid:
    /// 1 2 3
    /// 6 5 4
    /// 7 8 9
    fn ix_to_grid(&self, ix: usize) -> (usize, usize) {
        let y = ix / COLS;
        let x = if y.is_multiple_of(2) {
            ix % COLS
        } else {
            COLS - (ix % COLS) - 1
        };
        (x, y)
    }

    fn render_led(&self, ix: usize) -> RGB8 {
        let (x_u, y_u) = self.ix_to_grid(ix);
        let idx = y_u * COLS + x_u;

        let ambient = point::field(&*self.points, x_u, y_u);
        let mut color = ambient;

        let mut ember = palette::color(self.config.palette, self.heat_a[idx], x_u, y_u);
        if self.config.tint_by_field {
            ember = palette::tint(ember, ambient);
        }
        if self.config.reactive {
            ember = palette::scale(ember, self.intensity());
        }
        color.r = color.r.saturating_add(ember.r);
        color.g = color.g.saturating_add(ember.g);
        color.b = color.b.saturating_add(ember.b);

        if self.config.two_channel {
            // Second board is a full-colour accent in its own palette, held at
            // half brightness so board A stays dominant and the pair fits the
            // brightness budget. (viz idea #5)
            let ember_b = palette::scale(
                palette::color(self.config.palette_b, self.heat_b[idx], x_u, y_u),
                128,
            );
            color.r = color.r.saturating_add(ember_b.r);
            color.g = color.g.saturating_add(ember_b.g);
            color.b = color.b.saturating_add(ember_b.b);
        }
        color
    }

    /// Calm dims the palette to ~40%, churn drives it to full. (viz idea #2)
    fn intensity(&self) -> u8 {
        96 + (self.activity as u16 * 159 / 255) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_grid_reverses_odd_rows() {
        let mut points = [];
        let grid = Grid::builder(&mut points).build();
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
        let grid = Grid::builder(&mut points).seed(Seed::Empty).build();
        let near = grid.render_led(0).r;
        let far = grid.render_led(N_LEDS / 2).r;
        assert!(
            near > far,
            "point LED ({near}) should outshine a far LED ({far})"
        );
        assert_eq!(
            far, 0,
            "a point half the grid away should not light this LED"
        );
    }

    #[test]
    fn seeded_board_lights_up_and_keeps_burning_after_update() {
        let mut points = [];
        let mut grid = Grid::builder(&mut points).seed(Seed::Glider).build();
        assert!(
            grid.brightness() > 0,
            "a seeded board should light some LEDs"
        );
        grid.update();
        assert!(
            grid.brightness() > 0,
            "the afterglow should persist across a frame"
        );
    }

    #[test]
    fn reseed_swaps_the_pattern() {
        let mut points = [];
        let mut grid = Grid::builder(&mut points).seed(Seed::Glider).build();
        let lit = grid.brightness();
        grid.reseed(Seed::Empty);
        assert_eq!(grid.brightness(), 0, "reseeding to Empty clears the board");
        grid.reseed(Seed::Acorn);
        assert!(grid.brightness() > 0 && grid.brightness() != lit);
    }
}
