//! Save files, in the platform's data folder: `%APPDATA%\Lanista\data` on Windows,
//! `~/.local/share/lanista` on Linux and the Steam Deck.

use std::path::{Path, PathBuf};

use lanista_core::design_data::DesignData;
use lanista_core::save::{from_json, to_json};
use lanista_core::state::GameState;
use reesch::Audio;

/// Written after every week, so quitting never loses a week played.
pub const AUTO: &str = "autosave.json";
/// The two autosaves before `AUTO`, newest first, to go back when a week went wrong.
const OLDER_AUTOS: [&str; 2] = ["autosave-2.json", "autosave-3.json"];
/// The save slots, written when the player asks; the first keeps the name of the
/// single manual save of before.
pub const SLOTS: [&str; 3] = ["save.json", "save-2.json", "save-3.json"];
/// The code of the language the player chose.
const LANGUAGE: &str = "language.txt";
/// The player's settings, one `key = value` per line: the music and effects volumes,
/// full screen and the interface's size.
const SETTINGS: &str = "settings.txt";
/// Every panic, with its backtrace, for a player to send when the game crashes.
const CRASH: &str = "crash.txt";

/// The folder of save files, if the platform has one.
pub fn folder() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "Lanista").map(|d| d.data_dir().to_path_buf())
}

/// Also appends each panic to `crash.txt` in `folder`, after the usual message on stderr.
pub fn log_panics(folder: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let text = format!(
            "Lanista {}, {seconds} s after 1970: {info}
{}
",
            env!("CARGO_PKG_VERSION"),
            std::backtrace::Backtrace::force_capture()
        );
        let _ = std::fs::create_dir_all(&folder);
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(folder.join(CRASH))
            .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()));
    }));
}

/// Writes `state` to `file` in `folder`, whole or not at all: a crash while
/// writing leaves the previous save in place. Writing `AUTO` keeps the two before it
/// in `OLDER_AUTOS`.
pub fn write(folder: &Path, file: &str, state: &GameState) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let path = folder.join(file);
    let partial = path.with_extension("json.tmp");
    std::fs::write(&partial, to_json(state))?;
    if file == AUTO {
        let [second, third] = OLDER_AUTOS.map(|f| folder.join(f));
        for (from, to) in [(&second, &third), (&path, &second)] {
            match std::fs::rename(from, to) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
    }
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

/// The window and interface settings, kept with the volumes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub fullscreen: bool,
    /// The interface's size, in percent of the theme's.
    pub interface: u16,
    /// Whether the cutscenes play: the fighters' entrance.
    pub cutscenes: bool,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            fullscreen: false,
            interface: 100,
            cutscenes: true,
        }
    }
}

/// Sets `audio` to the volumes kept in `folder` and returns the display kept there; a
/// missing file, line or value leaves that setting as it was.
pub fn read_settings(folder: &Path, audio: &mut Audio) -> Display {
    let mut display = Display::default();
    let Ok(text) = std::fs::read_to_string(folder.join(SETTINGS)) else {
        return display;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "music" => {
                if let Ok(volume) = value.parse() {
                    audio.set_music_volume(volume);
                }
            }
            "effects" => {
                if let Ok(volume) = value.parse() {
                    audio.set_effects_volume(volume);
                }
            }
            "fullscreen" => {
                if let Ok(fullscreen) = value.parse() {
                    display.fullscreen = fullscreen;
                }
            }
            "cutscenes" => {
                if let Ok(cutscenes) = value.parse() {
                    display.cutscenes = cutscenes;
                }
            }
            // Out of bounds, the interface would be unreadable or would not fit.
            "interface" => {
                if let Ok(size) = value.parse()
                    && (50..=300).contains(&size)
                {
                    display.interface = size;
                }
            }
            _ => {}
        }
    }
    display
}

/// Keeps the volumes of `audio` and `display` in `folder`.
pub fn write_settings(folder: &Path, audio: &Audio, display: Display) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let text = format!(
        "music = {}
effects = {}
fullscreen = {}
interface = {}
cutscenes = {}
",
        audio.music_volume(),
        audio.effects_volume(),
        display.fullscreen,
        display.interface,
        display.cutscenes
    );
    std::fs::write(folder.join(SETTINGS), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_save_reads_back() {
        let data = DesignData::load().unwrap();
        let folder = std::env::temp_dir().join(format!("lanista-saves-{}", std::process::id()));
        assert_eq!(read(&folder, SLOTS[0], &data), Ok(None));
        let state = GameState::new(7, &data);
        write(&folder, SLOTS[0], &state).unwrap();
        assert_eq!(read(&folder, SLOTS[0], &data), Ok(Some(state)));
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn the_last_three_autosaves_are_kept() {
        let data = DesignData::load().unwrap();
        let folder = std::env::temp_dir().join(format!("lanista-autos-{}", std::process::id()));
        let states: Vec<GameState> = (1..=4).map(|seed| GameState::new(seed, &data)).collect();
        for state in &states {
            write(&folder, AUTO, state).unwrap();
        }
        let kept = [AUTO, OLDER_AUTOS[0], OLDER_AUTOS[1]].map(|f| read(&folder, f, &data));
        assert_eq!(
            kept,
            [3, 2, 1].map(|i| Ok(Some(states[i].clone()))),
            "newest first"
        );
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

    #[test]
    fn the_settings_read_back_and_a_missing_one_keeps_its_default() {
        let folder = std::env::temp_dir().join(format!("lanista-settings-{}", std::process::id()));
        let volumes = |audio: &Audio| (audio.music_volume(), audio.effects_volume());
        let default = volumes(&Audio::default());
        let mut audio = Audio::default();
        assert_eq!(read_settings(&folder, &mut audio), Display::default());
        assert_eq!(volumes(&audio), default);
        audio.set_music_volume(0.25);
        audio.set_effects_volume(0.0);
        let display = Display {
            fullscreen: true,
            interface: 150,
            cutscenes: false,
        };
        write_settings(&folder, &audio, display).unwrap();
        let mut read = Audio::default();
        assert_eq!(read_settings(&folder, &mut read), display);
        assert_eq!(volumes(&read), (0.25, 0.0));
        // A file written by hand: an unknown key, bad values and missing lines.
        std::fs::write(
            folder.join(SETTINGS),
            "colour = red\nmusic = loud\nmusic = NaN\neffects=0.3\n",
        )
        .unwrap();
        let mut read = Audio::default();
        read_settings(&folder, &mut read);
        assert_eq!(volumes(&read), (default.0, 0.3));
        std::fs::remove_dir_all(folder).unwrap();
    }
}
