//! Sound effects and music, each with its own volume.

use macroquad::audio::{PlaySoundParams, Sound, play_sound, set_sound_volume, stop_sound};

use crate::Assets;

/// Plays effects and one looping music track. Volumes go from 0 to 1.
pub struct Audio {
    music_volume: f32,
    effects_volume: f32,
    /// Path and sound of the music playing.
    music: Option<(String, Sound)>,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            music_volume: 0.5,
            effects_volume: 0.8,
            music: None,
        }
    }
}

impl Audio {
    /// Plays a loaded sound once, at the effects volume.
    ///
    /// # Panics
    /// If the sound was not loaded.
    pub fn play_effect(&self, assets: &Assets, path: &str) {
        if self.effects_volume > 0.0 {
            play_sound(
                assets.sound(path),
                PlaySoundParams {
                    looped: false,
                    volume: self.effects_volume,
                },
            );
        }
    }

    /// Loops a loaded sound as the music, replacing the current one; does
    /// nothing if it is already playing.
    ///
    /// # Panics
    /// If the sound was not loaded.
    pub fn play_music(&mut self, assets: &Assets, path: &str) {
        if self
            .music
            .as_ref()
            .is_some_and(|(playing, _)| playing == path)
        {
            return;
        }
        self.stop_music();
        let sound = assets.sound(path).clone();
        play_sound(
            &sound,
            PlaySoundParams {
                looped: true,
                volume: self.music_volume,
            },
        );
        self.music = Some((path.to_owned(), sound));
    }

    pub fn stop_music(&mut self) {
        if let Some((_, sound)) = self.music.take() {
            stop_sound(&sound);
        }
    }

    #[must_use]
    pub fn music_volume(&self) -> f32 {
        self.music_volume
    }

    /// Also applies to the music playing. A NaN volume is ignored.
    pub fn set_music_volume(&mut self, volume: f32) {
        if volume.is_nan() {
            return;
        }
        self.music_volume = volume.clamp(0.0, 1.0);
        if let Some((_, sound)) = &self.music {
            set_sound_volume(sound, self.music_volume);
        }
    }

    #[must_use]
    pub fn effects_volume(&self) -> f32 {
        self.effects_volume
    }

    /// Applies to the next effects; those playing keep their volume. A NaN volume is ignored.
    pub fn set_effects_volume(&mut self, volume: f32) {
        if volume.is_nan() {
            return;
        }
        self.effects_volume = volume.clamp(0.0, 1.0);
    }
}
