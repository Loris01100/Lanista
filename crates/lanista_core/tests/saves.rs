//! Saves: a game reloads exactly as it was saved. Until the Steam release, saves are
//! not migrated, so no older save files are kept.

use lanista_core::design_data::DesignData;
use lanista_core::save::{from_json, to_json};
use lanista_core::state::{Decisions, GameState, next_week};
use proptest::prelude::*;

/// The game of `seed` after `weeks` weeks of suggested decisions.
fn played(seed: u64, weeks: u32, data: &DesignData) -> GameState {
    let mut state = GameState::new(seed, data);
    for _ in 0..weeks {
        next_week(&mut state, &Decisions::default(), data);
    }
    state
}

#[test]
fn a_fighter_gone_elsewhere_leaves_only_where_it_went() {
    let data = DesignData::load().unwrap();
    let state = played(1, 30, &data);
    let rosters = state.rosters();
    let left: Vec<_> = state
        .free_agents
        .iter()
        .chain(state.rivals.iter().flat_map(|r| &r.fighters))
        .filter(|f| f.moved_to.is_some())
        .collect();
    assert_ne!(left.len(), 0, "no fighter went elsewhere");
    for f in left {
        let there = rosters.fighter(f.moved_to.unwrap()).unwrap();
        assert_eq!(there.class, f.class);
        assert_eq!(f, &f.moved(f.moved_to.unwrap()));
    }
    // The player's club keeps its own whole.
    assert!(state.fighters.iter().all(|f| f.moved_to.is_none()));
    assert_eq!(from_json(&to_json(&state), &data).unwrap(), state);
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
