//! The whole game: the player's club, the colosseums and the week to play. One week is one call
//! to [`next_week`]; the interface only builds [`Decisions`] and reads the state.

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::calendar::Date;
use crate::colosseum::{
    Challenge, Colosseum, ColosseumId, Contender, TitleNews, WorldRules, pick_opponent, title_kept,
};
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction};
use crate::fighter::career::{
    Activity, CareerFighter, Status, WeekPlan, WeekReport, suggested_plan,
};
use crate::fighter::fame::FightResult;
use crate::rivals::{self, Rival};

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
    /// The rival clubs, owners of the residents, in the order of `data/world.ron`.
    pub rivals: Vec<Rival>,
    /// The residents' challenges to the club's fighters, to accept or refuse this week.
    pub challenges: Vec<Challenge>,
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
            rivals: Vec::new(),
            challenges: Vec::new(),
            rng,
        };
        state.open_world(data);
        state
    }

    /// Opens the colosseums and the rival clubs of `data` the game does not have yet:
    /// all of them in a new game, those added to `data/world.ron` since an older save.
    /// A new rival club starts with the player's starting money.
    pub fn open_world(&mut self, data: &DesignData) {
        for rules in data.world.colosseums.iter().skip(self.colosseums.len()) {
            let colosseum = Colosseum::open(&mut self.rng, rules, data, self.week);
            self.colosseums.push(colosseum);
        }
        let money = i64::from(data.economy.starting_money);
        let missing = data.world.rivals.len().saturating_sub(self.rivals.len());
        self.rivals
            .extend(std::iter::repeat_n(Rival { money }, missing));
    }

    /// Each resident's plan this week, by colosseum, from its club's rules
    /// (`rivals::plan`); each club's money goes to its fighters in colosseum order.
    #[must_use]
    pub fn rival_plans(&self, data: &DesignData) -> Vec<Vec<WeekPlan>> {
        let mut left: Vec<i64> = self.rivals.iter().map(|r| r.money).collect();
        let rest = WeekPlan {
            activity: Activity::Rest,
            care: false,
        };
        (0..)
            .zip(&self.colosseums)
            .enumerate()
            .map(|(c, (id, colosseum))| {
                let home = ColosseumId(id);
                (0..colosseum.residents.len())
                    .map(|r| match data.world.owner(c, r) {
                        Some((rival, profile)) if rival < left.len() => {
                            let chance = colosseum.chance(r, &data.world);
                            let fighter = &colosseum.residents[r];
                            rivals::plan(fighter, chance, home, &mut left[rival], profile, data)
                        }
                        _ => rest,
                    })
                    .collect()
            })
            .collect()
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
    /// of fighting or training in the last two weeks before its deadline, or else
    /// accepts a challenge it received and the club can pay for.
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
        if matches!(plan.activity, Activity::Fight(_) | Activity::Train)
            && let Some(challenge) = self.challenges.iter().find(|ch| {
                ch.fighter == i
                    && data
                        .world
                        .colosseum(ch.colosseum)
                        .is_some_and(|c| i64::from(c.purses.entry_fee) <= self.money)
            })
        {
            plan.activity = Activity::Challenge(challenge.colosseum, challenge.resident);
        }
        plan
    }

    /// Whether the club's fighter `i` may fight `resident` of colosseum `id` in a
    /// challenge: the resident challenged it this week or its club accepts the challenge.
    #[must_use]
    pub fn may_challenge(
        &self,
        i: usize,
        id: ColosseumId,
        resident: usize,
        data: &DesignData,
    ) -> bool {
        let received = Challenge {
            fighter: i,
            colosseum: id,
            resident,
        };
        let Ok(c) = usize::try_from(id.0) else {
            return false;
        };
        self.challenges.contains(&received)
            || self
                .colosseums
                .get(c)
                .zip(data.world.owner(c, resident))
                .is_some_and(|(colosseum, (_, profile))| {
                    colosseum.accepts_challenge(resident, i, profile.accept_percent, &data.world)
                })
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
    /// The challenges refused by a fighter who did not fight, and so paid for.
    pub refusals: Vec<Challenge>,
}

/// Plays one week for the whole club: signs a free agent if asked and there is room, then plays each
/// fighter's week in club order and pays for it, then ends the colosseums' week and moves to
/// the next week. A fight is against one of the colosseum's residents and moves both Elo; a
/// title fight is the only way a title changes hands, and the club's champion earns the
/// champion bonus for each defense it survives. A challenge is a fight against a chosen
/// resident; a fighter who refuses one and does not fight loses fame and Elo. The
/// residents' challenges for the next week come last.
///
/// A choice the club cannot pay for at its turn is dropped: no care, and rest instead
/// of training or of a fight with an entry fee. A title fight the fighter may not have
/// (`GameState::title_opponent`) is rest too, and so is a challenge it may not have
/// (`GameState::may_challenge`). Upkeep is always paid, even into debt.
pub fn next_week(state: &mut GameState, decisions: &Decisions, data: &DesignData) -> WeekSummary {
    let week = state.week;
    if decisions.hire_free_agent && state.can_hire(data) {
        let agent = CareerFighter::random(&mut state.rng, data);
        state.fighters.push(agent);
    }
    let (wanted, title_meetings) = week_plans(state, decisions, data);
    let rival_plans = state.rival_plans(data);
    // The residents who had their fight, against the club's fighters.
    let mut fought: Vec<_> = rival_plans.iter().map(|p| vec![false; p.len()]).collect();
    let economy = &data.economy;
    let mut ledger = Vec::new();
    let mut titles = Vec::new();
    let mut reports = Vec::with_capacity(state.fighters.len());
    for (i, fighter) in state.fighters.iter_mut().enumerate() {
        let plan = affordable(wanted[i], fighter, state.money, data);
        let fame = fighter.fame;
        // The colosseum and the resident met there.
        let meeting = match plan.activity {
            Activity::TitleFight(_) => title_meetings[i],
            activity => opponent(
                &mut state.rng,
                &state.colosseums,
                activity,
                &rival_plans,
                &fought,
            ),
        };
        if let Some((c, r)) = meeting {
            fought[c][r] = true;
        }
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
                let mut fee = 0;
                if let Activity::TitleFight(_) = plan.activity {
                    let news = title_fight(colosseum, club, resident, fight.result, week, data);
                    titles.push((c, news));
                    if let TitleNews::Fight {
                        champion, result, ..
                    } = news
                        && champion == club
                        && title_kept(result)
                    {
                        pay(Flow::ChampionBonus, i64::from(purses.champion_bonus));
                    }
                } else {
                    fee = i64::from(purses.entry_fee);
                    pay(Flow::EntryFee, -fee);
                    colosseum.record(club, resident, fight.result, &data.world);
                }
                let receipts = i64::from(purses.receipts(fame, fight.opponent.fame));
                pay(Flow::Receipts, receipts);
                let prize = i64::from(purses.win_prize);
                let mut theirs = receipts - fee;
                match fight.result {
                    FightResult::Won => pay(Flow::WinPrize, prize),
                    FightResult::Lost | FightResult::GaveUp => theirs += prize,
                    FightResult::Unfinished => {}
                }
                // The resident's club pays and earns the same.
                pay_rival(&mut state.rivals, &data.world, c, r, theirs);
            }
            pay(Flow::Upkeep, -i64::from(economy.upkeep));
        }
        reports.push(report);
    }
    let refusals = refusals(state, &reports, data);
    rival_weeks(state, &rival_plans, &fought, data);
    titles.extend(end_colosseum_weeks(state, week, data));
    state.week += 1;
    receive_challenges(state, data);
    WeekSummary {
        reports,
        ledger,
        titles,
        refusals,
    }
}

/// Ends the colosseums' week (`Colosseum::end_week`); returns what happened to their
/// titles, with the colosseum's index.
fn end_colosseum_weeks(
    state: &mut GameState,
    week: u64,
    data: &DesignData,
) -> Vec<(usize, TitleNews)> {
    let fighters = &state.fighters;
    let mut titles = Vec::new();
    for (c, (colosseum, rules)) in state
        .colosseums
        .iter_mut()
        .zip(&data.world.colosseums)
        .enumerate()
    {
        let news = colosseum.end_week(week, &mut state.rng, rules, data, fighters);
        titles.extend(news.into_iter().map(|n| (c, n)));
    }
    titles
}

/// Records the title fight of the club's fighter `club` against `resident` in
/// `colosseum`, `result` being the club fighter's.
fn title_fight(
    colosseum: &mut Colosseum,
    club: Contender,
    resident: Contender,
    result: FightResult,
    week: u64,
    data: &DesignData,
) -> TitleNews {
    let (champion, challenger, result) = if colosseum.champion == Some(club) {
        (club, resident, result)
    } else {
        (resident, club, opposite(result))
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
fn refusals(
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
    }
    refused
}

/// The residents' challenges for the week to play: in each colosseum where it is
/// ranked, a club's fighter fit to fight is challenged by the resident closest in Elo,
/// with the `challenge_percent` chance of the resident's club.
fn receive_challenges(state: &mut GameState, data: &DesignData) {
    for (i, fighter) in state.fighters.iter().enumerate() {
        let fit = suggested_plan(fighter, state.week).activity != Activity::Rest;
        if fighter.status != Status::Active || !fit {
            continue;
        }
        for (id, (c, colosseum)) in (0..).zip(state.colosseums.iter().enumerate()) {
            if let Some(resident) = colosseum.challenger(i)
                && let Some((_, profile)) = data.world.owner(c, resident)
                && state.rng.random_range(0..100) < profile.challenge_percent
            {
                state.challenges.push(Challenge {
                    fighter: i,
                    colosseum: ColosseumId(id),
                    resident,
                });
            }
        }
    }
}

/// The rival clubs' week, after the club's fights. In each colosseum, the residents
/// registered to fight and not met by the club's fighters are paired in ranking order,
/// so close in Elo, each pair fighting a duel that moves both Elo; one left without an
/// opponent trains instead. The others rest or train as planned, those met by the
/// club's fighters end their week, and each club pays its fighters' costs and upkeep
/// and takes their receipts and prizes.
// ponytail: real duels until the statistical model of phase 5 resolves fights between
// rivals.
fn rival_weeks(
    state: &mut GameState,
    plans: &[Vec<WeekPlan>],
    fought: &[Vec<bool>],
    data: &DesignData,
) {
    let world = &data.world;
    for (c, colosseum) in state.colosseums.iter_mut().enumerate() {
        let Some(purses) = world.colosseums.get(c).map(|r| &r.purses) else {
            continue;
        };
        let mut plans = plans[c].clone();
        let mut done = fought[c].clone();
        for (r, (resident, &met)) in colosseum.residents.iter_mut().zip(&done).enumerate() {
            if resident.status == Status::Active {
                let upkeep = -i64::from(data.economy.upkeep);
                pay_rival(&mut state.rivals, world, c, r, upkeep);
            }
            if met {
                resident.end_fought_week(&mut state.rng, data);
            }
        }
        let waiting: Vec<usize> = colosseum
            .ranking
            .iter()
            .filter_map(|rating| match rating.contender {
                Contender::Resident(r)
                    if !done[r]
                        && matches!(plans[r].activity, Activity::Fight(_))
                        && colosseum.residents[r].status == Status::Active =>
                {
                    Some(r)
                }
                _ => None,
            })
            .collect();
        for pair in waiting.chunks(2) {
            let &[a, b] = pair else {
                plans[pair[0]].activity = Activity::Train;
                continue;
            };
            let Ok([first, second]) = colosseum.residents.get_disjoint_mut([a, b]) else {
                continue;
            };
            let receipts = i64::from(purses.receipts(first.fame, second.fame));
            let report = first.play_week(plans[a], Some(second), &mut state.rng, data);
            second.end_fought_week(&mut state.rng, data);
            (done[a], done[b]) = (true, true);
            let Some(fight) = report.and_then(|r| r.fight) else {
                continue;
            };
            colosseum.record(
                Contender::Resident(a),
                Contender::Resident(b),
                fight.result,
                world,
            );
            for (r, result) in [(a, fight.result), (b, opposite(fight.result))] {
                let prize = if result == FightResult::Won {
                    purses.win_prize
                } else {
                    0
                };
                let earned = receipts + i64::from(prize) - i64::from(purses.entry_fee);
                pay_rival(&mut state.rivals, world, c, r, earned);
            }
        }
        for r in (0..colosseum.residents.len()).filter(|&r| !done[r]) {
            let plan = plans[r];
            if let Some(report) = colosseum.residents[r].play_week(plan, None, &mut state.rng, data)
            {
                let training = if plan.activity == Activity::Train {
                    data.economy.training_cost
                } else {
                    0
                };
                let cost = i64::from(report.care_cost) + i64::from(training);
                pay_rival(&mut state.rivals, world, c, r, -cost);
            }
        }
    }
}

/// Adds `amount` to the money of the club owning resident `r` of colosseum `c`.
fn pay_rival(rivals: &mut [Rival], world: &WorldRules, c: usize, r: usize, amount: i64) {
    if let Some(rival) = world.owner(c, r).and_then(|(i, _)| rivals.get_mut(i)) {
        rival.money += amount;
    }
}

/// Each fighter's wanted plan, `GameState::suggested_plan` without one, and the colosseum
/// and resident of its title fight. Title fights are settled before anyone plays, from the
/// rankings as they stand; one the fighter may not have turns into rest, and so does a
/// challenge it may not have.
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
            let allowed = match plan.activity {
                Activity::TitleFight(_) => meeting.is_some(),
                Activity::Challenge(id, r) => state.may_challenge(i, id, r, data),
                _ => true,
            };
            if !allowed {
                plan.activity = Activity::Rest;
            }
            (plan, meeting)
        })
        .unzip()
}

/// The colosseum and the resident met in a fight (`activity`): the one challenged while
/// alive (someone may have killed it earlier in the week), otherwise one at random among
/// those registered to fight there (in `plans`) who have not `fought` yet.
fn opponent(
    rng: &mut ChaCha8Rng,
    colosseums: &[Colosseum],
    activity: Activity,
    plans: &[Vec<WeekPlan>],
    fought: &[Vec<bool>],
) -> Option<(usize, usize)> {
    let (id, chosen) = match activity {
        Activity::Fight(id) => (id, None),
        Activity::Challenge(id, r) => (id, Some(r)),
        _ => return None,
    };
    let c = usize::try_from(id.0).ok()?;
    let colosseum = colosseums.get(c)?;
    let (plans, fought) = (plans.get(c)?, fought.get(c)?);
    let active = |r: &usize| {
        colosseum
            .residents
            .get(*r)
            .is_some_and(|f| f.status == Status::Active)
    };
    if let Some(r) = chosen.filter(active) {
        return Some((c, r));
    }
    let candidates: Vec<usize> = (0..colosseum.residents.len())
        .filter(|r| {
            active(r)
                && !fought.get(*r).copied().unwrap_or(true)
                && plans
                    .get(*r)
                    .is_some_and(|p| matches!(p.activity, Activity::Fight(_)))
        })
        .collect();
    Some((c, pick_opponent(rng, colosseum, &candidates)?))
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
        Activity::Fight(id) | Activity::Challenge(id, _) => {
            data.world.colosseum(id).map(|c| c.purses.entry_fee)
        }
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
    fn a_challenge_meets_the_chosen_resident_if_it_accepts() {
        let data = data();
        let rules = &data.world;
        let mut state = top_of_rome(&data);
        let rome = ColosseumId(1);
        let resident = 7;
        assert!(state.may_challenge(0, rome, resident, &data));
        let before = state.colosseums[1].residents[resident].clone();
        let summary = next_week(
            &mut state,
            &plan(Activity::Challenge(rome, resident), false),
            &data,
        );
        let report = summary.reports[0].as_ref().unwrap();
        assert_eq!(report.plan.activity, Activity::Challenge(rome, resident));
        let fight = report.fight.as_ref().unwrap();
        assert_eq!(fight.opponent, before);
        let fee = i64::from(rules.colosseums[1].purses.entry_fee);
        assert_eq!(flows(&summary)[0], (Flow::EntryFee, -fee));
        assert_ne!(
            state.colosseums[1].elo(Contender::Resident(resident), rules),
            rules.elo_start
        );
        // Far ahead in Elo, no resident accepts: a week of rest.
        state.colosseums[1].ranking[0].elo = 2000;
        state.challenges.clear();
        assert!(!state.may_challenge(0, rome, 2, &data));
        let summary = next_week(
            &mut state,
            &plan(Activity::Challenge(rome, 2), false),
            &data,
        );
        assert_eq!(
            summary.reports[0].as_ref().unwrap().plan.activity,
            Activity::Rest
        );
    }

    #[test]
    fn a_received_challenge_is_accepted_or_paid_for_when_not_fighting() {
        let data = data();
        let rules = &data.world;
        let mut state = top_of_rome(&data);
        state.colosseums[1].ranking[0].elo = 2000;
        let received = Challenge {
            fighter: 0,
            colosseum: ColosseumId(1),
            resident: 2,
        };
        // Received, so it may be fought however far ahead the fighter is.
        state.challenges = vec![received];
        assert!(state.may_challenge(0, ColosseumId(1), 2, &data));
        state.fighters[0].form = crate::fighter::form::Form::full();
        assert_eq!(
            state.suggested_plan(0, &data).activity,
            Activity::Challenge(ColosseumId(1), 2)
        );
        // Fighting elsewhere refuses it for nothing.
        let mut elsewhere = state.clone();
        let summary = next_week(
            &mut elsewhere,
            &plan(Activity::Fight(ColosseumId(0)), false),
            &data,
        );
        assert_eq!(summary.refusals, []);
        assert_eq!(elsewhere.colosseums[1].elo(Contender::Club(0), rules), 2000);
        // Training refuses it at a price.
        let fame = state.fighters[0].fame;
        let summary = next_week(&mut state, &plan(Activity::Train, false), &data);
        assert_eq!(summary.refusals, [received]);
        let refusal = &rules.challenges;
        assert_eq!(
            state.colosseums[1].elo(Contender::Club(0), rules),
            2000 - i32::try_from(refusal.refusal_elo).unwrap()
        );
        assert_eq!(
            state.fighters[0].fame,
            fame.changed_by(-i32::try_from(refusal.refusal_fame).unwrap())
        );
    }

    #[test]
    fn residents_challenge_the_ranked_fighters_fit_to_fight() {
        let data = data();
        let mut state = top_of_rome(&data);
        let mut received = 0;
        for _ in 0..20 {
            state.fighters[0].form = crate::fighter::form::Form::full();
            state.fighters[0].injuries.clear();
            state.fighters[0].illness = None;
            next_week(&mut state, &plan(Activity::Rest, false), &data);
            assert!(state.challenges.iter().all(|ch| ch.fighter == 0
                && ch.colosseum == ColosseumId(1)
                && Some(ch.resident) == state.colosseums[1].challenger(0)));
            received += state.challenges.len();
        }
        // A 25 % chance each week.
        assert!(
            (2..=10).contains(&received),
            "{received} challenges in 20 weeks"
        );
        // A tired fighter gets none.
        state.fighters[0].form = crate::fighter::form::Form::new(10);
        next_week(&mut state, &plan(Activity::Rest, false), &data);
        assert_eq!(state.challenges, []);
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

    #[test]
    fn rival_clubs_play_their_weeks_without_the_player() {
        let data = data();
        let mut state = GameState::new(3, &data);
        state.fighters[0].status = Status::Retired;
        let start = state.clone();
        for _ in 0..60 {
            next_week(&mut state, &Decisions::default(), &data);
        }
        for (c, colosseum) in state.colosseums.iter().enumerate() {
            let elos: Vec<i32> = colosseum.ranking.iter().map(|r| r.elo).collect();
            assert!(
                elos.iter().any(|&e| e > data.world.elo_start + 50),
                "{c}: {elos:?}"
            );
            assert!(
                elos.iter().any(|&e| e < data.world.elo_start - 50),
                "{c}: {elos:?}"
            );
            // They tire, get hurt, die or retire and are replaced.
            assert!(colosseum.residents.iter().any(|f| f.form.value() < 100));
            assert_ne!(colosseum.residents, start.colosseums[c].residents);
        }
        assert!(
            state
                .rivals
                .iter()
                .all(|r| r.money != start.rivals[0].money)
        );
    }

    #[test]
    fn residents_follow_their_clubs_rules_and_the_club_meets_those_registered() {
        let data = data();
        let mut state = GameState::new(3, &data);
        // Resident 0 of Capua belongs to the cautious club, resident 1 to the aggressive.
        assert_eq!(
            data.world.rivals[0].profile,
            crate::rivals::Profile::Cautious
        );
        assert_eq!(
            data.world.rivals[1].profile,
            crate::rivals::Profile::Aggressive
        );
        let capua = &mut state.colosseums[0];
        capua.residents[0].form = crate::fighter::form::Form::new(50);
        capua.residents[1].form = crate::fighter::form::Form::new(50);
        let plans = state.rival_plans(&data);
        assert_eq!(plans[0][0].activity, Activity::Rest);
        assert_eq!(plans[0][1].activity, Activity::Fight(ColosseumId(0)));
        // Only resident 1 registered and not fought yet: the club meets it.
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut only = vec![vec![plan(Activity::Rest, false).plans[0]; 15]; 2];
        only[0][1] = plans[0][1];
        let mut fought = vec![vec![false; 15]; 2];
        let (fight, challenge) = (
            Activity::Fight(ColosseumId(0)),
            Activity::Challenge(ColosseumId(0), 4),
        );
        let colosseums = &state.colosseums;
        assert_eq!(
            opponent(&mut rng, colosseums, fight, &only, &fought),
            Some((0, 1))
        );
        assert_eq!(
            opponent(&mut rng, colosseums, challenge, &only, &fought),
            Some((0, 4))
        );
        fought[0][1] = true;
        assert!(opponent(&mut rng, colosseums, fight, &only, &fought).is_some());
    }
}
