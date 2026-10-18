//! Assets loaded once, then looked up by their path under the assets folder
//! (see `macroquad::file::set_pc_assets_folder`).

use std::collections::BTreeMap;

use macroquad::Error;
use macroquad::audio::{self, Sound};
use macroquad::file;
use macroquad::texture::{self, FilterMode, Texture2D};

/// Textures (spritesheets included), sounds and fonts, keyed by path.
#[derive(Default)]
pub struct Assets {
    textures: BTreeMap<String, Texture2D>,
    sounds: BTreeMap<String, Sound>,
    fonts: BTreeMap<String, Vec<u8>>,
}

impl Assets {
    /// Loads a texture or spritesheet, unless already loaded. Pixel art: no
    /// blur when scaled up.
    ///
    /// # Errors
    /// If the file is missing or not an image.
    pub async fn load_texture(&mut self, path: &str) -> Result<(), Error> {
        if !self.textures.contains_key(path) {
            let texture = texture::load_texture(path).await?;
            texture.set_filter(FilterMode::Nearest);
            self.textures.insert(path.to_owned(), texture);
        }
        Ok(())
    }

    /// Loads a sound (WAV or OGG), unless already loaded.
    ///
    /// # Errors
    /// If the file is missing or not a sound.
    pub async fn load_sound(&mut self, path: &str) -> Result<(), Error> {
        if !self.sounds.contains_key(path) {
            let sound = audio::load_sound(path).await?;
            self.sounds.insert(path.to_owned(), sound);
        }
        Ok(())
    }

    /// Loads a font file (TTF or OTF) as bytes, for egui, unless already loaded.
    ///
    /// # Errors
    /// If the file is missing.
    pub async fn load_font(&mut self, path: &str) -> Result<(), Error> {
        if !self.fonts.contains_key(path) {
            let bytes = file::load_file(path).await?;
            self.fonts.insert(path.to_owned(), bytes);
        }
        Ok(())
    }

    /// # Panics
    /// If the texture was not loaded: a bug, not a runtime condition.
    #[must_use]
    #[track_caller]
    pub fn texture(&self, path: &str) -> &Texture2D {
        loaded(&self.textures, path)
    }

    /// The texture at `path`, if it was loaded: for optional ones, whose loading may
    /// have failed.
    #[must_use]
    pub fn try_texture(&self, path: &str) -> Option<&Texture2D> {
        self.textures.get(path)
    }

    /// # Panics
    /// If the sound was not loaded.
    #[must_use]
    #[track_caller]
    pub fn sound(&self, path: &str) -> &Sound {
        loaded(&self.sounds, path)
    }

    /// # Panics
    /// If the font was not loaded.
    #[must_use]
    #[track_caller]
    pub fn font(&self, path: &str) -> &[u8] {
        loaded(&self.fonts, path)
    }
}

#[track_caller]
fn loaded<'a, T>(assets: &'a BTreeMap<String, T>, path: &str) -> &'a T {
    assets
        .get(path)
        .unwrap_or_else(|| panic!("asset not loaded: {path}"))
}
