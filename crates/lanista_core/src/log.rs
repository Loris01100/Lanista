//! Fight log (who hits whom, with which spell, for how much) and the report built from it.

use std::fmt::{self, Write};

use crate::combat::Arena;
use crate::fighter::FighterId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    /// Tick it happened on.
    pub tick: u32,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// `caster` hit `target` with spell `spell` (index in the caster's `spells`).
    /// `damage` is the hit points actually lost, so a killing blow counts only what was left.
    Cast {
        caster: FighterId,
        target: FighterId,
        spell: usize,
        damage: u32,
    },
    Death {
        fighter: FighterId,
    },
}

/// Summary of a fight, built from its log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Ticks played.
    pub ticks: u32,
    /// At most one fighter is left alive.
    pub over: bool,
    /// The last fighter alive; `None` if the fight is not over or nobody survived.
    pub winner: Option<FighterId>,
    /// In the order of the arena's fighters.
    pub fighters: Vec<FighterReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FighterReport {
    pub id: FighterId,
    pub hp: u32,
    pub max_hp: u32,
    pub damage_dealt: u32,
    pub damage_taken: u32,
    pub spells_cast: u32,
}

impl Report {
    #[must_use]
    pub fn new(arena: &Arena) -> Self {
        let mut fighters: Vec<FighterReport> = arena
            .fighters
            .iter()
            .map(|f| FighterReport {
                id: f.id,
                hp: f.hp,
                max_hp: f.stats.hp,
                damage_dealt: 0,
                damage_taken: 0,
                spells_cast: 0,
            })
            .collect();
        let index = |id: FighterId| usize::try_from(id.0).ok();
        for event in &arena.log {
            if let EventKind::Cast {
                caster,
                target,
                damage,
                ..
            } = event.kind
            {
                if let Some(c) = index(caster).and_then(|i| fighters.get_mut(i)) {
                    c.damage_dealt = c.damage_dealt.saturating_add(damage);
                    c.spells_cast += 1;
                }
                if let Some(t) = index(target).and_then(|i| fighters.get_mut(i)) {
                    t.damage_taken = t.damage_taken.saturating_add(damage);
                }
            }
        }
        let over = arena.is_over();
        let winner = if over {
            arena.fighters.iter().find(|f| f.is_alive()).map(|f| f.id)
        } else {
            None
        };
        Self {
            ticks: arena.tick,
            over,
            winner,
            fighters,
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.over, self.winner) {
            (true, Some(winner)) => {
                writeln!(
                    f,
                    "over after {} ticks, winner: fighter {}",
                    self.ticks, winner.0
                )?;
            }
            (true, None) => writeln!(f, "over after {} ticks, no survivor", self.ticks)?,
            (false, _) => writeln!(f, "not over after {} ticks", self.ticks)?,
        }
        for r in &self.fighters {
            writeln!(
                f,
                "fighter {}: {}/{} HP, dealt {}, taken {}, {} spells cast",
                r.id.0, r.hp, r.max_hp, r.damage_dealt, r.damage_taken, r.spells_cast
            )?;
        }
        Ok(())
    }
}

/// The log as text, one event per line, with spell names.
#[must_use]
pub fn log_text(arena: &Arena) -> String {
    let mut text = String::new();
    for event in &arena.log {
        // Writing to a String cannot fail.
        let _ = match event.kind {
            EventKind::Cast {
                caster,
                target,
                spell,
                damage,
            } => {
                let name = arena
                    .fighter(caster)
                    .and_then(|c| c.spells.get(spell))
                    .map_or("?", |s| s.name.as_str());
                writeln!(
                    text,
                    "[{:>5}] fighter {} casts {} on fighter {}: {} damage",
                    event.tick, caster.0, name, target.0, damage
                )
            }
            EventKind::Death { fighter } => {
                writeln!(text, "[{:>5}] fighter {} dies", event.tick, fighter.0)
            }
        };
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{Action, CombatAi};
    use crate::combat::{Arena, run};
    use crate::fighter::test_fighter;

    struct Fixed(Action);

    impl CombatAi for Fixed {
        fn decide(&mut self, _: &Arena, _: FighterId) -> Action {
            self.0
        }
    }

    /// Fighter 0 hits fighter 1, which does nothing, until it runs out of endurance.
    fn one_sided() -> Arena {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.fighters[1].hp = 25;
        let hit = Action::Cast {
            spell: 0,
            target: FighterId(1),
        };
        let mut ais: Vec<Box<dyn CombatAi>> =
            vec![Box::new(Fixed(hit)), Box::new(Fixed(Action::Wait))];
        run(&mut arena, &mut ais, 600);
        arena
    }

    #[test]
    fn logs_hits_and_the_death() {
        let arena = one_sided();
        // 10 damage per hit, 25 HP: the third hit only takes the last 5.
        let damages: Vec<u32> = arena
            .log
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::Cast { damage, .. } => Some(damage),
                EventKind::Death { .. } => None,
            })
            .collect();
        assert_eq!(damages, [10, 10, 5]);
        assert_eq!(
            arena.log.last().map(|e| e.kind),
            Some(EventKind::Death {
                fighter: FighterId(1)
            })
        );
    }

    #[test]
    fn report_sums_the_log() {
        let report = Report::new(&one_sided());
        assert!(report.over);
        assert_eq!(report.winner, Some(FighterId(0)));
        assert_eq!(report.fighters[0].damage_dealt, 25);
        assert_eq!(report.fighters[0].spells_cast, 3);
        assert_eq!(report.fighters[1].damage_taken, 25);
        assert_eq!(report.fighters[1].hp, 0);
    }
}
