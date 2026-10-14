//! The whole game: the player's club and the week to play. One week is one call
//! to [`next_week`]; the interface only builds [`Decisions`] and reads the state.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::calendar::Date;
use crate::design_data::DesignData;
use crate::fighter::career::{CareerFighter, WeekPlan, WeekReport, suggested_plan};

#[derive(Debug, Clone, PartialEq)]
pub struct GameState {
    /// Number of the next week to play, from 1.
    pub week: u64,
    /// The player's club; a fighter's index is its place here.
    pub fighters: Vec<CareerFighter>,
    /// The only source of chance after the start: same seed and decisions, same game.
    pub rng: ChaCha8Rng,
}

impl GameState {
    /// A new game from `seed`: a club of one generated fighter, the same one
    /// `CareerFighter::random` draws first from that seed.
    // ponytail: one fighter until the fighter list screen and the market give a roster.
    #[must_use]
    pub fn new(seed: u64, data: &DesignData) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let fighters = vec![CareerFighter::random(&mut rng, data)];
        Self {
            week: 1,
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
        }
    }
}

/// The player's decisions for one week.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decisions {
    /// One plan per fighter, in club order; a fighter without one follows `suggested_plan`.
    pub plans: Vec<WeekPlan>,
}

/// Plays one week for the whole club, fighters in club order, then moves to the next
/// week. Returns each fighter's report, `None` for those retired or dead.
pub fn next_week(
    state: &mut GameState,
    decisions: &Decisions,
    data: &DesignData,
) -> Vec<Option<WeekReport>> {
    let week = state.week;
    let reports = state
        .fighters
        .iter_mut()
        .enumerate()
        .map(|(i, fighter)| {
            let plan = decisions
                .plans
                .get(i)
                .copied()
                .unwrap_or_else(|| suggested_plan(fighter, week));
            fighter.play_week(plan, &mut state.rng, data)
        })
        .collect();
    state.week += 1;
    reports
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fighter::career::{Activity, Status};

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    #[test]
    fn a_new_game_starts_at_week_one_with_the_seed_s_fighter() {
        let data = data();
        let state = GameState::new(7, &data);
        assert_eq!(state.week, 1);
        assert_eq!(state.date(&data), Date { season: 1, week: 1 });
        let fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(7), &data);
        assert_eq!(state.fighters, [fighter]);
    }

    #[test]
    fn a_week_follows_the_decisions_and_moves_on() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let rest = WeekPlan {
            activity: Activity::Rest,
            care: false,
        };
        let decisions = Decisions { plans: vec![rest] };
        let reports = next_week(&mut state, &decisions, &data);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].as_ref().unwrap().plan, rest);
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
        let before = state.fighters.clone();
        assert_eq!(next_week(&mut state, &Decisions::default(), &data), [None]);
        assert_eq!((state.week, state.fighters), (2, before));
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
