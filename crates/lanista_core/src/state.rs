//! The whole game: the player's club, the colosseums, the rival clubs, the market and the week
//! to play. One week is one call to [`next_week`]; the interface only builds [`Decisions`] and
//! reads the state.

use std::cmp::Reverse;

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::calendar::{Date, SeasonPhase};
use crate::colosseum::{
    Challenge, Colosseum, ColosseumId, Contender, Rosters, TitleNews, pick_opponent, title_kept,
};
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction};
use crate::fighter::care::CareRules;
use crate::fighter::career::{
    Activity, CareerFighter, Status, WeekPlan, WeekReport, suggested_plan,
};
use crate::fighter::fame::{Fame, FightResult};
use crate::fighter::{FighterId, percent_of};
use crate::market::{self, MarketNews, Move, SaleOffer, Target, value};
use crate::rivals::{self, Rival, RivalId};
use crate::staff::{
    DoctorRules, Medicine, RecruiterRules, Role, Scouting, Staff, StaffEffects, StaffMove,
    TrainerRules,
};

const REST: WeekPlan = WeekPlan {
    activity: Activity::Rest,
    care: false,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    /// Number of the next week to play, from 1.
    pub week: u64,
    /// The club's money; below 0 when upkeep ran it into debt.
    pub money: i64,
    /// The player's club; a fighter's index is its place here. One who leaves keeps
    /// its place, with its status.
    pub fighters: Vec<CareerFighter>,
    /// In the order of `data/world.ron`.
    pub colosseums: Vec<Colosseum>,
    /// The rival clubs and their fighters, in the order of `data/world.ron`.
    pub rivals: Vec<Rival>,
    /// The rival fighters' challenges to the club's fighters, to accept or refuse this week.
    pub challenges: Vec<Challenge>,
    /// The market's free agents, signed for nothing; never fewer than
    /// `MarketRules::min_free_agents` at the start of a week.
    pub free_agents: Vec<CareerFighter>,
    /// The club's fighters on the transfer list, by index: they fight on for the club
    /// until the player accepts an offer.
    pub transfer_list: Vec<usize>,
    /// The rival clubs' offers for them, open this week.
    pub sale_offers: Vec<SaleOffer>,
    /// The price a rival club asked instead of the player's last offer, open this week.
    pub counter_offer: Option<(RivalId, u32)>,
    pub staff: Staff,
    /// The only source of chance after the start: same seed and decisions, same game.
    pub rng: ChaCha8Rng,
}

impl GameState {
    /// A new game from `seed`: a club of one generated fighter, the same one
    /// `CareerFighter::random` draws first from that seed, the starting money, the
    /// rival clubs with their fighters, the colosseums and the free agents.
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
            free_agents: Vec::new(),
            transfer_list: Vec::new(),
            sale_offers: Vec::new(),
            counter_offer: None,
            staff: Staff::default(),
            rng,
        };
        state.open_world(data);
        state
    }

    /// Opens the rival clubs and the colosseums of `data` the game does not have yet:
    /// all of them in a new game, those added to `data/world.ron` since an older save.
    /// A new rival club starts with the player's starting money; a new colosseum with
    /// its `residents`, fighters as strong as its prestige shared out among the rival
    /// clubs in turn, all ranked there. Then brings the free agents up to their fewest.
    pub fn open_world(&mut self, data: &DesignData) {
        let money = i64::from(data.economy.starting_money);
        let missing = data.world.rivals.len().saturating_sub(self.rivals.len());
        let new = Rival {
            money,
            fighters: Vec::new(),
        };
        self.rivals.extend(std::iter::repeat_n(new, missing));
        let clubs = self.rivals.len();
        for (c, rules) in data.world.colosseums.iter().enumerate() {
            if c < self.colosseums.len() || clubs == 0 {
                continue;
            }
            let mut ranked = Vec::new();
            for r in 0..usize::try_from(rules.residents).unwrap_or(0) {
                let fighter = CareerFighter::random(&mut self.rng, data);
                let fighter = market::with_prestige(fighter, rules.prestige, data);
                let club = (c + r) % clubs;
                let roster = &mut self.rivals[club].fighters;
                ranked.push(RivalId {
                    club,
                    fighter: roster.len(),
                });
                roster.push(fighter);
            }
            let ranked: Vec<_> = ranked
                .into_iter()
                .map(|id| {
                    (
                        Contender::Rival(id),
                        &self.rivals[id.club].fighters[id.fighter],
                    )
                })
                .collect();
            let colosseum = Colosseum::open(&ranked, self.week, &data.world);
            self.colosseums.push(colosseum);
        }
        let fewest = usize::try_from(data.economy.market.min_free_agents).unwrap_or(0);
        while self.free_agents.len() < fewest {
            let agent = market::free_agent(&mut self.rng, data);
            self.free_agents.push(agent);
        }
    }

    /// Every fighter in the game, for the colosseums.
    #[must_use]
    pub fn rosters(&self) -> Rosters<'_> {
        Rosters {
            club: &self.fighters,
            rivals: &self.rivals,
        }
    }

    /// The rival fighter `id`.
    #[must_use]
    pub fn rival_fighter(&self, id: RivalId) -> Option<&CareerFighter> {
        self.rivals.get(id.club)?.fighters.get(id.fighter)
    }

    /// Each rival fighter's plan this week, by club, from its club's rules
    /// (`rivals::plan`); each club's money goes to its fighters in roster order.
    #[must_use]
    pub fn rival_plans(&self, data: &DesignData) -> Vec<Vec<WeekPlan>> {
        let rosters = self.rosters();
        self.rivals
            .iter()
            .enumerate()
            .map(|(club, rival)| {
                let mut left = rival.money;
                let profile = data.world.profile(club);
                (0..)
                    .zip(&rival.fighters)
                    .map(|(fighter, f)| match profile {
                        Some(profile) if f.status == Status::Active => {
                            let me = Contender::Rival(RivalId { club, fighter });
                            let chances: Vec<f64> = self
                                .colosseums
                                .iter()
                                .map(|c| c.chance(me, rosters, &data.world))
                                .collect();
                            rivals::plan(f, &chances, &mut left, profile, data)
                        }
                        _ => REST,
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

    /// Fighters neither retired, dead nor gone.
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

    /// The fame of the club's most famous fighter in activity.
    #[must_use]
    pub fn club_fame(&self) -> Fame {
        self.fighters
            .iter()
            .filter(|f| f.status == Status::Active)
            .map(|f| f.fame)
            .max_by_key(|f| f.value())
            .unwrap_or_default()
    }

    /// The suggested plans of every fighter this week, and no market move.
    #[must_use]
    pub fn suggested_decisions(&self, data: &DesignData) -> Decisions {
        Decisions {
            plans: (0..self.fighters.len())
                .map(|i| self.suggested_plan(i, data))
                .collect(),
            moves: Vec::new(),
            staff: Vec::new(),
        }
    }

    /// `career::suggested_plan` for the club's fighter `i`, who defends a title instead
    /// of fighting or training in the last two weeks before its deadline, or else
    /// accepts a challenge it received and the club can pay for.
    #[must_use]
    pub fn suggested_plan(&self, i: usize, data: &DesignData) -> WeekPlan {
        let Some(fighter) = self.fighters.get(i) else {
            return REST;
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
            plan.activity = Activity::Challenge(challenge.colosseum, challenge.rival);
        }
        plan
    }

    /// Whether the club's fighter `i` may fight `rival` in colosseum `id` in a
    /// challenge: the rival fighter challenged it this week, or its club accepts the
    /// challenge.
    #[must_use]
    pub fn may_challenge(
        &self,
        i: usize,
        id: ColosseumId,
        rival: RivalId,
        data: &DesignData,
    ) -> bool {
        let received = Challenge {
            fighter: i,
            colosseum: id,
            rival,
        };
        let active = self
            .rival_fighter(rival)
            .is_some_and(|f| f.status == Status::Active);
        let accepts = || {
            let colosseum = usize::try_from(id.0)
                .ok()
                .and_then(|c| self.colosseums.get(c));
            colosseum
                .zip(data.world.profile(rival.club))
                .is_some_and(|(colosseum, profile)| {
                    colosseum.accepts_challenge(rival, i, profile.accept_percent, &data.world)
                })
        };
        active && (self.challenges.contains(&received) || accepts())
    }

    /// The colosseum and the rival fighter the club's fighter `i` meets if it fights for
    /// the title of colosseum `id`; `None` if it may not (see `Colosseum::title_opponent`).
    #[must_use]
    pub fn title_opponent(
        &self,
        i: usize,
        id: ColosseumId,
        data: &DesignData,
    ) -> Option<(usize, RivalId)> {
        let c = usize::try_from(id.0).ok()?;
        let rival = self
            .colosseums
            .get(c)?
            .title_opponent(i, self.rosters(), &data.world)?;
        Some((c, rival))
    }

    /// The price the club of rival fighter `id` first asks: its value at the club's
    /// `sell_percent`. Hidden from the player, who learns it from a counter-offer.
    #[must_use]
    pub fn asking_price(&self, id: RivalId, data: &DesignData) -> Option<u32> {
        let fighter = self
            .rival_fighter(id)
            .filter(|f| f.status == Status::Active)?;
        let profile = data.world.profile(id.club)?;
        Some(percent_of(
            value(fighter, &data.economy.market),
            profile.sell_percent,
        ))
    }

    /// Whether rival club `club` has a place for one more fighter in activity.
    #[must_use]
    pub fn rival_has_place(&self, club: usize, data: &DesignData) -> bool {
        self.rivals
            .get(club)
            .and_then(|r| u32::try_from(r.active()).ok())
            .is_some_and(|n| n < data.world.roster.max)
    }

    /// Whether `m` may be made now: the fighter is there, the club has a place to sign,
    /// the money for an offer, the rival club a place and the money for its offer.
    #[must_use]
    pub fn may_move(&self, m: Move, data: &DesignData) -> bool {
        let active = |f: Option<&CareerFighter>| f.is_some_and(|f| f.status == Status::Active);
        let ours = |i: usize| active(self.fighters.get(i));
        match m {
            Move::Observe(Target::FreeAgent(a)) => a < self.free_agents.len(),
            Move::Observe(Target::Rival(id)) => active(self.rival_fighter(id)),
            Move::Sign(a) => a < self.free_agents.len() && self.can_hire(data),
            Move::Release(i) => ours(i),
            Move::List(i) => ours(i) && !self.transfer_list.contains(&i),
            Move::Unlist(i) => self.transfer_list.contains(&i),
            Move::AcceptOffer { fighter, rival } => {
                ours(fighter)
                    && self.rival_has_place(rival, data)
                    && self.sale_offers.iter().any(|o| {
                        o.fighter == fighter
                            && o.rival == rival
                            && self.rivals[rival].money >= i64::from(o.price)
                    })
            }
            Move::Offer(id, price) => {
                active(self.rival_fighter(id))
                    && price > 0
                    && i64::from(price) <= self.money
                    && self.can_hire(data)
            }
        }
    }

    /// The club's trainer, `None` without one or with a level missing from `data`.
    #[must_use]
    pub fn trainer<'a>(&self, data: &'a DesignData) -> Option<&'a TrainerRules> {
        data.economy.staff.trainer(self.staff.trainer?)
    }

    /// The club's doctor, `None` without one or with a level missing from `data`.
    #[must_use]
    pub fn doctor<'a>(&self, data: &'a DesignData) -> Option<&'a DoctorRules> {
        data.economy.staff.doctor(self.staff.doctor?)
    }

    /// The club's recruiter, `None` without one or with a level missing from `data`.
    #[must_use]
    pub fn recruiter<'a>(&self, data: &'a DesignData) -> Option<&'a RecruiterRules> {
        data.economy.staff.recruiter(self.staff.recruiter?)
    }

    /// What the club's trainer and doctor bring to its fighters' weeks.
    #[must_use]
    pub fn staff_effects(&self, data: &DesignData) -> StaffEffects {
        StaffEffects {
            coaching: self.trainer(data).map(|t| t.coaching).unwrap_or_default(),
            medicine: self.doctor(data).map(|d| d.medicine).unwrap_or_default(),
        }
    }

    /// What the club's recruiter shows of the fighters out of the club.
    #[must_use]
    pub fn scouting(&self, data: &DesignData) -> Scouting {
        self.recruiter(data).map(|r| r.scouting).unwrap_or_default()
    }

    /// Care prices and effect, set by the doctor.
    #[must_use]
    pub fn care_rules(&self, data: &DesignData) -> CareRules {
        self.staff_effects(data)
            .medicine
            .care_rules(&data.fighter.care)
    }

    /// The price of a training session, set by the trainer.
    #[must_use]
    pub fn training_cost(&self, data: &DesignData) -> u32 {
        self.trainer(data)
            .map_or(data.economy.training_cost, |t| t.training_cost)
    }

    /// Whether `m` may be made now: someone is hired for a post at a level of `data`
    /// the club does not have there yet, with the money for a first salary; dismissed
    /// if there is one.
    #[must_use]
    pub fn may_staff(&self, m: StaffMove, data: &DesignData) -> bool {
        match m {
            StaffMove::Hire(role, level) => {
                self.staff.level(role) != Some(level)
                    && data
                        .economy
                        .staff
                        .salary(role, level)
                        .is_some_and(|salary| i64::from(salary) <= self.money)
            }
            StaffMove::Dismiss(role) => self.staff.level(role).is_some(),
        }
    }

    /// The fighter out of the club `target` names.
    fn target_mut(&mut self, target: Target) -> Option<&mut CareerFighter> {
        match target {
            Target::FreeAgent(a) => self.free_agents.get_mut(a),
            Target::Rival(id) => self.rivals.get_mut(id.club)?.fighters.get_mut(id.fighter),
        }
    }
}

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
#[derive(Debug, Clone, PartialEq)]
pub struct WeekSummary {
    /// Each fighter's report, in club order; `None` for those not in activity.
    pub reports: Vec<Option<WeekReport>>,
    /// Every move of money, in the order it happened.
    pub ledger: Vec<Transaction>,
    /// What happened to the titles, with the colosseum's index, in the order it happened.
    pub titles: Vec<(usize, TitleNews)>,
    /// The challenges refused by a fighter who did not fight, and so paid for.
    pub refusals: Vec<Challenge>,
    /// What the club did and heard on the market, in the order it happened.
    pub market: Vec<MarketNews>,
    /// The changes of staff made.
    pub staff: Vec<StaffMove>,
}

/// Plays one week for the whole club: the market moves and the change of staff before
/// the week, the staff's salaries, then each fighter's week in club order, paid for, then the rival clubs' week, the answer to
/// the player's offer, the free agents' week and the rival clubs' market, then the
/// colosseums' titles, and moves to the next week. A fight is against a rival fighter
/// and moves both Elo; a title fight is the only way a title changes hands, and the
/// club's champion earns the champion bonus for each defense it survives. A challenge
/// is a fight against a chosen rival fighter; a fighter who refuses one and does not
/// fight loses fame and Elo. The rival fighters' challenges for the next week come last.
///
/// A choice the club cannot pay for at its turn is dropped: no care, and rest instead
/// of training or of a fight with an entry fee. A title fight the fighter may not have
/// (`GameState::title_opponent`) is rest too, and so is a challenge it may not have
/// (`GameState::may_challenge`). Upkeep and salaries are always paid, even into debt.
pub fn next_week(state: &mut GameState, decisions: &Decisions, data: &DesignData) -> WeekSummary {
    let mut ledger = Vec::new();
    let mut market = market_moves(state, &decisions.moves, data, &mut ledger);
    let staff = staff_week(state, &decisions.staff, data, &mut ledger);
    let effects = state.staff_effects(data);
    // A training session's price, and the doctor's for care.
    let costs = (state.training_cost(data), effects.medicine);
    let (wanted, title_meetings) = week_plans(state, decisions, data);
    let rival_plans = state.rival_plans(data);
    // The rival fighters who had their fight, against the club's fighters.
    let mut fought: Vec<_> = rival_plans.iter().map(|p| vec![false; p.len()]).collect();
    let mut titles = Vec::new();
    let mut reports = Vec::with_capacity(state.fighters.len());
    for (i, fighter) in state.fighters.iter_mut().enumerate() {
        let plan = affordable(wanted[i], fighter, state.money, costs, data);
        let fame = fighter.fame;
        // The colosseum and the rival fighter met there.
        let meeting = match plan.activity {
            Activity::TitleFight(_) => title_meetings[i],
            activity => opponent(
                &mut state.rng,
                (&state.colosseums, &state.rivals),
                activity,
                &rival_plans,
                &fought,
            ),
        };
        // Met in the rosters `fought` was made from.
        if let Some((_, id)) = meeting {
            fought[id.club][id.fighter] = true;
        }
        let opponent =
            meeting.and_then(|(_, id)| state.rivals.get_mut(id.club)?.fighters.get_mut(id.fighter));
        let report = fighter.play_week(plan, opponent, effects, &mut state.rng, data);
        if let Some(report) = &report {
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
                pay(Flow::Training, -i64::from(costs.0));
            }
            if let (Some(fight), Some((c, id))) = (&report.fight, meeting)
                && let Some(rules) = data.world.colosseums.get(c)
            {
                let purses = &rules.purses;
                let (club, rival) = (Contender::Club(i), Contender::Rival(id));
                let colosseum = &mut state.colosseums[c];
                let mut fee = 0;
                if let Activity::TitleFight(_) = plan.activity {
                    let news = title_fight(colosseum, club, rival, fight.result, state.week, data);
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
                    colosseum.record(club, rival, fight.result, &data.world);
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
                // The rival fighter's club pays and earns the same.
                pay_rival(&mut state.rivals, id.club, theirs);
            }
            pay(Flow::Upkeep, -i64::from(data.economy.upkeep));
        }
        reports.push(report);
    }
    let refusals = refusals(state, &reports, data);
    rival_weeks(state, &rival_plans, &fought, data);
    market.extend(answer_offer(state, &decisions.moves, data, &mut ledger));
    free_agent_weeks(state, data);
    rival_market(state, data);
    titles.extend(end_colosseum_weeks(state, state.week, data));
    state.open_world(data);
    trim_market(state, data);
    state.week += 1;
    receive_challenges(state, data);
    WeekSummary {
        reports,
        ledger,
        titles,
        refusals,
        market,
        staff,
    }
}

/// The changes of staff `moves` made before the week, the first for each post, each
/// if still allowed at its turn, then the salaries paid in `ledger`, post by post, even
/// into debt; returns the changes made.
fn staff_week(
    state: &mut GameState,
    moves: &[StaffMove],
    data: &DesignData,
    ledger: &mut Vec<Transaction>,
) -> Vec<StaffMove> {
    let mut made = Vec::new();
    let mut seen = Vec::new();
    for &m in moves {
        if seen.contains(&m.role()) {
            continue;
        }
        seen.push(m.role());
        if !state.may_staff(m, data) {
            continue;
        }
        match m {
            StaffMove::Hire(role, level) => state.staff.set(role, Some(level)),
            StaffMove::Dismiss(role) => state.staff.set(role, None),
        }
        made.push(m);
    }
    for role in Role::ALL {
        let salary = state
            .staff
            .level(role)
            .and_then(|level| data.economy.staff.salary(role, level));
        if let Some(salary) = salary {
            let amount = -i64::from(salary);
            state.money += amount;
            ledger.push(Transaction {
                fighter: None,
                flow: Flow::Salary,
                amount,
            });
        }
    }
    made
}

/// The market moves made before the week, in this order, each if still allowed: the
/// first observation, accepted offers, releases, the transfer list, then signings. Last
/// week's counter-offer closes.
fn market_moves(
    state: &mut GameState,
    moves: &[Move],
    data: &DesignData,
    ledger: &mut Vec<Transaction>,
) -> Vec<MarketNews> {
    state.counter_offer = None;
    let mut news = Vec::new();
    if let Some(target) = moves.iter().find_map(|&m| match m {
        Move::Observe(target) => Some(target),
        _ => None,
    }) && let Some(fighter) = state.target_mut(target)
    {
        fighter.observed_weeks += 1;
    }
    let order = |m: &Move| match *m {
        Move::AcceptOffer { .. } => (0, Reverse(0)),
        Move::Release(_) => (1, Reverse(0)),
        Move::List(_) | Move::Unlist(_) => (2, Reverse(0)),
        // Signed from the last, so that each index still names the same free agent.
        Move::Sign(a) => (3, Reverse(a)),
        Move::Observe(_) | Move::Offer(..) => (u8::MAX, Reverse(0)),
    };
    let mut moves: Vec<Move> = moves.to_vec();
    moves.sort_by_key(order);
    for m in moves {
        if order(&m).0 == u8::MAX || !state.may_move(m, data) {
            continue;
        }
        match m {
            Move::AcceptOffer { fighter, rival } => {
                let Some(&offer) = state
                    .sale_offers
                    .iter()
                    .find(|o| o.fighter == fighter && o.rival == rival)
                else {
                    continue;
                };
                let sold = state.fighters[fighter].clone();
                state.fighters[fighter].status = Status::Sold;
                state.rivals[rival].fighters.push(sold);
                let price = i64::from(offer.price);
                state.rivals[rival].money -= price;
                state.money += price;
                ledger.push(Transaction {
                    fighter: Some(fighter),
                    flow: Flow::Sale,
                    amount: price,
                });
                state.sale_offers.retain(|o| o.fighter != fighter);
                news.push(MarketNews::Sold(offer));
            }
            Move::Release(i) => {
                let released = state.fighters[i].clone();
                state.fighters[i].status = Status::Released;
                state.free_agents.push(released);
                news.push(MarketNews::Released(i));
            }
            Move::List(i) => state.transfer_list.push(i),
            Move::Unlist(i) => state.transfer_list.retain(|&l| l != i),
            Move::Sign(a) => {
                let agent = state.free_agents.remove(a);
                state.fighters.push(agent);
                news.push(MarketNews::Signed(state.fighters.len() - 1));
            }
            Move::Observe(_) | Move::Offer(..) => {}
        }
    }
    state
        .transfer_list
        .retain(|&i| state.fighters[i].status == Status::Active);
    news
}

/// The answer to the first offer of `moves`, if still allowed (see `negotiate`).
fn answer_offer(
    state: &mut GameState,
    moves: &[Move],
    data: &DesignData,
    ledger: &mut Vec<Transaction>,
) -> Option<MarketNews> {
    let (id, price) = moves.iter().find_map(|&m| match m {
        Move::Offer(id, price) => Some((id, price)),
        _ => None,
    })?;
    state
        .may_move(Move::Offer(id, price), data)
        .then(|| negotiate(state, id, price, data, ledger))
}

/// The answer to the player's offer of `price` for rival fighter `id`, which
/// `GameState::may_move` allows. At the club's asking price or more, the club accepts
/// and the fighter decides (`market::refuses_to_join`); joining, it becomes the
/// club's, paid for in `ledger`. From `counter_percent` of it, the club asks its price
/// for the next week; below, it refuses.
fn negotiate(
    state: &mut GameState,
    id: RivalId,
    price: u32,
    data: &DesignData,
    ledger: &mut Vec<Transaction>,
) -> MarketNews {
    let market = &data.economy.market;
    let Some(asking) = state.asking_price(id, data) else {
        return MarketNews::Failed(id);
    };
    if price < asking {
        let counter =
            u64::from(price) * 100 >= u64::from(asking) * u64::from(market.counter_percent);
        if !counter {
            return MarketNews::ClubRefused(id);
        }
        state.counter_offer = Some((id, asking));
        return MarketNews::Countered(id, asking);
    }
    let club_fame = state.club_fame();
    let fighter = &state.rivals[id.club].fighters[id.fighter];
    if market::refuses_to_join(&mut state.rng, fighter, club_fame, market) {
        return MarketNews::FighterRefused(id);
    }
    if !state.can_hire(data) || i64::from(price) > state.money {
        return MarketNews::Failed(id);
    }
    let bought = fighter.clone();
    state.rivals[id.club].fighters[id.fighter].status = Status::Sold;
    state.fighters.push(bought);
    let amount = i64::from(price);
    state.money -= amount;
    state.rivals[id.club].money += amount;
    let fighter = state.fighters.len() - 1;
    ledger.push(Transaction {
        fighter: Some(fighter),
        flow: Flow::Purchase,
        amount: -amount,
    });
    MarketNews::Bought {
        from: id,
        fighter,
        price,
    }
}

/// Whether the week played starts the off-season.
fn off_season_starts(state: &GameState, data: &DesignData) -> bool {
    let today = state.date(data);
    let calendar = &data.calendar;
    today.phase(calendar) == SeasonPhase::OffSeason
        && today.week == calendar.competition_weeks + calendar.great_event_weeks + 1
}

/// The free agents' week: they rest, and leave the market if they die or retire.
fn free_agent_weeks(state: &mut GameState, data: &DesignData) {
    for agent in &mut state.free_agents {
        agent.play_week(REST, None, StaffEffects::default(), &mut state.rng, data);
    }
    state.free_agents.retain(|f| f.status == Status::Active);
}

/// The rival clubs' market. At the start of the off-season, each releases its
/// fighters of `release_age` and more worth less than its average fighter, and the
/// rookies arrive. Each club below `roster.wanted` fighters recruits (see
/// `rival_signings`); then each club with a place, keeping its `care_reserve`, offers
/// its `buy_percent` of their value for the player's listed fighters.
fn rival_market(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    if off_season_starts(state, data) {
        for rival in &mut state.rivals {
            let values: Vec<u64> = rival
                .fighters
                .iter()
                .filter(|f| f.status == Status::Active)
                .map(|f| u64::from(value(f, market)))
                .collect();
            let count = u64::try_from(values.len()).unwrap_or(u64::MAX);
            let average = values.iter().sum::<u64>().checked_div(count).unwrap_or(0);
            for fighter in &mut rival.fighters {
                if fighter.status == Status::Active
                    && fighter.age >= market.release_age
                    && u64::from(value(fighter, market)) < average
                {
                    state.free_agents.push(fighter.clone());
                    fighter.status = Status::Released;
                }
            }
        }
        for _ in 0..market.rookies {
            let rookie = market::rookie(&mut state.rng, data);
            state.free_agents.push(rookie);
        }
    }
    rival_signings(state, data);
    state.sale_offers.clear();
    for &i in &state.transfer_list {
        let Some(fighter) = state.fighters.get(i).filter(|f| f.status == Status::Active) else {
            continue;
        };
        let worth = value(fighter, market);
        for (k, rival) in state.rivals.iter().enumerate() {
            let Some(profile) = data.world.profile(k) else {
                continue;
            };
            let price = percent_of(worth, profile.buy_percent);
            let place = u32::try_from(rival.active()).is_ok_and(|n| n < data.world.roster.max);
            if place && rival.money - i64::from(price) >= i64::from(profile.care_reserve) {
                state.sale_offers.push(SaleOffer {
                    fighter: i,
                    rival: k,
                    price,
                });
            }
        }
    }
}

/// Each rival club below `roster.wanted` fighters in activity recruits until it has
/// them: the most valuable free agent if worth at least its weakest fighter; if not, it
/// first tries, once a week, to buy the most valuable fighter of another club worth
/// that much that it can pay at the seller's `sell_percent` keeping its own
/// `care_reserve`, if the fighter agrees (`market::refuses_to_join`, against the
/// buyer's best fame); failing that, it signs the free agent anyway.
fn rival_signings(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    let wanted = usize::try_from(data.world.roster.wanted).unwrap_or(0);
    for buyer in 0..state.rivals.len() {
        let mut tried = false;
        while state.rivals[buyer].active() < wanted {
            let active = || {
                state.rivals[buyer]
                    .fighters
                    .iter()
                    .filter(|f| f.status == Status::Active)
            };
            let weakest = active().map(|f| value(f, market)).min().unwrap_or(0);
            let agent = most_valuable(&state.free_agents, data);
            let suits = agent.is_some_and(|a| value(&state.free_agents[a], market) >= weakest);
            if !suits && !tried {
                tried = true;
                if buy_from_rival(state, buyer, weakest, data) {
                    continue;
                }
            }
            let Some(agent) = agent else { break };
            let signed = state.free_agents.remove(agent);
            state.rivals[buyer].fighters.push(signed);
        }
    }
}

/// Rival club `buyer` buys the most valuable fighter of another club worth at least
/// `floor` that it can pay, if the fighter agrees; returns whether it did.
fn buy_from_rival(state: &mut GameState, buyer: usize, floor: u32, data: &DesignData) -> bool {
    let market = &data.economy.market;
    let Some(reserve) = data.world.profile(buyer).map(|p| i64::from(p.care_reserve)) else {
        return false;
    };
    let money = state.rivals[buyer].money;
    let best = state
        .rivals
        .iter()
        .enumerate()
        .filter(|&(club, _)| club != buyer)
        .flat_map(|(club, rival)| {
            (0..rival.fighters.len()).map(move |fighter| RivalId { club, fighter })
        })
        .filter_map(|id| {
            let worth = value(state.rival_fighter(id)?, market);
            let price = state.asking_price(id, data)?;
            (worth >= floor && money - i64::from(price) >= reserve).then_some((worth, id, price))
        })
        .max_by_key(|&(worth, id, _)| (worth, Reverse(id)));
    let Some((_, id, price)) = best else {
        return false;
    };
    let fame = state.rivals[buyer]
        .fighters
        .iter()
        .filter(|f| f.status == Status::Active)
        .map(|f| f.fame)
        .max_by_key(|f| f.value())
        .unwrap_or_default();
    let fighter = &state.rivals[id.club].fighters[id.fighter];
    if market::refuses_to_join(&mut state.rng, fighter, fame, market) {
        return false;
    }
    let bought = fighter.clone();
    state.rivals[id.club].fighters[id.fighter].status = Status::Sold;
    state.rivals[id.club].money += i64::from(price);
    state.rivals[buyer].money -= i64::from(price);
    state.rivals[buyer].fighters.push(bought);
    true
}

/// Index of the most valuable of `fighters`, the first of equals.
fn most_valuable(fighters: &[CareerFighter], data: &DesignData) -> Option<usize> {
    (0..fighters.len()).max_by_key(|&a| (value(&fighters[a], &data.economy.market), Reverse(a)))
}

/// Sends the least valuable free agents away beyond `MarketRules::max_free_agents`.
fn trim_market(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    let most = usize::try_from(market.max_free_agents).unwrap_or(usize::MAX);
    while state.free_agents.len() > most {
        let Some(least) = (0..state.free_agents.len())
            .min_by_key(|&a| (value(&state.free_agents[a], market), Reverse(a)))
        else {
            break;
        };
        state.free_agents.remove(least);
    }
}

/// The colosseums' end of `week`: a title whose holder no longer fights goes to the
/// best ranked still fighting; a rival champion at its deadline defends the title
/// against the best-ranked other rival fighter, a club champion past it loses the
/// title. Those who no longer fight leave the rankings. Returns what happened to the
/// titles, with the colosseum's index.
fn end_colosseum_weeks(
    state: &mut GameState,
    week: u64,
    data: &DesignData,
) -> Vec<(usize, TitleNews)> {
    let mut titles = Vec::new();
    for c in 0..state.colosseums.len() {
        let rosters = Rosters {
            club: &state.fighters,
            rivals: &state.rivals,
        };
        let colosseum = &mut state.colosseums[c];
        let mut news: Vec<TitleNews> = colosseum
            .vacate_if_gone(week, rosters)
            .into_iter()
            .collect();
        news.extend(colosseum.strip(week, rosters, &data.world));
        let defense = match colosseum.champion {
            Some(Contender::Rival(champion)) if week >= colosseum.defense_deadline(&data.world) => {
                colosseum
                    .best_rival(rosters, Some(champion))
                    .map(|challenger| (champion, challenger))
            }
            _ => None,
        };
        if let Some((champion, challenger)) = defense {
            let result = rival_duel(
                &mut state.rivals,
                champion,
                challenger,
                &mut state.rng,
                data,
            );
            let (champion, challenger) = (Contender::Rival(champion), Contender::Rival(challenger));
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
            };
            news.extend(colosseum.vacate_if_gone(week, rosters));
        }
        let rosters = Rosters {
            club: &state.fighters,
            rivals: &state.rivals,
        };
        state.colosseums[c]
            .ranking
            .retain(|r| rosters.active(r.contender));
        titles.extend(news.into_iter().map(|n| (c, n)));
    }
    titles
}

/// A title defense between rival fighters `a`, the champion, and `b`, who take only
/// fame, record and death from it; returns `a`'s result.
fn rival_duel(
    rivals: &mut [Rival],
    a: RivalId,
    b: RivalId,
    rng: &mut ChaCha8Rng,
    data: &DesignData,
) -> FightResult {
    let rules = &data.fighter;
    let mut first = rivals[a.club].fighters[a.fighter].clone();
    let mut second = rivals[b.club].fighters[b.fighter].clone();
    let (_, _, arena) = first.duel(&second, rng, data);
    let (stronger, weaker) = (
        second.stronger_than(&first, rules),
        first.stronger_than(&second, rules),
    );
    let (outcome, _) = first.take_fame_and_death(&arena, FighterId(0), stronger, rules);
    second.take_fame_and_death(&arena, FighterId(1), weaker, rules);
    rivals[a.club].fighters[a.fighter] = first;
    rivals[b.club].fighters[b.fighter] = second;
    outcome.result
}

/// Records the title fight of the club's fighter `club` against `rival` in
/// `colosseum`, `result` being the club fighter's.
fn title_fight(
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

/// The rival fighters' challenges for the week to play: in each colosseum where it is
/// ranked, a club's fighter fit to fight is challenged by the rival fighter closest in
/// Elo, with the `challenge_percent` chance of that fighter's club.
fn receive_challenges(state: &mut GameState, data: &DesignData) {
    let rosters = Rosters {
        club: &state.fighters,
        rivals: &state.rivals,
    };
    for (i, fighter) in state.fighters.iter().enumerate() {
        let fit = suggested_plan(fighter, state.week).activity != Activity::Rest;
        if fighter.status != Status::Active || !fit {
            continue;
        }
        for (id, colosseum) in (0..).zip(&state.colosseums) {
            if let Some(rival) = colosseum.challenger(i, rosters)
                && let Some(profile) = data.world.profile(rival.club)
                && state.rng.random_range(0..100) < profile.challenge_percent
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

/// The rival clubs' week, after the club's fights. In each colosseum, the rival
/// fighters registered there and not met by the club's fighters are paired in Elo
/// order, each pair fighting a duel that moves both Elo; one left without an opponent
/// trains instead. The others rest or train as planned, those met by the club's
/// fighters end their week, and each club pays its fighters' costs and upkeep and
/// takes their receipts and prizes.
// ponytail: real duels until the statistical model of phase 5 resolves fights between
// rivals.
fn rival_weeks(
    state: &mut GameState,
    plans: &[Vec<WeekPlan>],
    fought: &[Vec<bool>],
    data: &DesignData,
) {
    let world = &data.world;
    let upkeep = i64::from(data.economy.upkeep);
    for (rival, met) in state.rivals.iter_mut().zip(fought) {
        for (fighter, &met) in rival.fighters.iter_mut().zip(met) {
            if fighter.status == Status::Active {
                rival.money -= upkeep;
                if met {
                    fighter.end_fought_week(&mut state.rng, data);
                }
            }
        }
    }
    let mut plans = plans.to_vec();
    let mut done = fought.to_vec();
    for (c, (colosseum, purses)) in state
        .colosseums
        .iter_mut()
        .zip(world.colosseums.iter().map(|r| &r.purses))
        .enumerate()
    {
        let here = Activity::Fight(ColosseumId(u32::try_from(c).unwrap_or(u32::MAX)));
        let mut waiting: Vec<(i32, RivalId)> = Vec::new();
        for (club, rival) in state.rivals.iter().enumerate() {
            for (fighter, f) in rival.fighters.iter().enumerate() {
                let id = RivalId { club, fighter };
                if f.status == Status::Active
                    && !done[club][fighter]
                    && plans[club][fighter].activity == here
                {
                    waiting.push((colosseum.elo(Contender::Rival(id), world), id));
                }
            }
        }
        waiting.sort_by_key(|&(elo, id)| (Reverse(elo), id));
        for pair in waiting.chunks(2) {
            let &[(_, a), (_, b)] = pair else {
                let (_, alone) = pair[0];
                plans[alone.club][alone.fighter].activity = Activity::Train;
                continue;
            };
            let mut second = state.rivals[b.club].fighters[b.fighter].clone();
            let first = &mut state.rivals[a.club].fighters[a.fighter];
            let receipts = i64::from(purses.receipts(first.fame, second.fame));
            let plan = plans[a.club][a.fighter];
            let report = first.play_week(
                plan,
                Some(&mut second),
                StaffEffects::default(),
                &mut state.rng,
                data,
            );
            second.end_fought_week(&mut state.rng, data);
            state.rivals[b.club].fighters[b.fighter] = second;
            done[a.club][a.fighter] = true;
            done[b.club][b.fighter] = true;
            let Some(fight) = report.and_then(|r| r.fight) else {
                continue;
            };
            colosseum.record(
                Contender::Rival(a),
                Contender::Rival(b),
                fight.result,
                world,
            );
            for (id, result) in [(a, fight.result), (b, opposite(fight.result))] {
                let prize = if result == FightResult::Won {
                    purses.win_prize
                } else {
                    0
                };
                let earned = receipts + i64::from(prize) - i64::from(purses.entry_fee);
                pay_rival(&mut state.rivals, id.club, earned);
            }
        }
    }
    for (club, rival) in state.rivals.iter_mut().enumerate() {
        for (fighter, f) in rival.fighters.iter_mut().enumerate() {
            if done[club].get(fighter).copied().unwrap_or(true) {
                continue;
            }
            let plan = plans[club][fighter];
            if let Some(report) =
                f.play_week(plan, None, StaffEffects::default(), &mut state.rng, data)
            {
                let training = if plan.activity == Activity::Train {
                    data.economy.training_cost
                } else {
                    0
                };
                rival.money -= i64::from(report.care_cost) + i64::from(training);
            }
        }
    }
}

/// Adds `amount` to the money of rival club `club`.
fn pay_rival(rivals: &mut [Rival], club: usize, amount: i64) {
    if let Some(rival) = rivals.get_mut(club) {
        rival.money += amount;
    }
}

/// Each fighter's wanted plan, `GameState::suggested_plan` without one, and the colosseum
/// and rival fighter of its title fight. Title fights are settled before anyone plays,
/// from the rankings as they stand; one the fighter may not have turns into rest, and so
/// does a challenge it may not have.
fn week_plans(
    state: &GameState,
    decisions: &Decisions,
    data: &DesignData,
) -> (Vec<WeekPlan>, Vec<Option<(usize, RivalId)>>) {
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
                Activity::Challenge(id, rival) => state.may_challenge(i, id, rival, data),
                _ => true,
            };
            if !allowed {
                plan.activity = Activity::Rest;
            }
            (plan, meeting)
        })
        .unzip()
}

/// The colosseum and the rival fighter met in a fight (`activity`): the one challenged
/// while still fighting (someone may have killed it earlier in the week), otherwise one
/// at random among those registered there (in `plans`) who have not `fought` yet, or
/// else among those ranked there, or else among anyone.
fn opponent(
    rng: &mut ChaCha8Rng,
    (colosseums, rivals): (&[Colosseum], &[Rival]),
    activity: Activity,
    plans: &[Vec<WeekPlan>],
    fought: &[Vec<bool>],
) -> Option<(usize, RivalId)> {
    let (id, chosen) = match activity {
        Activity::Fight(id) => (id, None),
        Activity::Challenge(id, rival) => (id, Some(rival)),
        _ => return None,
    };
    let c = usize::try_from(id.0).ok()?;
    let colosseum = colosseums.get(c)?;
    let active = |r: &RivalId| {
        rivals
            .get(r.club)
            .and_then(|k| k.fighters.get(r.fighter))
            .is_some_and(|f| f.status == Status::Active)
    };
    if let Some(rival) = chosen.filter(active) {
        return Some((c, rival));
    }
    let free = |r: &RivalId| {
        active(r)
            && !fought
                .get(r.club)
                .and_then(|f| f.get(r.fighter))
                .copied()
                .unwrap_or(true)
    };
    let everyone: Vec<RivalId> = rivals
        .iter()
        .enumerate()
        .flat_map(|(club, r)| (0..r.fighters.len()).map(move |fighter| RivalId { club, fighter }))
        .filter(free)
        .collect();
    let registered: Vec<RivalId> = everyone
        .iter()
        .copied()
        .filter(|r| {
            plans
                .get(r.club)
                .and_then(|p| p.get(r.fighter))
                .is_some_and(|p| p.activity == Activity::Fight(id))
        })
        .collect();
    let ranked: Vec<RivalId> = colosseum
        .ranking
        .iter()
        .filter_map(|r| match r.contender {
            Contender::Rival(id) if free(&id) => Some(id),
            _ => None,
        })
        .collect();
    let candidates = [registered, ranked, everyone]
        .into_iter()
        .find(|c| !c.is_empty())?;
    Some((c, pick_opponent(rng, &candidates)?))
}

/// The other side's result in a duel.
fn opposite(result: FightResult) -> FightResult {
    match result {
        FightResult::Won => FightResult::Lost,
        FightResult::Lost | FightResult::GaveUp => FightResult::Won,
        FightResult::Unfinished => FightResult::Unfinished,
    }
}

/// `plan` without what `money` cannot pay for: care first, at the price of the doctor's
/// `medicine`, then the activity, a training session costing `training_cost`.
fn affordable(
    mut plan: WeekPlan,
    fighter: &CareerFighter,
    money: i64,
    (training_cost, medicine): (u32, Medicine),
    data: &DesignData,
) -> WeekPlan {
    let mut left = money;
    if plan.care {
        let care = medicine.care_rules(&data.fighter.care);
        let cost = i64::from(fighter.care_cost(&care));
        if cost > left {
            plan.care = false;
        } else {
            left -= cost;
        }
    }
    let cost = match plan.activity {
        Activity::Rest => Some(0),
        Activity::Train => Some(training_cost),
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
    use crate::fighter::injury::{Injury, Location, Severity};
    use crate::rivals::Profile;

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    fn plan(activity: Activity, care: bool) -> Decisions {
        Decisions {
            plans: vec![WeekPlan { activity, care }],
            ..Decisions::default()
        }
    }

    fn moves(moves: &[Move]) -> Decisions {
        Decisions {
            plans: vec![REST],
            moves: moves.to_vec(),
            staff: Vec::new(),
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
    fn the_rival_clubs_open_with_the_colosseums_fighters() {
        let data = data();
        let state = GameState::new(7, &data);
        // Fifteen in each colosseum, shared out in turn: ten for each club.
        assert!(state.rivals.iter().all(|r| r.fighters.len() == 10));
        for colosseum in &state.colosseums {
            assert_eq!(colosseum.ranking.len(), 15);
            assert!(matches!(colosseum.champion, Some(Contender::Rival(_))));
        }
        let first = |c: usize| match state.colosseums[c].ranking[0].contender {
            Contender::Rival(id) => state.rival_fighter(id).unwrap().clone(),
            Contender::Club(_) => unreachable!(),
        };
        let total = |f: &CareerFighter| f.stats.to_array().iter().sum::<u32>();
        // Rome's are stronger.
        assert!(total(&first(1)) > total(&first(0)));
        assert_eq!(
            state.free_agents.len(),
            usize::try_from(data.economy.market.min_free_agents).unwrap()
        );
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
    fn a_trainer_is_paid_every_week_and_sets_the_session_s_price() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let trainer = data.economy.staff.trainer(3).unwrap();
        let mut train = plan(Activity::Train, false);
        train.staff = vec![StaffMove::Hire(Role::Trainer, 3)];
        let summary = next_week(&mut state, &train, &data);
        assert_eq!(summary.staff, [StaffMove::Hire(Role::Trainer, 3)]);
        assert_eq!(state.staff.trainer, Some(3));
        assert_eq!(summary.ledger[0].fighter, None);
        let salary = (Flow::Salary, -i64::from(trainer.salary));
        let session = (Flow::Training, -i64::from(trainer.training_cost));
        assert_eq!(flows(&summary)[..2], [salary, session]);
        // Paid on, without a new decision.
        let summary = next_week(&mut state, &plan(Activity::Rest, false), &data);
        assert_eq!(summary.staff, []);
        assert_eq!(flows(&summary)[0], salary);
        let mut dismiss = plan(Activity::Rest, false);
        dismiss.staff = vec![StaffMove::Dismiss(Role::Trainer)];
        let summary = next_week(&mut state, &dismiss, &data);
        assert_eq!(state.staff.trainer, None);
        assert!(
            flows(&summary)
                .iter()
                .all(|&(flow, _)| flow != Flow::Salary)
        );
    }

    #[test]
    fn a_trainer_is_hired_only_with_a_first_salary_at_a_level_on_offer() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let levels = u32::try_from(data.economy.staff.trainers.len()).unwrap();
        assert!(state.may_staff(StaffMove::Hire(Role::Trainer, 1), &data));
        assert!(!state.may_staff(StaffMove::Hire(Role::Trainer, 0), &data));
        assert!(!state.may_staff(StaffMove::Hire(Role::Trainer, levels + 1), &data));
        assert!(!state.may_staff(StaffMove::Dismiss(Role::Trainer), &data));
        state.money = 0;
        let mut hire = plan(Activity::Rest, false);
        hire.staff = vec![StaffMove::Hire(Role::Trainer, 1)];
        let summary = next_week(&mut state, &hire, &data);
        assert_eq!(summary.staff, []);
        assert_eq!(state.staff.trainer, None);
        // A trainer already there is paid even into debt.
        state.staff.trainer = Some(1);
        state.money = 0;
        next_week(&mut state, &plan(Activity::Rest, false), &data);
        assert!(state.money < 0);
        assert!(!state.may_staff(StaffMove::Hire(Role::Trainer, 2), &data));
    }

    #[test]
    fn a_trainer_spares_form_at_training() {
        let data = data();
        let mut alone = GameState::new(7, &data);
        let mut coached = alone.clone();
        coached.staff.trainer = Some(5);
        next_week(&mut alone, &plan(Activity::Train, false), &data);
        next_week(&mut coached, &plan(Activity::Train, false), &data);
        let spared = data.economy.staff.trainer(5).unwrap().coaching.form_spared;
        assert_eq!(
            coached.fighters[0].form.value(),
            alone.fighters[0].form.value() + spared
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
        assert_eq!(report.plan, REST);
        assert_eq!(state.money, -i64::from(data.economy.upkeep));
    }

    #[test]
    fn each_post_is_hired_once_a_week_and_paid_in_turn() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let staff = &data.economy.staff;
        let mut week = plan(Activity::Rest, false);
        week.staff = vec![
            StaffMove::Hire(Role::Recruiter, 2),
            StaffMove::Hire(Role::Doctor, 4),
            // A second change for the same post does not count.
            StaffMove::Hire(Role::Recruiter, 5),
        ];
        let summary = next_week(&mut state, &week, &data);
        assert_eq!(summary.staff, week.staff[..2]);
        assert_eq!(
            (
                state.staff.trainer,
                state.staff.doctor,
                state.staff.recruiter
            ),
            (None, Some(4), Some(2))
        );
        let salary = |s: u32| (Flow::Salary, -i64::from(s));
        assert_eq!(
            flows(&summary)[..2],
            [
                salary(staff.doctor(4).unwrap().salary),
                salary(staff.recruiter(2).unwrap().salary)
            ]
        );
        assert_eq!(state.scouting(&data), staff.recruiter(2).unwrap().scouting);
    }

    #[test]
    fn the_doctor_sets_the_price_of_care() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.staff.doctor = Some(5);
        state.fighters[0].injuries.push(Injury {
            location: Location::Body,
            severity: Severity::Severe,
            weeks_left: 8,
        });
        let medicine = data.economy.staff.doctor(5).unwrap().medicine;
        let price = state.fighters[0].care_cost(&state.care_rules(&data));
        let usual = state.fighters[0].care_cost(&data.fighter.care);
        assert_eq!(price, usual * medicine.care_cost_percent / 100);
        let summary = next_week(&mut state, &plan(Activity::Rest, true), &data);
        assert_eq!(flows(&summary)[1], (Flow::Care, -i64::from(price)));
        let kept = data.fighter.care.weeks_kept_percent - medicine.care_weeks_off_percent;
        // Care, then a week of healing.
        assert_eq!(
            state.fighters[0].injuries[0].weeks_left,
            (8 * kept).div_ceil(100) - 1
        );
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
        let price = state.fighters[0].care_cost(&data.fighter.care);
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
    fn a_fight_meets_a_rival_fighter_and_moves_both_elo() {
        let data = data();
        let rules = &data.world;
        let mut state = GameState::new(7, &data);
        let rome = ColosseumId(1);
        let summary = next_week(&mut state, &plan(Activity::Fight(rome), false), &data);
        let fight = summary.reports[0].as_ref().unwrap().fight.as_ref().unwrap();
        let fee = i64::from(rules.colosseums[1].purses.entry_fee);
        assert_eq!(flows(&summary)[0], (Flow::EntryFee, -fee));
        let rome = &state.colosseums[1];
        let club = rome.elo(Contender::Club(0), rules);
        assert_eq!(club > rules.elo_start, fight.result == FightResult::Won);
        let opponent = state
            .rivals
            .iter()
            .flat_map(|r| &r.fighters)
            .position(|f| f.potential == fight.opponent.potential);
        assert!(opponent.is_some());
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

    /// The rival fighter ranked `rank` in Rome.
    fn ranked(state: &GameState, rank: usize) -> RivalId {
        let Contender::Rival(id) = state.colosseums[1].ranking[rank].contender else {
            panic!("a rival fighter at rank {rank}");
        };
        id
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
    fn rival_champions_defend_without_the_player() {
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
    fn a_challenge_meets_the_chosen_rival_if_its_club_accepts() {
        let data = data();
        let rules = &data.world;
        let mut state = top_of_rome(&data);
        let rome = ColosseumId(1);
        let rival = ranked(&state, 7);
        assert!(state.may_challenge(0, rome, rival, &data));
        let before = state.rival_fighter(rival).unwrap().clone();
        let summary = next_week(
            &mut state,
            &plan(Activity::Challenge(rome, rival), false),
            &data,
        );
        let report = summary.reports[0].as_ref().unwrap();
        assert_eq!(report.plan.activity, Activity::Challenge(rome, rival));
        let fight = report.fight.as_ref().unwrap();
        assert_eq!(fight.opponent, before);
        let fee = i64::from(rules.colosseums[1].purses.entry_fee);
        assert_eq!(flows(&summary)[0], (Flow::EntryFee, -fee));
        if state.rival_fighter(rival).unwrap().status == Status::Active {
            assert_ne!(
                state.colosseums[1].elo(Contender::Rival(rival), rules),
                rules.elo_start
            );
        }
        // Far ahead in Elo, no rival fighter accepts: a week of rest.
        let other = ranked(&state, 3);
        state.colosseums[1].ranking[0].elo = 2000;
        state.challenges.clear();
        if state.colosseums[1].ranking[0].contender == Contender::Club(0) {
            assert!(!state.may_challenge(0, rome, other, &data));
            let summary = next_week(
                &mut state,
                &plan(Activity::Challenge(rome, other), false),
                &data,
            );
            assert_eq!(
                summary.reports[0].as_ref().unwrap().plan.activity,
                Activity::Rest
            );
        }
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
            rival: ranked(&state, 2),
        };
        // Received, so it may be fought however far ahead the fighter is.
        state.challenges = vec![received];
        assert!(state.may_challenge(0, ColosseumId(1), received.rival, &data));
        state.fighters[0].form = crate::fighter::form::Form::full();
        assert_eq!(
            state.suggested_plan(0, &data).activity,
            Activity::Challenge(ColosseumId(1), received.rival)
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
    fn rival_fighters_challenge_the_ranked_fighters_fit_to_fight() {
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
                && Some(ch.rival) == state.colosseums[1].challenger(0, state.rosters())));
            received += state.challenges.len();
        }
        // Between 10 % and 40 % each week, by the challenger's club.
        assert!(
            (1..=14).contains(&received),
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
        let agent = state.free_agents[2].clone();
        let summary = next_week(&mut state, &moves(&[Move::Sign(2)]), &data);
        assert_eq!(summary.market, [MarketNews::Signed(1)]);
        assert_eq!(state.fighters.len(), 2);
        assert!(summary.reports[1].is_some());
        assert_eq!(state.fighters[1].status, Status::Active);
        assert_eq!(state.fighters[1].potential, agent.potential);
        assert!(
            !state
                .free_agents
                .iter()
                .any(|f| f.potential == agent.potential)
        );
    }

    #[test]
    fn a_full_club_signs_no_one() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let hire = moves(&[Move::Sign(0)]);
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
        // Several signings in a week sign the free agents chosen.
        let mut state = GameState::new(7, &data);
        let (first, third) = (state.free_agents[0].clone(), state.free_agents[2].clone());
        next_week(&mut state, &moves(&[Move::Sign(0), Move::Sign(2)]), &data);
        assert_eq!(state.fighters[1].potential, third.potential);
        assert_eq!(state.fighters[2].potential, first.potential);
    }

    #[test]
    fn a_rival_fighter_is_bought_by_negotiation() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.money = 10_000;
        let target = RivalId {
            club: 2,
            fighter: 0,
        };
        let asking = state.asking_price(target, &data).unwrap();
        let wanted = state.rival_fighter(target).unwrap().clone();
        // Far too low: refused.
        let summary = next_week(
            &mut state,
            &moves(&[Move::Offer(target, asking / 2)]),
            &data,
        );
        assert_eq!(summary.market, [MarketNews::ClubRefused(target)]);
        // Close: the club asks its price.
        let asking = state.asking_price(target, &data).unwrap();
        let close = asking * 9 / 10;
        let summary = next_week(&mut state, &moves(&[Move::Offer(target, close)]), &data);
        let Some((countered, asked)) = state.counter_offer else {
            panic!("a counter-offer, not {:?}", summary.market);
        };
        assert_eq!(summary.market, [MarketNews::Countered(target, asked)]);
        assert_eq!(countered, target);
        // Above its price (which a week may move): the club accepts, then the fighter
        // decides.
        let before = state.money;
        let asked = asked + 500;
        let summary = next_week(&mut state, &moves(&[Move::Offer(target, asked)]), &data);
        match summary.market[..] {
            [
                MarketNews::Bought {
                    from,
                    fighter,
                    price,
                },
            ] => {
                assert_eq!((from, fighter, price), (target, 1, asked));
                assert_eq!(state.fighters[1].potential, wanted.potential);
                assert_eq!(state.rival_fighter(target).unwrap().status, Status::Sold);
                assert!(flows(&summary).contains(&(Flow::Purchase, -i64::from(asked))));
                assert!(state.money < before);
            }
            [MarketNews::FighterRefused(from)] => assert_eq!(from, target),
            ref news => panic!("bought or refused, not {news:?}"),
        }
        // No offer beyond the club's money.
        state.money = 10;
        assert!(!state.may_move(Move::Offer(target, 11), &data));
    }

    #[test]
    fn a_listed_fighter_fights_on_until_sold() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let list = moves(&[Move::List(0)]);
        let train = Decisions {
            plans: vec![WeekPlan {
                activity: Activity::Train,
                care: false,
            }],
            moves: list.moves.clone(),
            staff: Vec::new(),
        };
        // A merchant in debt cannot keep its reserve.
        state.rivals[2].money = -1000;
        let summary = next_week(&mut state, &train, &data);
        let report = summary.reports[0].as_ref().unwrap();
        assert_eq!(report.plan.activity, Activity::Train);
        assert_eq!(state.transfer_list, [0]);
        // Each club with a place and its reserve after paying makes an offer.
        let offers: Vec<usize> = state.sale_offers.iter().map(|o| o.rival).collect();
        assert_eq!(offers, [0, 1]);
        let worth = value(&state.fighters[0], &data.economy.market);
        let aggressive = data.world.profiles.get(Profile::Aggressive);
        let offer = *state.sale_offers.iter().find(|o| o.rival == 1).unwrap();
        assert_eq!(offer.price, percent_of(worth, aggressive.buy_percent));
        // A full club makes none.
        let mut full = state.clone();
        let extra = full.rivals[1].fighters[0].clone();
        full.rivals[1].fighters.extend([extra.clone(), extra]);
        full.sale_offers.clear();
        next_week(&mut full, &Decisions::default(), &data);
        assert!(full.sale_offers.iter().all(|o| o.rival != 1));
        // Accepted: sold, and the fighter joins its new club.
        let sold = state.fighters[0].clone();
        let accept = moves(&[Move::AcceptOffer {
            fighter: 0,
            rival: 1,
        }]);
        let summary = next_week(&mut state, &accept, &data);
        assert_eq!(summary.market, [MarketNews::Sold(offer)]);
        assert_eq!(flows(&summary), [(Flow::Sale, i64::from(offer.price))]);
        assert_eq!(summary.reports, [None]);
        assert_eq!(state.fighters[0].status, Status::Sold);
        assert_eq!(state.transfer_list, Vec::<usize>::new());
        assert!(
            state.rivals[1]
                .fighters
                .iter()
                .any(|f| f.potential == sold.potential)
        );
    }

    #[test]
    fn a_released_fighter_becomes_a_free_agent() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let released = state.fighters[0].clone();
        let summary = next_week(&mut state, &moves(&[Move::Release(0)]), &data);
        assert_eq!(summary.market, [MarketNews::Released(0)]);
        assert_eq!(summary.ledger, []);
        assert_eq!(state.fighters[0].status, Status::Released);
        let on_market = state
            .free_agents
            .iter()
            .chain(state.rivals.iter().flat_map(|r| &r.fighters))
            .any(|f| f.potential == released.potential);
        assert!(on_market);
        assert!(!state.may_move(Move::Release(0), &data));
        assert!(!state.may_move(Move::List(0), &data));
    }

    #[test]
    fn observing_a_fighter_reveals_more_of_it() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let target = RivalId {
            club: 1,
            fighter: 4,
        };
        let observe = moves(&[Move::Observe(Target::Rival(target))]);
        next_week(&mut state, &observe, &data);
        next_week(&mut state, &observe, &data);
        assert_eq!(state.rival_fighter(target).unwrap().observed_weeks, 2);
    }

    #[test]
    fn rookies_arrive_and_old_fighters_worth_less_are_released_in_the_off_season() {
        let data = data();
        let market = &data.economy.market;
        let mut state = GameState::new(3, &data);
        state.fighters[0].status = Status::Retired;
        let calendar = &data.calendar;
        let off_season = u64::from(calendar.competition_weeks + calendar.great_event_weeks + 1);
        while state.week < off_season {
            next_week(&mut state, &Decisions::default(), &data);
        }
        // Two old fighters of the cautious club: the best stays, the worst goes.
        let roster = &mut state.rivals[0].fighters;
        let active: Vec<usize> = (0..roster.len())
            .filter(|&j| roster[j].status == Status::Active)
            .collect();
        let worth = |f: &CareerFighter| value(f, market);
        let best = *active.iter().max_by_key(|&&j| worth(&roster[j])).unwrap();
        let worst = *active.iter().min_by_key(|&&j| worth(&roster[j])).unwrap();
        for j in [best, worst] {
            roster[j].age = market.release_age;
            roster[j].weeks_this_year = 0;
        }
        next_week(&mut state, &Decisions::default(), &data);
        let status = |j: usize| state.rivals[0].fighters[j].status;
        assert_ne!(status(best), Status::Released);
        assert_eq!(status(worst), Status::Released);
        let rookies = state
            .free_agents
            .iter()
            .chain(state.rivals.iter().flat_map(|r| &r.fighters))
            .filter(|f| f.age == market.rookie_age && f.status == Status::Active)
            .count();
        assert!(rookies >= 8, "{rookies} rookies");
        assert!(state.free_agents.len() <= usize::try_from(market.max_free_agents).unwrap());
    }

    #[test]
    fn a_rival_club_buys_from_another_when_no_free_agent_suits_it() {
        let data = data();
        let market = &data.economy.market;
        let mut state = GameState::new(7, &data);
        // Only weak free agents, and the cautious club rich and one fighter short.
        for agent in &mut state.free_agents {
            agent.stats = agent.stats.map(|_| 1);
        }
        state.rivals[0].money = 100_000;
        state.rivals[0].fighters[0].status = Status::Dead;
        let mut bought = None;
        for _ in 0..20 {
            let before = state.clone();
            rival_signings(&mut state, &data);
            let newcomer = state.rivals[0].fighters.last().unwrap().potential;
            let from = (1..state.rivals.len())
                .flat_map(|club| {
                    let fighters = &state.rivals[club].fighters;
                    (0..fighters.len()).map(move |fighter| RivalId { club, fighter })
                })
                .find(|&id| {
                    let f = state.rival_fighter(id).unwrap();
                    f.status == Status::Sold && f.potential == newcomer
                });
            if let Some(id) = from {
                bought = Some((before, id));
                break;
            }
            // The fighter refused: again, with other draws.
            state = before;
            state.rng = ChaCha8Rng::seed_from_u64(state.rng.random());
        }
        let (before, id) = bought.expect("a purchase in 20 tries");
        let price = before.asking_price(id, &data).unwrap();
        let fighter = before.rival_fighter(id).unwrap();
        // The most valuable of the other clubs, paid at its club's price.
        let best = (1..before.rivals.len())
            .flat_map(|club| before.rivals[club].fighters.iter())
            .filter(|f| f.status == Status::Active)
            .map(|f| value(f, market))
            .max()
            .unwrap();
        assert_eq!(value(fighter, market), best);
        assert_eq!(state.rivals[0].money, 100_000 - i64::from(price));
        assert_eq!(state.rivals[0].active(), 10);
        // The seller fills its place again.
        assert_eq!(state.rivals[id.club].active(), 10);
        // A club that cannot pay signs the free agent anyway.
        let mut poor = GameState::new(7, &data);
        for agent in &mut poor.free_agents {
            agent.stats = agent.stats.map(|_| 1);
        }
        poor.rivals[0].money = 0;
        poor.rivals[0].fighters[0].status = Status::Dead;
        let agents = poor.free_agents.len();
        rival_signings(&mut poor, &data);
        assert_eq!(poor.free_agents.len(), agents - 1);
        assert_eq!(poor.rivals[0].active(), 10);
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
        }
        let wanted = usize::try_from(data.world.roster.wanted).unwrap();
        for (rival, before) in state.rivals.iter().zip(&start.rivals) {
            // They tire, get hurt, die or retire, and the club signs free agents.
            assert!(rival.fighters.len() > before.fighters.len());
            assert!(rival.active() >= wanted);
            assert!(rival.fighters.iter().any(|f| f.form.value() < 100));
            assert_ne!(rival.money, before.money);
        }
    }

    #[test]
    fn rival_fighters_follow_their_clubs_rules_and_the_club_meets_those_registered() {
        let data = data();
        let mut state = GameState::new(3, &data);
        assert_eq!(data.world.rivals[0].profile, Profile::Cautious);
        assert_eq!(data.world.rivals[1].profile, Profile::Aggressive);
        state.rivals[0].fighters[0].form = crate::fighter::form::Form::new(50);
        state.rivals[1].fighters[0].form = crate::fighter::form::Form::new(50);
        let plans = state.rival_plans(&data);
        assert_eq!(plans[0][0].activity, Activity::Rest);
        assert!(matches!(plans[1][0].activity, Activity::Fight(_)));
        // Only one registered in Capua and not fought yet: the club meets it.
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let mut only: Vec<Vec<WeekPlan>> = state
            .rivals
            .iter()
            .map(|r| vec![REST; r.fighters.len()])
            .collect();
        let capua = Activity::Fight(ColosseumId(0));
        only[1][0].activity = capua;
        let mut fought: Vec<Vec<bool>> = only.iter().map(|p| vec![false; p.len()]).collect();
        let world = (&state.colosseums[..], &state.rivals[..]);
        let one = RivalId {
            club: 1,
            fighter: 0,
        };
        assert_eq!(
            opponent(&mut rng, world, capua, &only, &fought),
            Some((0, one))
        );
        let four = RivalId {
            club: 0,
            fighter: 4,
        };
        let challenge = Activity::Challenge(ColosseumId(0), four);
        assert_eq!(
            opponent(&mut rng, world, challenge, &only, &fought),
            Some((0, four))
        );
        fought[1][0] = true;
        assert!(opponent(&mut rng, world, capua, &only, &fought).is_some_and(|(_, r)| r != one));
    }
}
