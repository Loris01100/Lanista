//! The week loop: the player's [`Decisions`], [`next_week`] playing them and the
//! [`WeekSummary`] it returns.

use crate::colosseum::{Contender, TitleNews, title_kept};
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction, Wealth};
use crate::fighter::career::{Activity, Status, WeekPlan, WeekReport};
use crate::fighter::fame::{Fame, FightResult};
use crate::fighter::percent_of;
use crate::market::{MarketNews, Move};
use crate::relations::Reason;
use crate::staff::{StaffMove, StaffNews};
use serde::{Deserialize, Serialize};

use super::clubs::{club_lives, off_season_starts, rival_staff, staff_week, wealth_changes};
use super::colosseums::{end_colosseum_weeks, title_fight};
use super::melee::melees;
use super::outsiders::{met, opponent, outsider_weeks, pay_rival};
use super::transfers::{answer_offer, market_moves, rival_market, trim_market};
use super::{GameState, News, WorldNews, affordable, entry_fee, relate, week_plans};

/// The player's decisions for one week.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decisions {
    /// One plan per fighter, in club order; a fighter without one follows
    /// `GameState::suggested_plan`.
    pub plans: Vec<WeekPlan>,
    /// Market moves, each made if `GameState::may_move` allows it at its turn: the
    /// first observation, then accepted offers, releases, the transfer list and
    /// signings before the week, and the first offer for a rival fighter after it.
    /// Signing a free agent costs nothing: with the free colosseum, the safety net
    /// that lets a club without money or fighters start again.
    pub moves: Vec<Move>,
    /// Changes of staff, each made before the week if `GameState::may_staff` allows it
    /// at its turn; only the first for each post counts.
    pub staff: Vec<StaffMove>,
}

/// What a week did to the club.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeekSummary {
    /// Each fighter's report, in club order; `None` for those not in activity.
    pub reports: Vec<Option<WeekReport>>,
    /// Every move of money, in the order it happened.
    pub ledger: Vec<Transaction>,
    /// What happened to the titles, with the colosseum's index, in the order it happened.
    pub titles: Vec<(usize, TitleNews)>,
    /// What the club did and heard on the market, in the order it happened.
    pub market: Vec<MarketNews>,
    /// The changes of staff made.
    pub staff: Vec<StaffNews>,
    /// The club's new wealth, when it changed at the start of the off-season.
    pub wealth: Option<Wealth>,
}

/// Plays one week for the whole club: the market moves and the change of staff before
/// the week, the staff's salaries, then each fighter's week in
/// club order, paid for, then the week of the rival clubs and the free agents, the
/// answer to the player's offer, everyone a year older at the start of the off-season,
/// each club's new wealth at the start of the off-season
/// and the rival clubs' market, then the colosseums' titles, and moves to the next
/// week. A fight is against an outsider, a rival fighter or a free agent, and
/// moves both Elo; a title fight is the only way a title changes hands, and the
/// club's champion earns the champion bonus for each defense it survives. A challenge
/// is a fight against a chosen rival fighter whose club accepts it.
///
/// A choice the club cannot pay for at its turn is dropped: no care, and rest instead
/// of training or of a fight with an entry fee. A title fight the fighter may not have
/// (`GameState::title_opponent`) is rest too, and so is a challenge it may not have
/// (`GameState::may_challenge`). Upkeep and salaries are always paid, even into debt.
pub fn next_week(state: &mut GameState, decisions: &Decisions, data: &DesignData) -> WeekSummary {
    let (week, money, before) = (state.week, state.money, state.fighters.clone());
    let mut ledger = Vec::new();
    let mut market = market_moves(state, &decisions.moves, data, &mut ledger);
    let staff = staff_week(state, &decisions.staff, data, &mut ledger);
    let effects = state.staff_effects();
    // A training session's price, and the doctor's for care.
    let costs = (state.training_cost(data), effects.medicine);
    let (mut wanted, title_meetings) = week_plans(state, decisions, data);
    rival_staff(state, data);
    let mut plans = state.outsider_plans(data);
    // The outsiders who had their fight, against the club's fighters or in a melee.
    let mut fought = Vec::new();
    let reserved = reserved(&wanted, &title_meetings);
    let mut melee_weeks = melees(
        state,
        (&mut wanted, &mut plans),
        costs,
        (&mut fought, &reserved, &mut ledger),
        data,
    );
    let mut titles = Vec::new();
    let mut reports = Vec::with_capacity(state.fighters.len());
    for i in 0..state.fighters.len() {
        // A melee was the whole week, paid for but the upkeep.
        let report = if let Some(report) = melee_weeks.get_mut(i).and_then(Option::take) {
            Some(report)
        } else {
            let fighter = &mut state.fighters[i];
            let plan = affordable(
                wanted[i],
                fighter,
                (state.money, state.continent),
                costs,
                data,
            );
            let fame = fighter.fame;
            // The colosseum and the outsider met there.
            let meeting = match plan.activity {
                Activity::TitleFight(_) => title_meetings[i],
                activity => opponent(
                    &mut state.rng,
                    (&state.colosseums, &state.rivals, &state.free_agents),
                    activity,
                    &plans,
                    &fought,
                ),
            };
            fought.extend(meeting.map(|(_, them)| them));
            let opponent =
                meeting.and_then(|(_, them)| met(&mut state.rivals, &mut state.free_agents, them));
            let mut report = fighter.play_week(plan, opponent, effects, &mut state.rng, data);
            if let Some(report) = &mut report {
                let paid = (i, fame, costs.0);
                settle_week(
                    state,
                    paid,
                    (report, meeting),
                    (&mut ledger, &mut titles),
                    data,
                );
            }
            report
        };
        if report.is_some() {
            let upkeep = -i64::from(data.economy.upkeep);
            state.money += upkeep;
            ledger.push(Transaction {
                fighter: Some(i),
                flow: Flow::Upkeep,
                amount: upkeep,
            });
        }
        reports.push(report);
    }
    outsider_weeks(state, plans, &fought, data);
    market.extend(answer_offer(state, &decisions.moves, data, &mut ledger));
    birthdays(state, &mut reports, data);
    let wealth = end_week(state, &mut titles, data);
    let summary = WeekSummary {
        reports,
        ledger,
        titles,
        market,
        staff,
        wealth,
    };
    let played = (before.as_slice(), state.fighters.as_slice());
    let money = (money, state.money);
    state.journal.record(week, played, money, &summary, data);
    summary
}

/// What the week `report` of the club's fighter `i`, of `fame` before it, costs and
/// brings, upkeep apart: care, a training session at `training`, and its fight against
/// the outsider of `meeting` in that colosseum: a title fight (`titles`, the champion
/// bonus for a defense survived), or else a fight moving both Elo for the entry fee;
/// receipts, and the prize to the winner. The rival fighter's club pays and earns the
/// same; a free agent fights for its name alone. Against a club that declared itself
/// the player's rival, the receipts grow and both fighters gain fame (added to
/// `report`). The club's money moves into `ledger`.
fn settle_week(
    state: &mut GameState,
    (i, fame, training): (usize, Fame, u32),
    (report, meeting): (&mut WeekReport, Option<(usize, Contender)>),
    (ledger, titles): (&mut Vec<Transaction>, &mut Vec<(usize, TitleNews)>),
    data: &DesignData,
) {
    let mut pay = |flow, amount: i64| {
        if amount != 0 {
            state.money += amount;
            ledger.push(Transaction {
                fighter: Some(i),
                flow,
                amount,
            });
        }
    };
    pay(Flow::Care, -i64::from(report.care_cost));
    if report.plan.activity == Activity::Train {
        pay(Flow::Training, -i64::from(training));
    }
    let (Some(fight), Some((c, rival))) = (&mut report.fight, meeting) else {
        return;
    };
    let Some(rules) = data.world.colosseums.get(c) else {
        return;
    };
    let purses = &rules.purses;
    let club = Contender::Club(i);
    let colosseum = &mut state.colosseums[c];
    let mut fee = 0;
    let mut taken = false;
    if let Activity::TitleFight(_) = report.plan.activity {
        let news = title_fight(colosseum, club, rival, fight.result, state.week, data);
        titles.push((c, news));
        taken = matches!(news, TitleNews::Fight { champion, result, .. }
            if champion == rival && !title_kept(result));
        if let TitleNews::Fight {
            champion, result, ..
        } = news
            && champion == club
            && title_kept(result)
        {
            pay(Flow::ChampionBonus, i64::from(purses.champion_bonus));
        }
    } else {
        fee = i64::from(entry_fee(c, state.continent, data));
        pay(Flow::EntryFee, -fee);
        colosseum.record(club, rival, fight.result, &data.world);
    }
    let theirs = fight.opponent().map_or(Fame::default(), |o| o.fame);
    let mut receipts = purses.receipts(fame, theirs);
    let rivalry = &data.world.relations.rivalry;
    // Not `is_rival`: `pay` holds the club's money.
    if let Contender::Rival(id) = rival
        && state
            .relations
            .get(&id.club)
            .is_some_and(|r| r.rival_since.is_some())
    {
        receipts = percent_of(receipts, rivalry.receipts_percent);
        let bonus = i32::try_from(rivalry.fame).unwrap_or(i32::MAX);
        fight.fame_change = fight.fame_change.saturating_add(bonus);
        let mine = &mut state.fighters[i].fame;
        *mine = mine.changed_by(bonus);
        if let Some(f) = state.rivals[id.club].fighters.get_mut(id.fighter) {
            f.fame = f.fame.changed_by(bonus);
        }
    }
    let receipts = i64::from(receipts);
    pay(Flow::Receipts, receipts);
    let prize = i64::from(purses.win_prize);
    let mut theirs = receipts - fee;
    match fight.result {
        FightResult::Won => pay(Flow::WinPrize, prize),
        FightResult::Lost | FightResult::GaveUp => theirs += prize,
        FightResult::Unfinished => {}
    }
    if let Contender::Rival(id) = rival {
        pay_rival(&mut state.rivals, id.club, theirs);
        if taken {
            let why = Reason::TitleTaken {
                fighter: i,
                colosseum: c,
            };
            relate(state, id.club, why, data);
        }
        let killed = state
            .rival_fighter(id)
            .is_some_and(|f| f.status == Status::Dead);
        let why = if killed {
            Some(Reason::Killed {
                fighter: i,
                victim: id,
            })
        } else {
            (fight.result == FightResult::Won).then_some(Reason::Beaten {
                fighter: i,
                rival: id,
            })
        };
        if let Some(why) = why {
            relate(state, id.club, why, data);
        }
    }
}

/// At the start of the off-season, every fighter in activity, the club's, the rival
/// clubs' and the free agents, is a year older (`CareerFighter::birthday`), whenever
/// it arrived: the club's fighters' `reports` tell it.
fn birthdays(state: &mut GameState, reports: &mut [Option<WeekReport>], data: &DesignData) {
    if !off_season_starts(state, data) {
        return;
    }
    let rng = &mut state.rng;
    for (i, fighter) in state.fighters.iter_mut().enumerate() {
        if fighter.status == Status::Active {
            let gains = fighter.birthday(rng, data);
            if let Some(Some(report)) = reports.get_mut(i) {
                report.birthday = true;
                report.season_gains = gains;
            }
        }
    }
    let others = state.rivals.iter_mut().flat_map(|r| &mut r.fighters);
    for fighter in others.chain(&mut state.free_agents) {
        if fighter.status == Status::Active {
            fighter.birthday(rng, data);
        }
    }
}

/// The rival fighters the club's fighters meet this week in a title fight or a
/// challenge, by their `wanted` plans and `title_meetings`: they keep their week for it.
fn reserved(wanted: &[WeekPlan], title_meetings: &[Option<(usize, Contender)>]) -> Vec<Contender> {
    wanted
        .iter()
        .zip(title_meetings)
        .filter_map(|(plan, meeting)| match plan.activity {
            Activity::TitleFight(_) => meeting.map(|(_, them)| them),
            Activity::Challenge(_, rival) => Some(Contender::Rival(rival)),
            _ => None,
        })
        .collect()
}

/// The world's end of the week, once everyone fought: each club's new wealth at the
/// start of the off-season, the rival clubs' market, the colosseums' titles, told in
/// `titles`, the market topped up and trimmed, and the next week opens. Returns the
/// club's new wealth if it changed.
fn end_week(
    state: &mut GameState,
    titles: &mut Vec<(usize, TitleNews)>,
    data: &DesignData,
) -> Option<Wealth> {
    let week = state.week;
    for (&club, relation) in &mut state.relations {
        relation.fade(&data.world.relations);
        if let Some(declared) = relation.declare(week, &data.world.relations) {
            let what = WorldNews::Rivalry(club, declared);
            state.news.push(News { week, what });
        }
    }
    let wealth = wealth_changes(state, data);
    club_lives(state, data);
    rival_market(state, data);
    titles.extend(end_colosseum_weeks(state, state.week, data));
    let changes = titles.iter().filter(|&&(_, news)| match news {
        TitleNews::Fight { result, .. } => !title_kept(result),
        TitleNews::Stripped { .. } | TitleNews::Vacated { .. } => true,
    });
    let week = state.week;
    state.news.extend(changes.map(|&(c, news)| News {
        week,
        what: WorldNews::Title(c, news),
    }));
    let kept = u64::from(data.world.news_weeks);
    state.news.retain(|n| n.week + kept > week);
    state.open_world(data);
    trim_market(state, data);
    state.week += 1;
    wealth
}
