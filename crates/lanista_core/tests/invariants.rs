//! Invariants checked on random duels between utility AIs.

use lanista_core::ai::{CombatAi, UtilityAi};
use lanista_core::combat::{AccuracyRules, Arena, RADIUS, Vec2, run, tick};
use lanista_core::design_data::DesignData;
use lanista_core::duel::{ARENA_SIZE, MAX_TICKS, STATS};
use lanista_core::fighter::injury::Location;
use lanista_core::fighter::personality::{Personality, TraitValue};
use lanista_core::fighter::{ClassId, Fighter, FighterId, Stats};
use lanista_core::log::{EventKind, Report};
use proptest::prelude::*;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

fn stats() -> impl Strategy<Value = Stats> {
    (
        1..300u32,
        0..100u32,
        0..100u32,
        0..60u32,
        0..60u32,
        0..200u32,
        0..200u32,
    )
        .prop_map(
            |(hp, melee_attack, ranged_attack, accuracy, speed, melee_defense, ranged_defense)| {
                Stats {
                    hp,
                    melee_attack,
                    ranged_attack,
                    accuracy,
                    speed,
                    melee_defense,
                    ranged_defense,
                }
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
    let data = DesignData::load().unwrap();
    let fighters = (0..2u32)
        .zip(setup.into_iter().zip(stats))
        .map(|(id, ((class, position), stats))| {
            let class_data = data.class(ClassId(class)).unwrap();
            Fighter::new(FighterId(id), ClassId(class), class_data, stats, position)
        })
        .collect();
    let ais: Vec<Box<dyn CombatAi>> = vec![Box::new(UtilityAi), Box::new(UtilityAi)];
    let arena = Arena::new(ARENA_SIZE, ARENA_SIZE, fighters, seed).with_rules(data.combat);
    (arena, ais)
}

proptest! {
    #[test]
    fn hp_gauges_and_positions_stay_in_bounds_whatever_the_personalities(
        setup in setup(),
        stats in [stats(), stats()],
        seed: u64,
    ) {
        let (mut arena, mut ais) = duel(setup, stats, seed);
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        for f in &mut arena.fighters {
            f.condition.personality = Personality::random(&mut rng);
        }
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

    // ponytail: equal stats only and every attack hitting; with random stats or misses
    // two fighters can run out of resources without a kill, as nothing regenerates yet.
    #[test]
    fn a_fight_between_equals_who_never_miss_always_ends(setup in setup(), seed: u64) {
        let (arena, mut ais) = duel(setup, [STATS; 2], seed);
        let mut arena = arena.with_accuracy(AccuracyRules::ALWAYS);
        run(&mut arena, &mut ais, MAX_TICKS);
        prop_assert!(arena.is_over(), "still going after {} ticks", arena.tick);
    }

    #[test]
    fn a_fighter_down_is_never_hit_and_never_acts(
        setup in setup(),
        determination in [0..=100u32, 0..=100u32],
        seed: u64,
    ) {
        let (mut arena, mut ais) = duel(setup, [STATS; 2], seed);
        // Stuns at every blunt hit, to see many of them.
        arena.stun.blunt_hundredths = 1_000_000;
        for (f, d) in arena.fighters.iter_mut().zip(determination) {
            f.condition.personality.determination = TraitValue::new(d);
        }
        while !arena.is_over() && arena.tick < MAX_TICKS {
            let before = arena.fighters.clone();
            let logged = arena.log.len();
            tick(&mut arena, &mut ais);
            for (old, new) in before.iter().zip(&arena.fighters) {
                // Down for the whole tick: it did not get up at its start.
                if old.stunned_ticks > 1 {
                    prop_assert_eq!(new.hp, old.hp);
                    prop_assert_eq!(new.position, old.position);
                    let cast = arena.log[logged..].iter().any(|e| {
                        matches!(e.kind, EventKind::Cast { caster, .. } if caster == old.id)
                    });
                    prop_assert!(!cast);
                }
                if new.knocked_out {
                    prop_assert!(new.is_alive() && new.stuns > 0);
                }
            }
        }
    }

    #[test]
    fn a_beheaded_fighter_is_dead_and_one_that_lost_a_limb_is_out_alive(
        setup in setup(),
        seed: u64,
    ) {
        let (mut arena, mut ais) = duel(setup, [STATS; 2], seed);
        // Cuts far more often, to see many of them.
        arena.cut.limb_hundredths = 2_000;
        arena.cut.head_hundredths = 500;
        while !arena.is_over() && arena.tick < MAX_TICKS {
            let before = arena.fighters.clone();
            tick(&mut arena, &mut ais);
            for (old, new) in before.iter().zip(&arena.fighters) {
                // Out of the fight: never hit again.
                if old.severed.is_some() {
                    prop_assert_eq!(new.hp, old.hp);
                }
                match new.severed {
                    Some(Location::Head) => prop_assert!(!new.is_alive()),
                    Some(_) => prop_assert!(new.is_alive() && !new.is_fighting()),
                    None => {}
                }
            }
        }
        let report = Report::new(&arena);
        for f in &arena.fighters {
            prop_assert!(f.severed.is_none() || report.winner != Some(f.id));
        }
    }

    #[test]
    fn a_fighter_who_gave_up_is_never_hit_again(
        setup in setup(),
        thresholds in [0..=100u32, 0..=100u32],
        seed: u64,
    ) {
        let (mut arena, mut ais) = duel(setup, [STATS; 2], seed);
        for (f, threshold) in arena.fighters.iter_mut().zip(thresholds) {
            f.surrender_hp = threshold;
        }
        let mut hp_when_out: [Option<u32>; 2] = [None; 2];
        while !arena.is_over() && arena.tick < MAX_TICKS {
            tick(&mut arena, &mut ais);
            for (f, out) in arena.fighters.iter().zip(&mut hp_when_out) {
                if let Some(hp) = *out {
                    prop_assert_eq!(f.hp, hp);
                } else if f.surrendered {
                    prop_assert!(f.is_alive() && f.hp <= f.surrender_hp);
                    *out = Some(f.hp);
                }
            }
        }
    }
}
