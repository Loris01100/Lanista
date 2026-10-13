//! Invariants checked on random duels between utility AIs.

use lanista_core::ai::{CombatAi, UtilityAi};
use lanista_core::combat::{Arena, RADIUS, Vec2, run, tick};
use lanista_core::design_data::DesignData;
use lanista_core::duel::{ARENA_SIZE, MAX_TICKS, STATS};
use lanista_core::fighter::{ClassId, Fighter, FighterId, Stats};
use proptest::prelude::*;

fn stats() -> impl Strategy<Value = Stats> {
    (
        1..300u32,
        0..100u32,
        0..100u32,
        0..60u32,
        0..200u32,
        0..200u32,
    )
        .prop_map(
            |(hp, melee_attack, ranged_attack, speed, melee_defense, ranged_defense)| Stats {
                hp,
                melee_attack,
                ranged_attack,
                speed,
                melee_defense,
                ranged_defense,
            },
        )
}

fn position() -> impl Strategy<Value = Vec2> {
    (RADIUS..ARENA_SIZE - RADIUS, RADIUS..ARENA_SIZE - RADIUS).prop_map(|(x, y)| Vec2::new(x, y))
}

/// Class index and position of each fighter.
type Setup = [(u32, Vec2); 2];

fn setup() -> impl Strategy<Value = Setup> {
    [(0..2u32, position()), (0..2u32, position())]
}

fn duel(setup: Setup, stats: [Stats; 2], seed: u64) -> (Arena, Vec<Box<dyn CombatAi>>) {
    let data = DesignData::from_ron(include_str!("../../../data/classes.ron")).unwrap();
    let fighters = (0..2u32)
        .zip(setup.into_iter().zip(stats))
        .map(|(id, ((class, position), stats))| {
            let class_data = data.class(ClassId(class)).unwrap();
            Fighter::new(FighterId(id), ClassId(class), class_data, stats, position)
        })
        .collect();
    let ais: Vec<Box<dyn CombatAi>> = vec![Box::new(UtilityAi), Box::new(UtilityAi)];
    (Arena::new(ARENA_SIZE, ARENA_SIZE, fighters, seed), ais)
}

proptest! {
    #[test]
    fn hp_gauges_and_positions_stay_in_bounds(
        setup in setup(),
        stats in [stats(), stats()],
        seed: u64,
    ) {
        let (mut arena, mut ais) = duel(setup, stats, seed);
        while !arena.is_over() && arena.tick < MAX_TICKS {
            tick(&mut arena, &mut ais);
            for f in &arena.fighters {
                prop_assert!(f.hp <= f.stats.hp);
                prop_assert!(f.mana.current <= f.mana.max);
                prop_assert!(f.endurance.current <= f.endurance.max);
                prop_assert!((RADIUS..=ARENA_SIZE - RADIUS).contains(&f.position.x));
                prop_assert!((RADIUS..=ARENA_SIZE - RADIUS).contains(&f.position.y));
            }
        }
    }

    // ponytail: equal stats only; with random ones two fighters can run out of
    // resources without a kill, as nothing regenerates yet.
    #[test]
    fn a_fight_between_equals_always_ends(setup in setup(), seed: u64) {
        let (mut arena, mut ais) = duel(setup, [STATS; 2], seed);
        run(&mut arena, &mut ais, MAX_TICKS);
        prop_assert!(arena.is_over(), "still going after {} ticks", arena.tick);
    }
}
