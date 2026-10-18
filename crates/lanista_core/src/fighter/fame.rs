//! Fame: a gauge from 0 to 100 that follows a fighter's style and daring.
//! It will raise the takings the fighter brings in (economy, phase 4).

use serde::{Deserialize, Serialize};

use crate::combat::Arena;
use crate::fighter::{Fighter, FighterId};
use crate::log::EventKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(from = "u32", into = "u32")]
pub struct Fame(u32);

impl Fame {
    pub const MAX: u32 = 100;

    /// Fame `value`, capped at [`Fame::MAX`].
    #[must_use]
    pub fn new(value: u32) -> Self {
        Self(value.min(Self::MAX))
    }

    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }

    /// Fame after `change` points, staying within 0 to [`Fame::MAX`].
    #[must_use]
    pub fn changed_by(self, change: i32) -> Self {
        let value = i64::from(self.0) + i64::from(change);
        Self(u32::try_from(value.clamp(0, i64::from(Self::MAX))).unwrap_or(0))
    }
}

/// What a fight meant for one fighter's fame.
// ponytail: flights and King of the hill come with the phases that add them; refused
// equipment brings its bonus apart (`equipment::Refused::fame_bonus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FightOutcome {
    pub result: FightResult,
    /// The opponent was stronger; Elo will decide it in phase 4, the caller does for now.
    pub against_stronger: bool,
    /// Hits that took at least `FameRules::spectacular_hit_percent` of the target's max HP.
    pub spectacular_hits: u32,
}

/// How a fight ended for one fighter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FightResult {
    Won,
    /// Killed, or still fighting when someone else won.
    Lost,
    GaveUp,
    /// Stopped by the time limit with no winner: a fight without spark.
    Unfinished,
}

impl FightOutcome {
    /// The outcome of the finished fight in `arena` for fighter `me`.
    #[must_use]
    pub fn from_arena(
        arena: &Arena,
        me: FighterId,
        against_stronger: bool,
        rules: &FameRules,
    ) -> Self {
        let over = arena.is_over();
        let fighter = arena.fighter(me);
        let spectacular_hits = arena
            .log
            .iter()
            .filter(|event| match event.kind {
                EventKind::Cast {
                    caster,
                    target,
                    damage,
                    ..
                } => {
                    let max_hp = arena.fighter(target).map_or(0, |t| t.stats.hp);
                    caster == me
                        && u64::from(damage) * 100
                            >= u64::from(max_hp) * u64::from(rules.spectacular_hit_percent)
                }
                _ => false,
            })
            .count();
        // A fighter out of a melee that reached the time limit still lost.
        let result = if fighter.is_some_and(|f| f.surrendered) {
            FightResult::GaveUp
        } else if !fighter.is_some_and(Fighter::is_fighting) {
            FightResult::Lost
        } else if over {
            FightResult::Won
        } else {
            FightResult::Unfinished
        };
        Self {
            result,
            against_stronger,
            spectacular_hits: u32::try_from(spectacular_hits).unwrap_or(u32::MAX),
        }
    }

    /// Fame points this outcome brings, negative for a loss of fame.
    #[must_use]
    pub fn fame_change(&self, rules: &FameRules) -> i32 {
        let result = match self.result {
            FightResult::Won if self.against_stronger => {
                i64::from(rules.victory) + i64::from(rules.victory_against_stronger)
            }
            FightResult::Won => i64::from(rules.victory),
            FightResult::Lost => 0,
            FightResult::GaveUp => -i64::from(rules.surrender),
            FightResult::Unfinished => -i64::from(rules.dull_fight),
        };
        let change = result + i64::from(self.spectacular_hits) * i64::from(rules.spectacular_hit);
        i32::try_from(change).unwrap_or(if change < 0 { i32::MIN } else { i32::MAX })
    }
}

/// Fame gains and losses, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FameRules {
    pub victory: u32,
    /// Added to `victory` when the opponent was stronger.
    pub victory_against_stronger: u32,
    /// Per spectacular hit.
    pub spectacular_hit: u32,
    /// A hit is spectacular from this percentage of the target's max HP.
    pub spectacular_hit_percent: u32,
    /// Lost when giving up.
    pub surrender: u32,
    /// Lost by every fighter when the fight ends on the time limit.
    pub dull_fight: u32,
    /// Gained for fighting without armour, without weapon, or without either
    /// (see `equipment::Refused::fame_bonus`).
    pub refused_armour: u32,
    pub refused_weapon: u32,
    pub refused_all: u32,
    /// Gained in a melee by the last one standing, less for each place below, none
    /// for the first one out.
    pub melee_survival: u32,
}

// Saves go through `new`: an edited value stays within its bounds.
impl From<u32> for Fame {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl From<Fame> for u32 {
    fn from(value: Fame) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{Action, CombatAi};
    use crate::combat::run;
    use crate::fighter::test_fighter;

    const RULES: FameRules = FameRules {
        victory: 5,
        victory_against_stronger: 5,
        spectacular_hit: 2,
        spectacular_hit_percent: 25,
        surrender: 8,
        dull_fight: 3,
        refused_armour: 3,
        refused_weapon: 6,
        refused_all: 12,
        melee_survival: 10,
    };

    struct Fixed(Action);

    impl CombatAi for Fixed {
        fn decide(&mut self, _: &Arena, _: FighterId) -> Action {
            self.0
        }
    }

    /// Fighter 0 hits fighter 1 (10 damage a hit, 100 HP max) for at most `ticks` ticks.
    fn one_sided(hp: u32, surrender_hp: u32, ticks: u32) -> Arena {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.fighters[1].hp = hp;
        arena.fighters[1].surrender_hp = surrender_hp;
        let hit = Action::Cast {
            spell: 0,
            target: FighterId(1),
        };
        let mut ais: Vec<Box<dyn CombatAi>> =
            vec![Box::new(Fixed(hit)), Box::new(Fixed(Action::Wait))];
        run(&mut arena, &mut ais, ticks);
        arena
    }

    #[test]
    fn fame_stays_within_bounds() {
        assert_eq!(Fame::new(150).value(), 100);
        assert_eq!(Fame::new(95).changed_by(10).value(), 100);
        assert_eq!(Fame::new(5).changed_by(-10).value(), 0);
        assert_eq!(Fame::new(40).changed_by(-10).value(), 30);
    }

    #[test]
    fn winner_and_loser_who_gave_up() {
        let arena = one_sided(25, 15, 600);
        let winner = FightOutcome::from_arena(&arena, FighterId(0), true, &RULES);
        assert_eq!(winner.result, FightResult::Won);
        assert_eq!(winner.fame_change(&RULES), 10);
        let loser = FightOutcome::from_arena(&arena, FighterId(1), false, &RULES);
        assert_eq!(loser.result, FightResult::GaveUp);
        assert_eq!(loser.fame_change(&RULES), -8);
    }

    #[test]
    fn a_fight_on_the_time_limit_is_dull_for_both() {
        // 3 hits in 90 ticks against 100 HP: nobody falls.
        let arena = one_sided(100, 0, 90);
        for id in [FighterId(0), FighterId(1)] {
            let outcome = FightOutcome::from_arena(&arena, id, false, &RULES);
            assert_eq!(outcome.result, FightResult::Unfinished);
            assert_eq!(outcome.fame_change(&RULES), -3);
        }
    }

    #[test]
    fn big_hits_are_spectacular() {
        let arena = one_sided(25, 0, 600);
        let rules = FameRules {
            spectacular_hit_percent: 10,
            ..RULES
        };
        // 10, 10 and 5 damage out of 100 max HP: two hits reach 10 %.
        let outcome = FightOutcome::from_arena(&arena, FighterId(0), false, &rules);
        assert_eq!(outcome.spectacular_hits, 2);
        assert_eq!(outcome.fame_change(&rules), 5 + 2 * 2);
    }
}
