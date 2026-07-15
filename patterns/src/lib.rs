#![cfg_attr(not(test), no_std)]

#[cfg(test)]
#[macro_use]
extern crate approx;

mod life;
mod palette;
mod point;
mod seeds;

pub use life::Rule;
pub use palette::{Blend, Palette};
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

/// Ceiling on the summed activation across every LED channel. `render` scales
/// the whole frame down to fit, so a bright frame dims uniformly rather than
/// browning out the PSU.
// ponytail: raise if the panel/PSU can take a brighter board.
const BRIGHTNESS_BUDGET: u32 = 75 * 255 * 3;

/// Recent board fingerprints kept for cycle detection; a repeat means a cycle
/// of period <= this. Period-15 oscillators (Pentadecathlon) escape it.
const HISTORY: usize = 6;
/// Consecutive cyclic frames before auto-advancing to the next seed (~1.5 s).
const SETTLE: u32 = 12;
/// Hard ceiling of frames per seed (~16 s), so nothing lingers forever.
const MAX_FRAMES: u32 = 120;

/// Single central gravity well, in normalised board coordinates.
const WELL: (f32, f32) = (0.5, 0.5);
/// Well strength and Plummer softening for `Config::gravity`.
// ponytail: the two knobs to tune orbits — raise GRAVITY for tighter/faster
// orbits, raise SOFTENING to tame close passes. Bound orbits want them paired.
const GRAVITY: f32 = 0.0002;
const SOFTENING: f32 = 0.12;

/// Collision radius for `Config::collide`, in normalised units (particles
/// touch when centres are within twice this).
const COLLIDE_RADIUS: f32 = 0.07;
/// Max collision sparks stamped per frame (a handful of particles at most).
const MAX_HITS: usize = 16;

/// Initial board for a channel: Conway starts from its seed; the other rules
/// begin empty and ignite/spawn their own activity.
fn initial_board(rule: Rule, ox: usize, oy: usize) -> [bool; N_LEDS] {
    match rule {
        Rule::Conway { seed } => seeds::board(seed, ox, oy),
        _ => [false; N_LEDS],
    }
}

/// Board cell covering normalised position `(x, y)`, row-major.
fn cell_of(x: f32, y: f32) -> usize {
    let cx = ((x * COLS as f32) as usize).min(COLS - 1);
    let cy = ((y * ROWS as f32) as usize).min(ROWS - 1);
    cy * COLS + cx
}

/// Everything a control surface can flip at runtime.
#[derive(Clone, Copy)]
pub struct Config {
    /// Channel A: the rule (and, for Conway, its seed) driving the main board.
    pub rule: Rule,
    /// Channel B: an optional second board on the blue channel, with its own
    /// rule — e.g. a Conway glider over raindrops. `None` = single channel.
    /// (viz idea #5)
    pub rule_b: Option<Rule>,
    pub palette: Palette,
    /// Palette for channel B's ember contribution. (viz idea #5)
    pub palette_b: Palette,
    /// How the two channels combine: additive RGB, or channel-balance-as-hue.
    pub blend: Blend,
    /// Flames pick up the drifting point field's hue. (viz idea #1)
    pub tint_by_field: bool,
    /// Palette brightness tracks how much the board is churning. (viz idea #2)
    pub reactive: bool,
    /// Particles orbit a central gravity well and stamp comet trails into the
    /// heat buffer. Layers on the point field, independent of the rules.
    pub gravity: bool,
    /// Particles bounce in a box and collide elastically, sparking the heat
    /// buffer on impact. Layers on the point field, independent of the rules.
    pub collide: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            rule: Rule::Conway { seed: Seed::Glider },
            rule_b: None,
            palette: Palette::Fire,
            palette_b: Palette::Ice,
            blend: Blend::Add,
            tint_by_field: false,
            reactive: false,
            gravity: false,
            collide: false,
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
    /// Cycle detection for auto-advancing nonperiodic Conway seeds.
    history: [u64; HISTORY],
    settled: u32,
    on_seed: u32,
}

pub struct GridBuilder<'a> {
    points: &'a mut [Point],
    config: Config,
}

impl<'a> GridBuilder<'a> {
    pub fn rule(mut self, rule: Rule) -> Self {
        self.config.rule = rule;
        self
    }
    /// Enable a second channel with its own rule (e.g. Conway over raindrops).
    pub fn rule_b(mut self, rule: Rule) -> Self {
        self.config.rule_b = Some(rule);
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
    pub fn blend(mut self, blend: Blend) -> Self {
        self.config.blend = blend;
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
    pub fn gravity(mut self, on: bool) -> Self {
        self.config.gravity = on;
        self
    }
    pub fn collide(mut self, on: bool) -> Self {
        self.config.collide = on;
        self
    }
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }
    pub fn build(self) -> Grid<'a> {
        let state_a = initial_board(self.config.rule, 0, 0);
        let state_b = match self.config.rule_b {
            Some(rule) => initial_board(rule, CHANNEL_B_SHIFT.0, CHANNEL_B_SHIFT.1),
            None => [false; N_LEDS],
        };
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
            history: [0; HISTORY],
            settled: 0,
            on_seed: 0,
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

    /// Render every LED, uniformly dimming the whole frame if it would exceed
    /// the brightness budget (so hue is preserved, only overall level drops).
    pub fn render(&self) -> impl Iterator<Item = RGB8> + '_ {
        let total = self.brightness();
        let clip = move |v: u8| {
            if total > BRIGHTNESS_BUDGET {
                (v as u32 * BRIGHTNESS_BUDGET / total) as u8
            } else {
                v
            }
        };
        (0..N_LEDS).map(move |ix| {
            let c = self.render_led(ix);
            RGB8 {
                r: clip(c.r),
                g: clip(c.g),
                b: clip(c.b),
            }
        })
    }

    /// Summed activation the board *wants* to draw, before any clipping.
    pub fn brightness(&self) -> u32 {
        (0..N_LEDS).fold(0u32, |a, ix| {
            let c = self.render_led(ix);
            a + c.r as u32 + c.g as u32 + c.b as u32
        })
    }

    pub fn update(&mut self) {
        if self.config.gravity {
            point::gravitate(self.points, WELL, GRAVITY, SOFTENING);
        }
        let mut hits = [(0.0f32, 0.0f32); MAX_HITS];
        let n_hits = if self.config.collide {
            point::collide(self.points, COLLIDE_RADIUS, &mut hits)
        } else {
            0
        };
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

        if let Some(rule_b) = self.config.rule_b {
            let current = self.state_b;
            let next = life::step(
                rule_b,
                &current,
                &self.heat_b,
                &mut self.rng,
                &mut self.pond_b,
            );
            life::decay_heat(&mut self.heat_b, &next);
            self.state_b = next;
        }

        if self.config.gravity {
            self.stamp_trails();
        }
        for &(x, y) in &hits[..n_hits] {
            self.heat_a[cell_of(x, y)] = u8::MAX; // collision spark
        }
        self.maybe_advance();
    }

    /// Reheat each particle's cell (and the well) to full, after the board's
    /// `decay_heat` has run. Next frame's decay fades these into comet trails.
    fn stamp_trails(&mut self) {
        for i in 0..self.points.len() {
            let p = &self.points[i];
            self.heat_a[cell_of(p.x, p.y)] = u8::MAX;
        }
        self.heat_a[cell_of(WELL.0, WELL.1)] = u8::MAX;
    }

    /// Auto-advance the slideshow: nonperiodic Conway seeds collapse into short
    /// cycles on this small torus, so when the interesting transient is over
    /// (a detected cycle, or a hard timeout) move to the next such seed. Only
    /// these seeds cycle — oscillators, spaceships and the other rules sustain
    /// themselves and are left running.
    fn maybe_advance(&mut self) {
        let Rule::Conway { seed } = self.config.rule else {
            return;
        };
        if !seed.is_nonperiodic() {
            return;
        }
        self.on_seed += 1;
        let fp = self.fingerprint();
        let cycling = self.history.contains(&fp);
        self.history[self.on_seed as usize % HISTORY] = fp;
        self.settled = if cycling { self.settled + 1 } else { 0 };

        if self.settled > SETTLE || self.on_seed > MAX_FRAMES {
            self.reseed(seed.next_nonperiodic());
            self.history = [0; HISTORY];
            self.settled = 0;
            self.on_seed = 0;
        }
    }

    /// Reseed channel A to a Conway pattern — the "next pattern" control and the
    /// slideshow's auto-advance. Channel B is independent and untouched.
    pub fn reseed(&mut self, seed: Seed) {
        self.config.rule = Rule::Conway { seed };
        self.state_a = seeds::board(seed, 0, 0);
        life::seed_heat(&mut self.heat_a, &self.state_a);
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

    /// FNV-1a hash of channel A. A repeat within the last few frames means the
    /// board has fallen into a short cycle (dead, still life, blinker …) — the
    /// cue that the interesting transient is over, used by `maybe_advance`.
    /// Channel A only: a random channel-B background would never let it repeat.
    pub fn fingerprint(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &c in self.state_a.iter() {
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

        // Hue-balance mode replaces the additive channel combination: the two
        // heats fold into one hue-on-balance colour, with the drifting field
        // added on top. (Ignores the per-channel palettes and tint/reactive,
        // which are additive-mode modifiers.)
        if self.config.rule_b.is_some() && self.config.blend == Blend::HueBalance {
            let mut color = palette::hue_balance(self.heat_a[idx], self.heat_b[idx]);
            color.r = color.r.saturating_add(ambient.r);
            color.g = color.g.saturating_add(ambient.g);
            color.b = color.b.saturating_add(ambient.b);
            return color;
        }

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

        if self.config.rule_b.is_some() {
            // Channel B is a full-colour accent in its own palette, held at
            // half brightness so channel A stays dominant and the pair fits the
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
        let grid = Grid::builder(&mut points)
            .rule(Rule::Conway { seed: Seed::Empty })
            .build();
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
        let mut grid = Grid::builder(&mut points)
            .rule(Rule::Conway { seed: Seed::Glider })
            .build();
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
        let mut grid = Grid::builder(&mut points)
            .rule(Rule::Conway { seed: Seed::Glider })
            .build();
        let lit = grid.brightness();
        grid.reseed(Seed::Empty);
        assert_eq!(grid.brightness(), 0, "reseeding to Empty clears the board");
        grid.reseed(Seed::Acorn);
        assert!(grid.brightness() > 0 && grid.brightness() != lit);
    }

    #[test]
    fn render_clips_a_hot_frame_to_the_budget() {
        // a broad, full-white point floods every LED well past the budget
        let mut points = [Point {
            x: 0.5,
            y: 0.5,
            color: [255, 255, 255].into(),
            scale: 100.0,
            ..Point::default()
        }];
        let grid = Grid::builder(&mut points)
            .rule(Rule::Conway { seed: Seed::Empty })
            .build();
        assert!(
            grid.brightness() > BRIGHTNESS_BUDGET,
            "test setup should exceed the budget before clipping"
        );
        let shown: u32 = grid
            .render()
            .map(|c| c.r as u32 + c.g as u32 + c.b as u32)
            .sum();
        assert!(
            shown <= BRIGHTNESS_BUDGET,
            "render must clip to the budget, got {shown}"
        );
    }

    #[test]
    fn nonperiodic_conway_seed_auto_advances_on_timeout() {
        let mut points = [];
        let mut grid = Grid::builder(&mut points)
            .rule(Rule::Conway { seed: Seed::Acorn })
            .build();
        for _ in 0..=MAX_FRAMES {
            grid.update();
        }
        assert!(
            matches!(
                grid.config().rule,
                Rule::Conway {
                    seed: Seed::RPentomino
                }
            ),
            "Acorn should hand off to the next nonperiodic seed by the timeout"
        );
    }

    #[test]
    fn periodic_seed_is_left_running() {
        let mut points = [];
        let mut grid = Grid::builder(&mut points)
            .rule(Rule::Conway { seed: Seed::Toad })
            .build();
        for _ in 0..=MAX_FRAMES {
            grid.update();
        }
        assert!(
            matches!(grid.config().rule, Rule::Conway { seed: Seed::Toad }),
            "an oscillator seed should never be auto-advanced"
        );
    }
}
