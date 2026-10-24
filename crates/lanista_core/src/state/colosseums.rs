//! The colosseums' weeks: their end ([`end_colosseum_weeks`]: titles vacated, stripped
//! and defended) and title fights.

use crate::colosseum::{Colosseum, Contender, Rosters, TitleNews};
use crate::combat::ArenaKind;
use crate::design_data::DesignData;
use crate::fighter::fame::FightResult;

use super::outsiders::{outsider, put_outsider};
use super::{GameState, opposite};

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
