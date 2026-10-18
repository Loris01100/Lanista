//! Invariants checked on random duels between utility AIs.

use lanista_core::ai::{CombatAi, UtilityAi};
use lanista_core::combat::{
    AccuracyRules, Arena, ArenaKind, RADIUS, Range, TICKS_PER_DECISION, Vec2, run, shot_blocked,
    tick,
};
use lanista_core::design_data::DesignData;
use lanista_core::duel::{ARENA_SIZE, MAX_TICKS, STATS};
use lanista_core::fighter::injury::Location;
use lanista_core::fighter::personality::{Personality, TraitValue};
use lanista_core::fighter::{ClassId, Fighter, FighterId, Stats};
use lanista_core::log::{EventKind, Report};
use lanista_core::melee::melee_arena;
use proptest::prelude::*;
use rand::{RngExt, SeedableRng};
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
    let arena = Arena::new(ARENA_SIZE, ARENA_SIZE, fighters, seed).with_rules(&data.combat);
    (arena, ais)
}

/// A melee of `count` fighters of random classes and personalities on `kind`'s floor.
fn melee(kind: ArenaKind, count: u32, seed: u64) -> (Arena, Vec<Box<dyn CombatAi>>) {
    let data = DesignData::load().unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let fighters = (0..count)
        .map(|id| {
            let class = ClassId(rng.random_range(0..2));
            let class_data = data.class(class).unwrap();
            let mut f = Fighter::new(FighterId(id), class, class_data, STATS, Vec2::new(0.0, 0.0));
            f.condition.personality = Personality::random(&mut rng);
            f
        })
        .collect();
    let arena = melee_arena(fighters, kind, seed, &data.combat);
    let ais = (0..count)
        .map(|_| Box::new(UtilityAi) as Box<dyn CombatAi>)
        .collect();
    (arena, ais)
}

fn kind() -> impl Strategy<Value = ArenaKind> {
    prop::sample::select(ArenaKind::ALL.to_vec())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// In a melee, each fighter has one ally at most, a pact only binds two fighters
    /// still fighting, allies never hit each other, HP stay in bounds, everyone stays
    /// inside the walls, and the checks leave at most the final's fighters.
    #[test]
    fn a_melee_keeps_its_pacts_and_bounds(kind in kind(), count in 3..=20u32, seed: u64) {
        let (mut arena, mut ais) = melee(kind, count, seed);
        let rules = DesignData::load().unwrap().combat.melee;
        let few = usize::try_from(rules.pacts.break_fighting).unwrap();
        while !arena.is_over() && arena.tick < rules.max_ticks {
            let logged = arena.log.len();
            let decided = arena.tick.is_multiple_of(TICKS_PER_DECISION);
            let fighting = arena.fighters.iter().filter(|f| f.is_fighting()).count();
            tick(&mut arena, &mut ais);
            // Pacts are updated at decisions: none once few are left.
            if decided && fighting <= few {
                prop_assert_eq!(&arena.pacts, &[]);
            }
            for (i, &(a, b)) in arena.pacts.iter().enumerate() {
                prop_assert!(a != b);
                for &(c, d) in &arena.pacts[i + 1..] {
                    prop_assert!(a != c && a != d && b != c && b != d);
                }
            }
            for event in &arena.log[logged..] {
                if let EventKind::Cast { caster, target, .. } = event.kind {
                    prop_assert!(!arena.allied(caster, target));
                }
            }
            for f in &arena.fighters {
                prop_assert!(f.hp <= f.stats.hp);
                prop_assert!((RADIUS..=arena.width - RADIUS).contains(&f.position.x));
                prop_assert!((RADIUS..=arena.height - RADIUS).contains(&f.position.y));
            }
        }
        let fighting = arena.fighters.iter().filter(|f| f.is_fighting()).count();
        prop_assert!(fighting <= usize::try_from(rules.elimination.final_fighters).unwrap());
    }
}

proptest! {
    /// On every floor, fighters stay inside the walls and out of the pillars, and no
    /// ranged spell lands through a pillar.
    #[test]
    fn fighters_keep_out_of_pillars_and_shots_never_go_through(
        kind in kind(),
        classes in [0..2u32, 0..2u32],
        heights in [0.1..0.9f64, 0.1..0.9f64],
        seed: u64,
    ) {
        let data = DesignData::load().unwrap();
        let layout = data.combat.arenas.get(kind);
        let fighters = (0..2u32)
            .zip(classes.into_iter().zip(heights))
            .map(|(id, (class, height))| {
                let x = layout.width * if id == 0 { 0.15 } else { 0.85 };
                let position = Vec2::new(x, layout.height * height);
                let class_data = data.class(ClassId(class)).unwrap();
                Fighter::new(FighterId(id), ClassId(class), class_data, STATS, position)
            })
            .collect();
        let mut arena = Arena::new(1.0, 1.0, fighters, seed)
            .with_rules(&data.combat)
            .with_layout(layout);
        let mut ais: Vec<Box<dyn CombatAi>> = vec![Box::new(UtilityAi), Box::new(UtilityAi)];
        while !arena.is_over() && arena.tick < MAX_TICKS {
            let logged = arena.log.len();
            tick(&mut arena, &mut ais);
            for f in &arena.fighters {
                prop_assert!((RADIUS..=layout.width - RADIUS).contains(&f.position.x));
                prop_assert!((RADIUS..=layout.height - RADIUS).contains(&f.position.y));
                for p in &layout.pillars {
                    let off = Vec2::new(f.position.x - p.x, f.position.y - p.y);
                    prop_assert!(off.length() >= p.radius + RADIUS - 1e-9);
                }
            }
            for event in &arena.log[logged..] {
                if let EventKind::Cast { caster, target, spell, missed: false, .. } = event.kind {
                    let (c, t) = (arena.fighter(caster).unwrap(), arena.fighter(target).unwrap());
                    if c.spells[spell].range == Range::Ranged {
                        prop_assert!(!shot_blocked(&arena.pillars, c.position, t.position));
                    }
                }
            }
        }
    }

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
