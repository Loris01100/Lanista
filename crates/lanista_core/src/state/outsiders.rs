//! The outsiders' weeks: the rival fighters and the free agents, their plans
//! ([`OutsiderPlans`]), the duels they fight among themselves ([`outsider_weeks`]) and the
//! one the club's fighters meet ([`opponent`]).

use std::cmp::Reverse;

use rand_chacha::ChaCha8Rng;

use crate::colosseum::{Colosseum, ColosseumId, Contender, Rosters, WorldRules, pick_opponent};
use crate::design_data::DesignData;
use crate::fighter::career::{Activity, CareerFighter, Status, WeekPlan};
use crate::fighter::fame::FightResult;
use crate::fighter::form::FormEvent;
use crate::rivals::{Rival, RivalId};
use crate::staff::{Medicine, StaffEffects};

use super::{GameState, entry_fee, opposite};

/// What the staff of `outsider`'s club brings to its weeks: nothing for a free agent.
pub(super) fn outsider_effects(rivals: &[Rival], outsider: Contender) -> StaffEffects {
    match outsider {
        Contender::Rival(id) => rivals
            .get(id.club)
            .map(|r| r.staff.effects())
            .unwrap_or_default(),
        Contender::Club(_) | Contender::FreeAgent(_) => StaffEffects::default(),
    }
}

/// The outsider `them` met by a club's fighter, with its doctor's medicine.
pub(super) fn met<'a>(
    rivals: &'a mut [Rival],
    free_agents: &'a mut [CareerFighter],
    them: Contender,
) -> Option<(&'a mut CareerFighter, Medicine)> {
    let medicine = outsider_effects(rivals, them).medicine;
    outsider_mut(rivals, free_agents, them).map(|f| (f, medicine))
}

/// The week's plans of everyone out of the club, made before anyone plays.
#[derive(Debug, Clone)]
pub(super) struct OutsiderPlans {
    /// By rival club, in roster order (`GameState::rival_plans`).
    pub(super) rivals: Vec<Vec<WeekPlan>>,
    /// In market order (`GameState::free_agent_plans`).
    pub(super) free_agents: Vec<WeekPlan>,
}

impl OutsiderPlans {
    /// The plan of `outsider`; `None` for the club's fighters and those who came after
    /// the plans were made.
    pub(super) fn of(&self, outsider: Contender) -> Option<WeekPlan> {
        match outsider {
            Contender::Rival(id) => self.rivals.get(id.club)?.get(id.fighter).copied(),
            Contender::FreeAgent(a) => self.free_agents.get(a).copied(),
            Contender::Club(_) => None,
        }
    }

    pub(super) fn set_activity(&mut self, outsider: Contender, activity: Activity) {
        let plan = match outsider {
            Contender::Rival(id) => self
                .rivals
                .get_mut(id.club)
                .and_then(|p| p.get_mut(id.fighter)),
            Contender::FreeAgent(a) => self.free_agents.get_mut(a),
            Contender::Club(_) => None,
        };
        if let Some(plan) = plan {
            plan.activity = activity;
        }
    }
}

/// The outsider `outsider` names, a rival fighter or a free agent.
pub(super) fn outsider<'a>(
    rivals: &'a [Rival],
    free_agents: &'a [CareerFighter],
    outsider: Contender,
) -> Option<&'a CareerFighter> {
    let rosters = Rosters {
        club: &[],
        rivals,
        free_agents,
    };
    rosters.fighter(outsider)
}

pub(super) fn outsider_mut<'a>(
    rivals: &'a mut [Rival],
    free_agents: &'a mut [CareerFighter],
    outsider: Contender,
) -> Option<&'a mut CareerFighter> {
    match outsider {
        Contender::Rival(id) => rivals.get_mut(id.club)?.fighters.get_mut(id.fighter),
        Contender::FreeAgent(a) => free_agents.get_mut(a),
        Contender::Club(_) => None,
    }
}

/// Puts `fighter` back in the place of `outsider`.
pub(super) fn put_outsider(state: &mut GameState, outsider: Contender, fighter: CareerFighter) {
    if let Some(place) = outsider_mut(&mut state.rivals, &mut state.free_agents, outsider) {
        *place = fighter;
    }
}

/// Every outsider still fighting: the rival fighters club after club, then the free
/// agents.
pub(super) fn outsiders<'a>(
    rivals: &'a [Rival],
    free_agents: &'a [CareerFighter],
) -> impl Iterator<Item = Contender> + 'a {
    rivals
        .iter()
        .enumerate()
        .flat_map(|(club, r)| {
            (0..r.fighters.len()).map(move |fighter| Contender::Rival(RivalId { club, fighter }))
        })
        .chain((0..free_agents.len()).map(Contender::FreeAgent))
        .filter(move |&o| {
            outsider(rivals, free_agents, o).is_some_and(|f| f.status == Status::Active)
        })
}

/// The week of the rival clubs and the free agents, after the club's fights. In each
/// colosseum, the outsiders registered there and not met by the club's fighters are
/// paired in Elo order, but a spiteful one with a fighter it bears a grudge against
/// registered there too ([`pairs`]), each pair fighting a fight the statistical model resolves,
/// which moves both Elo; one left without an opponent trains instead, or rests if it
/// is a free agent, who cannot pay for training. The others rest or train as planned,
/// those met by the club's fighters end their week, and each rival club pays its
/// fighters' costs and upkeep and takes their receipts and prizes; a free agent pays
/// and earns nothing.
pub(super) fn outsider_weeks(
    state: &mut GameState,
    mut plans: OutsiderPlans,
    fought: &[Contender],
    data: &DesignData,
) {
    let world = &data.world;
    let upkeep = i64::from(data.economy.upkeep);
    for (club, rival) in state.rivals.iter_mut().enumerate() {
        for (fighter, f) in rival.fighters.iter_mut().enumerate() {
            if f.status == Status::Active {
                rival.money -= upkeep;
                if fought.contains(&Contender::Rival(RivalId { club, fighter })) {
                    f.end_fought_week(&mut state.rng, data);
                }
            }
        }
    }
    for (a, f) in state.free_agents.iter_mut().enumerate() {
        if f.status == Status::Active && fought.contains(&Contender::FreeAgent(a)) {
            f.end_fought_week(&mut state.rng, data);
        }
    }
    let mut done = fought.to_vec();
    for (c, (purses, arena, prestige)) in world
        .colosseums
        .iter()
        .map(|r| (&r.purses, r.arena, r.prestige))
        .enumerate()
        .take(state.colosseums.len())
    {
        let here = Activity::Fight(ColosseumId(u32::try_from(c).unwrap_or(u32::MAX)));
        let colosseum = &state.colosseums[c];
        let mut waiting: Vec<(i32, Contender)> = outsiders(&state.rivals, &state.free_agents)
            .filter(|o| !done.contains(o) && plans.of(*o).is_some_and(|p| p.activity == here))
            .map(|o| (colosseum.elo(o, world), o))
            .collect();
        waiting.sort_by_key(|&(elo, o)| (Reverse(elo), o));
        let waiting: Vec<Contender> = waiting.into_iter().map(|(_, o)| o).collect();
        let (pairs, alone) = pairs(waiting, |a, b| {
            let (rivals, free_agents) = (&state.rivals, &state.free_agents);
            let fighters = (
                outsider(rivals, free_agents, a),
                outsider(rivals, free_agents, b),
            );
            fighters
                .0
                .zip(fighters.1)
                .is_some_and(|(a, b)| a.bears_grudge(b.id) || b.bears_grudge(a.id))
        });
        if let Some(alone) = alone {
            let instead = match alone {
                Contender::Rival(_) => Activity::Train,
                _ => Activity::Rest,
            };
            plans.set_activity(alone, instead);
        }
        for (a, b) in pairs {
            let (rivals, free_agents) = (&state.rivals, &state.free_agents);
            let (Some(mut first), Some(mut second)) = (
                outsider(rivals, free_agents, a).cloned(),
                outsider(rivals, free_agents, b).cloned(),
            ) else {
                continue;
            };
            let receipts = i64::from(purses.receipts(first.fame, second.fame));
            let medicine = [a, b].map(|o| outsider_effects(&state.rivals, o).medicine);
            let result = first.modelled_fight(
                &mut second,
                arena,
                FormEvent::Fight,
                medicine,
                &mut state.rng,
                data,
            );
            for f in [&mut first, &mut second] {
                f.on_stage(prestige, false, &data.fighter);
                f.end_fought_week(&mut state.rng, data);
            }
            put_outsider(state, a, first);
            put_outsider(state, b, second);
            done.extend([a, b]);
            state.colosseums[c].record(a, b, result, world);
            for (them, result) in [(a, result), (b, opposite(result))] {
                let Contender::Rival(id) = them else {
                    continue;
                };
                let prize = if result == FightResult::Won {
                    purses.win_prize
                } else {
                    0
                };
                let Some(home) = state.rivals.get(id.club).map(|r| r.continent) else {
                    continue;
                };
                let fee = i64::from(entry_fee(c, home, data));
                pay_rival(
                    &mut state.rivals,
                    id.club,
                    receipts + i64::from(prize) - fee,
                );
            }
        }
    }
    planned_weeks(state, &plans, &done, data);
}

/// `waiting`, in Elo order, paired: each with the next one, unless one further on is its
/// `foe`; and the one left alone, if any.
fn pairs(
    mut waiting: Vec<Contender>,
    foe: impl Fn(Contender, Contender) -> bool,
) -> (Vec<(Contender, Contender)>, Option<Contender>) {
    let mut pairs = Vec::new();
    while !waiting.is_empty() {
        let a = waiting.remove(0);
        let Some(k) = waiting
            .iter()
            .position(|&b| foe(a, b))
            .or_else(|| (!waiting.is_empty()).then_some(0))
        else {
            return (pairs, Some(a));
        };
        pairs.push((a, waiting.remove(k)));
    }
    (pairs, None)
}

/// The planned weeks of the outsiders not in `done`, each rival club paying its
/// fighters' care and training at its staff's prices, its staff's effects with them.
fn planned_weeks(
    state: &mut GameState,
    plans: &OutsiderPlans,
    done: &[Contender],
    data: &DesignData,
) {
    for (club, rival) in state.rivals.iter_mut().enumerate() {
        let effects = rival.staff.effects();
        let session = rival.staff.training_cost(data.economy.training_cost);
        for (fighter, f) in rival.fighters.iter_mut().enumerate() {
            let me = Contender::Rival(RivalId { club, fighter });
            let Some(plan) = plans.of(me).filter(|_| !done.contains(&me)) else {
                continue;
            };
            if let Some(report) = f.play_week(plan, None, effects, &mut state.rng, data) {
                let training = if plan.activity == Activity::Train {
                    session
                } else {
                    0
                };
                rival.money -= i64::from(report.care_cost) + i64::from(training);
            }
        }
    }
    for (a, f) in state.free_agents.iter_mut().enumerate() {
        let me = Contender::FreeAgent(a);
        if let Some(plan) = plans.of(me).filter(|_| !done.contains(&me)) {
            f.play_week(plan, None, StaffEffects::default(), &mut state.rng, data);
        }
    }
}

/// Adds `amount` to the money of rival club `club`.
pub(super) fn pay_rival(rivals: &mut [Rival], club: usize, amount: i64) {
    if let Some(rival) = rivals.get_mut(club) {
        rival.money += amount;
    }
}

/// The colosseum and the outsider met in a fight (`activity`) by the club's fighter
/// `me`, the club's `i`th: the rival fighter challenged while still fighting (someone
/// may have killed it earlier in the week), otherwise the closest in Elo there, at
/// random between the equally close, among those registered there (in `plans`) who
/// have not `fought` yet, those `me` bears a grudge against first, or else among those
/// ranked there, or else among anyone.
pub(super) fn opponent(
    rng: &mut ChaCha8Rng,
    (colosseums, rivals, free_agents, rules): (
        &[Colosseum],
        &[Rival],
        &[CareerFighter],
        &WorldRules,
    ),
    (activity, i, me): (Activity, usize, &CareerFighter),
    plans: &OutsiderPlans,
    fought: &[Contender],
) -> Option<(usize, Contender)> {
    let (id, chosen) = match activity {
        Activity::Fight(id) => (id, None),
        Activity::Challenge(id, rival) => (id, Some(Contender::Rival(rival))),
        _ => return None,
    };
    let c = usize::try_from(id.0).ok()?;
    let colosseum = colosseums.get(c)?;
    let active = |o: &Contender| {
        outsider(rivals, free_agents, *o).is_some_and(|f| f.status == Status::Active)
    };
    if let Some(rival) = chosen.filter(active) {
        return Some((c, rival));
    }
    let everyone: Vec<Contender> = outsiders(rivals, free_agents)
        .filter(|o| !fought.contains(o))
        .collect();
    let registered: Vec<Contender> = everyone
        .iter()
        .copied()
        .filter(|&o| {
            plans
                .of(o)
                .is_some_and(|p| p.activity == Activity::Fight(id))
        })
        .collect();
    let foes: Vec<Contender> = registered
        .iter()
        .copied()
        .filter(|&o| outsider(rivals, free_agents, o).is_some_and(|f| me.bears_grudge(f.id)))
        .collect();
    let ranked: Vec<Contender> = colosseum
        .ranking
        .iter()
        .map(|r| r.contender)
        .filter(|o| everyone.contains(o))
        .collect();
    let candidates = [foes, registered, ranked, everyone]
        .into_iter()
        .find(|c| !c.is_empty())?;
    let mine = colosseum.elo(Contender::Club(i), rules);
    let gap = |o: &Contender| (colosseum.elo(*o, rules) - mine).unsigned_abs();
    let closest = candidates.iter().map(gap).min()?;
    let nearest: Vec<Contender> = candidates
        .into_iter()
        .filter(|o| gap(o) == closest)
        .collect();
    Some((c, pick_opponent(rng, &nearest)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foes_are_paired_before_the_elo_order() {
        let o = Contender::FreeAgent;
        let none = |_, _| false;
        assert_eq!(
            pairs(vec![o(0), o(1), o(2)], none),
            (vec![(o(0), o(1))], Some(o(2)))
        );
        let foes = |a, b| [a, b] == [o(0), o(2)];
        assert_eq!(
            pairs(vec![o(0), o(1), o(2), o(3)], foes),
            (vec![(o(0), o(2)), (o(1), o(3))], None)
        );
    }
}
