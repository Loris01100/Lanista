//! The melee, or King of the hill: up to 20 fighters in the arena at once, each for
//! itself, who may make pacts with a neighbour and break them. Its floor is the
//! colosseum's, grown with the number of fighters, who start on a circle.

use rand::RngExt;
use serde::Deserialize;

use crate::combat::{Arena, ArenaKind, ArenaLayout, CombatRules, Pillar, Vec2, edge_distance};
use crate::fighter::personality::Personality;
use crate::fighter::{Fighter, FighterId};
use crate::log::{Event, EventKind};

/// Melee rules, from `data/combat.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeRules {
    /// Most fighters in one melee.
    pub max_fighters: u32,
    /// The floor grows so that each fighter has at least this many square meters.
    pub square_meters_per_fighter: f64,
    /// Ticks a melee lasts at most (30 per second).
    pub max_ticks: u32,
    pub pacts: PactRules,
    pub shrink: ShrinkRules,
}

/// How a melee's walls close in, the guards tightening the ring, so that the last
/// fighters, however wary, end up within reach of each other.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShrinkRules {
    /// Ticks before the walls start moving (30 per second).
    pub after_ticks: u32,
    /// How fast each wall moves in, in meters per second.
    pub meters_per_second: f64,
    /// The floor's shorter side stops shrinking at this length, in meters.
    pub min_side: f64,
}

impl ShrinkRules {
    /// The walls never move: a duel.
    pub const NEVER: Self = Self {
        after_ticks: u32::MAX,
        meters_per_second: 0.0,
        min_side: 0.0,
    };
}

/// How fighters make and break pacts in a melee (see [`update_pacts`]).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PactRules {
    /// Two fighters this close, in meters between their edges, may make a pact.
    pub distance: f64,
    /// Chance, in percent at each decision, that two such fighters make one, from
    /// caution 0 to caution 100 of the less cautious.
    pub form_percent_least_cautious: u32,
    pub form_percent_most_cautious: u32,
    /// Chance, in percent at each decision, that a pact is broken, at aggression 100
    /// of the more aggressive; none at aggression 0.
    pub betrayal_percent_most_aggressive: u32,
    /// Every pact breaks once this many fighters, or fewer, are still fighting.
    pub break_fighting: u32,
}

impl PactRules {
    /// No pact is ever made: a duel, or a melee without alliances.
    pub const NEVER: Self = Self {
        distance: 0.0,
        form_percent_least_cautious: 0,
        form_percent_most_cautious: 0,
        betrayal_percent_most_aggressive: 0,
        break_fighting: 2,
    };
}

/// The floor of a melee of `count` fighters on `layout`: grown, pillars with it, so
/// that each fighter has `square_meters_per_fighter`, never shrunk.
#[must_use]
pub fn melee_layout(layout: &ArenaLayout, count: usize, rules: &MeleeRules) -> ArenaLayout {
    let wanted =
        f64::from(u32::try_from(count).unwrap_or(u32::MAX)) * rules.square_meters_per_fighter;
    let area = layout.width * layout.height;
    let scale = if area > 0.0 && wanted > area {
        libm::sqrt(wanted / area)
    } else {
        1.0
    };
    ArenaLayout {
        width: layout.width * scale,
        height: layout.height * scale,
        speed_percent: layout.speed_percent,
        pillars: layout
            .pillars
            .iter()
            .map(|p| Pillar {
                x: p.x * scale,
                y: p.y * scale,
                radius: p.radius,
            })
            .collect(),
    }
}

/// Where `count` fighters start on `layout`: evenly spaced on a circle around the
/// middle, the first on the left, the circle filling 80 % of the shorter side.
#[must_use]
pub fn start_positions(layout: &ArenaLayout, count: usize) -> Vec<Vec2> {
    let (cx, cy) = (layout.width / 2.0, layout.height / 2.0);
    let radius = 0.4 * layout.width.min(layout.height);
    let n = f64::from(u32::try_from(count).unwrap_or(u32::MAX)).max(1.0);
    (0..count)
        .map(|k| {
            let k = f64::from(u32::try_from(k).unwrap_or(u32::MAX));
            let angle = std::f64::consts::PI + std::f64::consts::TAU * k / n;
            Vec2::new(
                cx + radius * libm::cos(angle),
                cy + radius * libm::sin(angle),
            )
        })
        .collect()
}

/// The melee of `fighters` (each `FighterId` its index) on the floor of `kind`, grown
/// for them (see [`melee_layout`]), each placed on the starting circle in order (see
/// [`start_positions`]), with `rules` for hits, stuns, cuts and pacts. Seeded with
/// `seed`.
#[must_use]
pub fn melee_arena(
    mut fighters: Vec<Fighter>,
    kind: ArenaKind,
    seed: u64,
    rules: &CombatRules,
) -> Arena {
    let layout = melee_layout(rules.arenas.get(kind), fighters.len(), &rules.melee);
    let positions = start_positions(&layout, fighters.len());
    for (fighter, position) in fighters.iter_mut().zip(positions) {
        fighter.position = position;
    }
    Arena::new(layout.width, layout.height, fighters, seed)
        .with_rules(rules)
        .with_layout(&layout)
        .with_pacts(rules.melee.pacts)
        .with_shrink(rules.melee.shrink)
        .with_max_ticks(rules.melee.max_ticks)
}

/// A chance in percent, from `low` at trait 0 to `high` at trait 100.
fn trait_percent(low: u32, high: u32, value: u32) -> u64 {
    let (low, high) = (u64::from(low), u64::from(high));
    let value = u64::from(value.min(Personality::MAX));
    let max = u64::from(Personality::MAX);
    (low * (max - value) + high * value) / max
}

/// Draws a chance in percent on the arena's generator, only strictly between 0 and 100.
fn happens(arena: &mut Arena, percent: u64) -> bool {
    percent >= 100 || percent > 0 && u64::from(arena.rng.random_range(0..100_u32)) < percent
}

/// Updates the pacts at a decision, before the fighters decide, in this order: a pact
/// with a fighter out of the fight ends; each other pact is betrayed with the chance
/// of its more aggressive member (`PactBroken`); all break once `break_fighting` or
/// fewer fighters are still fighting (`PactBroken`); else, in index order, two
/// standing fighters without a pact, within `distance`, make one with the chance of
/// their less cautious member (`Pact`). Draws only for chances strictly between 0 and
/// 100, so an arena without pacts draws nothing.
pub fn update_pacts(arena: &mut Arena) {
    let rules = arena.pact_rules;
    let mut kept = Vec::with_capacity(arena.pacts.len());
    for (a, b) in std::mem::take(&mut arena.pacts) {
        let (Some(fa), Some(fb)) = (arena.fighter(a), arena.fighter(b)) else {
            continue;
        };
        if !fa.is_fighting() || !fb.is_fighting() {
            continue;
        }
        let aggression = |f: &crate::fighter::Fighter| f.condition.personality.aggression.value();
        let bolder = aggression(fa).max(aggression(fb));
        let betrayal = trait_percent(0, rules.betrayal_percent_most_aggressive, bolder);
        if happens(arena, betrayal) {
            log(arena, EventKind::PactBroken { a, b });
        } else {
            kept.push((a, b));
        }
    }
    let fighting = arena.fighters.iter().filter(|f| f.is_fighting()).count();
    if fighting <= usize::try_from(rules.break_fighting).unwrap_or(usize::MAX) {
        for (a, b) in kept {
            log(arena, EventKind::PactBroken { a, b });
        }
        return;
    }
    arena.pacts = kept;
    for i in 0..arena.fighters.len() {
        for j in i + 1..arena.fighters.len() {
            let (fa, fb) = (&arena.fighters[i], &arena.fighters[j]);
            let free = |id: FighterId| !arena.pacts.iter().any(|&(a, b)| a == id || b == id);
            if !fa.is_standing() || !fb.is_standing() || !free(fa.id) || !free(fb.id) {
                continue;
            }
            if edge_distance(fa, fb) > rules.distance {
                continue;
            }
            let caution = |f: &crate::fighter::Fighter| f.condition.personality.caution.value();
            let wary = caution(fa).min(caution(fb));
            let chance = trait_percent(
                rules.form_percent_least_cautious,
                rules.form_percent_most_cautious,
                wary,
            );
            let (a, b) = (fa.id, fb.id);
            if happens(arena, chance) {
                arena.pacts.push((a, b));
                log(arena, EventKind::Pact { a, b });
            }
        }
    }
}

fn log(arena: &mut Arena, kind: EventKind) {
    let tick = arena.tick;
    arena.log.push(Event { tick, kind });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{CombatAi, UtilityAi};
    use crate::combat::{TICKS_PER_DECISION, run, tick};
    use crate::design_data::DesignData;
    use crate::fighter::personality::TraitValue;
    use crate::fighter::test_fighter;

    /// Pacts made for sure between fighters within 2 m, never betrayed.
    const SURE: PactRules = PactRules {
        distance: 2.0,
        form_percent_least_cautious: 100,
        form_percent_most_cautious: 100,
        betrayal_percent_most_aggressive: 0,
        break_fighting: 2,
    };

    fn waiting(arena: &Arena) -> Vec<Box<dyn CombatAi>> {
        arena
            .fighters
            .iter()
            .map(|_| Box::new(Still) as Box<dyn CombatAi>)
            .collect()
    }

    struct Still;

    impl CombatAi for Still {
        fn decide(&mut self, _: &Arena, _: FighterId) -> crate::ai::Action {
            crate::ai::Action::Wait
        }
    }

    #[test]
    fn neighbours_make_a_pact_each_with_one_ally_at_most() {
        // 0, 1 and 2 close together, 3 far away.
        let fighters = vec![
            test_fighter(0, 5.0, 5.0),
            test_fighter(1, 6.5, 5.0),
            test_fighter(2, 8.0, 5.0),
            test_fighter(3, 18.0, 18.0),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0).with_pacts(SURE);
        let mut ais = waiting(&arena);
        tick(&mut arena, &mut ais);
        assert_eq!(arena.pacts, [(FighterId(0), FighterId(1))]);
        assert!(arena.allied(FighterId(1), FighterId(0)));
        assert!(!arena.allied(FighterId(1), FighterId(2)));
        assert_eq!(
            arena.log[0].kind,
            EventKind::Pact {
                a: FighterId(0),
                b: FighterId(1)
            }
        );
    }

    #[test]
    fn pacts_break_when_few_are_left_or_an_ally_is_out() {
        let fighters = vec![
            test_fighter(0, 5.0, 5.0),
            test_fighter(1, 6.5, 5.0),
            test_fighter(2, 15.0, 15.0),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0).with_pacts(SURE);
        let mut ais = waiting(&arena);
        tick(&mut arena, &mut ais);
        assert_eq!(arena.pacts.len(), 1);
        // Fighter 2 out: two left, the pact breaks.
        arena.fighters[2].hp = 0;
        for _ in 0..TICKS_PER_DECISION {
            tick(&mut arena, &mut ais);
        }
        assert_eq!(arena.pacts, []);
        assert!(
            arena
                .log
                .iter()
                .any(|e| matches!(e.kind, EventKind::PactBroken { .. }))
        );
    }

    #[test]
    fn the_aggressive_betray() {
        let fighters = vec![
            test_fighter(0, 5.0, 5.0),
            test_fighter(1, 6.5, 5.0),
            test_fighter(2, 12.0, 15.0),
            test_fighter(3, 19.0, 19.0),
        ];
        let treacherous = PactRules {
            betrayal_percent_most_aggressive: 100,
            ..SURE
        };
        let mut arena = Arena::new(20.0, 20.0, fighters, 0).with_pacts(treacherous);
        arena.fighters[1].condition.personality.aggression = TraitValue::new(100);
        let mut ais = waiting(&arena);
        tick(&mut arena, &mut ais);
        for _ in 0..TICKS_PER_DECISION {
            tick(&mut arena, &mut ais);
        }
        // Betrayed at the second decision, then made again: they are still neighbours.
        let kinds: Vec<EventKind> = arena.log.iter().map(|e| e.kind).collect();
        let (a, b) = (FighterId(0), FighterId(1));
        assert_eq!(
            kinds,
            [
                EventKind::Pact { a, b },
                EventKind::PactBroken { a, b },
                EventKind::Pact { a, b },
            ]
        );
    }

    #[test]
    fn allies_do_not_aim_at_each_other() {
        let fighters = vec![
            test_fighter(0, 5.0, 5.0),
            test_fighter(1, 6.5, 5.0),
            test_fighter(2, 9.0, 5.0),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0).with_pacts(SURE);
        let mut ais: Vec<Box<dyn CombatAi>> = (0..3)
            .map(|_| Box::new(UtilityAi) as Box<dyn CombatAi>)
            .collect();
        run(&mut arena, &mut ais, 300);
        // Allied from the first tick, 0 and 1 only ever hit 2, until the pact breaks.
        let broken = arena
            .log
            .iter()
            .position(|e| matches!(e.kind, EventKind::PactBroken { .. }))
            .unwrap_or(arena.log.len());
        for event in &arena.log[..broken] {
            if let EventKind::Cast { caster, target, .. } = event.kind {
                assert!(!(caster.0 < 2 && target.0 < 2), "{event:?}");
            }
        }
    }

    #[test]
    fn an_arena_without_pacts_draws_nothing_for_them() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 9);
        let before = arena.rng.clone();
        update_pacts(&mut arena);
        assert_eq!(arena.rng, before);
        assert!(arena.pacts.is_empty() && arena.log.is_empty());
    }

    #[test]
    fn the_walls_close_in_and_push_the_fighters_inward() {
        let shrink = ShrinkRules {
            after_ticks: 30,
            meters_per_second: 3.0,
            min_side: 4.0,
        };
        let fighters = vec![test_fighter(0, 1.0, 1.0), test_fighter(1, 29.0, 19.0)];
        let mut arena = Arena::new(30.0, 20.0, fighters, 0).with_shrink(shrink);
        let mut ais = waiting(&arena);
        for _ in 0..30 {
            tick(&mut arena, &mut ais);
        }
        assert_eq!(arena.inset, Vec2::new(0.0, 0.0));
        // Three meters per second for two seconds.
        for _ in 0..60 {
            tick(&mut arena, &mut ais);
        }
        assert!((arena.inset.x - 6.0).abs() < 1e-9, "{:?}", arena.inset);
        assert!((arena.fighters[0].position.x - 6.5).abs() < 1e-9);
        // The short walls stop at a 4 m floor, the long ones go on: a square at last.
        for _ in 0..30 {
            tick(&mut arena, &mut ais);
        }
        assert!((arena.inset.x - 9.0).abs() < 1e-9, "{:?}", arena.inset);
        assert_eq!(arena.inset.y, 8.0);
        for _ in 0..600 {
            tick(&mut arena, &mut ais);
        }
        assert_eq!(arena.inset, Vec2::new(13.0, 8.0));
        let last = arena.fighters[1].position;
        assert!(
            (last.x - 16.5).abs() < 1e-9 && (last.y - 11.5).abs() < 1e-9,
            "{last:?}"
        );
    }

    #[test]
    fn a_melee_of_twenty_ends_with_one_standing_almost_always() {
        use crate::fighter::personality::Personality;
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        let data = DesignData::load().unwrap();
        let ended = (0..20_u64)
            .filter(|&seed| {
                let mut rng = ChaCha8Rng::seed_from_u64(seed);
                let fighters = (0..20)
                    .map(|id| {
                        let class = crate::fighter::ClassId(rng.random_range(0..2));
                        let class_data = data.class(class).unwrap();
                        let at = Vec2::new(0.0, 0.0);
                        let stats = crate::duel::STATS;
                        let mut f = Fighter::new(FighterId(id), class, class_data, stats, at);
                        let personality = Personality::random(&mut rng);
                        f.surrender_hp = personality.surrender_hp(100, &data.fighter.personality);
                        f.condition.personality = personality;
                        f
                    })
                    .collect();
                let kind = ArenaKind::ALL[usize::try_from(seed % 3).unwrap()];
                let mut arena = melee_arena(fighters, kind, seed, &data.combat);
                let mut ais: Vec<Box<dyn CombatAi>> = (0..20)
                    .map(|_| Box::new(UtilityAi) as Box<dyn CombatAi>)
                    .collect();
                run(&mut arena, &mut ais, data.combat.melee.max_ticks);
                arena.is_over()
            })
            .count();
        // The walls closing in bring the last wary fighters within reach.
        assert!(ended >= 18, "{ended} of 20");
    }

    #[test]
    fn a_melee_floor_grows_with_its_fighters_and_they_start_on_a_circle() {
        let data = DesignData::load().unwrap();
        let rules = &data.combat.melee;
        let pillars = &data.combat.arenas.pillars;
        let duel = melee_layout(pillars, 2, rules);
        assert_eq!(&duel, pillars);
        let twenty = melee_layout(pillars, 20, rules);
        let area = twenty.width * twenty.height;
        assert!(
            (area - 20.0 * rules.square_meters_per_fighter).abs() < 1e-6,
            "{area}"
        );
        assert!((twenty.width / twenty.height - pillars.width / pillars.height).abs() < 1e-9);
        assert_eq!(twenty.pillars.len(), pillars.pillars.len());
        let positions = start_positions(&twenty, 20);
        assert!(positions[0].x < twenty.width / 2.0);
        for (i, p) in positions.iter().enumerate() {
            for q in &positions[i + 1..] {
                let gap = Vec2::new(p.x - q.x, p.y - q.y).length();
                assert!(gap > 2.0 * crate::combat::RADIUS, "{p:?} {q:?}");
            }
        }
    }
}
