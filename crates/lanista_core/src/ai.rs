use crate::combat::{Arena, Vec2, edge_distance, spell_damage};
use crate::fighter::{Fighter, FighterId};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Action {
    #[default]
    Wait,
    /// Move at full speed in this direction; its length is ignored.
    Move(Vec2),
    /// Cast spell `spell` (index in the fighter's `spells`) on `target`, as soon as possible
    /// until the next decision.
    Cast { spell: usize, target: FighterId },
}

/// Decides for one fighter; called every `TICKS_PER_DECISION` ticks.
pub trait CombatAi {
    fn decide(&mut self, arena: &Arena, me: FighterId) -> Action;
}

/// Score of closing in on an enemy no spell can reach. Any positive value works:
/// moving is only offered when no cast is possible.
const MOVE_SCORE: f64 = 1.0;

/// Utility AI: gives every possible action a score and plays the best one.
// ponytail: aims at the nearest enemy only, and ranged fighters do not keep their distance.
#[derive(Debug, Default)]
pub struct UtilityAi;

impl CombatAi for UtilityAi {
    fn decide(&mut self, arena: &Arena, me: FighterId) -> Action {
        best(&score_actions(arena, me))
    }
}

/// Every action worth considering for `me`, with its score; higher is better.
/// Waiting scores 0, a cast scores its damage, moving scores `MOVE_SCORE`.
#[must_use]
pub fn score_actions(arena: &Arena, me: FighterId) -> Vec<(Action, f64)> {
    let mut scored = vec![(Action::Wait, 0.0)];
    let Some(me) = arena.fighter(me) else {
        return scored;
    };
    let Some(target) = nearest_enemy(arena, me) else {
        return scored;
    };
    let distance = edge_distance(me, target);
    for (spell, data) in me.spells.iter().enumerate() {
        if me.can_cast(spell) && distance <= data.reach {
            let damage = spell_damage(&me.stats, &target.stats, data);
            let cast = Action::Cast {
                spell,
                target: target.id,
            };
            scored.push((cast, f64::from(damage)));
        }
    }
    let longest_reach = me.spells.iter().map(|s| s.reach).fold(0.0, f64::max);
    if distance > longest_reach {
        let toward = Vec2::new(
            target.position.x - me.position.x,
            target.position.y - me.position.y,
        );
        scored.push((Action::Move(toward), MOVE_SCORE));
    }
    scored
}

/// The highest-scored action; on a tie, the first one.
fn best(scored: &[(Action, f64)]) -> Action {
    let mut best = (Action::Wait, f64::NEG_INFINITY);
    for &(action, score) in scored {
        if score > best.1 {
            best = (action, score);
        }
    }
    best.0
}

/// The closest living other fighter; on a tie, the lowest ID.
fn nearest_enemy<'a>(arena: &'a Arena, me: &Fighter) -> Option<&'a Fighter> {
    let mut nearest: Option<(&Fighter, f64)> = None;
    for f in arena
        .fighters
        .iter()
        .filter(|f| f.is_alive() && f.id != me.id)
    {
        let distance = edge_distance(me, f);
        if nearest.is_none_or(|(_, d)| distance < d) {
            nearest = Some((f, distance));
        }
    }
    nearest.map(|(f, _)| f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::tick;
    use crate::design_data::DesignData;
    use crate::duel::STATS;
    use crate::fighter::{ClassId, test_fighter};

    #[test]
    fn waits_when_alone() {
        let arena = Arena::new(20.0, 20.0, vec![test_fighter(0, 5.0, 5.0)], 0);
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), Action::Wait);
    }

    #[test]
    fn moves_toward_an_enemy_out_of_reach() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 10.0, 5.0)];
        let arena = Arena::new(20.0, 20.0, fighters, 0);
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Move(Vec2::new(5.0, 0.0))
        );
    }

    #[test]
    fn casts_on_an_enemy_in_reach() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let arena = Arena::new(20.0, 20.0, fighters, 0);
        assert_eq!(
            UtilityAi.decide(&arena, FighterId(0)),
            Action::Cast {
                spell: 0,
                target: FighterId(1)
            }
        );
    }

    #[test]
    fn waits_in_reach_while_on_cooldown() {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.fighters[0].cooldowns[0] = 10;
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), Action::Wait);
    }

    fn shipped_classes() -> DesignData {
        DesignData::from_ron(include_str!("../../../data/classes.ron")).unwrap()
    }

    #[test]
    fn picks_the_strongest_ready_spell() {
        let data = shipped_classes();
        let murmillo = &data.classes[0];
        let fighters = vec![
            Fighter::new(
                FighterId(0),
                ClassId(0),
                murmillo,
                STATS,
                Vec2::new(5.0, 5.0),
            ),
            Fighter::new(
                FighterId(1),
                ClassId(0),
                murmillo,
                STATS,
                Vec2::new(6.5, 5.0),
            ),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        // Spell 2 (power 250) beats spell 1 (150), which beats spell 0 (100).
        let cast = |spell| Action::Cast {
            spell,
            target: FighterId(1),
        };
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), cast(2));
        arena.fighters[0].cooldowns[2] = 1;
        assert_eq!(UtilityAi.decide(&arena, FighterId(0)), cast(1));
    }

    #[test]
    fn a_duel_of_utility_ais_ends_with_a_death() {
        let data = shipped_classes();
        let fighters = vec![
            Fighter::new(
                FighterId(0),
                ClassId(0),
                &data.classes[0],
                STATS,
                Vec2::new(3.0, 10.0),
            ),
            Fighter::new(
                FighterId(1),
                ClassId(1),
                &data.classes[1],
                STATS,
                Vec2::new(17.0, 10.0),
            ),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        let mut ais: Vec<Box<dyn CombatAi>> = vec![Box::new(UtilityAi), Box::new(UtilityAi)];
        // One minute of fight at most.
        while arena.fighters.iter().all(Fighter::is_alive) && arena.tick < 1800 {
            tick(&mut arena, &mut ais);
        }
        assert!(
            arena.fighters.iter().any(|f| !f.is_alive()),
            "{:?}",
            arena.fighters.iter().map(|f| f.hp).collect::<Vec<_>>()
        );
    }
}
