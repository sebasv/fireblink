//! Curated presets that bundle rule + palette + effects into one selectable
//! "look". The seed is a separate axis, so a scene never changes the pattern.

use crate::{Config, Palette, Rule};

/// A named visual preset. Cycle with the scene button; the grid previews it live.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scene {
    Fire,
    IcePlasma,
    Rainbow,
    Wildfire,
    Interference,
    Reactive,
    AudioFire,
    Toxic,
}

impl Scene {
    pub const ALL: [Scene; 8] = [
        Scene::Fire,
        Scene::IcePlasma,
        Scene::Rainbow,
        Scene::Wildfire,
        Scene::Interference,
        Scene::Reactive,
        Scene::AudioFire,
        Scene::Toxic,
    ];

    /// The visual config for this scene. The `seed` field is a placeholder —
    /// `Grid::set_scene` preserves the current seed.
    pub fn config(self) -> Config {
        let base = Config::default();
        match self {
            Scene::Fire => Config {
                palette: Palette::Fire,
                ..base
            },
            Scene::IcePlasma => Config {
                palette: Palette::Ice,
                tint_by_field: true,
                ..base
            },
            Scene::Rainbow => Config {
                palette: Palette::SpatialRainbow,
                ..base
            },
            Scene::Wildfire => Config {
                palette: Palette::Fire,
                rule: Rule::Wildfire,
                ..base
            },
            Scene::Interference => Config {
                palette: Palette::Ice,
                two_channel: true,
                ..base
            },
            Scene::Reactive => Config {
                palette: Palette::Fire,
                reactive: true,
                audio_decay: true,
                ..base
            },
            Scene::AudioFire => Config {
                palette: Palette::Fire,
                tint_by_field: true,
                audio_beat: true,
                audio_decay: true,
                ..base
            },
            Scene::Toxic => Config {
                palette: Palette::Toxic,
                ..base
            },
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    pub fn next(self) -> Scene {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Scene {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}
