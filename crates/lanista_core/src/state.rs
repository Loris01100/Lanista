//! The whole game: the player's club and the week to play. One week is one call
//! to [`next_week`]; the interface only builds [`Decisions`] and reads the state.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::calendar::Date;
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction};
use crate::fighter::career::{Activity, CareerFighter, WeekPlan, WeekReport, suggested_plan};
use crate::fighter::fame::FightResult;

#[derive(Debug, Clone, PartialEq)]
pub struct GameState {
    /// Number of the next week to play, from 1.
    pub week: u64,
    /// The club's money; below 0 when upkeep ran it into debt.
    pub money: i64,
    /// The player's club; a fighter's index is its place here.
    pub fighters: Vec<CareerFighter>,
    /// The only source of chance after the start: same seed and decisions, same game.
    pub rng: ChaCha8Rng,
}

impl GameState {
    /// A new game from `seed`: a club of one generated fighter, the same one
    /// `CareerFighter::random` draws first from that seed, and the starting money.
    // ponytail: one fighter until the fighter list screen and the market give a roster.
    #[must_use]
    pub fn new(seed: u64, data: &DesignData) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let fighters = vec![CareerFighter::random(&mut rng, data)];
        Self {
            week: 1,
            money: i64::from(data.economy.starting_money),
            fighters,
            rng,
        }
    }

    /// Season, week of the season and so phase of the week to play.
    #[must_use]
    pub fn date(&self, data: &DesignData) -> Date {
        Date::of(self.week, &data.calendar)
    }

    /// The decisions `suggested_plan` would take for every fighter this week.
    #[must_use]
    pub fn suggested_decisions(&self) -> Decisions {
        Decisions {
            plans: self
                .fighters
                .iter()
                .map(|f| suggested_plan(f, self.week))
                .collect(),
            hire_free_agent: false,
        }
    }
}

/// The player's decisions for one week.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decisions {
    /// One plan per fighter, in club order; a fighter without one follows `suggested_plan`.
    pub plans: Vec<WeekPlan>,
    /// Sign a free agent before the week, for nothing: with the free colosseum, the
    /// safety net that lets a club without money or fighters start again.
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
}

/// Plays one week for the whole club: signs a free agent if asked, then plays each
/// fighter's week in club order and pays for it, then moves to the next week.
///
/// A choice the club cannot pay for at its turn is dropped: no care, and rest instead
/// of training or of a fight with an entry fee. Upkeep is always paid, even into debt.
pub fn next_week(state: &mut GameState, decisions: &Decisions, data: &DesignData) -> WeekSummary {
    let week = state.week;
    if decisions.hire_free_agent {
        let agent = CareerFighter::random(&mut state.rng, data);
        state.fighters.push(agent);
    }
    let economy = &data.economy;
    let colosseum = &economy.colosseum;
    let mut ledger = Vec::new();
    let mut reports = Vec::with_capacity(state.fighters.len());
    for (i, fighter) in state.fighters.iter_mut().enumerate() {
        let wanted = decisions
            .plans
            .get(i)
            .copied()
            .unwrap_or_else(|| suggested_plan(fighter, week));
        let plan = affordable(wanted, fighter, state.money, data);
        let fame = fighter.fame;
        let report = fighter.play_week(plan, &mut state.rng, data);
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
            if let Some(fight) = &report.fight {
                pay(Flow::EntryFee, -i64::from(colosseum.entry_fee));
                pay(
                    Flow::Receipts,
                    i64::from(colosseum.receipts(fame, fight.opponent.fame)),
                );
                if fight.result == FightResult::Won {
                    pay(Flow::WinPrize, i64::from(colosseum.win_prize));
                }
            }
            pay(Flow::Upkeep, -i64::from(economy.upkeep));
        }
        reports.push(report);
    }
    state.week += 1;
    WeekSummary { reports, ledger }
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
        Activity::Rest => 0,
        Activity::Train => data.economy.training_cost,
        Activity::Fight => data.economy.colosseum.entry_fee,
    };
    if i64::from(cost) > left {
        plan.activity = Activity::Rest;
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fighter::career::Status;
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
        let decisions = suggested.suggested_decisions();
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
        let colosseum = &data.economy.colosseum;
        // Even a broke club can fight: the colosseum is free.
        let mut state = GameState::new(7, &data);
        state.money = 0;
        let mut won = false;
        for _ in 0..20 {
            let summary = next_week(&mut state, &plan(Activity::Fight, false), &data);
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
    fn money_moves_only_through_the_ledger() {
        let data = data();
        let mut state = GameState::new(3, &data);
        for _ in 0..60 {
            let before = state.money;
            let decisions = state.suggested_decisions();
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
                let decisions = state.suggested_decisions();
                next_week(&mut state, &decisions, &data);
            }
            state
        };
        assert_eq!(game(), game());
    }
}
