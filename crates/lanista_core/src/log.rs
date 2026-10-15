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
    /// `damage` is the hit points actually lost, so a killing blow counts only what was left,
    /// and a missed spell (`missed`) none.
    Cast {
        caster: FighterId,
        target: FighterId,
        spell: usize,
        damage: u32,
        missed: bool,
    },
    Death {
        fighter: FighterId,
    },
    /// Reached its surrender threshold: out of the fight, alive.
    Surrender {
        fighter: FighterId,
    },
    /// Stunned by the hit just logged: down, neither acting nor hit.
    Stun {
        fighter: FighterId,
    },
    /// Got back up after a stun: fighting again.
    Rise {
        fighter: FighterId,
    },
    /// Did not get up after a stun: out of the fight, alive.
    KnockedOut {
        fighter: FighterId,
    },
}

/// Summary of a fight, built from its log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Ticks played.
    pub ticks: u32,
    /// At most one fighter is still fighting.
    pub over: bool,
    /// The last fighter still fighting; `None` if the fight is not over or nobody is left.
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
    /// Missed spells included.
    pub spells_cast: u32,
    pub misses: u32,
    /// Times stunned.
    pub stuns: u32,
    pub surrendered: bool,
    pub knocked_out: bool,
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
                misses: 0,
                stuns: 0,
                surrendered: f.surrendered,
                knocked_out: f.knocked_out,
            })
            .collect();
        let index = |id: FighterId| usize::try_from(id.0).ok();
        for event in &arena.log {
            match event.kind {
                EventKind::Cast {
                    caster,
                    target,
                    damage,
                    missed,
                    ..
                } => {
                    if let Some(c) = index(caster).and_then(|i| fighters.get_mut(i)) {
                        c.damage_dealt = c.damage_dealt.saturating_add(damage);
                        c.spells_cast += 1;
                        c.misses += u32::from(missed);
                    }
                    if let Some(t) = index(target).and_then(|i| fighters.get_mut(i)) {
                        t.damage_taken = t.damage_taken.saturating_add(damage);
                    }
                }
                EventKind::Stun { fighter } => {
                    if let Some(f) = index(fighter).and_then(|i| fighters.get_mut(i)) {
                        f.stuns += 1;
                    }
                }
                EventKind::Death { .. }
                | EventKind::Surrender { .. }
                | EventKind::Rise { .. }
                | EventKind::KnockedOut { .. } => {}
            }
        }
        let over = arena.is_over();
        let winner = if over {
            arena
                .fighters
                .iter()
                .find(|f| f.is_fighting())
                .map(|f| f.id)
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
            (true, None) => writeln!(f, "over after {} ticks, no winner", self.ticks)?,
            (false, _) => writeln!(f, "not over after {} ticks", self.ticks)?,
        }
        for r in &self.fighters {
            write!(
                f,
                "fighter {}: {}/{} HP, dealt {}, taken {}, {} spells cast, {} missed",
                r.id.0, r.hp, r.max_hp, r.damage_dealt, r.damage_taken, r.spells_cast, r.misses
            )?;
            match r.stuns {
                0 => {}
                1 => write!(f, ", stunned once")?,
                n => write!(f, ", stunned {n} times")?,
            }
            let out = if r.surrendered {
                ", gave up"
            } else if r.knocked_out {
                ", knocked out"
            } else {
                ""
            };
            writeln!(f, "{out}")?;
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
                missed,
            } => {
                let name = arena
                    .fighter(caster)
                    .and_then(|c| c.spells.get(spell))
                    .map_or("?", |s| s.name.as_str());
                let result = if missed {
                    "missed".to_owned()
                } else {
                    format!("{damage} damage")
                };
                writeln!(
                    text,
                    "[{:>5}] fighter {} casts {} on fighter {}: {result}",
                    event.tick, caster.0, name, target.0
                )
            }
            EventKind::Death { fighter } => {
                writeln!(text, "[{:>5}] fighter {} dies", event.tick, fighter.0)
            }
            EventKind::Surrender { fighter } => {
                writeln!(text, "[{:>5}] fighter {} gives up", event.tick, fighter.0)
            }
            EventKind::Stun { fighter } => {
                writeln!(text, "[{:>5}] fighter {} is stunned", event.tick, fighter.0)
            }
            EventKind::Rise { fighter } => {
                writeln!(text, "[{:>5}] fighter {} gets up", event.tick, fighter.0)
            }
            EventKind::KnockedOut { fighter } => {
                writeln!(
                    text,
                    "[{:>5}] fighter {} is knocked out",
                    event.tick, fighter.0
                )
            }
        };
    }
    text
}

/// Each fighter's place in a fight, in the arena's order, from the elimination order:
/// 1 for those still fighting at the end, then the later a fighter died, gave up or was
/// knocked out, the better. Fighters out on the same tick, or still fighting together
/// when time ran out, share a place.
#[must_use]
pub fn places(arena: &Arena) -> Vec<u32> {
    // The tick a fighter was out on; still fighting comes last, so best.
    let out: Vec<u64> = arena
        .fighters
        .iter()
        .map(|f| {
            arena
                .log
                .iter()
                .find(|e| match e.kind {
                    EventKind::Death { fighter }
                    | EventKind::Surrender { fighter }
                    | EventKind::KnockedOut { fighter } => fighter == f.id,
                    EventKind::Cast { .. } | EventKind::Stun { .. } | EventKind::Rise { .. } => {
                        false
                    }
                })
                .map_or(u64::MAX, |e| u64::from(e.tick))
        })
        .collect();
    out.iter()
        .map(|&mine| {
            let ahead = out.iter().filter(|&&other| other > mine).count();
            u32::try_from(ahead).map_or(u32::MAX, |n| n.saturating_add(1))
        })
        .collect()
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

    /// Fighter 0 hits fighter 1, which fights to the death.
    fn one_sided() -> Arena {
        one_sided_with_threshold(0)
    }

    #[test]
    fn places_follow_the_elimination_order() {
        let fighters = (0..4).map(|i| test_fighter(i, 2.0 + f64::from(i) * 3.0, 5.0));
        let mut arena = Arena::new(20.0, 20.0, fighters.collect(), 0);
        let out = |tick, kind| Event { tick, kind };
        arena.log = vec![
            out(
                5,
                EventKind::Death {
                    fighter: FighterId(3),
                },
            ),
            out(
                10,
                EventKind::Surrender {
                    fighter: FighterId(2),
                },
            ),
            out(
                10,
                EventKind::Death {
                    fighter: FighterId(1),
                },
            ),
        ];
        // Fighter 0 is still fighting: first. 1 and 2 went out together: second.
        assert_eq!(places(&arena), [1, 2, 2, 4]);
        arena.log.clear();
        // Time ran out on everyone still fighting: all first.
        assert_eq!(places(&arena), [1, 1, 1, 1]);
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
                _ => None,
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

    /// Fighter 0 hits fighter 1 (10 damage a hit, 25 HP) until the fight ends.
    fn one_sided_with_threshold(surrender_hp: u32) -> Arena {
        let fighters = vec![test_fighter(0, 5.0, 5.0), test_fighter(1, 6.5, 5.0)];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.fighters[1].hp = 25;
        arena.fighters[1].surrender_hp = surrender_hp;
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
    fn a_fighter_gives_up_at_its_threshold_and_lives() {
        let arena = one_sided_with_threshold(15);
        // 25 → 15 HP: at the threshold after the first hit.
        assert!(arena.is_over());
        assert_eq!(arena.fighters[1].hp, 15);
        assert!(arena.fighters[1].is_alive() && !arena.fighters[1].is_fighting());
        assert_eq!(
            arena.log.last().map(|e| e.kind),
            Some(EventKind::Surrender {
                fighter: FighterId(1)
            })
        );
        let report = Report::new(&arena);
        assert_eq!(report.winner, Some(FighterId(0)));
        assert!(report.fighters[1].surrendered);
        assert!(
            report.to_string().contains(
                "fighter 1: 15/100 HP, dealt 0, taken 10, 0 spells cast, 0 missed, gave up"
            )
        );
        assert!(log_text(&arena).ends_with(
            "fighter 1 gives up
"
        ));
    }

    #[test]
    fn a_blow_past_the_threshold_kills() {
        // 25 → 15 → 5: still above 4 after two hits, the third one kills.
        let arena = one_sided_with_threshold(4);
        assert!(!arena.fighters[1].is_alive());
        assert!(!arena.fighters[1].surrendered);
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
