//! The melee, or King of the hill: up to 20 fighters in the arena at once, each for
//! itself, who may make pacts with a neighbour and break them; every 15 seconds, those
//! who dealt the least damage are out, down to a final duel. Its floor is the
//! colosseum's, grown with the number of fighters, who start on a circle.

use std::cmp::Ordering;

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
    /// Only the first this many places bring money and fame (see
    /// [`place_share`](crate::fighter::melee::place_share)).
    pub paid_places: u32,
    pub pacts: PactRules,
    pub elimination: EliminationRules,
}

/// How a melee keeps its fighters fighting: every `every_ticks`, the next number of
/// `out` who dealt the least damage since the last check are out, a defeat alive, never
/// below `final_fighters`, who then fight the final (see [`eliminate`]).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EliminationRules {
    /// Ticks between two checks (30 per second).
    pub every_ticks: u32,
    /// Fighters put out at each check, in order; past the last, none.
    pub out: Vec<u32>,
    /// Fighters left for the final, whom no check puts out.
    pub final_fighters: u32,
}

impl EliminationRules {
    /// Nobody is ever put out: a duel.
    pub const NEVER: Self = Self {
        every_ticks: u32::MAX,
        out: Vec::new(),
        final_fighters: 0,
    };
}

/// The next check of a melee, as the fight stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cut {
    /// Ticks before it.
    pub ticks_left: u32,
    /// Fighters it puts out.
    pub out: u32,
    /// The most damage one it would put out has dealt: to stay, deal more. 0 when it
    /// puts nobody out.
    pub threshold: u32,
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
        element: layout.element,
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
        .with_elimination(rules.melee.elimination.clone())
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
/// of its more aggressive member, who betrays it (`PactBroken`); all break once `break_fighting` or
/// fewer fighters are still fighting (`PactBroken`); else, in index order, two
/// standing fighters without a pact, within `distance`, make one with the chance of
/// their less cautious member, plus their `Arena::pact_points` (`Pact`). Draws only for chances strictly between 0 and
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
        let (bolder, betrayer) = if aggression(fb) > aggression(fa) {
            (aggression(fb), b)
        } else {
            (aggression(fa), a)
        };
        let betrayal = trait_percent(0, rules.betrayal_percent_most_aggressive, bolder);
        if happens(arena, betrayal) {
            let betrayer = Some(betrayer);
            log(arena, EventKind::PactBroken { a, b, betrayer });
        } else {
            kept.push((a, b));
        }
    }
    let fighting = arena.fighters.iter().filter(|f| f.is_fighting()).count();
    if fighting <= usize::try_from(rules.break_fighting).unwrap_or(usize::MAX) {
        for (a, b) in kept {
            let betrayer = None;
            log(arena, EventKind::PactBroken { a, b, betrayer });
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
            let points = arena
                .pact_points
                .iter()
                .find(|&&(x, y, _)| (x, y) == (a, b) || (x, y) == (b, a))
                .map_or(0, |&(_, _, p)| i64::from(p));
            let chance =
                u64::try_from((i64::try_from(chance).unwrap_or(0) + points).max(0)).unwrap_or(0);
            if happens(arena, chance) {
                arena.pacts.push((a, b));
                log(arena, EventKind::Pact { a, b });
            }
        }
    }
}

/// Fighters check `number` (1 the first) of `arena` puts out of those `fighting`: its
/// number, without going below the final's; 0 past the last check.
fn out_at(arena: &Arena, number: u32, fighting: usize) -> usize {
    let rules = &arena.elimination;
    let wanted = number
        .checked_sub(1)
        .and_then(|k| rules.out.get(usize::try_from(k).ok()?))
        .map_or(0, |&n| usize::try_from(n).unwrap_or(usize::MAX));
    let kept = usize::try_from(rules.final_fighters).unwrap_or(usize::MAX);
    wanted.min(fighting.saturating_sub(kept))
}

/// Whether `arena` is a melee down to its final: a check would put nobody out any more.
#[must_use]
pub fn in_final(arena: &Arena) -> bool {
    let rules = &arena.elimination;
    let fighting = arena.fighters.iter().filter(|f| f.is_fighting()).count();
    !rules.out.is_empty() && fighting <= usize::try_from(rules.final_fighters).unwrap_or(usize::MAX)
}

/// Who goes out first: who dealt less damage since the last check, then who has the
/// smaller share of its HP left.
fn rank(a: &Fighter, b: &Fighter) -> Ordering {
    // The HP shares, hp / max, compared by cross products.
    let (ha, ma) = (u64::from(a.hp), u64::from(a.stats.hp.max(1)));
    let (hb, mb) = (u64::from(b.hp), u64::from(b.stats.hp.max(1)));
    a.round_damage
        .cmp(&b.round_damage)
        .then((ha * mb).cmp(&(hb * ma)))
}

/// The fighters still in the fight, by index, in [`rank`] order, equals in index order.
fn ranked(arena: &Arena) -> Vec<usize> {
    let fighters = &arena.fighters;
    let mut order: Vec<usize> = (0..fighters.len())
        .filter(|&i| fighters[i].is_fighting())
        .collect();
    order.sort_by(|&a, &b| rank(&fighters[a], &fighters[b]));
    order
}

/// The next check of `arena` that puts someone out, `None` without one: a duel, or the
/// final of a melee.
#[must_use]
pub fn next_cut(arena: &Arena) -> Option<Cut> {
    cut_of(arena, &ranked(arena))
}

/// [`next_cut`], the fighters still in the fight in `order` ([`ranked`]).
fn cut_of(arena: &Arena, order: &[usize]) -> Option<Cut> {
    let every = arena.elimination.every_ticks.max(1);
    let number = arena.tick / every + 1;
    let out = out_at(arena, number, order.len());
    if out == 0 {
        return None;
    }
    let threshold = out
        .checked_sub(1)
        .and_then(|k| order.get(k))
        .map_or(0, |&i| arena.fighters[i].round_damage);
    Some(Cut {
        ticks_left: number.saturating_mul(every) - arena.tick,
        out: u32::try_from(out).unwrap_or(u32::MAX),
        threshold,
    })
}

/// Whether `fighter` would be out if the next check came now, those tied with the last
/// one out included.
#[must_use]
pub fn in_danger(arena: &Arena, fighter: &Fighter) -> bool {
    if !fighter.is_fighting() {
        return false;
    }
    let order = ranked(arena);
    let Some(cut) = cut_of(arena, &order) else {
        return false;
    };
    let last = usize::try_from(cut.out - 1).ok().and_then(|k| order.get(k));
    last.is_some_and(|&i| rank(fighter, &arena.fighters[i]) != Ordering::Greater)
}

/// At each check of the arena's `EliminationRules`, at the end of every `every_ticks`
/// ticks, puts out its number of fighters in [`rank`] order (`Eliminated`), then starts
/// counting damage again. Of those equal at the edge, the ones out are drawn, the only
/// draw.
pub fn eliminate(arena: &mut Arena) {
    let every = arena.elimination.every_ticks;
    let played = arena.tick.saturating_add(1);
    if !played.is_multiple_of(every) {
        return;
    }
    let order = ranked(arena);
    let count = out_at(arena, played / every, order.len());
    if let Some(&edge) = count.checked_sub(1).and_then(|k| order.get(k)) {
        let edge = &arena.fighters[edge];
        let side = |i: &usize| rank(&arena.fighters[*i], edge);
        let mut out: Vec<usize> = order.iter().copied().filter(|i| side(i).is_lt()).collect();
        let mut tied: Vec<usize> = order.iter().copied().filter(|i| side(i).is_eq()).collect();
        let need = count - out.len();
        if need < tied.len() {
            shuffle(arena, &mut tied);
        }
        out.extend(tied.into_iter().take(need));
        out.sort_unstable();
        for i in out {
            let f = &mut arena.fighters[i];
            f.eliminated = true;
            f.action = crate::ai::Action::Wait;
            let fighter = f.id;
            log(arena, EventKind::Eliminated { fighter });
        }
    }
    for f in &mut arena.fighters {
        f.round_damage = 0;
    }
}

/// Shuffles `items` on the arena's generator (Fisher-Yates on u32 draws, the same on
/// every platform).
fn shuffle(arena: &mut Arena, items: &mut [usize]) {
    for k in (1..items.len()).rev() {
        let Ok(top) = u32::try_from(k) else { continue };
        let j = arena.rng.random_range(0..=top);
        items.swap(k, usize::try_from(j).unwrap_or(k));
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
    fn a_club_s_standing_moves_the_chance_of_a_pact() {
        // A third fighter far away, so that pacts may be made at all.
        let pair = || {
            vec![
                test_fighter(0, 5.0, 5.0),
                test_fighter(1, 6.5, 5.0),
                test_fighter(2, 18.0, 18.0),
            ]
        };
        let never = PactRules {
            form_percent_most_cautious: 0,
            ..SURE
        };
        let mut friends = Arena::new(20.0, 20.0, pair(), 0).with_pacts(never);
        friends.pact_points.push((FighterId(1), FighterId(0), 100));
        update_pacts(&mut friends);
        assert_eq!(friends.pacts, [(FighterId(0), FighterId(1))]);
        let mut rivals = Arena::new(20.0, 20.0, pair(), 0).with_pacts(SURE);
        rivals.pact_points.push((FighterId(0), FighterId(1), -100));
        update_pacts(&mut rivals);
        assert_eq!(rivals.pacts, []);
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
                EventKind::PactBroken {
                    a,
                    b,
                    betrayer: Some(b)
                },
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

    /// Every 2 s, 2 then 3 out, down to a final of 2.
    fn every_two_seconds() -> EliminationRules {
        EliminationRules {
            every_ticks: 60,
            out: vec![2, 3, 3],
            final_fighters: 2,
        }
    }

    fn six() -> Arena {
        let fighters = (0..6)
            .map(|i| test_fighter(i, 2.0 + 3.0 * f64::from(i), 5.0))
            .collect();
        Arena::new(20.0, 20.0, fighters, 0).with_elimination(every_two_seconds())
    }

    fn eliminated(arena: &Arena) -> Vec<(u32, u32)> {
        arena
            .log
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::Eliminated { fighter } => Some((e.tick, fighter.0)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn those_who_dealt_the_least_are_out_at_each_check_down_to_a_final() {
        let mut arena = six();
        let mut ais = waiting(&arena);
        for _ in 0..59 {
            tick(&mut arena, &mut ais);
        }
        for (f, dealt) in arena.fighters.iter_mut().zip([0, 5, 9, 9, 20, 3]) {
            f.round_damage = dealt;
        }
        let cut = next_cut(&arena).unwrap();
        assert_eq!(
            cut,
            Cut {
                ticks_left: 1,
                out: 2,
                threshold: 3
            }
        );
        tick(&mut arena, &mut ais);
        assert_eq!(eliminated(&arena), [(59, 0), (59, 5)]);
        assert!(arena.fighters[0].is_alive() && !arena.fighters[0].is_fighting());
        assert!(arena.fighters.iter().all(|f| f.round_damage == 0));
        // One gives up meanwhile: the next check puts one out, not three, keeping two
        // for the final.
        arena.fighters[1].surrendered = true;
        arena.fighters[4].round_damage = 1;
        assert!(!in_final(&arena));
        let cut = next_cut(&arena).unwrap();
        assert_eq!((cut.ticks_left, cut.out), (60, 1));
        for _ in 0..60 {
            tick(&mut arena, &mut ais);
        }
        assert_eq!(eliminated(&arena).len(), 3);
        // The final: no check, nobody else out.
        assert!(in_final(&arena));
        assert_eq!(next_cut(&arena), None);
        for _ in 0..600 {
            tick(&mut arena, &mut ais);
        }
        assert_eq!(eliminated(&arena).len(), 3);
        assert_eq!(arena.fighters.iter().filter(|f| f.is_fighting()).count(), 2);
    }

    #[test]
    fn those_in_danger_are_the_ones_the_check_would_put_out() {
        let mut arena = six();
        for (f, dealt) in arena.fighters.iter_mut().zip([0, 5, 9, 9, 20, 3]) {
            f.round_damage = dealt;
        }
        let danger: Vec<bool> = arena
            .fighters
            .iter()
            .map(|f| in_danger(&arena, f))
            .collect();
        assert_eq!(danger, [true, false, false, false, false, true]);
        // Equal damage: the one with less HP left goes first.
        arena.fighters[5].round_damage = 5;
        arena.fighters[1].hp = 50;
        assert!(in_danger(&arena, &arena.fighters[1]));
        assert!(!in_danger(&arena, &arena.fighters[5]));
    }

    #[test]
    fn of_equals_at_the_edge_the_ones_out_are_drawn() {
        let mut arena = six();
        arena.tick = 59;
        let before = arena.rng.clone();
        eliminate(&mut arena);
        assert_ne!(arena.rng, before);
        assert_eq!(eliminated(&arena).len(), 2);
        // No tie at the edge: no draw.
        let mut arena = six();
        arena.tick = 59;
        for (f, dealt) in arena.fighters.iter_mut().zip([0, 1, 2, 3, 4, 5]) {
            f.round_damage = dealt;
        }
        let before = arena.rng.clone();
        eliminate(&mut arena);
        assert_eq!(arena.rng, before);
        assert_eq!(eliminated(&arena), [(59, 0), (59, 1)]);
    }

    #[test]
    fn an_arena_without_eliminations_puts_nobody_out() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 15.0, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        assert_eq!(next_cut(&arena), None);
        let mut ais = waiting(&arena);
        for _ in 0..1800 {
            tick(&mut arena, &mut ais);
        }
        assert!(arena.fighters.iter().all(Fighter::is_fighting));
    }

    #[test]
    fn a_melee_of_twenty_nearly_always_ends_with_one_standing() {
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
        // The checks leave two, who fight the final; two out of mana and endurance,
        // which do not come back yet, may reach the time limit.
        assert!(ended >= 19, "{ended} of 20");
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
