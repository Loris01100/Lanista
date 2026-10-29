//! Fight log (who hits whom, with which spell, for how much) and the report built from it.

use serde::{Deserialize, Serialize};
use std::fmt::{self, Write};

use crate::combat::Arena;
use crate::fighter::injury::{Location, Side};
use crate::fighter::{Effect, FighterId};

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
    /// and a missed spell (`missed`) none. `part` is where it landed, drawn only for a hit
    /// in an arena with rules (`Arena::hits`).
    Cast {
        caster: FighterId,
        target: FighterId,
        spell: usize,
        damage: u32,
        missed: bool,
        part: Option<Location>,
    },
    Death {
        fighter: FighterId,
    },
    /// Gave up: out of the fight, alive. In an arena with a door, once through it.
    Surrender {
        fighter: FighterId,
    },
    /// Reached its surrender threshold in an arena with a door: runs for it, still in
    /// the fight, until its `Surrender` or its `Death`.
    Flee {
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
    /// Got out of the trap holding it (`fighter::Effect::Trapped`), hit or in time.
    Freed {
        fighter: FighterId,
    },
    /// Did not get up after a stun: out of the fight, alive.
    KnockedOut {
        fighter: FighterId,
    },
    /// A cutting hit, just logged, severed `part`: the head, followed by its `Death`,
    /// or a limb, out of the fight alive.
    Sever {
        fighter: FighterId,
        part: Location,
    },
    /// `a` and `b` became allies, in a melee: they no longer aim at each other.
    Pact {
        a: FighterId,
        b: FighterId,
    },
    /// The pact between `a` and `b` ended: betrayed by `betrayer`, or too few left
    /// in the fight.
    PactBroken {
        a: FighterId,
        b: FighterId,
        betrayer: Option<FighterId>,
    },
    /// `fighter` dealt too little damage before a melee's check: out, alive.
    Eliminated {
        fighter: FighterId,
    },
    /// The hit just logged left `effect` on `fighter` for a while.
    Afflicted {
        fighter: FighterId,
        effect: Effect,
    },
    /// `fighter` cast its gathering spell `spell` and added `amount` elements of the
    /// floor to its reserve, 0 if it was full.
    Gather {
        fighter: FighterId,
        spell: usize,
        amount: u32,
    },
    /// `fighter` cast its mine spell `spell`: a mine, or a trap, lies at its target's
    /// feet (`Arena::mines`).
    Lay {
        fighter: FighterId,
        spell: usize,
    },
    /// `owner`'s mine, laid with its spell `spell`, blasted and took `damage` HP from
    /// `target`, on `part`, drawn only in an arena with rules; `owner` itself may be one.
    /// For a trap, it caught `target`, its `Afflicted` following if it still fights.
    Blast {
        owner: FighterId,
        spell: usize,
        target: FighterId,
        damage: u32,
        part: Option<Location>,
    },
    /// `fighter`'s bleeding `part` took `damage` HP, dealt by `source`; a `Death` or
    /// `Surrender` may follow.
    Bled {
        fighter: FighterId,
        source: FighterId,
        part: Location,
        damage: u32,
    },
    /// `fighter`'s poison took `damage` HP, dealt by `source`; a `Death` or `Surrender`
    /// may follow.
    Poisoned {
        fighter: FighterId,
        source: FighterId,
        damage: u32,
    },
}

/// Summary of a fight, built from its log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// What a cutting hit severed: the head, or a limb.
    pub severed: Option<Location>,
    /// Put out of a melee for dealing too little damage.
    pub eliminated: bool,
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
                severed: f.severed,
                eliminated: f.eliminated,
            })
            .collect();
        let index = |id: FighterId| usize::try_from(id.0).ok();
        let mut previous = None;
        for event in &arena.log {
            match event.kind {
                EventKind::Cast {
                    caster,
                    target,
                    spell,
                    damage,
                    missed,
                    ..
                } => {
                    // The next blows of a cast: its caster, spell and tick again.
                    let blow = previous == Some((event.tick, caster, spell));
                    previous = Some((event.tick, caster, spell));
                    if let Some(c) = index(caster).and_then(|i| fighters.get_mut(i)) {
                        c.damage_dealt = c.damage_dealt.saturating_add(damage);
                        c.spells_cast += u32::from(!blow);
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
                // A fighter caught in its own mine deals itself nothing.
                EventKind::Bled {
                    fighter: target,
                    source: owner,
                    damage,
                    ..
                }
                | EventKind::Poisoned {
                    fighter: target,
                    source: owner,
                    damage,
                }
                | EventKind::Blast {
                    owner,
                    target,
                    damage,
                    ..
                } => {
                    if let Some(o) = index(owner).and_then(|i| fighters.get_mut(i))
                        && owner != target
                    {
                        o.damage_dealt = o.damage_dealt.saturating_add(damage);
                    }
                    if let Some(t) = index(target).and_then(|i| fighters.get_mut(i)) {
                        t.damage_taken = t.damage_taken.saturating_add(damage);
                    }
                }
                EventKind::Death { .. }
                | EventKind::Surrender { .. }
                | EventKind::Flee { .. }
                | EventKind::Rise { .. }
                | EventKind::Freed { .. }
                | EventKind::KnockedOut { .. }
                | EventKind::Sever { .. }
                | EventKind::Pact { .. }
                | EventKind::PactBroken { .. }
                | EventKind::Eliminated { .. }
                | EventKind::Afflicted { .. } => {}
                EventKind::Gather { fighter, .. } | EventKind::Lay { fighter, .. } => {
                    if let Some(f) = index(fighter).and_then(|i| fighters.get_mut(i)) {
                        f.spells_cast += 1;
                    }
                }
            }
        }
        Self {
            ticks: arena.tick,
            over: arena.is_over(),
            winner: winner(arena),
            fighters,
        }
    }
}

/// The one still fighting once the fight is over; none in a fight not over.
fn winner(arena: &Arena) -> Option<FighterId> {
    let over = arena.is_over();
    arena
        .fighters
        .iter()
        .find(|f| f.is_fighting())
        .map(|f| f.id)
        .filter(|_| over)
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
            let out = match r.severed {
                Some(Location::Head) => ", beheaded".to_owned(),
                Some(limb) => format!(", lost its {}", limb_name(limb)),
                None if r.surrendered => ", gave up".to_owned(),
                None if r.knocked_out => ", knocked out".to_owned(),
                None if r.eliminated => ", eliminated".to_owned(),
                None => String::new(),
            };
            writeln!(f, "{out}")?;
        }
        Ok(())
    }
}

/// A limb's name in the log: "left arm", "right leg"; "head" and "body" for the others.
fn limb_name(location: Location) -> &'static str {
    match location {
        Location::Head => "head",
        Location::Body => "body",
        Location::Arm(Side::Left) => "left arm",
        Location::Arm(Side::Right) => "right arm",
        Location::Leg(Side::Left) => "left leg",
        Location::Leg(Side::Right) => "right leg",
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
                part,
            } => {
                let name = spell_name(arena, caster, spell);
                let result = match (missed, part) {
                    (true, _) => "missed".to_owned(),
                    (false, Some(part)) => format!("{damage} damage to the {}", limb_name(part)),
                    (false, None) => format!("{damage} damage"),
                };
                writeln!(
                    text,
                    "[{:>5}] fighter {} casts {} on fighter {}: {result}",
                    event.tick, caster.0, name, target.0
                )
            }
            EventKind::Death { fighter }
            | EventKind::Surrender { fighter }
            | EventKind::Flee { fighter }
            | EventKind::Stun { fighter }
            | EventKind::Rise { fighter }
            | EventKind::Freed { fighter }
            | EventKind::KnockedOut { fighter } => {
                let what = match event.kind {
                    EventKind::Death { .. } => "dies",
                    EventKind::Surrender { .. } => "gives up",
                    EventKind::Flee { .. } => "runs for the door",
                    EventKind::Stun { .. } => "is stunned",
                    EventKind::Rise { .. } => "gets up",
                    EventKind::Freed { .. } => "is out of the trap",
                    _ => "is knocked out",
                };
                writeln!(text, "[{:>5}] fighter {} {what}", event.tick, fighter.0)
            }
            EventKind::Sever {
                fighter,
                part: Location::Head,
            } => {
                writeln!(
                    text,
                    "[{:>5}] fighter {} is beheaded",
                    event.tick, fighter.0
                )
            }
            EventKind::Sever { fighter, part } => {
                writeln!(
                    text,
                    "[{:>5}] fighter {} loses its {}",
                    event.tick,
                    fighter.0,
                    limb_name(part)
                )
            }
            EventKind::Pact { a, b } => {
                writeln!(
                    text,
                    "[{:>5}] fighters {} and {} make a pact",
                    event.tick, a.0, b.0
                )
            }
            EventKind::PactBroken { a, b, betrayer } => {
                let pact = format!("the pact between fighters {} and {}", a.0, b.0);
                match betrayer {
                    Some(f) => writeln!(text, "[{:>5}] fighter {} betrays {pact}", event.tick, f.0),
                    None => writeln!(text, "[{:>5}] {pact} ends", event.tick),
                }
            }
            EventKind::Eliminated { fighter } => {
                writeln!(
                    text,
                    "[{:>5}] fighter {} is out for dealing too little damage",
                    event.tick, fighter.0
                )
            }
            EventKind::Afflicted { .. } | EventKind::Bled { .. } | EventKind::Poisoned { .. } => {
                writeln!(text, "[{:>5}] {}", event.tick, effect_line(event.kind))
            }
            EventKind::Gather { .. } | EventKind::Lay { .. } | EventKind::Blast { .. } => {
                writeln!(
                    text,
                    "[{:>5}] {}",
                    event.tick,
                    ground_line(arena, event.kind)
                )
            }
        };
    }
    text
}

/// The name of `fighter`'s spell `spell`, "?" if there is none.
fn spell_name(arena: &Arena, fighter: FighterId, spell: usize) -> &str {
    arena
        .fighter(fighter)
        .and_then(|f| f.spells.get(spell))
        .map_or("?", |s| s.name.as_str())
}

/// Whether `fighter`'s spell `spell` lays traps (`SpellData::trap`).
fn is_trap(arena: &Arena, fighter: FighterId, spell: usize) -> bool {
    arena
        .fighter(fighter)
        .and_then(|f| f.spells.get(spell))
        .is_some_and(|s| s.trap.is_some())
}

/// The log's line for elements gathered, or a mine laid or blasting.
fn ground_line(arena: &Arena, kind: EventKind) -> String {
    match kind {
        EventKind::Gather {
            fighter,
            spell,
            amount,
        } => format!(
            "fighter {} casts {}: {amount} elements gathered",
            fighter.0,
            spell_name(arena, fighter, spell)
        ),
        EventKind::Lay { fighter, spell } => format!(
            "fighter {} casts {}: a {} is laid",
            fighter.0,
            spell_name(arena, fighter, spell),
            if is_trap(arena, fighter, spell) {
                "trap"
            } else {
                "mine"
            }
        ),
        EventKind::Blast {
            owner,
            spell,
            target,
            damage,
            part,
        } => format!(
            "fighter {}'s {} {}: fighter {} takes {damage} damage{}",
            owner.0,
            spell_name(arena, owner, spell),
            if is_trap(arena, owner, spell) {
                "snaps"
            } else {
                "blasts"
            },
            target.0,
            part.map_or(String::new(), |p| format!(" to the {}", limb_name(p)))
        ),
        _ => String::new(),
    }
}

/// The log's line for a lasting effect left by a hit, or a bleeding's or poison's damage.
fn effect_line(kind: EventKind) -> String {
    match kind {
        EventKind::Afflicted {
            fighter,
            effect: Effect::Bleeding { part, .. },
        } => format!("fighter {}'s {} bleeds", fighter.0, limb_name(part)),
        EventKind::Afflicted {
            fighter,
            effect: Effect::Blinded { percent },
        } => format!("fighter {} loses {percent} % accuracy", fighter.0),
        EventKind::Afflicted {
            fighter,
            effect: Effect::Trapped { damage_percent },
        } => format!(
            "fighter {} is caught in a trap, taking {damage_percent} % more damage",
            fighter.0
        ),
        EventKind::Afflicted {
            fighter,
            effect: Effect::Poisoned { hp_percent, .. },
        } => format!(
            "fighter {} is poisoned, losing {hp_percent} % of its HP each second",
            fighter.0
        ),
        EventKind::Poisoned {
            fighter, damage, ..
        } => format!("fighter {} loses {damage} to poison", fighter.0),
        EventKind::Bled {
            fighter,
            part,
            damage,
            ..
        } => format!(
            "fighter {} bleeds {damage} from the {}",
            fighter.0,
            limb_name(part)
        ),
        _ => String::new(),
    }
}

/// Each fighter's place in a fight, in the arena's order, from the elimination order:
/// 1 for those still fighting at the end, then the later a fighter died, gave up, was
/// knocked out or lost a limb, the better. Fighters out on the same tick, or still fighting together
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
                    | EventKind::KnockedOut { fighter }
                    | EventKind::Eliminated { fighter }
                    | EventKind::Sever { fighter, .. } => fighter == f.id,
                    EventKind::Cast { .. }
                    | EventKind::Stun { .. }
                    | EventKind::Flee { .. }
                    | EventKind::Rise { .. }
                    | EventKind::Freed { .. }
                    | EventKind::Pact { .. }
                    | EventKind::PactBroken { .. }
                    | EventKind::Afflicted { .. }
                    | EventKind::Bled { .. }
                    | EventKind::Poisoned { .. }
                    | EventKind::Gather { .. }
                    | EventKind::Lay { .. }
                    | EventKind::Blast { .. } => false,
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
