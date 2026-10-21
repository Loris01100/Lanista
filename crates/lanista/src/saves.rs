//! Save files, in the platform's data folder: `%APPDATA%\Lanista\data` on Windows,
//! `~/.local/share/lanista` on Linux and the Steam Deck.

use std::path::{Path, PathBuf};

use lanista_core::design_data::DesignData;
use lanista_core::save::{from_json, to_json};
use lanista_core::state::GameState;

/// Written after every week, so quitting never loses a week played.
pub const AUTO: &str = "autosave.json";
/// Written when the player asks.
pub const MANUAL: &str = "save.json";
/// The code of the language the player chose.
const LANGUAGE: &str = "language.txt";

/// The folder of save files, if the platform has one.
pub fn folder() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "Lanista").map(|d| d.data_dir().to_path_buf())
}

/// Writes `state` to `file` in `folder`, whole or not at all: a crash while
/// writing leaves the previous save in place.
pub fn write(folder: &Path, file: &str, state: &GameState) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let path = folder.join(file);
    let partial = path.with_extension("json.tmp");
    std::fs::write(&partial, to_json(state))?;
    std::fs::rename(partial, path)
}

/// Reads `file` in `folder`; `Ok(None)` if there is no such save.
pub fn read(folder: &Path, file: &str, data: &DesignData) -> Result<Option<GameState>, String> {
    match std::fs::read_to_string(folder.join(file)) {
        Ok(text) => from_json(&text, data).map(Some).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The language the player chose in `folder`, if any.
pub fn read_language(folder: &Path) -> Option<String> {
    let code = std::fs::read_to_string(folder.join(LANGUAGE)).ok()?;
    Some(code.trim().to_owned())
}

pub fn write_language(folder: &Path, code: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    std::fs::write(folder.join(LANGUAGE), code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_save_reads_back() {
        let data = DesignData::load().unwrap();
        let folder = std::env::temp_dir().join(format!("lanista-saves-{}", std::process::id()));
        assert_eq!(read(&folder, MANUAL, &data), Ok(None));
        let state = GameState::new(7, &data);
        write(&folder, MANUAL, &state).unwrap();
        assert_eq!(read(&folder, MANUAL, &data), Ok(Some(state)));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_chosen_language_reads_back() {
        let folder = std::env::temp_dir().join(format!("lanista-language-{}", std::process::id()));
        assert_eq!(read_language(&folder), None);
        write_language(&folder, "fr").unwrap();
        assert_eq!(read_language(&folder).as_deref(), Some("fr"));
        std::fs::remove_dir_all(folder).unwrap();
    }
}
