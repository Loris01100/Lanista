//! The whole game: the player's club, the colosseums and the week to play. One week is one call
//! to [`next_week`]; the interface only builds [`Decisions`] and reads the state.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::calendar::Date;
use crate::colosseum::{Colosseum, ColosseumId, Contender, TitleNews, pick_opponent, title_kept};
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction};
use crate::fighter::career::{
    Activity, CareerFighter, Status, WeekPlan, WeekReport, suggested_plan,
};
use crate::fighter::fame::FightResult;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    /// Number of the next week to play, from 1.
    pub week: u64,
    /// The club's money; below 0 when upkeep ran it into debt.
    pub money: i64,
    /// The player's club; a fighter's index is its place here.
    pub fighters: Vec<CareerFighter>,
    /// In the order of `data/world.ron`.
    pub colosseums: Vec<Colosseum>,
    /// The only source of chance after the start: same seed and decisions, same game.
    pub rng: ChaCha8Rng,
}

impl GameState {
    /// A new game from `seed`: a club of one generated fighter, the same one
    /// `CareerFighter::random` draws first from that seed, the starting money, and
    /// the colosseums with their residents.
    // ponytail: one fighter until the fighter list screen and the market give a roster.
    #[must_use]
    pub fn new(seed: u64, data: &DesignData) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let fighters = vec![CareerFighter::random(&mut rng, data)];
        let mut state = Self {
            week: 1,
            money: i64::from(data.economy.starting_money),
            fighters,
            colosseums: Vec::new(),
            rng,
        };
        state.open_colosseums(data);
        state
    }

    /// Opens the colosseums of `data` the game does not have yet: all of them in a
    /// new game, those added to `data/world.ron` since an older save.
    pub fn open_colosseums(&mut self, data: &DesignData) {
        for rules in data.world.colosseums.iter().skip(self.colosseums.len()) {
            let colosseum = Colosseum::open(&mut self.rng, rules, data, self.week);
            self.colosseums.push(colosseum);
        }
    }

    /// Season, week of the season and so phase of the week to play.
    #[must_use]
    pub fn date(&self, data: &DesignData) -> Date {
        Date::of(self.week, &data.calendar)
    }

    /// Fighters neither retired nor dead.
    #[must_use]
    pub fn active_fighters(&self) -> usize {
        self.fighters
            .iter()
            .filter(|f| f.status == Status::Active)
            .count()
    }

    /// Whether the club has a place for one more fighter in activity.
    #[must_use]
    pub fn can_hire(&self, data: &DesignData) -> bool {
        u32::try_from(self.active_fighters()).is_ok_and(|n| n < data.economy.max_fighters)
    }

    /// The suggested plans of every fighter this week.
    #[must_use]
    pub fn suggested_decisions(&self, data: &DesignData) -> Decisions {
        Decisions {
            plans: (0..self.fighters.len())
                .map(|i| self.suggested_plan(i, data))
                .collect(),
            hire_free_agent: false,
        }
    }

    /// `career::suggested_plan` for the club's fighter `i`, who defends a title instead
    /// of fighting or training in the last two weeks before its deadline.
    #[must_use]
    pub fn suggested_plan(&self, i: usize, data: &DesignData) -> WeekPlan {
        let Some(fighter) = self.fighters.get(i) else {
            return WeekPlan {
                activity: Activity::Rest,
                care: false,
            };
        };
        let mut plan = suggested_plan(fighter, self.week);
        let defense = self.colosseums.iter().position(|c| {
            c.champion == Some(Contender::Club(i))
                && self.week + 1 >= c.defense_deadline(&data.world)
        });
        if plan.activity != Activity::Rest
            && let Some(c) = defense.and_then(|c| u32::try_from(c).ok())
        {
            plan.activity = Activity::TitleFight(ColosseumId(c));
        }
        plan
    }

    /// The colosseum and the resident the club's fighter `i` meets if it fights for
    /// the title of colosseum `id`; `None` if it may not (see `Colosseum::title_opponent`).
    #[must_use]
    pub fn title_opponent(
        &self,
        i: usize,
        id: ColosseumId,
        data: &DesignData,
    ) -> Option<(usize, usize)> {
        let c = usize::try_from(id.0).ok()?;
        let resident = self
            .colosseums
            .get(c)?
            .title_opponent(i, &self.fighters, &data.world)?;
        Some((c, resident))
    }
}

/// The player's decisions for one week.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decisions {
    /// One plan per fighter, in club order; a fighter without one follows
    /// `GameState::suggested_plan`.
    pub plans: Vec<WeekPlan>,
    /// Sign a free agent before the week, for nothing: with the free colosseum, the
    /// safety net that lets a club without money or fighters start again. Ignored
    /// when the club is full (`GameState::can_hire`).
    // ponytail: a generated fighter, no choice; the market of phase 5 lists them.
    pub hire_free_agent: bool,
}

/// What a week did to the club.
#[derive(Debug, Clone, PartialEq)]
pub struct WeekSummary {
    /// Each fighter's report, in club order; `None` for those retired or dead.
    pub reports: Vec<Option<WeekReport>>,
    /// Every move of money, in the order it happened.
    pub ledger: Vec<Transaction>,
    /// What happened to the titles, with the colosseum's index, in the order it happened.
    pub titles: Vec<(usize, TitleNews)>,
}

/// Plays one week for the whole club: signs a free agent if asked and there is room, then plays each
/// fighter's week in club order and pays for it, then ends the colosseums' week and moves to
/// the next week. A fight is against one of the colosseum's residents and moves both Elo; a
/// title fight is the only way a title changes hands, and the club's champion earns the
/// champion bonus for each defense it survives.
///
/// A choice the club cannot pay for at its turn is dropped: no care, and rest instead
/// of training or of a fight with an entry fee. A title fight the fighter may not have
/// (`GameState::title_opponent`) is rest too. Upkeep is always paid, even into debt.
pub fn next_week(state: &mut GameState, decisions: &Decisions, data: &DesignData) -> WeekSummary {
    let week = state.week;
    if decisions.hire_free_agent && state.can_hire(data) {
        let agent = CareerFighter::random(&mut state.rng, data);
        state.fighters.push(agent);
    }
    let (wanted, title_meetings) = week_plans(state, decisions, data);
    let economy = &data.economy;
    let mut ledger = Vec::new();
    let mut titles = Vec::new();
    let mut reports = Vec::with_capacity(state.fighters.len());
    for (i, fighter) in state.fighters.iter_mut().enumerate() {
        let plan = affordable(wanted[i], fighter, state.money, data);
        let fame = fighter.fame;
        // The colosseum and the resident met there.
        let meeting = match plan.activity {
            Activity::Fight(id) => usize::try_from(id.0)
                .ok()
                .and_then(|c| Some((c, pick_opponent(&mut state.rng, state.colosseums.get(c)?)?))),
            Activity::TitleFight(_) => title_meetings[i],
            Activity::Rest | Activity::Train => None,
        };
        let opponent = meeting.and_then(|(c, r)| state.colosseums.get_mut(c)?.residents.get_mut(r));
        let report = fighter.play_week(plan, opponent, &mut state.rng, data);
        if let Some(report) = &report {
            let mut pay = |flow, amount: i64| {
                if amount != 0 {
                    state.money += amount;
                    ledger.push(Transaction {
                        fighter: i,
                        flow,
                        amount,
                    });
                }
            };
            pay(Flow::Care, -i64::from(report.care_cost));
            if report.plan.activity == Activity::Train {
                pay(Flow::Training, -i64::from(economy.training_cost));
            }
            if let (Some(fight), Some((c, r))) = (&report.fight, meeting)
                && let Some(rules) = data.world.colosseums.get(c)
            {
                let purses = &rules.purses;
                let (club, resident) = (Contender::Club(i), Contender::Resident(r));
                let colosseum = &mut state.colosseums[c];
                if let Activity::TitleFight(_) = plan.activity {
                    let (champion, challenger, result) = if colosseum.champion == Some(club) {
                        (club, resident, fight.result)
                    } else {
                        (resident, club, opposite(fight.result))
                    };
                    colosseum.record_title_fight(champion, challenger, result, week, &data.world);
                    titles.push((
                        c,
                        TitleNews::Fight {
                            champion,
                            challenger,
                            result,
                        },
                    ));
                    if champion == club && title_kept(result) {
                        pay(Flow::ChampionBonus, i64::from(purses.champion_bonus));
                    }
                } else {
                    pay(Flow::EntryFee, -i64::from(purses.entry_fee));
                    colosseum.record(club, resident, fight.result, &data.world);
                }
                pay(
                    Flow::Receipts,
                    i64::from(purses.receipts(fame, fight.opponent.fame)),
                );
                if fight.result == FightResult::Won {
                    pay(Flow::WinPrize, i64::from(purses.win_prize));
                }
            }
            pay(Flow::Upkeep, -i64::from(economy.upkeep));
        }
        reports.push(report);
    }
    let fighters = &state.fighters;
    for (c, (colosseum, rules)) in state
        .colosseums
        .iter_mut()
        .zip(&data.world.colosseums)
        .enumerate()
    {
        let news = colosseum.end_week(week, &mut state.rng, rules, data, fighters);
        titles.extend(news.into_iter().map(|n| (c, n)));
    }
    state.week += 1;
    WeekSummary {
        reports,
        ledger,
        titles,
    }
}

/// Each fighter's wanted plan, `GameState::suggested_plan` without one, and the colosseum
/// and resident of its title fight. Title fights are settled before anyone plays, from the
/// rankings as they stand; one the fighter may not have turns into rest.
fn week_plans(
    state: &GameState,
    decisions: &Decisions,
    data: &DesignData,
) -> (Vec<WeekPlan>, Vec<Option<(usize, usize)>>) {
    (0..state.fighters.len())
        .map(|i| {
            let mut plan = decisions
                .plans
                .get(i)
                .copied()
                .unwrap_or_else(|| state.suggested_plan(i, data));
            let meeting = match plan.activity {
                Activity::TitleFight(id) => state.title_opponent(i, id, data),
                _ => None,
            };
            if matches!(plan.activity, Activity::TitleFight(_)) && meeting.is_none() {
                plan.activity = Activity::Rest;
            }
            (plan, meeting)
        })
        .unzip()
}

/// The other side's result in a duel.
fn opposite(result: FightResult) -> FightResult {
    match result {
        FightResult::Won => FightResult::Lost,
        FightResult::Lost | FightResult::GaveUp => FightResult::Won,
        FightResult::Unfinished => FightResult::Unfinished,
    }
}

/// `plan` without what `money` cannot pay for: care first, then the activity.
fn affordable(
    mut plan: WeekPlan,
    fighter: &CareerFighter,
    money: i64,
    data: &DesignData,
) -> WeekPlan {
    let mut left = money;
    if plan.care {
        let cost = i64::from(fighter.care_cost(&data.fighter));
        if cost > left {
            plan.care = false;
        } else {
            left -= cost;
        }
    }
    let cost = match plan.activity {
        Activity::Rest => Some(0),
        Activity::Train => Some(data.economy.training_cost),
        Activity::Fight(id) => data.world.colosseum(id).map(|c| c.purses.entry_fee),
        // The title is fought for free.
        Activity::TitleFight(id) => data.world.colosseum(id).map(|_| 0),
    };
    // A colosseum missing from the data cannot be entered.
    if cost.is_none_or(|cost| i64::from(cost) > left) {
        plan.activity = Activity::Rest;
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colosseum::ColosseumId;
    use crate::fighter::injury::{Injury, Location, Severity};

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    fn plan(activity: Activity, care: bool) -> Decisions {
        Decisions {
            plans: vec![WeekPlan { activity, care }],
            hire_free_agent: false,
        }
    }

    fn flows(summary: &WeekSummary) -> Vec<(Flow, i64)> {
        summary.ledger.iter().map(|t| (t.flow, t.amount)).collect()
    }

    #[test]
    fn a_new_game_starts_at_week_one_with_the_seed_s_fighter() {
        let data = data();
        let state = GameState::new(7, &data);
        assert_eq!(state.week, 1);
        assert_eq!(state.date(&data), Date { season: 1, week: 1 });
        assert_eq!(state.money, i64::from(data.economy.starting_money));
        let fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(7), &data);
        assert_eq!(state.fighters, [fighter]);
    }

    #[test]
    fn a_week_follows_the_decisions_and_moves_on() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let decisions = plan(Activity::Rest, false);
        let summary = next_week(&mut state, &decisions, &data);
        assert_eq!(summary.reports.len(), 1);
        assert_eq!(
            summary.reports[0].as_ref().unwrap().plan,
            decisions.plans[0]
        );
        assert_eq!(state.week, 2);
    }

    #[test]
    fn missing_plans_follow_the_suggestion() {
        let data = data();
        let mut by_default = GameState::new(7, &data);
        let mut suggested = by_default.clone();
        let decisions = suggested.suggested_decisions(&data);
        next_week(&mut by_default, &Decisions::default(), &data);
        next_week(&mut suggested, &decisions, &data);
        assert_eq!(by_default, suggested);
    }

    #[test]
    fn the_week_goes_on_without_active_fighters() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.fighters[0].status = Status::Dead;
        let before = state.clone();
        let summary = next_week(&mut state, &Decisions::default(), &data);
        assert_eq!(summary.reports, [None]);
        assert_eq!(summary.ledger, []);
        assert_eq!((state.week, state.fighters), (2, before.fighters));
        assert_eq!(state.money, before.money);
    }

    #[test]
    fn upkeep_and_training_are_paid() {
        let data = data();
        let economy = &data.economy;
        let mut state = GameState::new(7, &data);
        let summary = next_week(&mut state, &plan(Activity::Train, false), &data);
        let cost = i64::from(economy.training_cost);
        let upkeep = i64::from(economy.upkeep);
        assert_eq!(
            flows(&summary),
            [(Flow::Training, -cost), (Flow::Upkeep, -upkeep)]
        );
        assert_eq!(
            state.money,
            i64::from(economy.starting_money) - cost - upkeep
        );
    }

    #[test]
    fn a_broke_club_rests_without_care_and_runs_into_debt() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.money = 0;
        state.fighters[0].injuries.push(Injury {
            location: Location::Body,
            severity: Severity::Medium,
            weeks_left: 4,
        });
        let summary = next_week(&mut state, &plan(Activity::Train, true), &data);
        let report = summary.reports[0].as_ref().unwrap();
        assert_eq!(report.plan, plan(Activity::Rest, false).plans[0]);
        assert_eq!(state.money, -i64::from(data.economy.upkeep));
    }

    #[test]
    fn care_is_paid_when_the_club_can() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.fighters[0].injuries.push(Injury {
            location: Location::Body,
            severity: Severity::Medium,
            weeks_left: 4,
        });
        let price = state.fighters[0].care_cost(&data.fighter);
        assert!(price > 0);
        let summary = next_week(&mut state, &plan(Activity::Rest, true), &data);
        assert_eq!(flows(&summary)[0], (Flow::Care, -i64::from(price)));
    }

    #[test]
    fn a_fight_brings_receipts_and_the_winner_s_prize() {
        let data = data();
        let colosseum = &data.world.colosseums[0].purses;
        // Even a broke club can fight: the first colosseum is free.
        let mut state = GameState::new(7, &data);
        state.money = 0;
        let mut won = false;
        for _ in 0..20 {
            let summary = next_week(
                &mut state,
                &plan(Activity::Fight(ColosseumId(0)), false),
                &data,
            );
            let Some(fight) = summary.reports[0].as_ref().and_then(|r| r.fight.as_ref()) else {
                break;
            };
            let flows = flows(&summary);
            assert!(flows.iter().any(|&(flow, amount)| flow == Flow::Receipts
                && amount >= i64::from(colosseum.receipts_base)));
            let prize = (Flow::WinPrize, i64::from(colosseum.win_prize));
            assert_eq!(flows.contains(&prize), fight.result == FightResult::Won);
            won |= fight.result == FightResult::Won;
        }
        assert!(won, "seed 7 wins a fight in 20 weeks");
    }

    #[test]
    fn a_fight_meets_a_resident_and_moves_both_elo() {
        let data = data();
        let rules = &data.world;
        let mut state = GameState::new(7, &data);
        assert_eq!(state.colosseums.len(), 2);
        let rome = ColosseumId(1);
        let summary = next_week(&mut state, &plan(Activity::Fight(rome), false), &data);
        let fight = summary.reports[0].as_ref().unwrap().fight.as_ref().unwrap();
        let fee = i64::from(rules.colosseums[1].purses.entry_fee);
        assert_eq!(flows(&summary)[0], (Flow::EntryFee, -fee));
        let rome = &state.colosseums[1];
        let club = rome.elo(Contender::Club(0), rules);
        assert_eq!(rome.ranking.len(), 16);
        let resident = rome
            .ranking
            .iter()
            .find(|r| r.elo != rules.elo_start && r.contender != Contender::Club(0))
            .unwrap();
        // Zero-sum: what one wins, the other loses.
        assert_eq!(club - rules.elo_start, rules.elo_start - resident.elo);
        assert_eq!(club > rules.elo_start, fight.result == FightResult::Won);
        assert_eq!(state.colosseums[0].ranking.len(), 15);
    }

    /// A game of seed 7 whose fighter leads Rome's ranking.
    fn top_of_rome(data: &DesignData) -> GameState {
        let mut state = GameState::new(7, data);
        state.colosseums[1].ranking.insert(
            0,
            crate::colosseum::Rating {
                contender: Contender::Club(0),
                elo: 1100,
            },
        );
        state
    }

    #[test]
    fn the_top_of_the_ranking_fights_for_the_title() {
        let data = data();
        let mut state = top_of_rome(&data);
        let champion = state.colosseums[1].champion.unwrap();
        let title = plan(Activity::TitleFight(ColosseumId(1)), false);
        let summary = next_week(&mut state, &title, &data);
        let report = summary.reports[0].as_ref().unwrap();
        let fight = report.fight.as_ref().unwrap();
        // Free to enter, and as tiring as a defense.
        assert!(
            flows(&summary)
                .iter()
                .all(|&(flow, _)| flow != Flow::EntryFee)
        );
        if state.fighters[0].status == Status::Active {
            assert_eq!(state.fighters[0].form.value(), 70);
        }
        let (c, news) = summary.titles[0];
        assert_eq!(c, 1);
        let TitleNews::Fight {
            champion: holder,
            challenger,
            result,
        } = news
        else {
            panic!("a title fight, not {news:?}");
        };
        assert_eq!((holder, challenger), (champion, Contender::Club(0)));
        assert_eq!(result, opposite(fight.result));
        let won = fight.result == FightResult::Won;
        assert_eq!(
            state.colosseums[1].champion == Some(Contender::Club(0)),
            won
        );
        assert_eq!(state.colosseums[1].title_since, 1);
    }

    #[test]
    fn the_club_s_champion_defends_and_earns_the_bonus() {
        let data = data();
        let mut state = top_of_rome(&data);
        state.colosseums[1].champion = Some(Contender::Club(0));
        let title = plan(Activity::TitleFight(ColosseumId(1)), false);
        let summary = next_week(&mut state, &title, &data);
        let Some(&(
            1,
            TitleNews::Fight {
                champion, result, ..
            },
        )) = summary.titles.first()
        else {
            panic!("a title fight, not {:?}", summary.titles);
        };
        assert_eq!(champion, Contender::Club(0));
        let bonus = (
            Flow::ChampionBonus,
            i64::from(data.world.colosseums[1].purses.champion_bonus),
        );
        assert_eq!(flows(&summary).contains(&bonus), title_kept(result));
        // No title fight for whoever may not have one: a week of rest.
        let summary = next_week(
            &mut state,
            &plan(Activity::TitleFight(ColosseumId(0)), false),
            &data,
        );
        assert_eq!(
            summary.reports[0].as_ref().unwrap().plan.activity,
            Activity::Rest
        );
    }

    #[test]
    fn the_suggestion_defends_a_title_before_its_deadline() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.colosseums[1].champion = Some(Contender::Club(0));
        let deadline = state.colosseums[1].defense_deadline(&data.world);
        assert_eq!(
            state.suggested_plan(0, &data).activity,
            Activity::Fight(ColosseumId(0))
        );
        state.week = deadline - 1;
        state.fighters[0].form = crate::fighter::form::Form::full();
        assert_eq!(
            state.suggested_plan(0, &data).activity,
            Activity::TitleFight(ColosseumId(1))
        );
        // Too tired to fight: rest, and the title will be lost.
        state.fighters[0].form = crate::fighter::form::Form::new(10);
        assert_eq!(state.suggested_plan(0, &data).activity, Activity::Rest);
    }

    #[test]
    fn resident_champions_defend_without_the_player() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let rest = plan(Activity::Rest, false);
        let mut fights = 0;
        for _ in 0..12 {
            let summary = next_week(&mut state, &rest, &data);
            fights += summary
                .titles
                .iter()
                .filter(|(_, n)| matches!(n, TitleNews::Fight { .. }))
                .count();
        }
        // Two colosseums, a defense every four weeks: weeks 5 and 9.
        assert_eq!(fights, 4);
    }

    #[test]
    fn a_club_that_cannot_pay_rome_rests() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.money = 0;
        let summary = next_week(
            &mut state,
            &plan(Activity::Fight(ColosseumId(1)), false),
            &data,
        );
        assert_eq!(
            summary.reports[0].as_ref().unwrap().plan.activity,
            Activity::Rest
        );
        // Nor can it fight in a colosseum missing from the data.
        let summary = next_week(
            &mut state,
            &plan(Activity::Fight(ColosseumId(9)), false),
            &data,
        );
        assert_eq!(
            summary.reports[0].as_ref().unwrap().plan.activity,
            Activity::Rest
        );
    }

    #[test]
    fn a_free_agent_joins_for_nothing_and_plays_the_week() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.fighters[0].status = Status::Dead;
        state.money = -100;
        let decisions = Decisions {
            plans: Vec::new(),
            hire_free_agent: true,
        };
        let summary = next_week(&mut state, &decisions, &data);
        assert_eq!(state.fighters.len(), 2);
        assert!(summary.reports[1].is_some());
        assert_eq!(state.fighters[1].status, Status::Active);
    }

    #[test]
    fn a_full_club_signs_no_one() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let hire = Decisions {
            plans: Vec::new(),
            hire_free_agent: true,
        };
        let max = usize::try_from(data.economy.max_fighters).unwrap();
        for _ in 0..max + 2 {
            next_week(&mut state, &hire, &data);
        }
        assert_eq!(state.active_fighters(), max);
        assert!(!state.can_hire(&data));
        // A fighter gone leaves a place.
        state.fighters[0].status = Status::Retired;
        assert!(state.can_hire(&data));
        next_week(&mut state, &hire, &data);
        assert_eq!(state.active_fighters(), max);
    }

    #[test]
    fn money_moves_only_through_the_ledger() {
        let data = data();
        let mut state = GameState::new(3, &data);
        for _ in 0..60 {
            let before = state.money;
            let decisions = state.suggested_decisions(&data);
            let summary = next_week(&mut state, &decisions, &data);
            let total: i64 = summary.ledger.iter().map(|t| t.amount).sum();
            assert_eq!(state.money, before + total);
        }
    }

    #[test]
    fn same_seed_same_decisions_same_game() {
        let game = || {
            let data = data();
            let mut state = GameState::new(9, &data);
            for _ in 0..20 {
                let decisions = state.suggested_decisions(&data);
                next_week(&mut state, &decisions, &data);
            }
            state
        };
        assert_eq!(game(), game());
    }
}
