//! The melee, or King of the hill: up to 20 fighters in the arena at once, each for
//! itself, who may make pacts with a neighbour and break them, and must keep dealing
//! damage or be out. Its floor is the colosseum's, grown with the number of fighters,
//! who start on a circle.

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
    pub quota: QuotaRules,
}

/// How a melee keeps its fighters fighting: every `every_ticks`, a fighter still in it
/// who dealt less than `min_damage` since the last check is out, a defeat alive (see
/// [`enforce_quota`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuotaRules {
    /// Ticks between two checks (30 per second).
    pub every_ticks: u32,
    /// Damage each fighter must deal between two checks.
    pub min_damage: u32,
}

impl QuotaRules {
    /// Nobody is ever put out: a duel.
    pub const NEVER: Self = Self {
        every_ticks: u32::MAX,
        min_damage: 0,
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
        .with_quota(rules.melee.quota)
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

/// At each check of the arena's `QuotaRules`, puts out every fighter still in the fight
/// who dealt less than `min_damage` since the last one (`Eliminated`), then starts the
/// count again. If all fall short, one stays and wins: who dealt the most, then who
/// has the largest share of its HP left, then one drawn among them, the only draw.
pub fn enforce_quota(arena: &mut Arena) {
    let rules = arena.quota;
    if rules.min_damage == 0 || arena.tick == 0 || !arena.tick.is_multiple_of(rules.every_ticks) {
        return;
    }
    let mut out: Vec<FighterId> = arena
        .fighters
        .iter()
        .filter(|f| f.is_fighting() && f.round_damage < rules.min_damage)
        .map(|f| f.id)
        .collect();
    let fighting = arena.fighters.iter().filter(|f| f.is_fighting()).count();
    if out.len() == fighting {
        let spared = last_one_in(arena, &out);
        out.retain(|&id| Some(id) != spared);
    }
    for id in out {
        if let Some(f) = arena.fighters.iter_mut().find(|f| f.id == id) {
            f.eliminated = true;
            f.action = crate::ai::Action::Wait;
        }
        log(arena, EventKind::Eliminated { fighter: id });
    }
    for f in &mut arena.fighters {
        f.round_damage = 0;
    }
}

/// Who of `short`, all still fighting and all short of the quota, stays: who dealt the
/// most, then who has the largest share of its HP left, then one drawn among them.
fn last_one_in(arena: &mut Arena, short: &[FighterId]) -> Option<FighterId> {
    // The HP share as hp / max, compared by cross products.
    let key = |f: &Fighter| {
        (
            f.round_damage,
            u64::from(f.hp),
            u64::from(f.stats.hp.max(1)),
        )
    };
    let better =
        |a: (u32, u64, u64), b: (u32, u64, u64)| a.0.cmp(&b.0).then((a.1 * b.2).cmp(&(b.1 * a.2)));
    let keys: Vec<(FighterId, (u32, u64, u64))> = short
        .iter()
        .filter_map(|&id| arena.fighter(id).map(|f| (id, key(f))))
        .collect();
    let best = keys.iter().map(|&(_, k)| k).max_by(|&a, &b| better(a, b))?;
    let tied: Vec<FighterId> = keys
        .iter()
        .filter(|&&(_, k)| better(k, best).is_eq())
        .map(|&(id, _)| id)
        .collect();
    if tied.len() < 2 {
        return tied.first().copied();
    }
    let top = u32::try_from(tied.len() - 1).unwrap_or(0);
    let drawn = arena.rng.random_range(0..=top);
    tied.get(usize::try_from(drawn).unwrap_or(0)).copied()
}

/// How pressed `fighter` is to deal damage before the next check: 0 once it has dealt
/// enough, or without a quota; else the share of the time between two checks gone by,
/// from 0 just after one to nearly 1 just before the next.
#[must_use]
pub fn quota_pressure(arena: &Arena, fighter: &Fighter) -> f64 {
    let rules = arena.quota;
    if rules.min_damage == 0 || fighter.round_damage >= rules.min_damage {
        return 0.0;
    }
    let every = rules.every_ticks.max(1);
    f64::from(arena.tick % every) / f64::from(every)
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

    /// Every 2 s, at least 10 damage.
    const QUOTA: QuotaRules = QuotaRules {
        every_ticks: 60,
        min_damage: 10,
    };

    #[test]
    fn a_fighter_who_deals_too_little_damage_is_out_at_the_check() {
        let fighters = vec![
            test_fighter(0, 5.0, 5.0),
            test_fighter(1, 8.0, 5.0),
            test_fighter(2, 15.0, 15.0),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0).with_quota(QUOTA);
        let mut ais = waiting(&arena);
        for _ in 0..60 {
            tick(&mut arena, &mut ais);
        }
        assert!(arena.fighters.iter().all(Fighter::is_fighting));
        arena.fighters[0].round_damage = 10;
        arena.fighters[1].round_damage = 9;
        // The check opens tick 60.
        tick(&mut arena, &mut ais);
        assert!(arena.fighters[0].is_fighting());
        assert!(arena.fighters[1].eliminated && arena.fighters[2].eliminated);
        assert!(!arena.fighters[1].is_fighting() && arena.fighters[1].is_alive());
        assert!(arena.is_over());
        assert!(arena.fighters.iter().all(|f| f.round_damage == 0));
        let out: Vec<_> = arena
            .log
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::Eliminated { fighter } => Some((e.tick, fighter)),
                _ => None,
            })
            .collect();
        assert_eq!(out, [(60, FighterId(1)), (60, FighterId(2))]);
    }

    #[test]
    fn when_all_fall_short_one_stays_and_wins() {
        let fighters: Vec<Fighter> = (0..4)
            .map(|i| test_fighter(i, 2.0 + 3.0 * f64::from(i), 5.0))
            .collect();
        let quota = Arena::new(20.0, 20.0, fighters, 0).with_quota(QUOTA);
        let left = |dealt: [u32; 4], hp: [u32; 4]| {
            let mut arena = quota.clone();
            arena.tick = 60;
            for ((f, dealt), hp) in arena.fighters.iter_mut().zip(dealt).zip(hp) {
                f.round_damage = dealt;
                f.hp = hp;
            }
            let before = arena.rng.clone();
            enforce_quota(&mut arena);
            assert!(arena.is_over());
            let drew = arena.rng != before;
            let fighting: Vec<bool> = arena.fighters.iter().map(Fighter::is_fighting).collect();
            (fighting, drew)
        };
        // Who dealt the most, then who has more HP left, with no draw.
        let (fighting, drew) = left([4, 7, 7, 0], [100, 50, 60, 100]);
        assert_eq!(fighting, [false, false, true, false]);
        assert!(!drew);
        // A perfect tie is drawn.
        let (fighting, drew) = left([0, 0, 0, 0], [80, 80, 80, 80]);
        assert_eq!(fighting.iter().filter(|&&f| f).count(), 1);
        assert!(drew);
    }

    #[test]
    fn the_pressure_grows_until_the_check_unless_the_quota_is_met() {
        let fighters = vec![test_fighter(0, 5.0, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.tick = 110;
        assert!(quota_pressure(&arena, &arena.fighters[0]).abs() < 1e-12);
        arena = arena.with_quota(QUOTA);
        arena.tick = 61;
        let early = quota_pressure(&arena, &arena.fighters[0]);
        arena.tick = 110;
        let late = quota_pressure(&arena, &arena.fighters[0]);
        assert!(early < 0.1 && late > 0.8, "{early} {late}");
        arena.fighters[0].round_damage = 10;
        assert!(quota_pressure(&arena, &arena.fighters[0]).abs() < 1e-12);
    }

    #[test]
    fn an_arena_without_quota_puts_nobody_out() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 15.0, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        let mut ais = waiting(&arena);
        for _ in 0..1800 {
            tick(&mut arena, &mut ais);
        }
        assert!(arena.fighters.iter().all(Fighter::is_fighting));
    }

    #[test]
    fn a_melee_of_twenty_always_ends_with_one_standing() {
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
        // The quota puts out those who wait, and leaves one if all fall short.
        assert_eq!(ended, 20);
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
