//! The standard duel, set up from a seed: the same in the game, the tools
//! (`simulate`, `batch`) and the tests.

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::ai::{CombatAi, UtilityAi};
use crate::combat::{Arena, Vec2, run};
use crate::design_data::DesignData;
use crate::fighter::{ClassId, Fighter, FighterId, Stats};

/// A fight lasts at most one minute.
pub const MAX_TICKS: u32 = 1800;
/// Floor size, in meters.
pub const ARENA_SIZE: f64 = 20.0;
/// Same stats for everyone, so only the classes make a difference.
pub const STATS: Stats = Stats {
    hp: 100,
    melee_attack: 20,
    ranged_attack: 20,
    accuracy: 30,
    speed: 30,
    melee_defense: 10,
    ranged_defense: 10,
};

/// A duel set up from `seed`: random classes, fighter 0 starts in the left quarter,
/// fighter 1 in the right quarter. The arena gets the same seed and `data`'s accuracy rules.
///
/// # Panics
/// If `data` has no class, which [`DesignData::from_ron`] rejects: a bug, not a data error.
#[must_use]
pub fn random_duel(data: &DesignData, seed: u64) -> Arena {
    assert!(!data.classes.is_empty(), "no class in the design data");
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let class_count = u32::try_from(data.classes.len()).unwrap_or(u32::MAX);
    let mut fighter = |id: u32, x_min: f64| {
        let class = ClassId(rng.random_range(0..class_count));
        let x = rng.random_range(x_min..x_min + ARENA_SIZE / 4.0);
        let y = rng.random_range(1.0..ARENA_SIZE - 1.0);
        let class_data = data.class(class).expect("drawn among the loaded classes");
        Fighter::new(FighterId(id), class, class_data, STATS, Vec2::new(x, y))
    };
    let fighters = vec![fighter(0, 1.0), fighter(1, ARENA_SIZE * 3.0 / 4.0 - 1.0)];
    Arena::new(ARENA_SIZE, ARENA_SIZE, fighters, seed).with_accuracy(data.combat.accuracy)
}

/// A utility AI for every fighter of `arena`.
#[must_use]
pub fn utility_ais(arena: &Arena) -> Vec<Box<dyn CombatAi>> {
    arena
        .fighters
        .iter()
        .map(|_| Box::new(UtilityAi) as Box<dyn CombatAi>)
        .collect()
}

/// Plays the fight to its end, or `MAX_TICKS`, with a utility AI for every fighter.
pub fn play(arena: &mut Arena) {
    let mut ais = utility_ais(arena);
    run(arena, &mut ais, MAX_TICKS);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_duel() {
        let data = DesignData::load().unwrap();
        assert_eq!(random_duel(&data, 3), random_duel(&data, 3));
        assert_ne!(random_duel(&data, 3), random_duel(&data, 4));
    }
}
