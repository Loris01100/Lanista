//! The clubs' lives at the off-season (wealth, ruin, closures and foundings) and the
//! staff's week, the player's and the rival clubs'.

use crate::calendar::SeasonPhase;
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction, Wealth};
use crate::fighter::career::Status;
use crate::rivals::{self, RivalId};
use crate::staff::{self, Staff, StaffMove, StaffNews};

use super::transfers::release_rival;
use super::{GameState, News, WorldNews};

/// At the start of the off-season, each club's wealth follows its money
/// (`Wealth::next`); returns the player's club's new wealth if it changed.
pub(super) fn wealth_changes(state: &mut GameState, data: &DesignData) -> Option<Wealth> {
    if !off_season_starts(state, data) {
        return None;
    }
    let rules = &data.economy.wealth;
    for rival in state.rivals.iter_mut().filter(|r| r.is_open()) {
        rival.wealth = rival.wealth.next(rival.money, rules);
    }
    let wealth = state.wealth.next(state.money, rules);
    (wealth != state.wealth).then(|| {
        state.wealth = wealth;
        wealth
    })
}

/// At the start of the off-season, after the wealth changes, each poor club with less
/// money than `clubs.ruined_below` is ruined for the season, the others are not; one
/// ruined `clubs.ruined_seasons` in a row closes (`close_club`). Then as many clubs are
/// founded as closed, at least `clubs.founded`, never more than `clubs.max` open, each
/// with generated fighters up to `roster.wanted`.
pub(super) fn club_lives(state: &mut GameState, data: &DesignData) {
    if !off_season_starts(state, data) {
        return;
    }
    let rules = &data.world.clubs;
    let mut closed = 0;
    for club in 0..state.rivals.len() {
        let rival = &mut state.rivals[club];
        if !rival.is_open() {
            continue;
        }
        let ruined = rival.wealth == Wealth::Poor && rival.money < i64::from(rules.ruined_below);
        rival.ruined_seasons = if ruined { rival.ruined_seasons + 1 } else { 0 };
        if rival.ruined_seasons >= rules.ruined_seasons {
            close_club(state, club);
            closed += 1;
        }
    }
    let open = state.rivals.iter().filter(|r| r.is_open()).count();
    let room = usize::try_from(rules.max).map_or(0, |max| max.saturating_sub(open));
    let founded = closed.max(usize::try_from(rules.founded).unwrap_or(0));
    for _ in 0..founded.min(room) {
        let club = rivals::random_club(&mut state.rng, (&state.rivals, &state.name), data);
        state.rivals.push(club);
        let club = state.rivals.len() - 1;
        state.fill_roster(club, data);
        state.news.push(News {
            week: state.week,
            what: WorldNews::Founded(club),
        });
    }
}

/// Rival club `club` closes: its fighters go to the market with their Elo and titles,
/// its staff leaves, and it plays no more.
fn close_club(state: &mut GameState, club: usize) {
    for fighter in 0..state.rivals[club].fighters.len() {
        if state.rivals[club].fighters[fighter].status == Status::Active {
            release_rival(state, RivalId { club, fighter });
        }
    }
    let rival = &mut state.rivals[club];
    rival.staff = Staff::default();
    rival.closed = Some(state.week);
    state.news.push(News {
        week: state.week,
        what: WorldNews::Closed(club),
    });
}

/// Each rival club's changes of staff this week, in club order, among the candidates
/// the club and the clubs before it left (`rivals::staff_moves`, the posts of its
/// wealth, with its money for each share of the salaries), then its salaries, paid
/// once for every `fighters_per_salary` fighters in activity, even into debt. The market's week ends with them: no one else hires before the next.
pub(super) fn rival_staff(state: &mut GameState, data: &DesignData) {
    let market = &mut state.staff_market;
    for rival in state.rivals.iter_mut().filter(|r| r.is_open()) {
        let shares = i64::from(data.economy.staff.salary_shares(rival.active()));
        let money = rival.money / shares;
        let rules = (rival.rules(data), rivals::staff_posts(rival.wealth));
        let moves = rivals::staff_moves(&rival.staff, money, rules, market);
        staff::make_moves(&mut rival.staff, market, &moves, money);
        let salaries: u32 = rival.staff.salaries().sum();
        rival.money -= i64::from(salaries) * shares;
    }
    staff::age_market(market);
}

/// The changes of staff `moves` made before the week, the first for each post, each
/// if still allowed at its turn (`GameState::may_staff`), then the salaries paid in
/// `ledger`, post by post, as many times as the club's fighters make it, even into
/// debt; returns the changes made.
pub(super) fn staff_week(
    state: &mut GameState,
    moves: &[StaffMove],
    data: &DesignData,
    ledger: &mut Vec<Transaction>,
) -> Vec<StaffNews> {
    let shares = i64::from(state.salary_shares(data));
    let made = staff::make_moves(
        &mut state.staff,
        &mut state.staff_market,
        moves,
        state.money / shares,
    );
    for salary in state.staff.salaries() {
        let amount = -i64::from(salary) * shares;
        state.money += amount;
        ledger.push(Transaction {
            fighter: None,
            flow: Flow::Salary,
            amount,
        });
    }
    made
}

/// Whether the week played starts the off-season.
pub(super) fn off_season_starts(state: &GameState, data: &DesignData) -> bool {
    let today = state.date(data);
    let calendar = &data.calendar;
    today.phase(calendar) == SeasonPhase::OffSeason
        && today.week == calendar.competition_weeks + calendar.great_event_weeks + 1
}
