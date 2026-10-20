//! The colosseums' weeks: their end ([`end_colosseum_weeks`]: titles vacated, stripped
//! and defended), title fights and the rival fighters' challenges.

use rand::RngExt;

use crate::colosseum::{Challenge, Colosseum, ColosseumId, Contender, Rosters, TitleNews};
use crate::combat::ArenaKind;
use crate::design_data::DesignData;
use crate::fighter::career::{Activity, Status, WeekReport, suggested_plan};
use crate::fighter::fame::FightResult;
use crate::relations::Reason;

use super::outsiders::{outsider, put_outsider};
use super::{GameState, opposite, relate};

/// The colosseums' end of `week`: a title whose holder no longer fights goes to the
/// best ranked still fighting; an outsider champion at its deadline defends the title
/// against the best-ranked other outsider, a club champion past it loses the title.
/// Those who no longer fight leave the rankings. Returns what happened to the titles,
/// with the colosseum's index.
pub(super) fn end_colosseum_weeks(
    state: &mut GameState,
    week: u64,
    data: &DesignData,
) -> Vec<(usize, TitleNews)> {
    let mut titles = Vec::new();
    for c in 0..state.colosseums.len() {
        let rosters = Rosters {
            club: &state.fighters,
            rivals: &state.rivals,
            free_agents: &state.free_agents,
        };
        let colosseum = &mut state.colosseums[c];
        let mut news: Vec<TitleNews> = colosseum
            .vacate_if_gone(week, rosters)
            .into_iter()
            .collect();
        news.extend(colosseum.strip(week, rosters, &data.world));
        let defense = match colosseum.champion {
            Some(champion)
                if !champion.is_club() && week >= colosseum.defense_deadline(&data.world) =>
            {
                colosseum
                    .best_outsider(rosters, Some(champion))
                    .map(|challenger| (champion, challenger))
            }
            _ => None,
        };
        if let Some((champion, challenger)) = defense {
            let arena = data
                .world
                .colosseums
                .get(c)
                .map_or(ArenaKind::Sand, |r| r.arena);
            let result = outsider_duel(state, champion, challenger, arena, data);
            let colosseum = &mut state.colosseums[c];
            colosseum.record_title_fight(champion, challenger, result, week, &data.world);
            news.push(TitleNews::Fight {
                champion,
                challenger,
                result,
            });
            // The duel may have killed one of them.
            let rosters = Rosters {
                club: &state.fighters,
                rivals: &state.rivals,
                free_agents: &state.free_agents,
            };
            news.extend(colosseum.vacate_if_gone(week, rosters));
        }
        let rosters = Rosters {
            club: &state.fighters,
            rivals: &state.rivals,
            free_agents: &state.free_agents,
        };
        state.colosseums[c]
            .ranking
            .retain(|r| rosters.active(r.contender));
        titles.extend(news.into_iter().map(|n| (c, n)));
    }
    titles
}

/// A title defense between outsiders `a`, the champion, and `b`, in an arena of
/// `kind`, resolved by the statistical model; they take only fame, record and death
/// from it. Returns `a`'s result.
fn outsider_duel(
    state: &mut GameState,
    a: Contender,
    b: Contender,
    kind: ArenaKind,
    data: &DesignData,
) -> FightResult {
    let (rivals, free_agents) = (&state.rivals, &state.free_agents);
    let (Some(mut first), Some(mut second)) = (
        outsider(rivals, free_agents, a).cloned(),
        outsider(rivals, free_agents, b).cloned(),
    ) else {
        return FightResult::Unfinished;
    };
    let [(outcome, _), _] = first.modelled_fame_and_death(&mut second, kind, &mut state.rng, data);
    put_outsider(state, a, first);
    put_outsider(state, b, second);
    outcome.result
}

/// Records the title fight of the club's fighter `club` against `rival` in
/// `colosseum`, `result` being the club fighter's.
pub(super) fn title_fight(
    colosseum: &mut Colosseum,
    club: Contender,
    rival: Contender,
    result: FightResult,
    week: u64,
    data: &DesignData,
) -> TitleNews {
    let (champion, challenger, result) = if colosseum.champion == Some(club) {
        (club, rival, result)
    } else {
        (rival, club, opposite(result))
    };
    colosseum.record_title_fight(champion, challenger, result, week, &data.world);
    TitleNews::Fight {
        champion,
        challenger,
        result,
    }
}

/// Clears the week's challenges; returns those refused by a fighter who did not fight,
/// after taking their price in fame and Elo.
pub(super) fn refusals(
    state: &mut GameState,
    reports: &[Option<WeekReport>],
    data: &DesignData,
) -> Vec<Challenge> {
    let rules = &data.world.challenges;
    let mut refused = std::mem::take(&mut state.challenges);
    refused.retain(|ch| {
        reports
            .get(ch.fighter)
            .and_then(Option::as_ref)
            .is_some_and(|r| matches!(r.plan.activity, Activity::Rest | Activity::Train))
    });
    for ch in &refused {
        let fighter = &mut state.fighters[ch.fighter];
        fighter.fame = fighter
            .fame
            .changed_by(-i32::try_from(rules.refusal_fame).unwrap_or(i32::MAX));
        if let Some(colosseum) = usize::try_from(ch.colosseum.0)
            .ok()
            .and_then(|c| state.colosseums.get_mut(c))
        {
            colosseum.lose_elo(Contender::Club(ch.fighter), rules.refusal_elo, &data.world);
        }
        let why = Reason::ChallengeRefused {
            fighter: ch.fighter,
            rival: ch.rival,
        };
        relate(state, ch.rival.club, why, data);
    }
    refused
}

/// The rival fighters' challenges for the week to play: in each colosseum where it is
/// ranked, a club's fighter fit to fight is challenged by the rival fighter closest in
/// Elo, with the `challenge_percent` chance of that fighter's club.
pub(super) fn receive_challenges(state: &mut GameState, data: &DesignData) {
    let rosters = Rosters {
        club: &state.fighters,
        rivals: &state.rivals,
        free_agents: &state.free_agents,
    };
    for (i, fighter) in state.fighters.iter().enumerate() {
        let fit = suggested_plan(fighter, state.week).activity != Activity::Rest;
        if fighter.status != Status::Active || !fit {
            continue;
        }
        for (id, colosseum) in (0..).zip(&state.colosseums) {
            if let Some(rival) = colosseum.challenger(i, rosters)
                && let Some(profile) = state.rivals.get(rival.club).map(|r| r.rules(data))
                && state.rng.random_range(0..100)
                    < profile.challenge_percent
                        + state.standing_rules(rival.club, data).challenge_points
            {
                state.challenges.push(Challenge {
                    fighter: i,
                    colosseum: ColosseumId(id),
                    rival,
                });
            }
        }
    }
}
