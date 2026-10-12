//! Shared by the command-line tools: fight setup and argument parsing.

use lanista_core::ai::{CombatAi, UtilityAi};
use lanista_core::combat::{Arena, Vec2, run};
use lanista_core::design_data::DesignData;
use lanista_core::fighter::{ClassId, Fighter, FighterId, Stats};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// A fight lasts at most one minute.
pub const MAX_TICKS: u32 = 1800;
/// Floor size, in meters.
pub const ARENA_SIZE: f64 = 20.0;
/// Same stats for everyone, so only the classes make a difference.
pub const STATS: Stats = Stats {
    hp: 100,
    melee_attack: 20,
    ranged_attack: 20,
    speed: 30,
    melee_defense: 10,
    ranged_defense: 10,
};

/// Reads `data/classes.ron` at run time, so balancing needs no rebuild.
///
/// # Errors
/// If the file cannot be read or parsed.
pub fn design_data() -> Result<DesignData, String> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/classes.ron");
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    DesignData::from_ron(&text).map_err(|e| format!("{path}: {e}"))
}

/// A duel set up from `seed`: random classes, fighter 0 starts in the left quarter,
/// fighter 1 in the right quarter. The arena gets the same seed.
///
/// # Panics
/// If `data` has no class.
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
    Arena::new(ARENA_SIZE, ARENA_SIZE, fighters, seed)
}

/// Plays the fight to its end, or `MAX_TICKS`, with a utility AI for every fighter.
pub fn play(arena: &mut Arena) {
    let mut ais: Vec<Box<dyn CombatAi>> = arena
        .fighters
        .iter()
        .map(|_| Box::new(UtilityAi) as Box<dyn CombatAi>)
        .collect();
    run(arena, &mut ais, MAX_TICKS);
}

/// Value of the `--name value` option among `args` (program name excluded), or `default`.
///
/// # Errors
/// On an unknown argument, a missing value or a value that is not a whole number.
pub fn option(
    args: impl IntoIterator<Item = String>,
    name: &str,
    default: u64,
) -> Result<u64, String> {
    let mut value = default;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg != name {
            return Err(format!(
                "unknown argument: {arg} (expected {name} <number>)"
            ));
        }
        let Some(text) = args.next() else {
            return Err(format!("{name} needs a value"));
        };
        value = text
            .parse()
            .map_err(|_| format!("{name}: not a whole number: {text}"))?;
    }
    Ok(value)
}

/// `numerator / denominator` with one decimal, rounded down, in integers only.
#[must_use]
pub fn one_decimal(numerator: u64, denominator: u64) -> String {
    if denominator == 0 {
        return "-".into();
    }
    let tenths = numerator * 10 / denominator;
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|&s| s.to_owned()).collect()
    }

    #[test]
    fn reads_an_option() {
        assert_eq!(option(args(&[]), "--seed", 42), Ok(42));
        assert_eq!(option(args(&["--seed", "7"]), "--seed", 42), Ok(7));
        assert!(option(args(&["--seed"]), "--seed", 42).is_err());
        assert!(option(args(&["--seed", "x"]), "--seed", 42).is_err());
        assert!(option(args(&["--graine", "7"]), "--seed", 42).is_err());
    }

    #[test]
    fn same_seed_same_duel() {
        let data = design_data().unwrap();
        assert_eq!(random_duel(&data, 3), random_duel(&data, 3));
        assert_ne!(random_duel(&data, 3), random_duel(&data, 4));
    }

    #[test]
    fn formats_one_decimal() {
        assert_eq!(one_decimal(1, 3), "0.3");
        assert_eq!(one_decimal(127, 30), "4.2");
        assert_eq!(one_decimal(1, 0), "-");
    }
}
