//! A melee between career fighters: the arena of `crate::melee` played to its end, then
//! what it leaves each of them, as a duel does (`CareerFighter::fight`). Only the last
//! one standing wins; the others lose, and fame follows each one's place.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::combat::{Arena, ArenaKind, Vec2, run};
use crate::design_data::DesignData;
use crate::duel::ais;
use crate::fighter::FighterId;
use crate::fighter::career::{CareerFighter, FightSetup, FightSummary, Status};
use crate::fighter::equipment::Refused;
use crate::fighter::fame::{FameRules, FightResult};
use crate::fighter::form::FormEvent;
use crate::fighter::injury::Injury;
use crate::log::{Event, EventKind, Report, places};
use crate::melee::melee_arena;
use crate::staff::Medicine;
use crate::stat_model::Ending;

/// How one fighter goes into a melee: the equipment it refuses, its club's doctor,
/// whether it is one of the player's fighters, whether its club is the player's
/// declared rival, so that it goes for them first, and the points its club's standing
/// adds to its chance of a pact with them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrant {
    pub refused: Refused,
    pub medicine: Medicine,
    pub of_club: bool,
    pub hunts_club: bool,
    pub pact_points: i32,
}

/// What a melee left one of its fighters.
#[derive(Debug, Clone, PartialEq)]
pub struct MeleeSide {
    /// Stronger than this fighter, on average, were the others.
    pub against_stronger: bool,
    pub refused: Refused,
    pub result: FightResult,
    pub died: bool,
    pub fame_change: i32,
    pub new_injury: Option<Injury>,
    /// Its doctor kept the new injury from leaving an after-effect.
    pub after_effect_spared: bool,
}

/// A pact made in a melee between the fighters `a` and `b`, by their index: kept to
/// its end, or betrayed by `betrayer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pact {
    pub a: usize,
    pub b: usize,
    pub betrayer: Option<usize>,
}

/// The pacts of a melee's `log`, in the order they were made.
#[must_use]
pub fn pacts(log: &[Event]) -> Vec<Pact> {
    let index = |id: FighterId| usize::try_from(id.0).unwrap_or(usize::MAX);
    let mut pacts: Vec<Pact> = Vec::new();
    for event in log {
        match event.kind {
            EventKind::Pact { a, b } => pacts.push(Pact {
                a: index(a),
                b: index(b),
                betrayer: None,
            }),
            EventKind::PactBroken {
                a,
                b,
                betrayer: Some(betrayer),
            } => {
                if let Some(pact) = pacts
                    .iter_mut()
                    .rev()
                    .find(|p| (p.a, p.b) == (index(a), index(b)))
                {
                    pact.betrayer = Some(index(betrayer));
                }
            }
            _ => {}
        }
    }
    pacts
}

/// A melee played: the fight itself, and what it left each fighter, in the arena's
/// order, which is the order of the fighters given to [`play_melee`].
#[derive(Debug, Clone, PartialEq)]
pub struct MeleeFight {
    pub seed: u64,
    /// The arena before its first tick: playing it with utility AIs gives this melee.
    pub start: Arena,
    pub report: Report,
    /// What the melee was played from: the fighters before it, as they went in.
    pub setup: FightSetup,
    /// Each one's place, 1 the best (see `log::places`), for the Elo.
    pub places: Vec<u32>,
    /// Whether each one fell at 0 HP yet survived the fatal blow.
    pub survived_fatal_blow: Vec<bool>,
    pub sides: Vec<MeleeSide>,
    pub pacts: Vec<Pact>,
    pub ticks: u32,
}

impl MeleeFight {
    /// The melee as fighter `i` lived it, `None` past the last one.
    #[must_use]
    pub fn summary(&self, i: usize) -> Option<FightSummary> {
        let side = self.sides.get(i)?;
        Some(FightSummary {
            seed: self.seed,
            setup: self.setup.clone(),
            report: self.report.clone(),
            me: FighterId(u32::try_from(i).ok()?),
            place: self.places.get(i).copied().unwrap_or(1),
            against_stronger: side.against_stronger,
            refused: side.refused,
            result: side.result,
            died: side.died,
            survived_fatal_blow: self.survived_fatal_blow.clone(),
            fame_change: side.fame_change,
            ticks: self.ticks,
        })
    }
}

/// The melee between `fighters` (as `entrants` go in, one each, missing ones fully
/// equipped without doctor) on the floor of `kind`, before its first tick, seeded with
/// `seed`: a declared rival club's fighters go for the player's first, and each club's
/// standing adds its points to their chance of a pact with them.
pub(crate) fn melee_start(
    fighters: &[CareerFighter],
    entrants: &[Entrant],
    kind: ArenaKind,
    seed: u64,
    data: &DesignData,
) -> Arena {
    let origin = Vec2::new(0.0, 0.0);
    let combat = fighters
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let id = FighterId(u32::try_from(i).unwrap_or(u32::MAX));
            let refused = entrants.get(i).map_or(Refused::NONE, |e| e.refused);
            f.combat_fighter(id, refused, data, origin)
        })
        .collect();
    let mut start = melee_arena(combat, kind, seed, &data.combat);
    for (hunter, h) in (0..).map(FighterId).zip(entrants) {
        for (prey, p) in (0..).map(FighterId).zip(entrants) {
            if h.hunts_club && p.of_club {
                start.hunts.push((hunter, prey));
            }
            if h.pact_points != 0 && p.of_club {
                start.pact_points.push((hunter, prey, h.pact_points));
            }
        }
    }
    start
}

/// Place `place`'s share of `full` in a melee of `count` where the first `paid` places
/// are paid, never the last one: all of it for the first, a `paid`th less for each
/// place below, nothing past the paid ones.
#[must_use]
pub fn place_share(full: u32, place: u32, (count, paid): (usize, u32)) -> u32 {
    let below = u64::try_from(count.saturating_sub(1)).unwrap_or(u64::MAX);
    let paid = u64::from(paid).min(below);
    let place = u64::from(place.max(1));
    if place > paid {
        return 0;
    }
    u32::try_from(u64::from(full) * (paid + 1 - place) / paid).unwrap_or(u32::MAX)
}

/// Fame a place brings in a melee of `count`: `melee_survival`'s share (see
/// [`place_share`]), the first `paid` places paid.
#[must_use]
pub fn place_fame(place: u32, (count, paid): (usize, u32), rules: &FameRules) -> u32 {
    place_share(rules.melee_survival, place, (count, paid))
}

/// The melee between `fighters` (as `entrants` go in, one each, missing ones fully
/// equipped without doctor) on the floor of `kind`, played to its end or its time
/// limit, then its effects on each, in order: fame, record and death, then morale,
/// form (a melee's cost), illness worsening, a possible injury and progression.
/// Fame is the place's share (see [`place_fame`]) and the refused equipment's bonus
/// alone, whatever the fighter won, gave up or showed;
/// a fighter is against stronger when the others' strength is above its own on average.
/// Nobody ends its week.
pub fn play_melee<R: Rng + ?Sized>(
    fighters: &mut [CareerFighter],
    entrants: &[Entrant],
    kind: ArenaKind,
    rng: &mut R,
    data: &DesignData,
) -> MeleeFight {
    let rules = &data.fighter;
    let entrant = |i: usize| entrants.get(i).copied().unwrap_or_default();
    let seed = rng.random::<u64>();
    let setup = FightSetup {
        kind,
        fighters: fighters.to_vec(),
        entrants: (0..fighters.len()).map(entrant).collect(),
        melee: true,
    };
    let start = setup.arena(seed, data);
    let mut arena = start.clone();
    let mut ais = ais(&arena);
    let ticks = arena.max_ticks;
    run(&mut arena, &mut ais, ticks);
    let places = places(&arena);
    let strengths: Vec<u64> = fighters.iter().map(|f| f.strength(rules)).collect();
    let total: u64 = strengths.iter().sum();
    let others = u64::try_from(fighters.len().saturating_sub(1)).unwrap_or(u64::MAX);
    let count = fighters.len();
    let taken: Vec<_> = fighters
        .iter_mut()
        .enumerate()
        .map(|(i, f)| {
            let id = FighterId(u32::try_from(i).unwrap_or(u32::MAX));
            let side = Ending::from_arena(&arena, id, &rules.fame);
            // The others' average above its own, without dividing.
            let against_stronger = total - strengths[i] > strengths[i] * others;
            let place = places.get(i).copied().unwrap_or(1);
            let paid = data.combat.melee.paid_places;
            let fame = place_fame(place, (count, paid), &rules.fame)
                .saturating_add(entrant(i).refused.fame_bonus(&rules.fame));
            let fame = i32::try_from(fame).unwrap_or(i32::MAX);
            (
                against_stronger,
                f.take_fame_and_side(side, (against_stronger, fame), rng, rules),
            )
        })
        .collect();
    let survived_fatal_blow = fighters
        .iter()
        .zip(&arena.fighters)
        .map(|(f, c)| !c.is_alive() && f.status != Status::Dead)
        .collect();
    let sides = fighters
        .iter_mut()
        .zip(taken)
        .enumerate()
        .map(|(i, (f, (against_stronger, taken)))| {
            let died = f.status == Status::Dead;
            let medicine = entrant(i).medicine;
            let (new_injury, after_effect_spared) =
                f.after_fight(taken, FormEvent::Melee, medicine, rng, rules);
            MeleeSide {
                against_stronger,
                refused: entrant(i).refused,
                result: taken.outcome.result,
                died,
                fame_change: taken.fame_change,
                new_injury,
                after_effect_spared,
            }
        })
        .collect();
    MeleeFight {
        seed,
        start,
        report: Report::new(&arena),
        setup,
        places,
        survived_fatal_blow,
        sides,
        pacts: pacts(&arena.log),
        ticks: arena.tick,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    fn twenty(data: &DesignData, seed: u64) -> (Vec<CareerFighter>, ChaCha8Rng) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let fighters = (0..20)
            .map(|_| CareerFighter::random(&mut rng, data))
            .collect();
        (fighters, rng)
    }

    #[test]
    fn a_pact_is_kept_unless_betrayed() {
        let (a, b, c) = (FighterId(0), FighterId(1), FighterId(2));
        let event = |kind| Event { tick: 0, kind };
        let log = [
            event(EventKind::Pact { a, b }),
            event(EventKind::PactBroken {
                a,
                b,
                betrayer: Some(b),
            }),
            event(EventKind::Pact { a, b }),
            event(EventKind::Pact { a: c, b: a }),
            event(EventKind::PactBroken {
                a: c,
                b: a,
                betrayer: None,
            }),
        ];
        let pact = |a, b, betrayer| Pact { a, b, betrayer };
        assert_eq!(
            pacts(&log),
            [pact(0, 1, Some(1)), pact(0, 1, None), pact(2, 0, None)]
        );
    }

    #[test]
    fn fame_follows_the_place() {
        let rules = data().fighter.fame;
        let full = rules.melee_survival;
        assert_eq!(place_fame(1, (20, 12), &rules), full);
        assert_eq!(place_fame(13, (20, 12), &rules), 0);
        assert_eq!(place_fame(20, (20, 12), &rules), 0);
        let middle = place_fame(6, (20, 12), &rules);
        assert!(middle < full && middle > 0);
        // A small melee pays all but its first one out.
        assert_eq!(place_fame(2, (3, 12), &rules), full / 2);
        assert_eq!(place_fame(3, (3, 12), &rules), 0);
        assert_eq!(place_fame(1, (1, 12), &rules), 0);
        assert_eq!(place_share(120, 12, (20, 12)), 10);
    }

    #[test]
    fn only_the_last_one_standing_wins_and_everyone_takes_the_melee() {
        let data = data();
        let (mut fighters, mut rng) = twenty(&data, 1);
        let before = fighters.clone();
        let melee = play_melee(&mut fighters, &[], ArenaKind::Sand, &mut rng, &data);
        let winners: Vec<usize> = (0..20)
            .filter(|&i| melee.sides[i].result == FightResult::Won)
            .collect();
        assert!(winners.len() <= 1);
        if let &[winner] = winners.as_slice() {
            assert_eq!(melee.places[winner], 1);
            assert_eq!(fighters[winner].record.wins, 1);
        }
        for (i, (f, old)) in fighters.iter().zip(&before).enumerate() {
            let side = &melee.sides[i];
            assert_eq!(side.died, f.status == Status::Dead);
            if !winners.contains(&i) && melee.report.over {
                assert!(matches!(
                    side.result,
                    FightResult::Lost | FightResult::GaveUp
                ));
            }
            assert_eq!(f.record.wins + f.record.losses + f.record.draws, 1);
            if !side.died {
                assert!(f.form.value() < old.form.value() || old.form.value() == 0);
            }
            // The place alone, whatever the fight showed, as its temperament takes it.
            let paid = (20, data.combat.melee.paid_places);
            let place = i64::from(place_fame(melee.places[i], paid, &data.fighter.fame));
            let temperaments = &data.fighter.personality.temperaments;
            let percent = i64::from(temperaments.get(old.personality.temperament).fame_percent);
            assert_eq!(i64::from(side.fame_change), place * percent / 100);
            let fame = i64::from(old.fame.value()) + i64::from(side.fame_change);
            assert_eq!(i64::from(f.fame.value()), fame.clamp(0, 100));
        }
        // Replaying the start with its fighters' AIs gives the same melee.
        let mut replay = melee.start.clone();
        let mut ais = ais(&replay);
        run(&mut replay, &mut ais, data.combat.melee.max_ticks);
        assert_eq!(Report::new(&replay), melee.report);
    }

    #[test]
    fn the_same_seed_plays_the_same_melee() {
        let data = data();
        let play = || {
            let (mut fighters, mut rng) = twenty(&data, 2);
            let melee = play_melee(&mut fighters, &[], ArenaKind::Pillars, &mut rng, &data);
            (melee, fighters)
        };
        assert_eq!(play(), play());
    }

    #[test]
    fn a_rival_club_s_fighters_hunt_the_player_s() {
        let data = data();
        let (mut fighters, mut rng) = twenty(&data, 3);
        let club = Entrant {
            of_club: true,
            ..Entrant::default()
        };
        let hunter = Entrant {
            hunts_club: true,
            ..Entrant::default()
        };
        let friend = Entrant {
            pact_points: 10,
            ..Entrant::default()
        };
        let entrants = [club, hunter, friend, hunter, club];
        let melee = play_melee(&mut fighters, &entrants, ArenaKind::Narrow, &mut rng, &data);
        let hunt = |a, b| (FighterId(a), FighterId(b));
        assert_eq!(
            melee.start.hunts,
            [hunt(1, 0), hunt(1, 4), hunt(3, 0), hunt(3, 4)]
        );
        let ally = |a, b| (FighterId(a), FighterId(b), 10);
        assert_eq!(melee.start.pact_points, [ally(2, 0), ally(2, 4)]);
    }

    #[test]
    fn each_fighter_sees_the_melee_from_its_place() {
        let data = data();
        let (mut fighters, mut rng) = twenty(&data, 3);
        let refused = Refused { armour: true };
        let entrants = [
            Entrant::default(),
            Entrant {
                refused,
                ..Entrant::default()
            },
        ];
        let melee = play_melee(&mut fighters, &entrants, ArenaKind::Narrow, &mut rng, &data);
        let summary = melee.summary(1).unwrap();
        assert_eq!(summary.me, FighterId(1));
        assert!(summary.is_melee());
        assert_eq!(summary.opponents().count(), 19);
        assert_eq!(summary.opponent(), Some(&melee.setup.fighters[0]));
        assert_eq!(summary.place, melee.places[1]);
        assert_eq!(summary.refused, refused);
        assert_eq!(summary.mine().map(|r| r.id), Some(FighterId(1)));
        let armour = data.fighter.equipment.armourless_defense_percent;
        let defense = melee.setup.fighters[1]
            .fighting_stats(&data.fighter)
            .melee_defense;
        assert_eq!(
            summary.start(&data).fighters[1].stats.melee_defense,
            defense * armour / 100
        );
        assert!(melee.summary(20).is_none());
    }
}
