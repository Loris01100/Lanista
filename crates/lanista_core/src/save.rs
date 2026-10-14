//! Saves: the [`GameState`] as JSON, with a version number. An older save is
//! brought up to date one version at a time before it is read.

use serde::Serialize;
use serde_json::Value;

use crate::design_data::DesignData;
use crate::state::GameState;

/// Version written in new saves. Raise it with any change to the saved types,
/// and add the migration from the previous version to [`MIGRATIONS`].
pub const VERSION: u64 = 1;

/// `MIGRATIONS[i]` turns a version `i + 1` save into a version `i + 2` one,
/// working on the JSON so the old types need not be kept.
const MIGRATIONS: [fn(&mut Value); 0] = [];
const _: () = assert!(MIGRATIONS.len() as u64 == VERSION - 1);

#[derive(Serialize)]
struct Save<'a> {
    version: u64,
    state: &'a GameState,
}

/// Why a save could not be read.
#[derive(Debug)]
pub enum SaveError {
    /// Not JSON, or not the expected fields.
    Json(serde_json::Error),
    /// No version, or one this game does not know (a save from a newer game).
    Version(Option<u64>),
    /// A fighter of a class missing from the design data.
    UnknownClass(u32),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "{e}"),
            Self::Version(None) => write!(f, "no version"),
            Self::Version(Some(v)) => write!(f, "unknown version {v} (latest is {VERSION})"),
            Self::UnknownClass(c) => write!(f, "unknown class {c}"),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            Self::Version(_) | Self::UnknownClass(_) => None,
        }
    }
}

/// The game as a save of the current [`VERSION`].
///
/// # Panics
/// Never: every saved type serializes to JSON.
#[must_use]
pub fn to_json(state: &GameState) -> String {
    let save = Save {
        version: VERSION,
        state,
    };
    serde_json::to_string_pretty(&save).expect("a game state serializes to JSON")
}

/// Reads a save of any known version, migrating it first.
///
/// # Errors
/// If the text is not a save, its version is unknown, or a fighter's class is not in `data`.
pub fn from_json(text: &str, data: &DesignData) -> Result<GameState, SaveError> {
    let mut save: Value = serde_json::from_str(text).map_err(SaveError::Json)?;
    let version = save.get("version").and_then(Value::as_u64);
    let from = match version {
        Some(v @ 1..=VERSION) => usize::try_from(v - 1).map_err(|_| SaveError::Version(version))?,
        _ => return Err(SaveError::Version(version)),
    };
    for migrate in &MIGRATIONS[from..] {
        migrate(&mut save);
    }
    let state: GameState = serde_json::from_value(save["state"].take()).map_err(SaveError::Json)?;
    // A fighter of an unknown class would panic when sent to fight.
    if let Some(f) = state
        .fighters
        .iter()
        .find(|f| data.class(f.class).is_none())
    {
        return Err(SaveError::UnknownClass(f.class.0));
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fighter::ClassId;
    use crate::state::{Decisions, next_week};

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    /// The game of `seed` after `weeks` weeks of suggested decisions.
    fn played(seed: u64, weeks: u32, data: &DesignData) -> GameState {
        let mut state = GameState::new(seed, data);
        for _ in 0..weeks {
            next_week(&mut state, &Decisions::default(), data);
        }
        state
    }

    #[test]
    fn a_saved_game_reloads_as_it_was_and_plays_on_the_same() {
        let data = data();
        let mut state = played(7, 12, &data);
        let mut reloaded = from_json(&to_json(&state), &data).unwrap();
        assert_eq!(reloaded, state);
        // The random generator was saved where it stood.
        for _ in 0..20 {
            let a = next_week(&mut state, &Decisions::default(), &data);
            let b = next_week(&mut reloaded, &Decisions::default(), &data);
            assert_eq!(a, b);
        }
        assert_eq!(reloaded, state);
    }

    #[test]
    fn unknown_versions_are_refused() {
        let data = data();
        let save = to_json(&GameState::new(7, &data));
        let newer = save.replacen("\"version\": 1", "\"version\": 2", 1);
        assert!(matches!(
            from_json(&newer, &data),
            Err(SaveError::Version(Some(2)))
        ));
        let none = save.replacen("\"version\": 1", "\"versio\": 1", 1);
        assert!(matches!(
            from_json(&none, &data),
            Err(SaveError::Version(None))
        ));
        assert!(matches!(from_json("{", &data), Err(SaveError::Json(_))));
    }

    #[test]
    fn edited_saves_cannot_break_the_game() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.fighters[0].class = ClassId(99);
        assert!(matches!(
            from_json(&to_json(&state), &data),
            Err(SaveError::UnknownClass(99))
        ));
        let state = GameState::new(7, &data);
        let form = format!("\"form\": {}", state.fighters[0].form.value());
        let edited = to_json(&state).replacen(&form, "\"form\": 250", 1);
        let reloaded = from_json(&edited, &data).unwrap();
        assert_eq!(reloaded.fighters[0].form.value(), 100);
    }
}
