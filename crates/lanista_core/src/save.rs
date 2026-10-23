//! Saves: the [`GameState`] as JSON, with a version number. Until the Steam release,
//! only saves of the current [`VERSION`] are read.

use serde::Serialize;
use serde_json::Value;

use crate::design_data::DesignData;
use crate::state::GameState;

/// Version written in new saves; a save of another version is refused. Until the Steam
/// release, saves are not migrated: a change to the saved types may make older saves
/// unreadable, and the game then starts anew. At the release, versions start again from
/// this one, with a migration from each version to the next.
pub const VERSION: u64 = 1;

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
    /// No version, or another than [`VERSION`].
    Version(Option<u64>),
    /// A fighter of a class missing from the design data.
    UnknownClass(u32),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "{e}"),
            Self::Version(None) => write!(f, "no version"),
            Self::Version(Some(v)) => write!(f, "version {v} instead of {VERSION}"),
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

/// Reads a save of the current [`VERSION`], and opens the colosseums and rival clubs of
/// `data` it does not have, and the free agents and staff candidates it lacks.
///
/// # Errors
/// If the text is not a save of this version, or a fighter's class is not in `data`.
pub fn from_json(text: &str, data: &DesignData) -> Result<GameState, SaveError> {
    let mut save: Value = serde_json::from_str(text).map_err(SaveError::Json)?;
    let version = save.get("version").and_then(Value::as_u64);
    if version != Some(VERSION) {
        return Err(SaveError::Version(version));
    }
    let mut state: GameState =
        serde_json::from_value(save["state"].take()).map_err(SaveError::Json)?;
    // A fighter of an unknown class would panic when sent to fight.
    let rivals = state.rivals.iter().flat_map(|r| &r.fighters);
    if let Some(f) = state
        .fighters
        .iter()
        .chain(rivals)
        .chain(&state.free_agents)
        .chain(state.journal.fought())
        .find(|f| data.class(f.class).is_none())
    {
        return Err(SaveError::UnknownClass(f.class.0));
    }
    state.open_world(data);
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
    fn other_versions_are_refused() {
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
        let mut state = GameState::new(7, &data);
        state.rivals[1].fighters[0].class = ClassId(98);
        assert!(matches!(
            from_json(&to_json(&state), &data),
            Err(SaveError::UnknownClass(98))
        ));
        let state = GameState::new(7, &data);
        let form = format!("\"form\": {}", state.fighters[0].form.value());
        let edited = to_json(&state).replacen(&form, "\"form\": 250", 1);
        let reloaded = from_json(&edited, &data).unwrap();
        assert_eq!(reloaded.fighters[0].form.value(), 100);
    }
}
