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
use crate::combat::ArenaKind;
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction, Wealth};
use crate::fighter::care::CareRules;
use crate::fighter::career::{
    Activity, CareerFighter, Status, WeekPlan, WeekReport, suggested_plan,
};
use crate::fighter::equipment::Refused;
use crate::fighter::fame::{Fame, FightResult};
use crate::fighter::form::FormEvent;
use crate::fighter::melee::{Entrant, MeleeFight, play_melee};
use crate::fighter::percent_of;
use crate::market::{self, MarketNews, Move, SaleOffer, Target, estimated_potential, value};
use crate::rivals::{self, Rival, RivalId};
use crate::staff::{
    self, Candidate, DoctorRules, Medicine, RecruiterRules, Scouting, Staff, StaffEffects,
    StaffMove, StaffNews, TrainerRules,
};

const REST: WeekPlan = WeekPlan {
    activity: Activity::Rest,
    care: false,
    refused: Refused::NONE,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    /// Number of the next week to play, from 1.
    pub week: u64,
    /// The club's money; below 0 when upkeep ran it into debt.
    pub money: i64,
    /// Chosen at the start, then changing with the money at each off-season
    /// (`Wealth::next`); sets the patron's gift.
    pub wealth: Wealth,
    /// The player's club; a fighter's index is its place here. One who leaves keeps
    /// its place, with its status.
    pub fighters: Vec<CareerFighter>,
    /// In the order of `data/world.ron`.
    pub colosseums: Vec<Colosseum>,
    /// The rival clubs and their fighters, in the order of `data/world.ron`.
    pub rivals: Vec<Rival>,
    /// The rival fighters' challenges to the club's fighters, to accept or refuse this week.
    pub challenges: Vec<Challenge>,
    /// Everyone who came to the market, signed for nothing. The free agents are those
    /// still active, never fewer than `MarketRules::min_free_agents` at the start of a
    /// week; like the clubs' fighters, one who leaves keeps its place, with its status:
    /// `Sold` once signed, `Retired` if it gave up waiting for a club.
    pub free_agents: Vec<CareerFighter>,
    /// The club's fighters on the transfer list, by index: they fight on for the club
    /// until the player accepts an offer.
    pub transfer_list: Vec<usize>,
    /// The rival clubs' offers for them, open this week.
    pub sale_offers: Vec<SaleOffer>,
    /// The price a rival club asked instead of the player's last offer, open this week.
    pub counter_offer: Option<(RivalId, u32)>,
    pub staff: Staff,
    /// The staff candidates on the market, for the club and the rival clubs alike;
    /// one hired leaves it.
    pub staff_market: Vec<Candidate>,
    /// The only source of chance after the start: same seed and decisions, same game.
    pub rng: ChaCha8Rng,
}

impl GameState {
    /// A new game from `seed` with a modest club (see `with_wealth`).
    #[must_use]
    pub fn new(seed: u64, data: &DesignData) -> Self {
        Self::with_wealth(seed, Wealth::Modest, data)
    }

    /// A new game from `seed`: a club of `wealth` with one generated fighter, the same
    /// one `CareerFighter::random` draws first from that seed, its wealth's starting
    /// money, the rival clubs with their fighters, the colosseums and the free agents.
    #[must_use]
    pub fn with_wealth(seed: u64, wealth: Wealth, data: &DesignData) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let fighters = vec![CareerFighter::random(&mut rng, data)];
        let mut state = Self {
            week: 1,
            money: i64::from(data.economy.wealth.get(wealth).starting_money),
            wealth,
            fighters,
            colosseums: Vec::new(),
            rivals: Vec::new(),
            challenges: Vec::new(),
            free_agents: Vec::new(),
            transfer_list: Vec::new(),
            sale_offers: Vec::new(),
            counter_offer: None,
            staff: Staff::default(),
            staff_market: Vec::new(),
            rng,
        };
        state.open_world(data);
        state
    }

    /// Opens the rival clubs and the colosseums of `data` the game does not have yet:
    /// all of them in a new game, those added to `data/world.ron` since an older save.
    /// A new rival club starts with its wealth and that wealth's starting money; a new
    /// colosseum with its `residents`, fighters as strong as its prestige shared out
    /// among the rival clubs in turn, all ranked there. Then brings the free agents up
    /// to their fewest, and the staff candidates up to `StaffMarketRules::candidates`
    /// for each post.
    pub fn open_world(&mut self, data: &DesignData) {
        let opened = self.rivals.len();
        let wealth = &data.economy.wealth;
        self.rivals
            .extend(data.world.rivals.iter().skip(opened).map(|rules| Rival {
                money: i64::from(wealth.get(rules.wealth).starting_money),
                fighters: Vec::new(),
                staff: Staff::default(),
                wealth: rules.wealth,
            }));
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
        while self.available_free_agents() < fewest {
            let agent = market::free_agent(&mut self.rng, data);
            self.free_agents.push(agent);
        }
        staff::fill_market(&mut self.staff_market, &data.economy.staff, &mut self.rng);
    }

    /// Every fighter in the game, for the colosseums.
    #[must_use]
    pub fn rosters(&self) -> Rosters<'_> {
        Rosters {
            club: &self.fighters,
            rivals: &self.rivals,
            free_agents: &self.free_agents,
        }
    }

    /// The free agents on the market: neither signed, retired nor dead.
    #[must_use]
    pub fn available_free_agents(&self) -> usize {
        self.free_agents
            .iter()
            .filter(|f| f.status == Status::Active)
            .count()
    }

    /// Gives the Elo and titles of `from` to `to` in every colosseum: the same fighter,
    /// gone to another club or to the market.
    fn transfer(&mut self, from: Contender, to: Contender) {
        for colosseum in &mut self.colosseums {
            colosseum.transfer(from, to);
        }
    }

    /// The rival fighters' and the free agents' plans; in the season's opening week,
    /// each one that would fight a duel enters that colosseum's melee instead.
    fn outsider_plans(&self, data: &DesignData) -> OutsiderPlans {
        let mut plans = OutsiderPlans {
            rivals: self.rival_plans(data),
            free_agents: self.free_agent_plans(data),
        };
        if self.opens_season(data) {
            for plan in plans
                .rivals
                .iter_mut()
                .flatten()
                .chain(&mut plans.free_agents)
            {
                if let Activity::Fight(id) = plan.activity {
                    plan.activity = Activity::Melee(id);
                }
            }
        }
        plans
    }

    /// Whether the week to play opens the season: each colosseum's melee, which makes
    /// the season's first ranking.
    #[must_use]
    pub fn opens_season(&self, data: &DesignData) -> bool {
        self.date(data).opens_season()
    }

    /// The chance, from 0 to 1, Elo gives `contender` in each colosseum, in their order
    /// (`Colosseum::chance`).
    fn chances(&self, contender: Contender, data: &DesignData) -> Vec<f64> {
        self.colosseums
            .iter()
            .map(|c| c.chance(contender, self.rosters(), &data.world))
            .collect()
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
                            let chances = self.chances(me, data);
                            let rules = (profile, &rival.staff);
                            rivals::plan(f, &chances, &mut left, rules, data)
                        }
                        _ => REST,
                    })
                    .collect()
            })
            .collect()
    }

    /// Each free agent's plan this week, in market order: it plays by the free agents'
    /// profile (`WorldRules::free_agents`) as a manager without money, so it fights
    /// only where entry is free, to show its worth, and is neither trained nor cared
    /// for (`rivals::plan`).
    #[must_use]
    pub fn free_agent_plans(&self, data: &DesignData) -> Vec<WeekPlan> {
        let profile = data.world.profiles.get(data.world.free_agents);
        (0..)
            .zip(&self.free_agents)
            .map(|(a, f)| {
                if f.status != Status::Active {
                    return REST;
                }
                let chances = self.chances(Contender::FreeAgent(a), data);
                rivals::plan(f, &chances, &mut 0, (profile, &Staff::default()), data)
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
        // The season opens with the melee where it would have fought.
        if let (true, Activity::Fight(id)) = (self.opens_season(data), plan.activity) {
            plan.activity = Activity::Melee(id);
        }
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

    /// The colosseum and the outsider the club's fighter `i` meets if it fights for the
    /// title of colosseum `id`; `None` if it may not (see `Colosseum::title_opponent`).
    #[must_use]
    pub fn title_opponent(
        &self,
        i: usize,
        id: ColosseumId,
        data: &DesignData,
    ) -> Option<(usize, Contender)> {
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
            Move::Observe(Target::FreeAgent(a)) => active(self.free_agents.get(a)),
            Move::Observe(Target::Rival(id)) => active(self.rival_fighter(id)),
            Move::Sign(a) => active(self.free_agents.get(a)) && self.can_hire(data),
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

    #[must_use]
    pub fn trainer(&self) -> Option<&TrainerRules> {
        self.staff.trainer()
    }

    #[must_use]
    pub fn doctor(&self) -> Option<&DoctorRules> {
        self.staff.doctor()
    }

    #[must_use]
    pub fn recruiter(&self) -> Option<&RecruiterRules> {
        self.staff.recruiter()
    }

    /// What the club's trainer and doctor bring to its fighters' weeks.
    #[must_use]
    pub fn staff_effects(&self) -> StaffEffects {
        self.staff.effects()
    }

    /// What the club's recruiter shows of the fighters out of the club.
    #[must_use]
    pub fn scouting(&self) -> Scouting {
        self.recruiter().map(|r| r.scouting).unwrap_or_default()
    }

    /// Care prices and effect, set by the doctor.
    #[must_use]
    pub fn care_rules(&self, data: &DesignData) -> CareRules {
        self.staff_effects().medicine.care_rules(&data.fighter.care)
    }

    /// The price of a training session, set by the trainer.
    #[must_use]
    pub fn training_cost(&self, data: &DesignData) -> u32 {
        self.staff.training_cost(data.economy.training_cost)
    }

    /// Whether `m` may be made now: a candidate on the market hired with the money for
    /// a first salary, paid as many times as the club's fighters make it
    /// (`StaffRules::salary_shares`), or a post held emptied.
    #[must_use]
    pub fn may_staff(&self, m: StaffMove, data: &DesignData) -> bool {
        let money = self.money / i64::from(self.salary_shares(data));
        staff::may_make(&self.staff, &self.staff_market, money, m)
    }

    /// How many times the club pays each salary this week, for its fighters in activity.
    #[must_use]
    pub fn salary_shares(&self, data: &DesignData) -> u32 {
        data.economy.staff.salary_shares(self.active_fighters())
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
    pub staff: Vec<StaffNews>,
    /// The club's new wealth, when it changed at the start of the off-season.
    pub wealth: Option<Wealth>,
}

/// Plays one week for the whole club: the market moves and the change of staff before
/// the week, the staff's salaries and the patron's gift, then each fighter's week in
/// club order, paid for, then the week of the rival clubs and the free agents, the
/// answer to the player's offer, each club's new wealth at the start of the off-season
/// and the rival clubs' market, then the colosseums' titles, and moves to the next
/// week. A fight is against an outsider, a rival fighter or a free agent, and
/// moves both Elo; a title fight is the only way a title changes hands, and the
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
            let plan = affordable(wanted[i], fighter, state.money, costs, data);
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
            let report = fighter.play_week(plan, opponent, effects, &mut state.rng, data);
            if let Some(report) = &report {
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
    let refusals = refusals(state, &reports, data);
    outsider_weeks(state, plans, &fought, data);
    market.extend(answer_offer(state, &decisions.moves, data, &mut ledger));
    let wealth = end_week(state, &mut titles, data);
    WeekSummary {
        reports,
        ledger,
        titles,
        refusals,
        market,
        staff,
        wealth,
    }
}

/// What the week `report` of the club's fighter `i`, of `fame` before it, costs and
/// brings, upkeep apart: care, a training session at `training`, and its fight against
/// the outsider of `meeting` in that colosseum: a title fight (`titles`, the champion
/// bonus for a defense survived), or else a fight moving both Elo for the entry fee;
/// receipts, and the prize to the winner. The rival fighter's club pays and earns the
/// same; a free agent fights for its name alone. The club's money moves into `ledger`.
fn settle_week(
    state: &mut GameState,
    (i, fame, training): (usize, Fame, u32),
    (report, meeting): (&WeekReport, Option<(usize, Contender)>),
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
    let (Some(fight), Some((c, rival))) = (&report.fight, meeting) else {
        return;
    };
    let Some(rules) = data.world.colosseums.get(c) else {
        return;
    };
    let purses = &rules.purses;
    let club = Contender::Club(i);
    let colosseum = &mut state.colosseums[c];
    let mut fee = 0;
    if let Activity::TitleFight(_) = report.plan.activity {
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
    let theirs = fight.opponent().map_or(Fame::default(), |o| o.fame);
    let receipts = i64::from(purses.receipts(fame, theirs));
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

/// Fewest fighters in a melee, below which it is called off.
const MELEE_MIN_FIGHTERS: usize = 3;

/// The season-opening melees, before the club's other fights, one per colosseum (see
/// [`melee_lineup`] for who fights, [`settle_melee`] for what it pays). The outsiders
/// who fought are added to `fought`, their week to end with the others'; the club's
/// fighters' weeks are returned, in club order, and the club's money moves are in
/// `ledger`.
fn melees(
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
        let played = melee_entries(state, &entries, (wanted, rules.arena), effects, data);
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
    let fee = world.colosseums.get(c)?.purses.entry_fee;
    let most = usize::try_from(data.combat.melee.max_fighters).unwrap_or(usize::MAX);
    let here = Activity::Melee(ColosseumId(u32::try_from(c).ok()?));
    // The club's money promised to fees and care, before anyone is paid.
    let mut promised = 0;
    let mut club = Vec::new();
    for (i, f) in state.fighters.iter().enumerate() {
        if wanted[i].activity != here || f.status != Status::Active {
            continue;
        }
        let plan = affordable(wanted[i], f, state.money - promised, costs, data);
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
/// [`melee_entries`] returns it: everyone's Elo by place (`Colosseum::record_places`);
/// each fighter's club pays the fee and takes the receipts of a melee of that size, the
/// last one standing's the melee prize, a free agent paying and earning nothing; the
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
    let Some(purses) = world.colosseums.get(c).map(|r| &r.purses) else {
        return;
    };
    let places: Vec<(Contender, u32)> = entries
        .iter()
        .copied()
        .zip(melee.places.iter().copied())
        .collect();
    if let Some(colosseum) = state.colosseums.get_mut(c) {
        colosseum.record_places(&places, world);
    }
    let fee = i64::from(purses.entry_fee);
    for (k, &entry) in entries.iter().enumerate() {
        let side = &melee.sides[k];
        let receipts = i64::from(purses.melee_receipts(melee.before[k].fame, entries.len()));
        let prize = if side.result == FightResult::Won {
            i64::from(purses.melee_prize)
        } else {
            0
        };
        match entry {
            Contender::Club(i) => {
                let Some(mut report) = club_weeks[k].take() else {
                    continue;
                };
                for (flow, amount) in [
                    (Flow::Care, -i64::from(report.care_cost)),
                    (Flow::EntryFee, -fee),
                    (Flow::Receipts, receipts),
                    (Flow::WinPrize, prize),
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
                pay_rival(&mut state.rivals, id.club, receipts + prize - fee);
                fought.push(entry);
            }
            Contender::FreeAgent(_) => fought.push(entry),
        }
    }
}

/// The melee of `entries`, on the floor of `kind`, played: each club's fighter begins
/// its week following its `wanted` plan (care, paid by the caller), then everyone
/// fights, and each one goes back where it came from. Returns the melee and, for each
/// entry, the club's fighter's week begun; `None` if an entry is nobody.
fn melee_entries(
    state: &mut GameState,
    entries: &[Contender],
    (wanted, kind): (&[WeekPlan], ArenaKind),
    effects: StaffEffects,
    data: &DesignData,
) -> Option<(MeleeFight, Vec<Option<WeekReport>>)> {
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
            });
        } else {
            fighters.push(outsider(&state.rivals, &state.free_agents, entry)?.clone());
            entrants.push(Entrant {
                refused: Refused::NONE,
                medicine: outsider_effects(&state.rivals, entry).medicine,
            });
            weeks.push(None);
        }
    }
    let melee = play_melee(&mut fighters, &entrants, kind, &mut state.rng, data);
    for (&entry, f) in entries.iter().zip(fighters) {
        match entry {
            Contender::Club(i) => state.fighters[i] = f,
            outsider => put_outsider(state, outsider, f),
        }
    }
    Some((melee, weeks))
}

/// Shuffles `items` on `rng` (Fisher-Yates on u32 draws, the same on every platform).
fn shuffle<T>(rng: &mut ChaCha8Rng, items: &mut [T]) {
    for k in (1..items.len()).rev() {
        let Ok(top) = u32::try_from(k) else { continue };
        let j = rng.random_range(0..=top);
        items.swap(k, usize::try_from(j).unwrap_or(k));
    }
}

/// The world's end of the week, once everyone fought: each club's new wealth at the
/// start of the off-season, the rival clubs' market, the colosseums' titles, told in
/// `titles`, the market topped up and trimmed; then the next week opens with the rival
/// fighters' challenges. Returns the club's new wealth if it changed.
fn end_week(
    state: &mut GameState,
    titles: &mut Vec<(usize, TitleNews)>,
    data: &DesignData,
) -> Option<Wealth> {
    let wealth = wealth_changes(state, data);
    rival_market(state, data);
    titles.extend(end_colosseum_weeks(state, state.week, data));
    state.open_world(data);
    trim_market(state, data);
    state.week += 1;
    receive_challenges(state, data);
    wealth
}

/// At the start of the off-season, each club's wealth follows its money
/// (`Wealth::next`); returns the player's club's new wealth if it changed.
fn wealth_changes(state: &mut GameState, data: &DesignData) -> Option<Wealth> {
    if !off_season_starts(state, data) {
        return None;
    }
    let rules = &data.economy.wealth;
    for rival in &mut state.rivals {
        rival.wealth = rival.wealth.next(rival.money, rules);
    }
    let wealth = state.wealth.next(state.money, rules);
    (wealth != state.wealth).then(|| {
        state.wealth = wealth;
        wealth
    })
}

/// Each rival club's changes of staff this week, in club order, among the candidates
/// the club and the clubs before it left (`rivals::staff_moves`, the posts of its
/// wealth, with its money for each share of the salaries), then its salaries, paid
/// once for every `fighters_per_salary` fighters in activity, even into debt, and its
/// patron's gift. The market's week ends with them: no one else hires before the next.
fn rival_staff(state: &mut GameState, data: &DesignData) {
    let market = &mut state.staff_market;
    for (club, rival) in state.rivals.iter_mut().enumerate() {
        let shares = i64::from(data.economy.staff.salary_shares(rival.active()));
        let money = rival.money / shares;
        if let Some(profile) = data.world.profile(club) {
            let rules = (profile, rivals::staff_posts(rival.wealth));
            let moves = rivals::staff_moves(&rival.staff, money, rules, market);
            staff::make_moves(&mut rival.staff, market, &moves, money);
        }
        let salaries: u32 = rival.staff.salaries().sum();
        rival.money -= i64::from(salaries) * shares;
        rival.money += i64::from(data.economy.wealth.get(rival.wealth).patron);
    }
    staff::age_market(market);
}

/// What the staff of `outsider`'s club brings to its weeks: nothing for a free agent.
fn outsider_effects(rivals: &[Rival], outsider: Contender) -> StaffEffects {
    match outsider {
        Contender::Rival(id) => rivals
            .get(id.club)
            .map(|r| r.staff.effects())
            .unwrap_or_default(),
        Contender::Club(_) | Contender::FreeAgent(_) => StaffEffects::default(),
    }
}

/// The outsider `them` met by a club's fighter, with its doctor's medicine.
fn met<'a>(
    rivals: &'a mut [Rival],
    free_agents: &'a mut [CareerFighter],
    them: Contender,
) -> Option<(&'a mut CareerFighter, Medicine)> {
    let medicine = outsider_effects(rivals, them).medicine;
    outsider_mut(rivals, free_agents, them).map(|f| (f, medicine))
}

/// The changes of staff `moves` made before the week, the first for each post, each
/// if still allowed at its turn (`GameState::may_staff`), then the salaries paid in
/// `ledger`, post by post, as many times as the club's fighters make it, even into
/// debt, and the patron's gift; returns the changes made.
fn staff_week(
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
    let patron = i64::from(data.economy.wealth.get(state.wealth).patron);
    if patron != 0 {
        state.money += patron;
        ledger.push(Transaction {
            fighter: None,
            flow: Flow::Patron,
            amount: patron,
        });
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
        Move::AcceptOffer { .. } => 0,
        Move::Release(_) => 1,
        Move::List(_) | Move::Unlist(_) => 2,
        Move::Sign(_) => 3,
        Move::Observe(_) | Move::Offer(..) => u8::MAX,
    };
    let mut moves: Vec<Move> = moves.to_vec();
    moves.sort_by_key(order);
    for m in moves {
        if order(&m) == u8::MAX || !state.may_move(m, data) {
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
                let to = RivalId {
                    club: rival,
                    fighter: state.rivals[rival].fighters.len() - 1,
                };
                state.transfer(Contender::Club(fighter), Contender::Rival(to));
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
                let to = Contender::FreeAgent(state.free_agents.len() - 1);
                state.transfer(Contender::Club(i), to);
                news.push(MarketNews::Released(i));
            }
            Move::List(i) => state.transfer_list.push(i),
            Move::Unlist(i) => state.transfer_list.retain(|&l| l != i),
            Move::Sign(a) => {
                let agent = state.free_agents[a].clone();
                state.free_agents[a].status = Status::Sold;
                state.fighters.push(agent);
                let i = state.fighters.len() - 1;
                state.transfer(Contender::FreeAgent(a), Contender::Club(i));
                news.push(MarketNews::Signed(i));
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
    state.transfer(Contender::Rival(id), Contender::Club(fighter));
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

/// The rival clubs' market. At the start of the off-season, each releases its
/// fighters of `release_age` and more worth less than its average fighter, a rich club
/// at any age those worth less than `rich_release_percent` of it, and the rookies
/// arrive. Each club below `roster.wanted` fighters recruits (see `rival_signings`), a
/// rich one buys better fighters (`rich_purchases`); then each club with a place,
/// keeping its `care_reserve`, offers its `buy_percent` of their value for the player's
/// listed fighters.
fn rival_market(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    if off_season_starts(state, data) {
        for club in 0..state.rivals.len() {
            let rival = &state.rivals[club];
            let values: Vec<u64> = rival
                .fighters
                .iter()
                .filter(|f| f.status == Status::Active)
                .map(|f| u64::from(value(f, market)))
                .collect();
            let count = u64::try_from(values.len()).unwrap_or(u64::MAX);
            let average = values.iter().sum::<u64>().checked_div(count).unwrap_or(0);
            let rich = rival.wealth == Wealth::Rich;
            let gone: Vec<usize> = (0..rival.fighters.len())
                .filter(|&i| {
                    let fighter = &rival.fighters[i];
                    let worth = u64::from(value(fighter, market));
                    fighter.status == Status::Active
                        && ((fighter.age >= market.release_age && worth < average)
                            || (rich
                                && worth * 100 < average * u64::from(market.rich_release_percent)))
                })
                .collect();
            for fighter in gone {
                release_rival(state, RivalId { club, fighter });
            }
        }
        for _ in 0..market.rookies {
            let rookie = market::rookie(&mut state.rng, data);
            state.free_agents.push(rookie);
        }
    }
    rival_signings(state, data);
    rich_purchases(state, data);
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

/// Rival fighter `id` lets go by its club: a free agent now, with its Elo and titles.
fn release_rival(state: &mut GameState, id: RivalId) {
    let Some(fighter) = state.rivals[id.club].fighters.get_mut(id.fighter) else {
        return;
    };
    state.free_agents.push(fighter.clone());
    fighter.status = Status::Released;
    let to = Contender::FreeAgent(state.free_agents.len() - 1);
    state.transfer(Contender::Rival(id), to);
}

/// The value of rival club `club`'s weakest fighter in activity, and its index; `None`
/// for a club without any.
fn weakest(state: &GameState, club: usize, data: &DesignData) -> Option<(u32, usize)> {
    let market = &data.economy.market;
    let fighters = &state.rivals[club].fighters;
    (0..fighters.len())
        .filter(|&i| fighters[i].status == Status::Active)
        .map(|i| (value(&fighters[i], market), i))
        .min()
}

/// The free agent rival club `club` would sign first: for a poor club, the one of the
/// best potential its recruiter sees (`market::estimated_potential`), the youngest of
/// equals; for the others, or a poor club whose recruiter cannot tell, the most
/// valuable.
fn wanted_agent(state: &GameState, club: usize, data: &DesignData) -> Option<usize> {
    let rival = &state.rivals[club];
    let scouting = rival
        .staff
        .recruiter()
        .map(|r| r.scouting)
        .unwrap_or_default();
    let agents = &state.free_agents;
    let promising = (0..agents.len())
        .filter(|&a| agents[a].status == Status::Active)
        .filter_map(|a| {
            let potential = estimated_potential(&agents[a], &scouting)?;
            Some((potential, Reverse(agents[a].age), Reverse(a)))
        })
        .max()
        .map(|(.., Reverse(a))| a);
    match promising {
        Some(a) if rival.wealth == Wealth::Poor => Some(a),
        _ => most_valuable(agents, data),
    }
}

/// Each rival club below `roster.wanted` fighters in activity recruits until it has
/// them: the free agent it wants first (`wanted_agent`) if worth at least its weakest
/// fighter; if not, a club that is not poor first tries, once a week, to buy the most
/// valuable fighter of another club worth that much that it can pay at the seller's
/// `sell_percent` keeping its own `care_reserve`, if the fighter agrees
/// (`market::refuses_to_join`, against the buyer's best fame); failing that, it signs
/// the free agent anyway. A poor club signs the free agent of the best potential it
/// sees, whatever it is worth now. A club that just sold a fighter this way fills its
/// place from the free agents only, so that it never buys back the fighter it sold.
fn rival_signings(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    let wanted = usize::try_from(data.world.roster.wanted).unwrap_or(0);
    let mut sold = vec![false; state.rivals.len()];
    for buyer in 0..state.rivals.len() {
        let poor = state.rivals[buyer].wealth == Wealth::Poor;
        let mut tried = sold[buyer] || poor;
        while state.rivals[buyer].active() < wanted {
            let weakest = weakest(state, buyer, data).map_or(0, |(worth, _)| worth);
            let agent = wanted_agent(state, buyer, data);
            let suits = agent.is_some_and(|a| value(&state.free_agents[a], market) >= weakest);
            if !suits && !tried {
                tried = true;
                if let Some(seller) = buy_from_rival(state, buyer, weakest, data) {
                    sold[seller] = true;
                    continue;
                }
            }
            let Some(agent) = agent else { break };
            let signed = state.free_agents[agent].clone();
            state.free_agents[agent].status = Status::Sold;
            let roster = &mut state.rivals[buyer].fighters;
            roster.push(signed);
            let to = RivalId {
                club: buyer,
                fighter: roster.len() - 1,
            };
            state.transfer(Contender::FreeAgent(agent), Contender::Rival(to));
        }
    }
}

/// Each rich rival club, once a week, tries to buy the most valuable fighter of another
/// club worth at least `rich_upgrade_percent` of its weakest (see `buy_from_rival`);
/// past `roster.max`, it then lets its weakest go to the market.
fn rich_purchases(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    let max = usize::try_from(data.world.roster.max).unwrap_or(usize::MAX);
    for buyer in 0..state.rivals.len() {
        if state.rivals[buyer].wealth != Wealth::Rich {
            continue;
        }
        let Some((worth, _)) = weakest(state, buyer, data) else {
            continue;
        };
        let floor = percent_of(worth, market.rich_upgrade_percent);
        if buy_from_rival(state, buyer, floor, data).is_some()
            && state.rivals[buyer].active() > max
            && let Some((_, fighter)) = weakest(state, buyer, data)
        {
            release_rival(
                state,
                RivalId {
                    club: buyer,
                    fighter,
                },
            );
        }
    }
}

/// Rival club `buyer` buys the most valuable fighter of another club no richer than it
/// worth at least `floor` that it can pay, if the fighter agrees; returns the seller if
/// it did. A club sells to a richer one, never to a poorer, and buys only keeping its
/// `care_reserve` and half the money of its wealth (`WealthLevel::from`), so as not to
/// fall from its level by buying.
fn buy_from_rival(
    state: &mut GameState,
    buyer: usize,
    floor: u32,
    data: &DesignData,
) -> Option<usize> {
    let market = &data.economy.market;
    let money = state.rivals[buyer].money;
    let wealth = state.rivals[buyer].wealth;
    let level = i64::from(data.economy.wealth.get(wealth).from) / 2;
    let reserve = data
        .world
        .profile(buyer)
        .map(|p| i64::from(p.care_reserve).max(level))?;
    let best = state
        .rivals
        .iter()
        .enumerate()
        .filter(|&(club, rival)| club != buyer && rival.wealth <= wealth)
        .flat_map(|(club, rival)| {
            (0..rival.fighters.len()).map(move |fighter| RivalId { club, fighter })
        })
        .filter_map(|id| {
            let worth = value(state.rival_fighter(id)?, market);
            let price = state.asking_price(id, data)?;
            (worth >= floor && money - i64::from(price) >= reserve).then_some((worth, id, price))
        })
        .max_by_key(|&(worth, id, _)| (worth, Reverse(id)));
    let (_, id, price) = best?;
    let fame = state.rivals[buyer]
        .fighters
        .iter()
        .filter(|f| f.status == Status::Active)
        .map(|f| f.fame)
        .max_by_key(|f| f.value())
        .unwrap_or_default();
    let fighter = &state.rivals[id.club].fighters[id.fighter];
    if market::refuses_to_join(&mut state.rng, fighter, fame, market) {
        return None;
    }
    let bought = fighter.clone();
    state.rivals[id.club].fighters[id.fighter].status = Status::Sold;
    state.rivals[id.club].money += i64::from(price);
    state.rivals[buyer].money -= i64::from(price);
    let roster = &mut state.rivals[buyer].fighters;
    roster.push(bought);
    let to = RivalId {
        club: buyer,
        fighter: roster.len() - 1,
    };
    state.transfer(Contender::Rival(id), Contender::Rival(to));
    Some(id.club)
}

/// Index of the most valuable of `fighters` still active, the first of equals.
fn most_valuable(fighters: &[CareerFighter], data: &DesignData) -> Option<usize> {
    (0..fighters.len())
        .filter(|&a| fighters[a].status == Status::Active)
        .max_by_key(|&a| (value(&fighters[a], &data.economy.market), Reverse(a)))
}

/// Beyond `MarketRules::max_free_agents`, the least valuable free agents give up
/// waiting for a club: they retire.
fn trim_market(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    let most = usize::try_from(market.max_free_agents).unwrap_or(usize::MAX);
    while state.available_free_agents() > most {
        let Some(least) = (0..state.free_agents.len())
            .filter(|&a| state.free_agents[a].status == Status::Active)
            .min_by_key(|&a| (value(&state.free_agents[a], market), Reverse(a)))
        else {
            break;
        };
        state.free_agents[least].status = Status::Retired;
    }
}

/// The colosseums' end of `week`: a title whose holder no longer fights goes to the
/// best ranked still fighting; an outsider champion at its deadline defends the title
/// against the best-ranked other outsider, a club champion past it loses the title.
/// Those who no longer fight leave the rankings. Returns what happened to the titles,
/// with the colosseum's index.
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
        free_agents: &state.free_agents,
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

/// The week's plans of everyone out of the club, made before anyone plays.
#[derive(Debug, Clone)]
struct OutsiderPlans {
    /// By rival club, in roster order (`GameState::rival_plans`).
    rivals: Vec<Vec<WeekPlan>>,
    /// In market order (`GameState::free_agent_plans`).
    free_agents: Vec<WeekPlan>,
}

impl OutsiderPlans {
    /// The plan of `outsider`; `None` for the club's fighters and those who came after
    /// the plans were made.
    fn of(&self, outsider: Contender) -> Option<WeekPlan> {
        match outsider {
            Contender::Rival(id) => self.rivals.get(id.club)?.get(id.fighter).copied(),
            Contender::FreeAgent(a) => self.free_agents.get(a).copied(),
            Contender::Club(_) => None,
        }
    }

    fn set_activity(&mut self, outsider: Contender, activity: Activity) {
        let plan = match outsider {
            Contender::Rival(id) => self
                .rivals
                .get_mut(id.club)
                .and_then(|p| p.get_mut(id.fighter)),
            Contender::FreeAgent(a) => self.free_agents.get_mut(a),
            Contender::Club(_) => None,
        };
        if let Some(plan) = plan {
            plan.activity = activity;
        }
    }
}

/// The outsider `outsider` names, a rival fighter or a free agent.
fn outsider<'a>(
    rivals: &'a [Rival],
    free_agents: &'a [CareerFighter],
    outsider: Contender,
) -> Option<&'a CareerFighter> {
    let rosters = Rosters {
        club: &[],
        rivals,
        free_agents,
    };
    rosters.fighter(outsider)
}

fn outsider_mut<'a>(
    rivals: &'a mut [Rival],
    free_agents: &'a mut [CareerFighter],
    outsider: Contender,
) -> Option<&'a mut CareerFighter> {
    match outsider {
        Contender::Rival(id) => rivals.get_mut(id.club)?.fighters.get_mut(id.fighter),
        Contender::FreeAgent(a) => free_agents.get_mut(a),
        Contender::Club(_) => None,
    }
}

/// Puts `fighter` back in the place of `outsider`.
fn put_outsider(state: &mut GameState, outsider: Contender, fighter: CareerFighter) {
    if let Some(place) = outsider_mut(&mut state.rivals, &mut state.free_agents, outsider) {
        *place = fighter;
    }
}

/// Every outsider still fighting: the rival fighters club after club, then the free
/// agents.
fn outsiders<'a>(
    rivals: &'a [Rival],
    free_agents: &'a [CareerFighter],
) -> impl Iterator<Item = Contender> + 'a {
    rivals
        .iter()
        .enumerate()
        .flat_map(|(club, r)| {
            (0..r.fighters.len()).map(move |fighter| Contender::Rival(RivalId { club, fighter }))
        })
        .chain((0..free_agents.len()).map(Contender::FreeAgent))
        .filter(move |&o| {
            outsider(rivals, free_agents, o).is_some_and(|f| f.status == Status::Active)
        })
}

/// The week of the rival clubs and the free agents, after the club's fights. In each
/// colosseum, the outsiders registered there and not met by the club's fighters are
/// paired in Elo order, each pair fighting a fight the statistical model resolves,
/// which moves both Elo; one left without an opponent trains instead, or rests if it
/// is a free agent, who cannot pay for training. The others rest or train as planned,
/// those met by the club's fighters end their week, and each rival club pays its
/// fighters' costs and upkeep and takes their receipts and prizes; a free agent pays
/// and earns nothing.
fn outsider_weeks(
    state: &mut GameState,
    mut plans: OutsiderPlans,
    fought: &[Contender],
    data: &DesignData,
) {
    let world = &data.world;
    let upkeep = i64::from(data.economy.upkeep);
    for (club, rival) in state.rivals.iter_mut().enumerate() {
        for (fighter, f) in rival.fighters.iter_mut().enumerate() {
            if f.status == Status::Active {
                rival.money -= upkeep;
                if fought.contains(&Contender::Rival(RivalId { club, fighter })) {
                    f.end_fought_week(&mut state.rng, data);
                }
            }
        }
    }
    for (a, f) in state.free_agents.iter_mut().enumerate() {
        if f.status == Status::Active && fought.contains(&Contender::FreeAgent(a)) {
            f.end_fought_week(&mut state.rng, data);
        }
    }
    let mut done = fought.to_vec();
    for (c, (purses, arena)) in world
        .colosseums
        .iter()
        .map(|r| (&r.purses, r.arena))
        .enumerate()
        .take(state.colosseums.len())
    {
        let here = Activity::Fight(ColosseumId(u32::try_from(c).unwrap_or(u32::MAX)));
        let colosseum = &state.colosseums[c];
        let mut waiting: Vec<(i32, Contender)> = outsiders(&state.rivals, &state.free_agents)
            .filter(|o| !done.contains(o) && plans.of(*o).is_some_and(|p| p.activity == here))
            .map(|o| (colosseum.elo(o, world), o))
            .collect();
        waiting.sort_by_key(|&(elo, o)| (Reverse(elo), o));
        for pair in waiting.chunks(2) {
            let &[(_, a), (_, b)] = pair else {
                let (_, alone) = pair[0];
                let instead = match alone {
                    Contender::Rival(_) => Activity::Train,
                    _ => Activity::Rest,
                };
                plans.set_activity(alone, instead);
                continue;
            };
            let (rivals, free_agents) = (&state.rivals, &state.free_agents);
            let (Some(mut first), Some(mut second)) = (
                outsider(rivals, free_agents, a).cloned(),
                outsider(rivals, free_agents, b).cloned(),
            ) else {
                continue;
            };
            let receipts = i64::from(purses.receipts(first.fame, second.fame));
            let medicine = [a, b].map(|o| outsider_effects(&state.rivals, o).medicine);
            let result = first.modelled_fight(
                &mut second,
                arena,
                FormEvent::Fight,
                medicine,
                &mut state.rng,
                data,
            );
            first.end_fought_week(&mut state.rng, data);
            second.end_fought_week(&mut state.rng, data);
            put_outsider(state, a, first);
            put_outsider(state, b, second);
            done.extend([a, b]);
            state.colosseums[c].record(a, b, result, world);
            for (them, result) in [(a, result), (b, opposite(result))] {
                let Contender::Rival(id) = them else {
                    continue;
                };
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
    planned_weeks(state, &plans, &done, data);
}

/// The planned weeks of the outsiders not in `done`, each rival club paying its
/// fighters' care and training at its staff's prices, its staff's effects with them.
fn planned_weeks(
    state: &mut GameState,
    plans: &OutsiderPlans,
    done: &[Contender],
    data: &DesignData,
) {
    for (club, rival) in state.rivals.iter_mut().enumerate() {
        let effects = rival.staff.effects();
        let session = rival.staff.training_cost(data.economy.training_cost);
        for (fighter, f) in rival.fighters.iter_mut().enumerate() {
            let me = Contender::Rival(RivalId { club, fighter });
            let Some(plan) = plans.of(me).filter(|_| !done.contains(&me)) else {
                continue;
            };
            if let Some(report) = f.play_week(plan, None, effects, &mut state.rng, data) {
                let training = if plan.activity == Activity::Train {
                    session
                } else {
                    0
                };
                rival.money -= i64::from(report.care_cost) + i64::from(training);
            }
        }
    }
    for (a, f) in state.free_agents.iter_mut().enumerate() {
        let me = Contender::FreeAgent(a);
        if let Some(plan) = plans.of(me).filter(|_| !done.contains(&me)) {
            f.play_week(plan, None, StaffEffects::default(), &mut state.rng, data);
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
/// and outsider of its title fight. Title fights are settled before anyone plays, from
/// the rankings as they stand; one the fighter may not have turns into rest, and so does
/// a challenge it may not have.
fn week_plans(
    state: &GameState,
    decisions: &Decisions,
    data: &DesignData,
) -> (Vec<WeekPlan>, Vec<Option<(usize, Contender)>>) {
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
                Activity::Melee(_) => state.opens_season(data),
                _ => true,
            };
            if !allowed {
                plan.activity = Activity::Rest;
            }
            (plan, meeting)
        })
        .unzip()
}

/// The colosseum and the outsider met in a fight (`activity`): the rival fighter
/// challenged while still fighting (someone may have killed it earlier in the week),
/// otherwise one at random among those registered there (in `plans`) who have not
/// `fought` yet, or else among those ranked there, or else among anyone.
fn opponent(
    rng: &mut ChaCha8Rng,
    (colosseums, rivals, free_agents): (&[Colosseum], &[Rival], &[CareerFighter]),
    activity: Activity,
    plans: &OutsiderPlans,
    fought: &[Contender],
) -> Option<(usize, Contender)> {
    let (id, chosen) = match activity {
        Activity::Fight(id) => (id, None),
        Activity::Challenge(id, rival) => (id, Some(Contender::Rival(rival))),
        _ => return None,
    };
    let c = usize::try_from(id.0).ok()?;
    let colosseum = colosseums.get(c)?;
    let active = |o: &Contender| {
        outsider(rivals, free_agents, *o).is_some_and(|f| f.status == Status::Active)
    };
    if let Some(rival) = chosen.filter(active) {
        return Some((c, rival));
    }
    let everyone: Vec<Contender> = outsiders(rivals, free_agents)
        .filter(|o| !fought.contains(o))
        .collect();
    let registered: Vec<Contender> = everyone
        .iter()
        .copied()
        .filter(|&o| {
            plans
                .of(o)
                .is_some_and(|p| p.activity == Activity::Fight(id))
        })
        .collect();
    let ranked: Vec<Contender> = colosseum
        .ranking
        .iter()
        .map(|r| r.contender)
        .filter(|o| everyone.contains(o))
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
        Activity::Fight(id) | Activity::Challenge(id, _) | Activity::Melee(id) => {
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
    use crate::staff::{Person, Role};

    /// The design data, without patrons: most tests follow the club's money flow by
    /// flow, and the patron's gift would come into each.
    fn data() -> DesignData {
        let mut data = shipped_data();
        let wealth = &mut data.economy.wealth;
        for level in [&mut wealth.poor, &mut wealth.modest, &mut wealth.rich] {
            level.patron = 0;
        }
        data
    }

    fn shipped_data() -> DesignData {
        DesignData::load().unwrap()
    }

    fn plan(activity: Activity, care: bool) -> Decisions {
        Decisions {
            plans: vec![WeekPlan {
                activity,
                care,
                refused: Refused::NONE,
            }],
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
    fn the_season_opens_with_a_melee_in_every_colosseum_even_without_the_club() {
        let data = data();
        let mut state = GameState::new(3, &data);
        assert!(state.opens_season(&data));
        let before = state.clone();
        let summary = next_week(&mut state, &plan(Activity::Rest, false), &data);
        let outsiders = |s: &GameState| -> u32 {
            let rivals = s.rivals.iter().flat_map(|r| &r.fighters);
            let records = rivals.chain(&s.free_agents).map(|f| f.record);
            records.map(|r| r.wins + r.losses + r.draws).sum()
        };
        // Many fought in the melees, each a fight of their record.
        assert!(outsiders(&state) >= outsiders(&before) + 2 * 3);
        for (c, colosseum) in state.colosseums.iter().enumerate() {
            let moved = colosseum
                .ranking
                .iter()
                .filter(|r| before.colosseums[c].elo(r.contender, &data.world) != r.elo)
                .count();
            assert!(moved >= 3, "colosseum {c}: {moved}");
        }
        assert!(summary.reports[0].as_ref().unwrap().fight.is_none());
        // Past the first week, no melee until next season.
        assert!(!state.opens_season(&data));
        let summary = next_week(
            &mut state,
            &plan(Activity::Melee(ColosseumId(0)), false),
            &data,
        );
        let report = summary.reports[0].as_ref().unwrap();
        assert_eq!(report.plan.activity, Activity::Rest);
        state.week = u64::from(data.calendar.season_weeks()) + 1;
        assert!(state.opens_season(&data));
    }

    #[test]
    fn the_club_s_fighters_share_a_melee_and_pay_and_earn_for_it() {
        let data = data();
        let mut state = GameState::new(3, &data);
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        state.fighters.push(CareerFighter::random(&mut rng, &data));
        let capua = Activity::Melee(ColosseumId(0));
        let entered = WeekPlan {
            activity: capua,
            care: false,
            refused: Refused::NONE,
        };
        let decisions = Decisions {
            plans: vec![entered; 2],
            ..Decisions::default()
        };
        let summary = next_week(&mut state, &decisions, &data);
        let fights: Vec<&crate::fighter::career::FightSummary> = summary
            .reports
            .iter()
            .map(|r| r.as_ref().unwrap().fight.as_ref().unwrap())
            .collect();
        // One melee, seen by each from its own place.
        assert_eq!(fights[0].seed, fights[1].seed);
        assert_ne!(fights[0].me, fights[1].me);
        assert!(fights[0].is_melee());
        let size = fights[0].opponents.len() + 1;
        assert!((3..=20).contains(&size));
        let purses = &data.world.colosseums[0].purses;
        for (i, fight) in fights.iter().enumerate() {
            let money: Vec<(Flow, i64)> = summary
                .ledger
                .iter()
                .filter(|t| t.fighter == Some(i))
                .map(|t| (t.flow, t.amount))
                .collect();
            let base = i64::from(purses.receipts_base);
            assert!(
                money
                    .iter()
                    .any(|&(flow, amount)| flow == Flow::Receipts && amount >= base)
            );
            let prize = (Flow::WinPrize, i64::from(purses.melee_prize));
            assert_eq!(money.contains(&prize), fight.result == FightResult::Won);
            assert!(money.contains(&(Flow::Upkeep, -i64::from(data.economy.upkeep))));
            // Ranked in Capua from its place.
            assert_ne!(
                state.colosseums[0].elo(Contender::Club(i), &data.world),
                data.world.elo_start
            );
        }
    }

    #[test]
    fn a_new_game_starts_at_week_one_with_the_seed_s_fighter() {
        let data = data();
        let state = GameState::new(7, &data);
        assert_eq!(state.week, 1);
        assert_eq!(state.date(&data), Date { season: 1, week: 1 });
        let wealth = &data.economy.wealth;
        assert_eq!(state.wealth, Wealth::Modest);
        assert_eq!(state.money, i64::from(wealth.modest.starting_money));
        let fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(7), &data);
        assert_eq!(state.fighters, [fighter]);
        // A rich club starts with more money, and the same fighter.
        let rich = GameState::with_wealth(7, Wealth::Rich, &data);
        assert_eq!(rich.money, i64::from(wealth.rich.starting_money));
        assert_eq!(rich.fighters, state.fighters);
        // The rival clubs start with the wealth of data/world.ron.
        for (rival, rules) in state.rivals.iter().zip(&data.world.rivals) {
            assert_eq!(rival.wealth, rules.wealth);
            assert_eq!(
                rival.money,
                i64::from(wealth.get(rules.wealth).starting_money)
            );
        }
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
            Contender::Club(_) | Contender::FreeAgent(_) => unreachable!(),
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
        let data = shipped_data();
        let economy = &data.economy;
        let mut state = GameState::new(7, &data);
        let summary = next_week(&mut state, &plan(Activity::Train, false), &data);
        let cost = i64::from(economy.training_cost);
        let upkeep = i64::from(economy.upkeep);
        let modest = &economy.wealth.modest;
        let patron = i64::from(modest.patron);
        assert_eq!(
            flows(&summary),
            [
                (Flow::Patron, patron),
                (Flow::Training, -cost),
                (Flow::Upkeep, -upkeep)
            ]
        );
        assert_eq!(
            state.money,
            i64::from(modest.starting_money) + patron - cost - upkeep
        );
    }

    /// Puts someone for `role`, exactly as `level` is, on the market; returns its place.
    fn offered(state: &mut GameState, role: Role, level: u32, data: &DesignData) -> usize {
        let person = Person::of_level(role, level, &data.economy.staff).unwrap();
        state.staff_market.push(Candidate {
            person,
            weeks_left: 1,
        });
        state.staff_market.len() - 1
    }

    #[test]
    fn a_trainer_is_paid_every_week_and_sets_the_session_s_price() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let trainer = data.economy.staff.trainer(3).unwrap();
        let mut train = plan(Activity::Train, false);
        train.staff = vec![StaffMove::Hire(offered(
            &mut state,
            Role::Trainer,
            3,
            &data,
        ))];
        let summary = next_week(&mut state, &train, &data);
        assert_eq!(summary.staff, [StaffNews::Hired(Role::Trainer, 3)]);
        assert_eq!(state.staff.level(Role::Trainer), Some(3));
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
    fn a_candidate_on_the_market_is_hired_only_with_a_first_salary() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let first = offered(&mut state, Role::Trainer, 1, &data);
        assert!(state.may_staff(StaffMove::Hire(first), &data));
        assert!(!state.may_staff(StaffMove::Hire(first + 1), &data));
        assert!(!state.may_staff(StaffMove::Dismiss(Role::Trainer), &data));
        state.money = 0;
        let mut hire = plan(Activity::Rest, false);
        hire.staff = vec![StaffMove::Hire(first)];
        let summary = next_week(&mut state, &hire, &data);
        assert_eq!(summary.staff, []);
        assert_eq!(state.staff.trainer, None);
        // A trainer already there is paid even into debt.
        state.staff = Staff::of_levels(&[(Role::Trainer, 1)], &data.economy.staff);
        state.money = 0;
        next_week(&mut state, &plan(Activity::Rest, false), &data);
        assert!(state.money < 0);
        let second = offered(&mut state, Role::Trainer, 2, &data);
        assert!(!state.may_staff(StaffMove::Hire(second), &data));
    }

    #[test]
    fn a_trainer_spares_form_at_training() {
        let data = data();
        let mut alone = GameState::new(7, &data);
        let mut coached = alone.clone();
        coached.staff = Staff::of_levels(&[(Role::Trainer, 5)], &data.economy.staff);
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
            StaffMove::Hire(offered(&mut state, Role::Recruiter, 2, &data)),
            StaffMove::Hire(offered(&mut state, Role::Doctor, 4, &data)),
            // A second change for the same post does not count.
            StaffMove::Hire(offered(&mut state, Role::Recruiter, 5, &data)),
        ];
        let summary = next_week(&mut state, &week, &data);
        assert_eq!(
            summary.staff,
            [
                StaffNews::Hired(Role::Recruiter, 2),
                StaffNews::Hired(Role::Doctor, 4)
            ]
        );
        assert_eq!(
            Role::ALL.map(|r| state.staff.level(r)),
            [None, Some(4), Some(2)]
        );
        let salary = |s: u32| (Flow::Salary, -i64::from(s));
        assert_eq!(
            flows(&summary)[..2],
            [
                salary(staff.doctor(4).unwrap().salary),
                salary(staff.recruiter(2).unwrap().salary)
            ]
        );
        assert_eq!(state.scouting(), staff.recruiter(2).unwrap().scouting);
        // Candidates come back for each post by the next week.
        for role in Role::ALL {
            let candidates = state
                .staff_market
                .iter()
                .filter(|c| c.person.role() == role);
            assert_eq!(candidates.count(), 3);
        }
    }

    #[test]
    fn the_doctor_sets_the_price_of_care() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.staff = Staff::of_levels(&[(Role::Doctor, 5)], &data.economy.staff);
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
        let mut state = GameState::new(2, &data);
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
        assert!(won, "seed 2 wins a fight in 20 weeks");
    }

    #[test]
    fn a_fight_meets_a_rival_fighter_and_moves_both_elo() {
        let data = data();
        let rules = &data.world;
        let mut state = GameState::new(7, &data);
        // Past the season's opening, whose melees keep the rival fighters busy.
        state.week = 2;
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
            .position(|f| Some(f.potential) == fight.opponent().map(|o| o.potential));
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
        // The season opens with its melee, where it would have fought.
        assert_eq!(
            state.suggested_plan(0, &data).activity,
            Activity::Melee(ColosseumId(0))
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
        assert_eq!(fight.opponent(), Some(&before));
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
        // Received, so it may be fought however far ahead the fighter is; past the
        // season's opening melee, which the suggestion would rather enter.
        state.week = 2;
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
        // Off the market, its entry kept.
        assert_eq!(state.free_agents[2].status, Status::Sold);
        assert!(!state.may_move(Move::Sign(2), &data));
    }

    #[test]
    fn a_full_club_signs_no_one() {
        let data = data();
        let mut state = GameState::new(7, &data);
        // The first free agent on the market.
        let hire = |state: &GameState| {
            let a = (0..state.free_agents.len())
                .find(|&a| state.free_agents[a].status == Status::Active)
                .unwrap();
            moves(&[Move::Sign(a)])
        };
        let max = usize::try_from(data.economy.max_fighters).unwrap();
        for _ in 0..max + 2 {
            let decisions = hire(&state);
            next_week(&mut state, &decisions, &data);
        }
        assert_eq!(state.active_fighters(), max);
        assert!(!state.can_hire(&data));
        // A fighter gone leaves a place.
        state.fighters[0].status = Status::Retired;
        assert!(state.can_hire(&data));
        let decisions = hire(&state);
        next_week(&mut state, &decisions, &data);
        assert_eq!(state.active_fighters(), max);
        // Several signings in a week sign the free agents chosen, in order.
        let mut state = GameState::new(7, &data);
        let (first, third) = (state.free_agents[0].clone(), state.free_agents[2].clone());
        next_week(&mut state, &moves(&[Move::Sign(2), Move::Sign(0)]), &data);
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
                refused: Refused::NONE,
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
        // A full club makes none, even if some of its fighters die this week.
        let mut full = state.clone();
        let extra = full.rivals[1].fighters[0].clone();
        full.rivals[1].fighters.extend(vec![extra; 4]);
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
        // Too tired to fight that week, so that neither dies before the off-season.
        for j in [best, worst] {
            roster[j].age = market.release_age;
            roster[j].weeks_this_year = 0;
            roster[j].form = crate::fighter::form::Form::new(10);
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
        assert!(state.available_free_agents() <= usize::try_from(market.max_free_agents).unwrap());
    }

    /// The week of `state` that starts the off-season, played next.
    fn up_to_the_off_season(state: &mut GameState, data: &DesignData) {
        let calendar = &data.calendar;
        let off_season = u64::from(calendar.competition_weeks + calendar.great_event_weeks + 1);
        while state.week < off_season {
            next_week(state, &Decisions::default(), data);
        }
    }

    #[test]
    fn wealth_follows_money_at_the_off_season_and_sets_the_patron() {
        let data = shipped_data();
        let rules = &data.economy.wealth;
        let mut state = GameState::with_wealth(7, Wealth::Poor, &data);
        assert_eq!(state.money, i64::from(rules.poor.starting_money));
        state.fighters[0].status = Status::Retired;
        let summary = next_week(&mut state, &Decisions::default(), &data);
        assert_eq!(
            flows(&summary),
            [(Flow::Patron, i64::from(rules.poor.patron))]
        );
        assert_eq!(summary.wealth, None);
        up_to_the_off_season(&mut state, &data);
        // Rich enough to rise, while the rich rival club is ruined.
        state.money = i64::from(rules.modest.from) + 1000;
        state.rivals[0].money = 0;
        let summary = next_week(&mut state, &Decisions::default(), &data);
        assert_eq!(summary.wealth, Some(Wealth::Modest));
        assert_eq!(state.wealth, Wealth::Modest);
        assert_eq!(state.rivals[0].wealth, Wealth::Modest);
        let summary = next_week(&mut state, &Decisions::default(), &data);
        assert_eq!(
            flows(&summary),
            [(Flow::Patron, i64::from(rules.modest.patron))]
        );
        assert_eq!(summary.wealth, None);
    }

    #[test]
    fn a_poor_club_signs_the_free_agent_of_the_best_potential_its_recruiter_sees() {
        let data = data();
        let mut state = GameState::new(7, &data);
        let (poor, modest) = (1, 2);
        assert_eq!(state.rivals[poor].wealth, Wealth::Poor);
        assert_eq!(state.rivals[modest].wealth, Wealth::Modest);
        // A weak free agent of great potential, among the others.
        let gem = (0..state.free_agents.len())
            .find(|&a| state.free_agents[a].status == Status::Active)
            .unwrap();
        state.free_agents[gem].stats = state.free_agents[gem].stats.map(|_| 1);
        state.free_agents[gem].potential = state.free_agents[gem].potential.map(|_| 500);
        let potential = state.free_agents[gem].potential;
        // The modest club, one fighter short, wants what a fighter shows: not the gem.
        state.rivals[modest].fighters[0].status = Status::Dead;
        let mut first = state.clone();
        rival_signings(&mut first, &data);
        assert_ne!(
            first.rivals[modest].fighters.last().unwrap().potential,
            potential
        );
        // Without a recruiter able to tell, the poor club does the same.
        state.rivals[poor].fighters[0].status = Status::Dead;
        let mut blind = state.clone();
        rival_signings(&mut blind, &data);
        assert_ne!(
            blind.rivals[poor].fighters.last().unwrap().potential,
            potential
        );
        // With one, it signs the gem.
        let recruiter = Staff::of_levels(&[(Role::Recruiter, 5)], &data.economy.staff);
        state.rivals[poor].staff = recruiter;
        rival_signings(&mut state, &data);
        assert_eq!(
            state.rivals[poor].fighters.last().unwrap().potential,
            potential
        );
        assert_eq!(state.free_agents[gem].status, Status::Sold);
    }

    /// Makes `fighter` the one every club wants, and one who never refuses a famous club.
    fn star(fighter: &mut CareerFighter) {
        fighter.stats = fighter.stats.map(|_| 500);
        fighter.fame = Fame::default();
        fighter.personality.caution = crate::fighter::personality::TraitValue::new(0);
    }

    #[test]
    fn a_rich_club_buys_better_fighters_from_poorer_clubs_and_lets_its_weakest_go() {
        let data = data();
        let max = usize::try_from(data.world.roster.max).unwrap();
        let mut state = GameState::new(7, &data);
        let (rich, poor, modest) = (0, 1, 2);
        assert_eq!(state.rivals[rich].wealth, Wealth::Rich);
        state.rivals[rich].money = 100_000;
        // Full, and famous enough that no one refuses it.
        state.rivals[rich].fighters[1].fame = Fame::default().changed_by(40);
        while state.rivals[rich].active() < max {
            let fighter = state.rivals[rich].fighters[1].clone();
            state.rivals[rich].fighters.push(fighter);
        }
        let (_, weakest_one) = weakest(&state, rich, &data).unwrap();
        star(&mut state.rivals[poor].fighters[0]);
        let id = RivalId {
            club: poor,
            fighter: 0,
        };
        let price = i64::from(state.asking_price(id, &data).unwrap());
        rich_purchases(&mut state, &data);
        assert_eq!(state.rival_fighter(id).unwrap().status, Status::Sold);
        let bought = state.rivals[rich].fighters.last().unwrap();
        assert_eq!(bought.stats, state.rival_fighter(id).unwrap().stats);
        assert_eq!(state.rivals[rich].money, 100_000 - price);
        let released = &state.rivals[rich].fighters[weakest_one];
        assert_eq!(released.status, Status::Released);
        assert_eq!(state.rivals[rich].active(), max);
        // The modest club, with money and fame, never buys the star from the rich one.
        let star_value = value(bought, &data.economy.market);
        state.rivals[modest].money = 100_000;
        state.rivals[modest].fighters[1].fame = Fame::default().changed_by(40);
        let mut modest_try = state.clone();
        assert_eq!(
            buy_from_rival(&mut modest_try, modest, star_value, &data),
            None
        );
        // Rich too, it may.
        state.rivals[modest].wealth = Wealth::Rich;
        assert_eq!(
            buy_from_rival(&mut state, modest, star_value, &data),
            Some(rich)
        );
    }

    #[test]
    fn at_the_off_season_a_rich_club_lets_go_at_any_age_whoever_no_longer_performs() {
        let data = data();
        let mut state = GameState::new(3, &data);
        state.fighters[0].status = Status::Retired;
        let (rich, poor) = (0, 1);
        up_to_the_off_season(&mut state, &data);
        state.rivals[rich].money = 100_000;
        state.rivals[poor].money = 0;
        // A young fighter worth little in each club.
        let mut weak = Vec::new();
        for club in [rich, poor] {
            let roster = &mut state.rivals[club].fighters;
            let j = (0..roster.len())
                .find(|&j| roster[j].status == Status::Active)
                .unwrap();
            roster[j].stats = roster[j].stats.map(|_| 1);
            roster[j].age = 22;
            roster[j].weeks_this_year = 0;
            weak.push(j);
        }
        next_week(&mut state, &Decisions::default(), &data);
        assert_eq!(state.rivals[rich].wealth, Wealth::Rich);
        assert_eq!(
            state.rivals[rich].fighters[weak[0]].status,
            Status::Released
        );
        // The poor club keeps its young: they are what it bets on.
        assert_eq!(state.rivals[poor].wealth, Wealth::Poor);
        assert_ne!(
            state.rivals[poor].fighters[weak[1]].status,
            Status::Released
        );
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
        // The seller fills its place again, from the free agents: it does not buy back.
        assert_eq!(state.rivals[id.club].active(), 10);
        assert_eq!(
            state.rivals[id.club].money,
            before.rivals[id.club].money + i64::from(price)
        );
        // A club that cannot pay signs the free agent anyway.
        let mut poor = GameState::new(7, &data);
        for agent in &mut poor.free_agents {
            agent.stats = agent.stats.map(|_| 1);
        }
        poor.rivals[0].money = 0;
        poor.rivals[0].fighters[0].status = Status::Dead;
        let agents = poor.available_free_agents();
        rival_signings(&mut poor, &data);
        assert_eq!(poor.available_free_agents(), agents - 1);
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

    /// The milestone of phase 5: seasons follow one another where rankings, champions and
    /// markets change without the player.
    #[test]
    fn three_seasons_live_without_the_player() {
        let data = shipped_data();
        let mut state = GameState::new(1, &data);
        state.fighters[0].status = Status::Retired;
        let start = state.clone();
        let season = u64::from(data.calendar.season_weeks());
        let wanted = usize::try_from(data.world.roster.wanted).unwrap();
        let mut title_changes = vec![0; state.colosseums.len()];
        let mut leaders = Vec::new();
        for week in 1..=3 * season {
            let summary = next_week(&mut state, &Decisions::default(), &data);
            for (c, news) in summary.titles {
                if !matches!(news, TitleNews::Fight { result, .. } if title_kept(result)) {
                    title_changes[c] += 1;
                }
            }
            // A club that loses a fighter after its signings (sold to another, killed in
            // a title defense) recruits the next week.
            for rival in &state.rivals {
                assert!(rival.money >= 0, "week {week}: {}", rival.money);
                assert!(rival.active() + 1 >= wanted, "week {week}");
            }
            if week % season == 0 {
                let rosters = state.rosters();
                let firsts: Vec<Contender> = state
                    .colosseums
                    .iter()
                    .map(|c| c.active_ranking(rosters).next().unwrap().contender)
                    .collect();
                leaders.push(firsts);
            }
        }
        // Every title changed hands, and the first ranks are not those of the first season.
        assert!(title_changes.iter().all(|&n| n > 0), "{title_changes:?}");
        assert_ne!(leaders[0], leaders[2], "{leaders:?}");
        // The clubs replaced those who died, retired or were released in the off-season.
        // Fighters released at `release_age` are worth signing, so the clubs seldom buy
        // from one another (none here).
        for (rival, before) in state.rivals.iter().zip(&start.rivals) {
            assert!(rival.fighters.len() > before.fighters.len());
        }
        let released = state
            .rivals
            .iter()
            .flat_map(|r| &r.fighters)
            .filter(|f| f.status == Status::Released)
            .count();
        assert!(released > 0);
        // Three off-seasons of rookies, some signed by the clubs.
        let rookie_age = data.economy.market.rookie_age;
        let young = state
            .rivals
            .iter()
            .flat_map(|r| &r.fighters)
            .filter(|f| f.status == Status::Active && f.age <= rookie_age + 2)
            .count();
        assert!(young > 0);
        // The clubs hired among the candidates, a poor one its recruiter first (it may
        // not afford a trainer besides); their salaries, paid for every three fighters,
        // keep their money within bounds (up to 26 000 without them).
        for rival in &state.rivals {
            let staff = &rival.staff;
            let hired = staff.trainer.is_some() || staff.doctor.is_some();
            assert!(hired || staff.recruiter.is_some(), "{staff:?}");
            if rival.wealth == Wealth::Poor {
                assert!(rival.staff.recruiter.is_some());
            }
            assert!(rival.money < 25_000, "{}", rival.money);
        }
        // Clubs of different wealth live side by side, each one's following its money
        // (a rich club that spent too much on fighters is modest now).
        let mut wealths: Vec<Wealth> = state.rivals.iter().map(|r| r.wealth).collect();
        wealths.sort();
        wealths.dedup();
        assert!(wealths.len() >= 2, "{wealths:?}");
        // Free agents fought too, ranked in the colosseum free to enter; some were
        // signed with their record, and some died in the arena.
        let fights = |f: &CareerFighter| f.record.wins + f.record.losses + f.record.draws;
        let agents = &state.free_agents;
        assert!(
            agents
                .iter()
                .any(|f| f.status == Status::Active && fights(f) > 0)
        );
        assert!(
            agents
                .iter()
                .any(|f| f.status == Status::Sold && fights(f) > 0)
        );
        assert!(agents.iter().any(|f| f.status == Status::Dead));
        assert!(
            state.colosseums[0]
                .active_ranking(state.rosters())
                .any(|r| matches!(r.contender, Contender::FreeAgent(_)))
        );
        assert_eq!(state.date(&data).season, 4);
    }

    #[test]
    fn rival_clubs_hire_the_candidates_their_money_can_keep_and_pay_them() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.fighters[0].status = Status::Retired;
        state.rivals[0].money = 100_000;
        state.rivals[1].money = 0;
        state.staff_market.clear();
        for (role, level) in [(Role::Trainer, 3), (Role::Trainer, 5), (Role::Doctor, 5)] {
            offered(&mut state, role, level, &data);
        }
        next_week(&mut state, &Decisions::default(), &data);
        let staff = &data.economy.staff;
        let best = Staff::of_levels(&[(Role::Trainer, 5), (Role::Doctor, 5)], staff);
        assert_eq!(state.rivals[0].staff, best);
        assert_eq!(state.rivals[1].staff, Staff::default());
        // The salaries are paid each week: broke, the club lets its staff go.
        let salaries: u32 = best.salaries().sum();
        state.rivals[0].money = i64::from(salaries) - 1;
        next_week(&mut state, &Decisions::default(), &data);
        assert_eq!(state.rivals[0].staff, Staff::default());
    }

    #[test]
    fn free_agents_fight_where_entry_is_free_and_take_their_elo_to_the_club() {
        let data = data();
        let mut state = GameState::new(7, &data);
        state.fighters[0].status = Status::Retired;
        // Fit free agents register in Capua, free to enter, never in Rome.
        let plans = state.free_agent_plans(&data);
        assert!(
            plans
                .iter()
                .all(|p| matches!(p.activity, Activity::Rest | Activity::Fight(ColosseumId(0))))
        );
        assert!(
            plans
                .iter()
                .any(|p| p.activity == Activity::Fight(ColosseumId(0)))
        );
        assert!(plans.iter().all(|p| !p.care));
        let money: Vec<i64> = state.rivals.iter().map(|r| r.money).collect();
        while !state.colosseums[0]
            .ranking
            .iter()
            .any(|r| matches!(r.contender, Contender::FreeAgent(_)))
        {
            assert!(state.week < 10, "no free agent fought");
            next_week(&mut state, &Decisions::default(), &data);
        }
        assert_ne!(
            state.rivals.iter().map(|r| r.money).collect::<Vec<_>>(),
            money
        );
        let ranked = state.colosseums[0]
            .ranking
            .iter()
            .find_map(|r| match r.contender {
                Contender::FreeAgent(a) if state.free_agents[a].status == Status::Active => {
                    Some((a, r.elo))
                }
                _ => None,
            });
        let (a, elo) = ranked.expect("a ranked free agent still on the market");
        // Its first fight may be the season-opening melee, a draw at the time limit.
        let record = state.free_agents[a].record;
        assert!(record.wins + record.losses + record.draws > 0);
        // Signed, it keeps its Elo, now the club's; released, it takes it back.
        state.fighters[0].status = Status::Active;
        let stay = Decisions {
            plans: vec![REST, REST],
            ..moves(&[Move::Sign(a)])
        };
        next_week(&mut state, &stay, &data);
        assert_eq!(state.free_agents[a].status, Status::Sold);
        let club = &state.colosseums[0];
        assert_eq!(club.elo(Contender::Club(1), &data.world), elo);
        assert!(
            !club
                .ranking
                .iter()
                .any(|r| r.contender == Contender::FreeAgent(a))
        );
        let release = Decisions {
            plans: vec![REST, REST],
            ..moves(&[Move::Release(1)])
        };
        // Back on the market before the free agents who arrive at the end of the week.
        let released = state.free_agents.len();
        next_week(&mut state, &release, &data);
        let back = Contender::FreeAgent(released);
        let rank = state.colosseums[0]
            .ranking
            .iter()
            .find(|r| r.contender == back);
        assert!(rank.is_some() || state.free_agents[released].status != Status::Active);
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
        let mut only = OutsiderPlans {
            rivals: state
                .rivals
                .iter()
                .map(|r| vec![REST; r.fighters.len()])
                .collect(),
            free_agents: vec![REST; state.free_agents.len()],
        };
        let capua = Activity::Fight(ColosseumId(0));
        let one = Contender::Rival(RivalId {
            club: 1,
            fighter: 0,
        });
        only.set_activity(one, capua);
        let mut fought = Vec::new();
        let world = (
            &state.colosseums[..],
            &state.rivals[..],
            &state.free_agents[..],
        );
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
            Some((0, Contender::Rival(four)))
        );
        // A free agent registered there is met too.
        let agent = Contender::FreeAgent(2);
        only.set_activity(agent, capua);
        fought.push(one);
        assert_eq!(
            opponent(&mut rng, world, capua, &only, &fought),
            Some((0, agent))
        );
        fought.push(agent);
        assert!(opponent(&mut rng, world, capua, &only, &fought).is_some_and(|(_, r)| r != one));
    }
}
