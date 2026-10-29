//! The season-opening melees: who fights each colosseum's ([`melee_lineup`]), the fight
//! ([`melee_entries`]) and what it pays and leaves ([`settle_melee`]).

use std::cmp::Reverse;

use crate::colosseum::{ColosseumId, Contender};
use crate::combat::{Floor, shuffle};
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction};
use crate::fighter::career::{Activity, Status, WeekPlan, WeekReport};
use crate::fighter::equipment::Refused;
use crate::fighter::melee::{Entrant, MeleeFight, Pact, play_melee};
use crate::relations::Reason;
use crate::staff::{Medicine, StaffEffects};

use super::outsiders::{
    OutsiderPlans, outsider, outsider_effects, outsider_mut, outsiders, pay_rival, put_outsider,
};
use super::{GameState, affordable, entry_fee, relate};

/// Fewest fighters in a melee, below which it is called off.
const MELEE_MIN_FIGHTERS: usize = 3;

/// The season-opening melees, before the club's other fights, one per colosseum (see
/// [`melee_lineup`] for who fights, [`settle_melee`] for what it pays). The outsiders
/// who fought are added to `fought`, their week to end with the others'; the club's
/// fighters' weeks are returned, in club order, and the club's money moves are in
/// `ledger`.
pub(super) fn melees(
    state: &mut GameState,
    (wanted, plans): (&mut [WeekPlan], &mut OutsiderPlans),
    costs: (u32, Medicine),
    (fought, reserved, ledger): (&mut Vec<Contender>, &[Contender], &mut Vec<Transaction>),
    data: &DesignData,
) -> Vec<Option<WeekReport>> {
    let mut weeks: Vec<Option<WeekReport>> = vec![None; state.fighters.len()];
    if !state.opens_season(data) {
        return weeks;
    }
    let effects = state.staff_effects();
    let colosseums = data.world.colosseums.iter().enumerate();
    for (c, rules) in colosseums.take(state.colosseums.len()) {
        let lineup = melee_lineup(state, c, (wanted, plans), costs, (fought, reserved), data);
        let Some(entries) = lineup else {
            continue;
        };
        let floor = Floor {
            kind: rules.arena,
            element: data.world.element(rules.continent),
        };
        let played = melee_entries(state, &entries, (wanted, floor), effects, data);
        if let Some(played) = played {
            settle_melee(
                state,
                (c, &entries),
                played,
                (&mut weeks, fought, ledger),
                data,
            );
        }
    }
    weeks
}

/// Who fights colosseum `c`'s melee, in a random order: the club's fighters registered
/// there (`Activity::Melee` in `wanted`) as the club can pay, care and fee, at `costs`;
/// the outsiders registered there (in `plans`), then, if room is left, those ranked
/// there who would only train, none of those who `fought` or are `reserved` for a duel
/// with the club's fighters. At most `MeleeRules::max_fighters`, the club's fighters
/// first, then the outsiders by Elo, the others training (or resting, for a free
/// agent). Fewer than [`MELEE_MIN_FIGHTERS`]: it is called off, and they rest or train.
fn melee_lineup(
    state: &mut GameState,
    c: usize,
    (wanted, plans): (&mut [WeekPlan], &mut OutsiderPlans),
    costs: (u32, Medicine),
    (fought, reserved): (&[Contender], &[Contender]),
    data: &DesignData,
) -> Option<Vec<Contender>> {
    let world = &data.world;
    world.colosseums.get(c)?;
    let fee = entry_fee(c, state.continent, data);
    let most = usize::try_from(data.combat.melee.max_fighters).unwrap_or(usize::MAX);
    let here = Activity::Melee(ColosseumId(u32::try_from(c).ok()?));
    // The club's money promised to fees and care, before anyone is paid.
    let mut promised = 0;
    let mut club = Vec::new();
    for (i, f) in state.fighters.iter().enumerate() {
        if wanted[i].activity != here || f.status != Status::Active {
            continue;
        }
        let money = state.money - promised;
        let plan = affordable(wanted[i], f, (money, state.continent), costs, data);
        if plan.activity == here && club.len() < most {
            let care = costs.1.care_rules(&data.fighter.care);
            let care = if plan.care { f.care_cost(&care) } else { 0 };
            promised += i64::from(fee) + i64::from(care);
            club.push(i);
        }
        wanted[i] = plan;
    }
    let colosseum = state.colosseums.get(c)?;
    let elo = |o: Contender| colosseum.elo(o, world);
    let available = |o: &Contender| !fought.contains(o) && !reserved.contains(o);
    let mut registered: Vec<Contender> = outsiders(&state.rivals, &state.free_agents)
        .filter(|o| available(o) && plans.of(*o).is_some_and(|p| p.activity == here))
        .collect();
    registered.sort_by_key(|&o| (Reverse(elo(o)), o));
    let room = most.saturating_sub(club.len());
    if registered.len() < room {
        // The ranking fills the melee with those who would only train.
        let active = |o: Contender| {
            outsider(&state.rivals, &state.free_agents, o)
                .is_some_and(|f| f.status == Status::Active)
        };
        let filling: Vec<Contender> = colosseum
            .ranking
            .iter()
            .map(|r| r.contender)
            .filter(|&o| !o.is_club() && available(&o) && !registered.contains(&o) && active(o))
            .filter(|&o| plans.of(o).is_some_and(|p| p.activity == Activity::Train))
            .take(room - registered.len())
            .collect();
        registered.extend(filling);
    }
    let left_out = registered.split_off(room.min(registered.len()));
    let called_off = club.len() + registered.len() < MELEE_MIN_FIGHTERS;
    let instead = |o: Contender| match o {
        Contender::Rival(_) => Activity::Train,
        _ => Activity::Rest,
    };
    let sent_back = if called_off { &registered[..] } else { &[] };
    for &o in left_out.iter().chain(sent_back) {
        plans.set_activity(o, instead(o));
    }
    if called_off {
        for &i in &club {
            wanted[i].activity = Activity::Rest;
        }
        return None;
    }
    let mut entries: Vec<Contender> = club.into_iter().map(Contender::Club).collect();
    entries.extend(registered);
    shuffle(&mut state.rng, &mut entries);
    Some(entries)
}

/// What colosseum `c`'s melee of `entries` pays and leaves, `played` as
/// [`melee_entries`] returns it: what the stage does to the vain
/// (`CareerFighter::on_stage`); everyone's Elo by place (`Colosseum::record_places`);
/// each fighter's club pays the fee and takes its place's share of the melee prize (the
/// same for everyone, whatever its fame or relations), a free agent paying and earning
/// nothing; the
/// club pays its fighters' care, and they end their week, into `weeks`; the outsiders
/// go to `fought`, and the club's money moves into `ledger`.
fn settle_melee(
    state: &mut GameState,
    (c, entries): (usize, &[Contender]),
    (melee, mut club_weeks): (MeleeFight, Vec<Option<WeekReport>>),
    (weeks, fought, ledger): (
        &mut [Option<WeekReport>],
        &mut Vec<Contender>,
        &mut Vec<Transaction>,
    ),
    data: &DesignData,
) {
    let world = &data.world;
    let Some((purses, prestige)) = world.colosseums.get(c).map(|r| (&r.purses, r.prestige)) else {
        return;
    };
    for &entry in entries {
        let fighter = match entry {
            Contender::Club(i) => state.fighters.get_mut(i),
            outsider => outsider_mut(&mut state.rivals, &mut state.free_agents, outsider),
        };
        if let Some(f) = fighter {
            f.on_stage(prestige, false, &data.fighter);
        }
    }
    let places: Vec<(Contender, u32)> = entries
        .iter()
        .copied()
        .zip(melee.places.iter().copied())
        .collect();
    if let Some(colosseum) = state.colosseums.get_mut(c) {
        colosseum.record_places(&places, world);
    }
    for (club, why) in melee.pacts.iter().filter_map(|&p| pact_reason(entries, p)) {
        relate(state, club, why, data);
    }
    let fee = |home| i64::from(entry_fee(c, home, data));
    for (k, &entry) in entries.iter().enumerate() {
        let side = &melee.sides[k];
        let place = melee.places.get(k).copied().unwrap_or(1);
        let paid = (entries.len(), data.combat.melee.paid_places);
        let receipts = i64::from(purses.melee_receipts(place, paid));
        match entry {
            Contender::Club(i) => {
                let Some(mut report) = club_weeks[k].take() else {
                    continue;
                };
                for (flow, amount) in [
                    (Flow::Care, -i64::from(report.care_cost)),
                    (Flow::EntryFee, -fee(state.continent)),
                    (Flow::Receipts, receipts),
                ] {
                    if amount != 0 {
                        state.money += amount;
                        ledger.push(Transaction {
                            fighter: Some(i),
                            flow,
                            amount,
                        });
                    }
                }
                if let Some(summary) = melee.summary(k) {
                    let taken = (summary, side.new_injury, side.after_effect_spared);
                    state.fighters[i].end_melee_week(&mut report, taken, &mut state.rng, data);
                }
                weeks[i] = Some(report);
            }
            Contender::Rival(id) => {
                let Some(home) = state.rivals.get(id.club).map(|r| r.continent) else {
                    continue;
                };
                pay_rival(&mut state.rivals, id.club, receipts - fee(home));
                fought.push(entry);
            }
            Contender::FreeAgent(_) => fought.push(entry),
        }
    }
}

/// The rival club whose relation `pact` of a melee of `entries` moves, and why: one
/// between a club's fighter and a rival fighter, kept, or betrayed by the club's
/// fighter; betrayed by the rival fighter, it changes nothing.
pub(super) fn pact_reason(entries: &[Contender], pact: Pact) -> Option<(usize, Reason)> {
    let (mine, fighter, rival) = match (*entries.get(pact.a)?, *entries.get(pact.b)?) {
        (Contender::Club(i), Contender::Rival(id)) => (pact.a, i, id),
        (Contender::Rival(id), Contender::Club(i)) => (pact.b, i, id),
        _ => return None,
    };
    let why = match pact.betrayer {
        None => Reason::PactKept { fighter, rival },
        Some(k) if k == mine => Reason::Betrayed { fighter, rival },
        Some(_) => return None,
    };
    Some((rival.club, why))
}

/// The melee of `entries`, on `floor`, played: each club's fighter begins
/// its week following its `wanted` plan (care, paid by the caller), then everyone
/// fights, and each one goes back where it came from. Returns the melee and, for each
/// entry, the club's fighter's week begun; `None` if an entry is nobody, before anyone's
/// week begins.
pub(super) fn melee_entries(
    state: &mut GameState,
    entries: &[Contender],
    (wanted, floor): (&[WeekPlan], Floor),
    effects: StaffEffects,
    data: &DesignData,
) -> Option<(MeleeFight, Vec<Option<WeekReport>>)> {
    let rosters = state.rosters();
    let known = |entry: Contender| {
        rosters.fighter(entry).is_some()
            && !matches!(entry, Contender::Club(i) if i >= wanted.len())
    };
    if !entries.iter().all(|&entry| known(entry)) {
        return None;
    }
    let mut fighters = Vec::with_capacity(entries.len());
    let mut entrants = Vec::with_capacity(entries.len());
    let mut weeks = Vec::with_capacity(entries.len());
    for &entry in entries {
        if let Contender::Club(i) = entry {
            let plan = *wanted.get(i)?;
            let f = state.fighters.get_mut(i)?;
            weeks.push(f.begin_melee_week(plan, effects, data));
            fighters.push(f.clone());
            entrants.push(Entrant {
                refused: plan.refused,
                medicine: effects.medicine,
                of_club: true,
                hunts_club: false,
                pact_points: 0,
            });
        } else {
            fighters.push(outsider(&state.rivals, &state.free_agents, entry)?.clone());
            entrants.push(Entrant {
                refused: Refused::NONE,
                medicine: outsider_effects(&state.rivals, entry).medicine,
                of_club: false,
                hunts_club: matches!(entry, Contender::Rival(id) if state.is_rival(id.club)),
                pact_points: match entry {
                    Contender::Rival(id) => state.standing_rules(id.club, data).pact_points,
                    _ => 0,
                },
            });
            weeks.push(None);
        }
    }
    let melee = play_melee(&mut fighters, &entrants, floor, &mut state.rng, data);
    for (&entry, f) in entries.iter().zip(fighters) {
        match entry {
            Contender::Club(i) => state.fighters[i] = f,
            outsider => put_outsider(state, outsider, f),
        }
    }
    Some((melee, weeks))
}
