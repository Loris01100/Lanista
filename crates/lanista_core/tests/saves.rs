//! Saves: a game reloads exactly as it was saved, and the save files of every
//! version, kept in `tests/saves/`, still load once migrated.
//!
//! When `save::VERSION` goes up, keep the files already there and write the new
//! version's file with `cargo test -p lanista_core --test saves -- --ignored`.

use std::path::PathBuf;

use lanista_core::design_data::DesignData;
use lanista_core::save::{VERSION, from_json, to_json};
use lanista_core::state::{Decisions, GameState, next_week};
use proptest::prelude::*;

fn saves() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/saves")
}

fn kept(version: u64) -> PathBuf {
    saves().join(format!("v{version}.json"))
}

/// The game of `seed` after `weeks` weeks of suggested decisions.
fn played(seed: u64, weeks: u32, data: &DesignData) -> GameState {
    let mut state = GameState::new(seed, data);
    for _ in 0..weeks {
        next_week(&mut state, &Decisions::default(), data);
    }
    state
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn any_game_reloads_exactly_as_it_was(seed: u64, weeks in 0..40u32) {
        let data = DesignData::load().unwrap();
        let state = played(seed, weeks, &data);
        prop_assert_eq!(from_json(&to_json(&state), &data).unwrap(), state);
    }
}

#[test]
fn every_version_has_a_kept_save() {
    for version in 1..=VERSION {
        assert!(kept(version).exists(), "no kept save for version {version}");
    }
}

#[test]
fn kept_saves_load_and_play_on() {
    let data = DesignData::load().unwrap();
    for version in 1..=VERSION {
        let text = std::fs::read_to_string(kept(version)).unwrap();
        let mut state = from_json(&text, &data)
            .unwrap_or_else(|e| panic!("version {version} does not load: {e}"));
        assert_eq!(from_json(&to_json(&state), &data).unwrap(), state);
        for _ in 0..10 {
            next_week(&mut state, &Decisions::default(), &data);
        }
    }
}

/// Fails when the saved types change without a new version: their save would no
/// longer be written as the kept one.
#[test]
fn the_current_version_writes_its_kept_save_unchanged() {
    let data = DesignData::load().unwrap();
    let text = std::fs::read_to_string(kept(VERSION)).unwrap();
    // An editor may have added a final newline.
    assert_eq!(to_json(&from_json(&text, &data).unwrap()), text.trim_end());
}

/// Writes the kept save of the current version: seed 7 signing free agents from week
/// 3, played past a birthday until a fighter is both hurt and ill and a challenge is
/// pending, so that every saved field has a value.
#[test]
#[ignore = "run once when `save::VERSION` goes up"]
fn keep_a_save_of_the_current_version() {
    let path = kept(VERSION);
    assert!(!path.exists(), "{} is kept already", path.display());
    let data = DesignData::load().unwrap();
    let mut state = played(7, 2, &data);
    let hire = Decisions {
        plans: Vec::new(),
        hire_free_agent: true,
    };
    next_week(&mut state, &hire, &data);
    while state.week <= 31
        || !state
            .fighters
            .iter()
            .any(|f| !f.injuries.is_empty() && f.illness.is_some())
        || state.challenges.is_empty()
    {
        assert!(
            state.week < 1000,
            "no fighter both hurt and ill with a challenge pending"
        );
        next_week(&mut state, &hire, &data);
    }
    std::fs::create_dir_all(saves()).unwrap();
    std::fs::write(path, to_json(&state)).unwrap();
}
