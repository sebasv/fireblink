//! Pure UI state machine for the 3-button + grid-as-screen control scheme.
//!
//! No I/O: the firmware samples GPIO and feeds a `pressed: bool` per frame into
//! each [`ButtonState`]; the resulting [`Press`] events drive [`Controls`],
//! which mutates the [`Grid`] and manages the transient HUD. This keeps all the
//! timing/cycling logic host-testable.

use crate::{Grid, Hud, Scene, Seed, life};

/// The three physical buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Scene,
    Seed,
    Action,
}

/// A completed button gesture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Press {
    Short,
    Long,
}

// ponytail: frame-count thresholds assume a ~20 ms frame; retune if the loop
// cadence changes.
const DEBOUNCE_FRAMES: u16 = 2;
const LONG_FRAMES: u16 = 25;
const HUD_FRAMES: u16 = 40;

/// Debounce + short/long detection for one button. Feed it `pressed` each frame.
#[derive(Default)]
pub struct ButtonState {
    held: u16,
    fired_long: bool,
}

impl ButtonState {
    /// Advance one frame. Emits `Long` the instant the hold threshold is crossed,
    /// or `Short` on release of a debounced-but-not-long press.
    pub fn update(&mut self, pressed: bool) -> Option<Press> {
        if pressed {
            self.held += 1;
            if self.held == LONG_FRAMES && !self.fired_long {
                self.fired_long = true;
                return Some(Press::Long);
            }
            return None;
        }
        let held = core::mem::take(&mut self.held);
        let fired_long = core::mem::take(&mut self.fired_long);
        if !fired_long && held >= DEBOUNCE_FRAMES {
            Some(Press::Short)
        } else {
            None
        }
    }
}

/// Tracks the current selection and drives the grid + HUD from button gestures.
pub struct Controls {
    scene: Scene,
    seed: Seed,
    rng: u32,
    hud_frames: u16,
    idle_frames: u32,
    dirty: bool,
}

impl Controls {
    pub fn new(scene: Scene, seed: Seed) -> Self {
        Controls {
            scene,
            seed,
            rng: 0xC0FF_EE01,
            hud_frames: 0,
            idle_frames: 0,
            dirty: false,
        }
    }

    pub fn scene(&self) -> Scene {
        self.scene
    }

    pub fn seed(&self) -> Seed {
        self.seed
    }

    /// Frames since the last input — the firmware uses this to decide when to
    /// persist the current selection.
    pub fn idle_frames(&self) -> u32 {
        self.idle_frames
    }

    /// Consume the "selection changed" flag (returns true once per change).
    pub fn take_dirty(&mut self) -> bool {
        core::mem::take(&mut self.dirty)
    }

    /// Apply a button gesture to the grid. Every change reapplies the current
    /// scene with the current seed, so the two axes stay independent: the scene
    /// picks the rule family, the seed fills the Conway pattern.
    ///
    /// - Scene: short = next, long = previous
    /// - Seed: short = next, long = previous
    /// - Action: short = shuffle scene+seed, long = restart current pattern
    pub fn press(&mut self, grid: &mut Grid, button: Button, press: Press) {
        match (button, press) {
            (Button::Scene, Press::Short) => self.scene = self.scene.next(),
            (Button::Scene, Press::Long) => self.scene = self.scene.prev(),
            (Button::Seed, Press::Short) => self.seed = self.seed.next(),
            (Button::Seed, Press::Long) => self.seed = self.seed.prev(),
            (Button::Action, Press::Short) => {
                self.scene = Scene::ALL[self.rand(Scene::ALL.len())];
                self.seed = Seed::ALL[self.rand(Seed::ALL.len())];
            }
            (Button::Action, Press::Long) => {} // restart the current selection
        }
        grid.set_scene(self.scene, self.seed);
        self.hud_frames = HUD_FRAMES;
        self.idle_frames = 0;
        self.dirty = true;
        self.show_hud(grid);
    }

    /// Advance timers one frame; clears the HUD when it expires.
    pub fn tick(&mut self, grid: &mut Grid) {
        self.idle_frames = self.idle_frames.saturating_add(1);
        if self.hud_frames > 0 {
            self.hud_frames -= 1;
            if self.hud_frames == 0 {
                grid.set_hud(None);
            }
        }
    }

    fn show_hud(&self, grid: &mut Grid) {
        grid.set_hud(Some(Hud {
            scene_ix: self.scene.index() as u8,
            seed_ix: self.seed.index() as u8,
        }));
    }

    fn rand(&mut self, modulo: usize) -> usize {
        life::xorshift(&mut self.rng) as usize % modulo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Point, Rule};

    #[test]
    fn short_press_fires_on_release() {
        let mut b = ButtonState::default();
        assert_eq!(b.update(true), None);
        assert_eq!(b.update(true), None);
        assert_eq!(b.update(false), Some(Press::Short));
    }

    #[test]
    fn a_bounce_shorter_than_debounce_is_ignored() {
        let mut b = ButtonState::default();
        assert_eq!(b.update(true), None); // held = 1, below DEBOUNCE_FRAMES
        assert_eq!(b.update(false), None);
    }

    #[test]
    fn long_press_fires_at_threshold_then_release_is_silent() {
        let mut b = ButtonState::default();
        let mut got_long = false;
        for _ in 0..LONG_FRAMES {
            if b.update(true) == Some(Press::Long) {
                got_long = true;
            }
        }
        assert!(got_long, "holding past the threshold should fire Long");
        assert_eq!(b.update(false), None, "release after Long should be silent");
    }

    #[test]
    fn scene_button_cycles_scenes_without_touching_the_seed() {
        let mut points: [Point; 0] = [];
        let mut grid = Grid::builder(&mut points)
            .rule(Rule::Conway { seed: Seed::Acorn })
            .build();
        let mut ctl = Controls::new(Scene::Fire, Seed::Acorn);
        ctl.press(&mut grid, Button::Scene, Press::Short);
        assert_eq!(ctl.scene(), Scene::Fire.next());
        assert!(
            matches!(grid.config().rule, Rule::Conway { seed: Seed::Acorn }),
            "a Conway scene keeps the selected seed"
        );
    }

    #[test]
    fn hud_clears_after_timeout() {
        let mut points: [Point; 0] = [];
        let mut grid = Grid::builder(&mut points)
            .rule(Rule::Conway { seed: Seed::Empty })
            .build();
        let mut ctl = Controls::new(Scene::Fire, Seed::Glider);
        ctl.press(&mut grid, Button::Scene, Press::Short);
        assert!(
            grid.render().next().unwrap().r > 0,
            "HUD marker should be lit"
        );
        for _ in 0..HUD_FRAMES {
            ctl.tick(&mut grid);
        }
        assert_eq!(grid.render().next().unwrap().r, 0, "HUD should clear");
    }
}
