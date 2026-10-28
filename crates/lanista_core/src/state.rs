//! The whole game: the player's club, the colosseums, the rival clubs, the market and the week
//! to play. One week is one call to [`next_week`]; the interface only builds [`Decisions`] and
//! reads the state.

use std::collections::BTreeMap;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::calendar::Date;
use crate::colosseum::{Colosseum, ColosseumId, Contender, Continent, Odds, Rosters, TitleNews};
use crate::design_data::DesignData;
use crate::economy::{Contract, Terms, Wealth};
use crate::fighter::care::CareRules;
use crate::fighter::career::{Activity, CareerFighter, Status, WeekPlan, suggested_plan};
use crate::fighter::equipment::Refused;
use crate::fighter::fame::{Fame, FightResult};
use crate::fighter::percent_of;
use crate::fighter::traits::Trait;
use crate::journal::Journal;
use crate::market::{self, Move, SaleOffer, Target, value};
use crate::names::{self, ClubName, FighterName};
use crate::relations::{Club, Reason, Relation, Standing, StandingRules, TieRules};
use crate::rivals::{self, Rival, RivalId, Venue};
use crate::staff::{
    self, Candidate, DoctorRules, Medicine, RecruiterRules, Scouting, Staff, StaffEffects,
    StaffMove, TrainerRules,
};

mod clubs;
mod colosseums;
mod melee;
mod outsiders;
mod transfers;
mod week;

use outsiders::OutsiderPlans;

pub use week::{Decisions, WeekSummary, next_week};

const REST: WeekPlan = WeekPlan {
    activity: Activity::Rest,
    care: false,
    refused: Refused::NONE,
    loadout: 0,
    focus: None,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    /// The player's club's name, chosen or drawn at the start; no rival club takes it.
    /// Empty in a save from before the club had one, until it is loaded
    /// (`GameState::open_world`).
    #[serde(default)]
    pub name: ClubName,
    /// The key of the culture its name is of, in `data/names.ron`.
    #[serde(default)]
    pub culture: String,
    /// Number of the next week to play, from 1.
    pub week: u64,
    /// The club's money; below 0 when upkeep ran it into debt.
    pub money: i64,
    /// Chosen at the start, then changing with the money at each off-season
    /// (`Wealth::next`).
    pub wealth: Wealth,
    /// The club's home, chosen at the start: registering elsewhere costs the journey.
    pub continent: Continent,
    /// The player's club; a fighter's index is its place here. One who leaves keeps
    /// its place, with its status.
    pub fighters: Vec<CareerFighter>,
    /// In the order of `data/world.ron`.
    pub colosseums: Vec<Colosseum>,
    /// The rival clubs and their fighters, drawn when the world opens.
    pub rivals: Vec<Rival>,
    /// Everyone who came to the market, signed for nothing. The free agents are those
    /// still active, never fewer than `MarketRules::min_free_agents` at the start of a
    /// week; like the clubs' fighters, one who leaves keeps its place, with its status:
    /// `Sold` once signed, as a moved entry (`CareerFighter::moved_to`), `Retired` if it
    /// gave up waiting for a club.
    #[serde(with = "crate::fighter::career::entries")]
    pub free_agents: Vec<CareerFighter>,
    /// The club's fighters on the transfer list, by index: they fight on for the club
    /// until the player accepts an offer.
    pub transfer_list: Vec<usize>,
    /// The club's fighters not for sale, by index: no rival club offers for them.
    pub kept: Vec<usize>,
    /// The rival clubs' offers for the club's fighters, open this week.
    pub sale_offers: Vec<SaleOffer>,
    /// The price a rival club asked instead of the player's last offer, open this week.
    pub counter_offer: Option<(RivalId, u32)>,
    pub staff: Staff,
    /// The staff candidates on the market, for the club and the rival clubs alike;
    /// one hired leaves it.
    pub staff_market: Vec<Candidate>,
    /// What happened in the world, oldest first, over the last `WorldRules::news_weeks`.
    pub news: Vec<News>,
    /// Each rival club's relation with the player's club, by its index; a club
    /// missing has a relation of 0.
    pub relations: BTreeMap<usize, Relation>,
    /// The club's weeks, as its screen tells them again; empty in a save from before
    /// it was kept.
    #[serde(default)]
    pub journal: Journal,
    /// The last `CareerFighter::id` given.
    #[serde(default)]
    pub last_fighter_id: u64,
    /// The only source of chance after the start: same seed and decisions, same game.
    pub rng: ChaCha8Rng,
}

impl GameState {
    /// A new game from `seed` with a modest club in Europe (see `start`).
    #[must_use]
    pub fn new(seed: u64, data: &DesignData) -> Self {
        Self::start(seed, (Wealth::Modest, Continent::Europe), None, data)
    }

    /// A new game from `seed`: a club of `wealth` at home on `continent` with one
    /// generated fighter, the same one `CareerFighter::random` draws first from that
    /// seed, its wealth's starting money, the rival clubs with their fighters, the
    /// colosseums and the free agents. The club bears `name`, its culture's key and
    /// its name as the player chose them, or else one drawn in a culture of its
    /// continent once the rival clubs have theirs.
    #[must_use]
    pub fn start(
        seed: u64,
        (wealth, continent): (Wealth, Continent),
        name: Option<(String, ClubName)>,
        data: &DesignData,
    ) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let fighters = vec![CareerFighter::random(&mut rng, data)];
        let (culture, name) = name.unwrap_or_default();
        let mut state = Self {
            name,
            culture,
            week: 1,
            money: i64::from(data.economy.wealth.get(wealth).starting_money),
            wealth,
            continent,
            fighters,
            colosseums: Vec::new(),
            rivals: Vec::new(),
            free_agents: Vec::new(),
            transfer_list: Vec::new(),
            kept: Vec::new(),
            sale_offers: Vec::new(),
            counter_offer: None,
            staff: Staff::default(),
            staff_market: Vec::new(),
            news: Vec::new(),
            relations: BTreeMap::new(),
            journal: Journal::default(),
            last_fighter_id: 0,
            rng,
        };
        state.open_world(data);
        state
    }

    /// Opens the rival clubs and the colosseums of `data` the game does not have yet:
    /// all of them in a new game, those added to `data/world.ron` since an older save.
    /// Rival clubs are drawn up to `clubs.count` (`rivals::random_club`); a new
    /// colosseum with its `residents`, fighters as strong as its prestige shared out
    /// in turn among the rival clubs of its continent (all of them if it has none), all
    /// ranked there; then each new club gets
    /// generated fighters up to `roster.wanted`, ranked nowhere yet. Then brings the free agents up
    /// to their fewest, and the staff candidates up to `StaffMarketRules::candidates`
    /// for each post. Last, names the fighters in activity who have none, and the
    /// player's club if it has none (`name_the_club`), and gives a contract to the
    /// club's fighters in activity who have none: the first one, those of an older save.
    pub fn open_world(&mut self, data: &DesignData) {
        let drawn = self.rivals.len();
        let count = usize::try_from(data.world.clubs.count).unwrap_or(0);
        while self.rivals.len() < count {
            let club = rivals::random_club(&mut self.rng, (&self.rivals, &self.name), data);
            self.rivals.push(club);
        }
        let clubs = self.rivals.len();
        for (c, rules) in data.world.colosseums.iter().enumerate() {
            if c < self.colosseums.len() || clubs == 0 {
                continue;
            }
            let mut homes: Vec<usize> = (0..clubs)
                .filter(|&k| {
                    self.rivals[k].is_open() && self.rivals[k].continent == rules.continent
                })
                .collect();
            if homes.is_empty() {
                homes = (0..clubs).collect();
            }
            let mut ranked = Vec::new();
            for r in 0..usize::try_from(rules.residents).unwrap_or(0) {
                let fighter = CareerFighter::random(&mut self.rng, data);
                let fighter = market::with_prestige(fighter, rules.prestige, data);
                let club = homes[(c + r) % homes.len()];
                let fighter = self.named(fighter, Some(self.rivals[club].continent), data);
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
        for club in drawn..self.rivals.len() {
            self.fill_roster(club, data);
        }
        let fewest = usize::try_from(data.economy.market.min_free_agents).unwrap_or(0);
        while self.available_free_agents() < fewest {
            let agent = market::free_agent(&mut self.rng, data);
            let agent = self.named(agent, None, data);
            self.free_agents.push(agent);
        }
        staff::fill_market(&mut self.staff_market, &data.economy.staff, &mut self.rng);
        self.name_the_nameless(data);
        self.name_the_club(data);
        for i in 0..self.fighters.len() {
            let fighter = &self.fighters[i];
            if fighter.status == Status::Active && fighter.contract.is_none() {
                self.fighters[i].contract = Some(self.contract_for(fighter, data));
            }
        }
    }

    /// The terms `fighter` asks for (`ContractRules::asked_terms`).
    #[must_use]
    pub fn asked_terms(fighter: &CareerFighter, data: &DesignData) -> Terms {
        let economy = &data.economy;
        economy.contracts.asked_terms(fighter, &economy.market)
    }

    /// The contract `fighter` signs with the club this week on its asked terms.
    #[must_use]
    pub fn contract_for(&self, fighter: &CareerFighter, data: &DesignData) -> Contract {
        Self::asked_terms(fighter, data).signed(self.date(data).season)
    }

    /// Names the player's club if it has no name: a culture of its continent, then a
    /// name of it that no open rival club bears.
    fn name_the_club(&mut self, data: &DesignData) {
        if !self.name.name.is_empty() {
            return;
        }
        let key = rivals::culture_on(&mut self.rng, Some(self.continent), data);
        let Some(culture) = key.and_then(|key| data.culture(&key)) else {
            return;
        };
        let taken: Vec<&ClubName> = self
            .rivals
            .iter()
            .filter(|r| r.is_open())
            .map(|r| &r.name)
            .collect();
        self.name = names::club_name(&mut self.rng, culture, &taken);
        self.culture.clone_from(&culture.key);
    }

    /// Rival club `club` gets generated fighters up to `roster.wanted`, ranked nowhere.
    fn fill_roster(&mut self, club: usize, data: &DesignData) {
        let wanted = usize::try_from(data.world.roster.wanted).unwrap_or(0);
        while self.rivals[club].active() < wanted {
            let fighter = CareerFighter::random(&mut self.rng, data);
            let fighter = self.named(fighter, Some(self.rivals[club].continent), data);
            self.rivals[club].fighters.push(fighter);
        }
    }

    /// `fighter` named as it comes into the world: the next `id`, a culture drawn on
    /// `continent` (on any without one, `rivals::culture_on`), then one of its
    /// fighters' names that no fighter in activity bears (`names::fighter_name`).
    fn named(
        &mut self,
        mut fighter: CareerFighter,
        continent: Option<Continent>,
        data: &DesignData,
    ) -> CareerFighter {
        self.last_fighter_id += 1;
        fighter.id = self.last_fighter_id;
        let culture = rivals::culture_on(&mut self.rng, continent, data);
        let Some(culture) = culture.and_then(|key| data.culture(&key)) else {
            return fighter;
        };
        let rivals = self.rivals.iter().flat_map(|r| &r.fighters);
        let taken: Vec<&FighterName> = self
            .fighters
            .iter()
            .chain(rivals)
            .chain(&self.free_agents)
            .filter(|f| f.status == Status::Active && f.moved_to.is_none())
            .map(|f| &f.name)
            .collect();
        fighter.name = names::fighter_name(&mut self.rng, culture, &taken);
        fighter.culture.clone_from(&culture.key);
        fighter
    }

    /// Names the fighters in activity who have none: the club's first, and all those
    /// of a game saved before fighters had names. The club's are of its continent, a
    /// rival club's of its own, the free agents of any.
    fn name_the_nameless(&mut self, data: &DesignData) {
        let nameless = |f: &CareerFighter| {
            f.status == Status::Active && f.moved_to.is_none() && f.name.is_empty()
        };
        for i in 0..self.fighters.len() {
            if nameless(&self.fighters[i]) {
                let fighter = self.fighters[i].clone();
                self.fighters[i] = self.named(fighter, Some(self.continent), data);
            }
        }
        for club in 0..self.rivals.len() {
            let home = self.rivals[club].continent;
            for i in 0..self.rivals[club].fighters.len() {
                if nameless(&self.rivals[club].fighters[i]) {
                    let fighter = self.rivals[club].fighters[i].clone();
                    self.rivals[club].fighters[i] = self.named(fighter, Some(home), data);
                }
            }
        }
        for a in 0..self.free_agents.len() {
            if nameless(&self.free_agents[a]) {
                let fighter = self.free_agents[a].clone();
                self.free_agents[a] = self.named(fighter, None, data);
            }
        }
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

    /// Where `contender` fights now: itself, or where it went by its moved entries.
    #[must_use]
    pub fn whereabouts(&self, contender: Contender) -> Contender {
        let mut here = contender;
        while let Some(to) = self.rosters().fighter(here).and_then(|f| f.moved_to) {
            here = to;
        }
        here
    }

    /// Gives the Elo and titles of `from` to `to` in every colosseum: the same fighter,
    /// gone to another club or to the market. A rival club's fighter or a free agent
    /// leaves only a moved entry behind; the player's club keeps its own whole. Gone
    /// from a club, its tie with it moves by `TieRules::released` to the market, by
    /// `sold` to another club.
    fn transfer(&mut self, from: Contender, to: Contender, rules: &TieRules) {
        for colosseum in &mut self.colosseums {
            colosseum.transfer(from, to);
        }
        let club = match from {
            Contender::Club(_) => Some(Club::Player),
            Contender::Rival(id) => Some(Club::Rival(id.club)),
            Contender::FreeAgent(_) => None,
        };
        let arrived = match to {
            Contender::Club(i) => self.fighters.get_mut(i),
            Contender::Rival(to) => self
                .rivals
                .get_mut(to.club)
                .and_then(|r| r.fighters.get_mut(to.fighter)),
            Contender::FreeAgent(a) => self.free_agents.get_mut(a),
        };
        if let (Some(club), Some(fighter)) = (club, arrived) {
            let points = match to {
                Contender::FreeAgent(_) => rules.released,
                _ => rules.sold,
            };
            fighter.change_tie(club, points);
        }
        let left = match from {
            Contender::Club(_) => None,
            Contender::Rival(id) => self
                .rivals
                .get_mut(id.club)
                .and_then(|r| r.fighters.get_mut(id.fighter)),
            Contender::FreeAgent(a) => self.free_agents.get_mut(a),
        };
        if let Some(fighter) = left {
            *fighter = fighter.moved(to);
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

    /// Each colosseum's [`Odds`], in their order, as the week starts.
    fn odds(&self, data: &DesignData) -> Vec<Odds> {
        let rosters = self.rosters();
        self.colosseums
            .iter()
            .map(|c| c.odds(rosters, &data.world))
            .collect()
    }

    /// Each colosseum as `contender` sees it, in their order: the chance Elo gives it
    /// there (`odds`, see `Colosseum::chance`), and the fee from `home`, its club's continent, the
    /// journey included; a free agent, `None`, travels for nothing and is at home
    /// everywhere; whether `fighter`, the contender, finds its lucky floor there, or a
    /// fighter it bears a grudge against in the ranking.
    fn venues(
        &self,
        odds: &[Odds],
        (contender, fighter): (Contender, &CareerFighter),
        home: Option<Continent>,
        data: &DesignData,
    ) -> Vec<Venue> {
        let world = &data.world;
        let rosters = self.rosters();
        odds.iter()
            .zip(&world.colosseums)
            .zip(&self.colosseums)
            .zip(0..)
            .map(|(((odds, rules), colosseum), id)| Venue {
                chance: odds.chance(contender),
                fee: home
                    .and_then(|home| world.entry_fee(ColosseumId(id), home))
                    .unwrap_or(rules.purses.entry_fee),
                home: home.is_none_or(|home| home == rules.continent),
                lucky: fighter.has(Trait::Superstitious)
                    && fighter.lucky_floor == Some(rules.arena),
                revenge: colosseum.ranking.iter().any(|r| {
                    rosters
                        .fighter(r.contender)
                        .is_some_and(|o| fighter.bears_grudge(o.id))
                }),
            })
            .collect()
    }

    /// What the club pays to register a fighter in colosseum `id`, the journey
    /// included (`WorldRules::entry_fee`).
    #[must_use]
    pub fn entry_fee(&self, id: ColosseumId, data: &DesignData) -> Option<u32> {
        data.world.entry_fee(id, self.continent)
    }

    /// The rival fighter `id`.
    #[must_use]
    pub fn rival_fighter(&self, id: RivalId) -> Option<&CareerFighter> {
        self.rivals.get(id.club)?.fighters.get(id.fighter)
    }

    /// Each rival fighter's plan this week, by club, from its club's rules
    /// (`rivals::plan`); each club's money, but the week's upkeep, goes to its fighters
    /// in roster order.
    #[must_use]
    pub fn rival_plans(&self, data: &DesignData) -> Vec<Vec<WeekPlan>> {
        let odds = self.odds(data);
        self.rivals
            .iter()
            .enumerate()
            .map(|(club, rival)| {
                let upkeep = i64::try_from(rival.active()).unwrap_or(i64::MAX);
                let mut left = rival.money - upkeep * i64::from(data.economy.upkeep);
                let profile = Some(rival.rules(data));
                (0..)
                    .zip(&rival.fighters)
                    .map(|(fighter, f)| match profile {
                        Some(profile) if f.status == Status::Active => {
                            let me = Contender::Rival(RivalId { club, fighter });
                            let venues = self.venues(&odds, (me, f), Some(rival.continent), data);
                            let rules = (profile, &rival.staff, true);
                            rivals::plan(f, &venues, &mut left, rules, data)
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
        let odds = self.odds(data);
        (0..)
            .zip(&self.free_agents)
            .map(|(a, f)| {
                if f.status != Status::Active {
                    return REST;
                }
                let venues = self.venues(&odds, (Contender::FreeAgent(a), f), None, data);
                rivals::plan(
                    f,
                    &venues,
                    &mut 0,
                    (profile, &Staff::default(), false),
                    data,
                )
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

    /// `career::suggested_plan` for the club's fighter `i`, its fights in the first
    /// colosseum of the club's continent (`WorldRules::first_colosseum`), who defends a
    /// title instead
    /// of fighting or training in the last two weeks before its deadline.
    #[must_use]
    pub fn suggested_plan(&self, i: usize, data: &DesignData) -> WeekPlan {
        let Some(fighter) = self.fighters.get(i) else {
            return REST;
        };
        let mut plan = suggested_plan(fighter, self.week);
        if plan.activity == Activity::Fight(ColosseumId(0)) {
            plan.activity = Activity::Fight(data.world.first_colosseum(self.continent));
        }
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
        plan
    }

    /// Whether the club's fighter `i` may fight `rival` in colosseum `id` in a
    /// challenge: the rival fighter still fights and its club accepts the challenge.
    #[must_use]
    pub fn may_challenge(
        &self,
        i: usize,
        id: ColosseumId,
        rival: RivalId,
        data: &DesignData,
    ) -> bool {
        let active = self
            .rival_fighter(rival)
            .is_some_and(|f| f.status == Status::Active);
        let accepts = || {
            let colosseum = usize::try_from(id.0)
                .ok()
                .and_then(|c| self.colosseums.get(c));
            colosseum
                .zip(self.rivals.get(rival.club).map(|r| r.rules(data)))
                .is_some_and(|(colosseum, profile)| {
                    let points = self.standing_rules(rival.club, data).challenge_points;
                    let needed = profile.accept_percent.saturating_sub(points);
                    colosseum.accepts_challenge(rival, i, needed, &data.world)
                })
        };
        active && accepts()
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

    /// The colosseum and the outsiders the club's fighter `i` may meet in a fight, a
    /// title fight or a challenge (`activity`) if the week were played now, one of them
    /// at random; `None` for any other activity. The other club fighters' fights of the
    /// week, and the market, may still change it.
    #[must_use]
    pub fn likely_opponents(
        &self,
        i: usize,
        activity: Activity,
        data: &DesignData,
    ) -> Option<(usize, Vec<Contender>)> {
        if let Activity::TitleFight(id) = activity {
            return self.title_opponent(i, id, data).map(|(c, o)| (c, vec![o]));
        }
        let me = self.fighters.get(i)?;
        let world = (
            self.colosseums.as_slice(),
            self.rivals.as_slice(),
            self.free_agents.as_slice(),
            &data.world,
        );
        let plans = self.outsider_plans(data);
        outsiders::likely_opponents(world, (activity, i, me), &plans, &[])
    }

    /// The price the club of rival fighter `id` first asks: its value at the club's
    /// `sell_percent`. Hidden from the player, who learns it from a counter-offer.
    #[must_use]
    pub fn asking_price(&self, id: RivalId, data: &DesignData) -> Option<u32> {
        let fighter = self
            .rival_fighter(id)
            .filter(|f| f.status == Status::Active && self.may_be_sold(f, data))?;
        let profile = self.rivals.get(id.club)?.rules(data);
        Some(percent_of(
            value(fighter, &data.economy.market),
            profile.sell_percent,
        ))
    }

    /// The price the club of rival fighter `id` first asks the player: its asking price
    /// at its standing's `price_percent`; `None` if it does not trade with the player.
    #[must_use]
    pub fn price_to_player(&self, id: RivalId, data: &DesignData) -> Option<u32> {
        let standing = self.standing_rules(id.club, data);
        standing
            .deals
            .then(|| self.asking_price(id, data))
            .flatten()
            .map(|price| percent_of(price, standing.price_percent))
    }

    /// What rival club `club` thinks of the player's club, from −100 to +100.
    #[must_use]
    pub fn relation(&self, club: usize) -> i32 {
        self.relations.get(&club).map_or(0, |r| r.score)
    }

    #[must_use]
    /// What rival club `club`'s relation comes to: a rival while the rivalry it
    /// declared lasts, whatever its score.
    pub fn standing(&self, club: usize, data: &DesignData) -> Standing {
        if self.is_rival(club) {
            Standing::Rival
        } else {
            data.world.relations.standings.of(self.relation(club))
        }
    }

    /// Whether rival club `club` declared itself the player's rival.
    #[must_use]
    pub fn is_rival(&self, club: usize) -> bool {
        self.relations
            .get(&club)
            .is_some_and(|r| r.rival_since.is_some())
    }

    fn standing_rules<'a>(&self, club: usize, data: &'a DesignData) -> &'a StandingRules {
        data.world
            .relations
            .standings
            .get(self.standing(club, data))
    }

    /// The first week `fighter` may be sold: half a season after it joined its club,
    /// so that no club sells on at once whom it just took in.
    #[must_use]
    pub fn sellable_from(fighter: &CareerFighter, data: &DesignData) -> u64 {
        let half = u64::from(data.calendar.season_weeks() / 2);
        fighter.joined.map_or(0, |week| week + half)
    }

    #[must_use]
    pub fn may_be_sold(&self, fighter: &CareerFighter, data: &DesignData) -> bool {
        self.week >= Self::sellable_from(fighter, data)
    }

    /// Whether rival club `club` has a place for one more fighter in activity.
    #[must_use]
    pub fn rival_has_place(&self, club: usize, data: &DesignData) -> bool {
        self.rivals
            .get(club)
            .filter(|r| r.is_open())
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
            Move::Sign(a, _) => active(self.free_agents.get(a)) && self.can_hire(data),
            Move::Release(i) => ours(i),
            Move::List(i) => {
                ours(i)
                    && self.may_be_sold(&self.fighters[i], data)
                    && !self.transfer_list.contains(&i)
                    && !self.kept.contains(&i)
            }
            Move::Unlist(i) => self.transfer_list.contains(&i),
            Move::Keep(i) => ours(i) && !self.kept.contains(&i) && !self.transfer_list.contains(&i),
            Move::Unkeep(i) => self.kept.contains(&i),
            Move::Renew(i, _) => {
                let season = self.date(data).season;
                ours(i) && self.fighters[i].contract.is_some_and(|c| c.ends <= season)
            }
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
                self.price_to_player(id, data).is_some()
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

/// Something that happened in the world, for the news feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldNews {
    /// The title of the colosseum at this index changed hands.
    Title(usize, TitleNews),
    /// Rival club `buyer` bought `fighter`, champion of the colosseum at index
    /// `colosseum`, for `price`; `fighter` is its place in its former club.
    ChampionBought {
        colosseum: usize,
        fighter: RivalId,
        buyer: usize,
        price: u32,
    },
    /// The rival club at this index closed, ruined.
    Closed(usize),
    /// The rival club at this index was founded.
    Founded(usize),
    /// The rival club at this index declared itself the player's rival (`true`), or
    /// ended its rivalry.
    Rivalry(usize, bool),
}

/// A world news item of week `week`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct News {
    pub week: u64,
    pub what: WorldNews,
}

/// Rival club `club`'s relation with the player's club moves for `why` this week.
fn relate(state: &mut GameState, club: usize, why: Reason, data: &DesignData) {
    let Some(rival) = state.rivals.get(club) else {
        return;
    };
    let profile = rival.rules(data);
    let relation = state.relations.entry(club).or_default();
    relation.change((state.week, why), profile, &data.world.relations);
}

/// Each fighter's wanted plan, `GameState::suggested_plan` without one, and the colosseum
/// and outsider of its title fight. Title fights are settled before anyone plays, from
/// the rankings as they stand; one the fighter may not have turns into rest, and so does
/// a challenge it may not have. An outsider meets only one of the club's fighters in a
/// title fight or a challenge: the first in club order; the others rest.
fn week_plans(
    state: &GameState,
    decisions: &Decisions,
    data: &DesignData,
) -> (Vec<WeekPlan>, Vec<Option<(usize, Contender)>>) {
    let mut taken = Vec::new();
    (0..state.fighters.len())
        .map(|i| {
            let mut plan = decisions
                .plans
                .get(i)
                .copied()
                .unwrap_or_else(|| state.suggested_plan(i, data));
            let mut meeting = match plan.activity {
                Activity::TitleFight(id) => state.title_opponent(i, id, data),
                _ => None,
            };
            let met = match plan.activity {
                Activity::TitleFight(_) => meeting.map(|(_, them)| them),
                Activity::Challenge(_, rival) => Some(Contender::Rival(rival)),
                _ => None,
            };
            let allowed = !met.is_some_and(|them| taken.contains(&them))
                && match plan.activity {
                    Activity::TitleFight(_) => meeting.is_some(),
                    Activity::Challenge(id, rival) => state.may_challenge(i, id, rival, data),
                    Activity::Melee(_) => state.opens_season(data),
                    _ => true,
                };
            if allowed {
                taken.extend(met);
            } else {
                plan.activity = Activity::Rest;
                meeting = None;
            }
            (plan, meeting)
        })
        .unzip()
}

/// What a club of `home` pays to register a fighter in colosseum `c`, the journey
/// included; nothing for a colosseum missing from the data.
fn entry_fee(c: usize, home: Continent, data: &DesignData) -> u32 {
    u32::try_from(c)
        .ok()
        .and_then(|c| data.world.entry_fee(ColosseumId(c), home))
        .unwrap_or(0)
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
/// `medicine`, then the activity, a training session costing `training_cost`, a fight
/// its entry fee from the club's `home` (`WorldRules::entry_fee`).
fn affordable(
    mut plan: WeekPlan,
    fighter: &CareerFighter,
    (money, home): (i64, Continent),
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
            data.world.entry_fee(id, home)
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
    use super::melee::{melee_entries, pact_reason};
    use super::outsiders::opponent;
    use super::transfers::{
        buy_from_rival, market_moves, release_rival, rich_purchases, rival_signings, weakest,
    };
    use super::*;
    use crate::colosseum::title_kept;
    use crate::combat::ArenaKind;
    use crate::economy::Flow;
    use crate::fighter::injury::{Injury, Location, Severity};
    use crate::fighter::melee::Pact;
    use crate::market::MarketNews;
    use crate::rivals::Profile;
    use crate::staff::{Person, Role, StaffNews};
    use rand::RngExt;

    /// The shipped data cut down to the world most tests were written for: three rival
    /// clubs, all in Europe, that never close nor are joined by new ones, and its first
    /// two colosseums, Capoue and Rome, of fifteen residents each.
    fn data() -> DesignData {
        let mut data = DesignData::load().unwrap();
        let world = &mut data.world;
        world.clubs.count = 3;
        world.clubs.founded = 0;
        world.clubs.ruined_seasons = u32::MAX;
        world.clubs.continents = vec![(1, Continent::Europe)];
        world.colosseums.truncate(2);
        for colosseum in &mut world.colosseums {
            colosseum.residents = 15;
        }
        data
    }

    /// A game from `seed` with a modest club (see `game_of`).
    fn game(seed: u64, data: &DesignData) -> GameState {
        game_of(seed, Wealth::Modest, data)
    }

    /// A game from `seed` with a club of `wealth`, whose rival clubs are, in order, a
    /// cautious and rich one, an aggressive and poor one, then a modest merchant, each
    /// with its wealth's starting money.
    fn game_of(seed: u64, wealth: Wealth, data: &DesignData) -> GameState {
        let mut state = GameState::start(seed, (wealth, Continent::Europe), None, data);
        let clubs = [
            (Profile::Cautious, Wealth::Rich),
            (Profile::Aggressive, Wealth::Poor),
            (Profile::Merchant, Wealth::Modest),
        ];
        for (rival, (profile, wealth)) in state.rivals.iter_mut().zip(clubs) {
            rival.profile = profile;
            rival.wealth = wealth;
            rival.money = i64::from(data.economy.wealth.get(wealth).starting_money);
        }
        state
    }

    fn plan(activity: Activity, care: bool) -> Decisions {
        Decisions {
            plans: vec![WeekPlan {
                activity,
                care,
                refused: Refused::NONE,
                loadout: 0,
                focus: None,
            }],
            ..Decisions::default()
        }
    }

    /// Signing free agent `a` on the terms it asks for.
    fn sign(state: &GameState, a: usize, data: &DesignData) -> Move {
        Move::Sign(a, GameState::asked_terms(&state.free_agents[a], data))
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

    /// Why rival club `club`'s relation moved, oldest first.
    fn reasons(state: &GameState, club: usize) -> Vec<Reason> {
        state
            .relations
            .get(&club)
            .map_or_else(Vec::new, |r| r.changes.iter().map(|c| c.why).collect())
    }

    #[test]
    fn the_season_opens_with_a_melee_in_every_colosseum_even_without_the_club() {
        let data = data();
        let mut state = game(3, &data);
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
        let mut state = game(2, &data);
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        state.fighters.push(CareerFighter::random(&mut rng, &data));
        let capua = Activity::Melee(ColosseumId(0));
        let entered = WeekPlan {
            activity: capua,
            care: false,
            refused: Refused::NONE,
            loadout: 0,
            focus: None,
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
        // The journal keeps the week, and its melee plays again from a reloaded save.
        let reloaded = crate::save::from_json(&crate::save::to_json(&state), &data).unwrap();
        let last = reloaded.journal.last.as_ref().unwrap();
        assert_eq!((last.week, &last.summary), (1, &summary));
        let mut replay = fights[0].start(&data);
        let mut ais = crate::duel::ais(&replay);
        let ticks = replay.max_ticks;
        crate::combat::run(&mut replay, &mut ais, ticks);
        assert_eq!(crate::log::Report::new(&replay), fights[0].report);
        let entries: Vec<usize> = reloaded.journal.fighters.iter().map(Vec::len).collect();
        assert_eq!(entries, [1, 1]);
        let size = fights[0].setup.fighters.len();
        assert!((3..=20).contains(&size));
        let purses = &data.world.colosseums[0].purses;
        for (i, fight) in fights.iter().enumerate() {
            let money: Vec<(Flow, i64)> = summary
                .ledger
                .iter()
                .filter(|t| t.fighter == Some(i))
                .map(|t| (t.flow, t.amount))
                .collect();
            // Its place's share, whoever it is.
            let paid = (size, data.combat.melee.paid_places);
            let share = i64::from(purses.melee_receipts(fight.place, paid));
            assert_eq!(
                money.contains(&(Flow::Receipts, share)),
                share != 0,
                "{money:?}"
            );
            assert!(!money.iter().any(|&(flow, _)| flow == Flow::WinPrize));
            assert!(money.contains(&(Flow::Upkeep, -i64::from(data.economy.upkeep))));
            // Ranked in Capua from its place, which may leave its Elo where it started,
            // unless it died there.
            let ranking = &state.colosseums[0].ranking;
            let ranked = ranking.iter().any(|r| r.contender == Contender::Club(i));
            assert_eq!(ranked, !fight.died);
        }
    }

    #[test]
    fn a_new_game_starts_at_week_one_with_the_seed_s_fighter() {
        let data = data();
        let state = game(7, &data);
        assert_eq!(state.week, 1);
        assert_eq!(state.date(&data), Date { season: 1, week: 1 });
        let wealth = &data.economy.wealth;
        assert_eq!(state.wealth, Wealth::Modest);
        assert_eq!(state.money, i64::from(wealth.modest.starting_money));
        // Named as it comes into the world, by a culture of the club's continent.
        let mut fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(7), &data);
        let named = &state.fighters[0];
        let culture = data.culture(&named.culture).unwrap();
        assert_eq!(culture.continent, state.continent);
        assert!(culture.fighters.contains(&named.name.name), "{named:?}");
        fighter.id = named.id;
        fighter.name.clone_from(&named.name);
        fighter.culture.clone_from(&named.culture);
        // Under contract until the third season's off-season.
        fighter.contract = Some(state.contract_for(&fighter, &data));
        assert_eq!(named.contract.map(|c| c.ends), Some(3));
        assert_eq!(state.fighters, [fighter]);
        // A rich club starts with more money, and the same fighter.
        let rich = game_of(7, Wealth::Rich, &data);
        assert_eq!(rich.money, i64::from(wealth.rich.starting_money));
        assert_eq!(rich.fighters, state.fighters);
        // The rival clubs start with their wealth's money.
        let state = GameState::new(7, &data);
        for rival in &state.rivals {
            assert_eq!(
                rival.money,
                i64::from(wealth.get(rival.wealth).starting_money)
            );
        }
    }

    #[test]
    fn the_rival_clubs_open_with_the_colosseums_fighters() {
        let data = data();
        let state = game(7, &data);
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
    fn the_club_bears_a_name_of_its_culture_or_the_one_chosen_and_no_rival_takes_it() {
        let data = DesignData::load().unwrap();
        let start = |name| GameState::start(7, (Wealth::Modest, Continent::Asia), name, &data);
        let drawn = start(None);
        let culture = data.culture(&drawn.culture).unwrap();
        assert_eq!(culture.continent, Continent::Asia);
        assert!(culture.names.contains(&drawn.name.name), "{:?}", drawn.name);
        assert!(
            culture.clubs.contains(&drawn.name.pattern),
            "{:?}",
            drawn.name
        );
        assert!(drawn.rivals.iter().all(|r| r.name != drawn.name));
        // The name a rival club would have drawn, chosen by the player, is the club's.
        let wanted = (
            drawn.rivals[0].culture.clone(),
            drawn.rivals[0].name.clone(),
        );
        let chosen = start(Some(wanted.clone()));
        assert_eq!((chosen.culture.clone(), chosen.name.clone()), wanted);
        assert!(chosen.rivals.iter().all(|r| r.name != chosen.name));
        // A game saved before the club had a name gets one when loaded.
        let mut old = drawn.clone();
        old.name = ClubName::default();
        old.culture.clear();
        old.open_world(&data);
        let culture = data.culture(&old.culture).unwrap();
        assert_eq!(culture.continent, Continent::Asia);
        assert!(culture.names.contains(&old.name.name), "{:?}", old.name);
    }

    #[test]
    fn the_world_opens_with_clubs_drawn_from_every_culture() {
        let data = DesignData::load().unwrap();
        let state = GameState::new(7, &data);
        let clubs = &data.world.clubs;
        assert_eq!(state.rivals.len(), usize::try_from(clubs.count).unwrap());
        let wanted = usize::try_from(data.world.roster.wanted).unwrap();
        let mut names = Vec::new();
        let mut cultures = Vec::new();
        for rival in &state.rivals {
            // Named by its culture, unlike any other, with a full roster.
            let culture = data.culture(&rival.culture).unwrap();
            assert!(culture.names.contains(&rival.name.name), "{:?}", rival.name);
            assert!(
                culture.clubs.contains(&rival.name.pattern),
                "{:?}",
                rival.name
            );
            assert!(rival.active() >= wanted);
            assert_eq!(
                rival.money,
                i64::from(data.economy.wealth.get(rival.wealth).starting_money)
            );
            names.push(&rival.name);
            cultures.push(rival.culture.as_str());
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), state.rivals.len());
        cultures.sort_unstable();
        cultures.dedup();
        assert!(cultures.len() > 5, "{cultures:?}");
        // Many poor clubs, few rich ones.
        let count = |wealth| state.rivals.iter().filter(|r| r.wealth == wealth).count();
        assert!(count(Wealth::Poor) > count(Wealth::Rich));
        assert!(count(Wealth::Rich) > 0);
        // Each continent has its share of the clubs, each club a culture of its own
        // continent and the residents of its continent's colosseums.
        for continent in Continent::ALL {
            let home: Vec<usize> = (0..state.rivals.len())
                .filter(|&k| state.rivals[k].continent == continent)
                .collect();
            assert_eq!(home.len(), 8, "{continent:?}");
            for &k in &home {
                let culture = data.culture(&state.rivals[k].culture).unwrap();
                assert_eq!(culture.continent, continent);
                // Its fighters are of its continent too.
                for f in &state.rivals[k].fighters {
                    let culture = data.culture(&f.culture).unwrap();
                    assert_eq!(culture.continent, continent);
                    assert!(culture.fighters.contains(&f.name.name), "{:?}", f.name);
                }
            }
            for id in data.world.colosseums_of(continent) {
                let colosseum = &state.colosseums[usize::try_from(id.0).unwrap()];
                assert!(colosseum.ranking.iter().all(|r| match r.contender {
                    Contender::Rival(id) => home.contains(&id.club),
                    _ => false,
                }));
            }
        }
        // The same seed draws the same world.
        assert_eq!(GameState::new(7, &data).rivals, state.rivals);
    }

    #[test]
    fn a_club_fights_at_home_and_pays_the_journey_abroad() {
        let data = DesignData::load().unwrap();
        let world = &data.world;
        let state = GameState::start(7, (Wealth::Modest, Continent::Asia), None, &data);
        // The suggestion sends it to the free colosseum of its continent.
        let samarkand = world.first_colosseum(Continent::Asia);
        let home = world.colosseum(samarkand).unwrap();
        assert_eq!(
            (home.continent, home.purses.entry_fee),
            (Continent::Asia, 0)
        );
        let fit = (1..=2)
            .map(|week| GameState {
                week,
                ..state.clone()
            })
            .map(|s| s.suggested_plan(0, &data).activity)
            .find(|&a| a != Activity::Train);
        assert!(matches!(fit, Some(Activity::Fight(id) | Activity::Melee(id)) if id == samarkand));
        // Rome costs the journey on top of its fee, Capoue the journey alone.
        let (capua, rome) = (ColosseumId(0), ColosseumId(1));
        assert_eq!(state.entry_fee(capua, &data), Some(world.travel_fee));
        assert_eq!(
            state.entry_fee(rome, &data),
            Some(world.colosseums[1].purses.entry_fee + world.travel_fee)
        );
        assert_eq!(state.entry_fee(samarkand, &data), Some(0));
        // The rival clubs plan their fights at home.
        let plans = state.rival_plans(&data);
        let mut home_fights = 0;
        let mut abroad = 0;
        for (rival, plans) in state.rivals.iter().zip(&plans) {
            for plan in plans {
                if let Activity::Fight(id) = plan.activity {
                    if world.colosseum(id).unwrap().continent == rival.continent {
                        home_fights += 1;
                    } else {
                        abroad += 1;
                    }
                }
            }
        }
        assert!(
            home_fights > 10 * abroad,
            "{home_fights} at home, {abroad} abroad"
        );
    }

    #[test]
    fn a_week_follows_the_decisions_and_moves_on() {
        let data = data();
        let mut state = game(7, &data);
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
        let mut by_default = game(7, &data);
        let mut suggested = by_default.clone();
        let decisions = suggested.suggested_decisions(&data);
        next_week(&mut by_default, &Decisions::default(), &data);
        next_week(&mut suggested, &decisions, &data);
        assert_eq!(by_default, suggested);
    }

    #[test]
    fn the_week_goes_on_without_active_fighters() {
        let data = data();
        let mut state = game(7, &data);
        state.fighters[0].status = Status::Dead;
        let before = state.clone();
        let summary = next_week(&mut state, &Decisions::default(), &data);
        assert_eq!(summary.reports, [None]);
        assert_eq!(summary.ledger, []);
        assert_eq!((state.week, state.fighters), (2, before.fighters));
        assert_eq!(state.money, before.money);
    }

    #[test]
    fn a_contract_is_renewed_in_its_last_season_or_ends_at_the_off_season() {
        use crate::fighter::morale::Morale;
        let data = data();
        let rules = &data.economy.contracts;
        let mut state = game(7, &data);
        // Worth enough for its wage not to round.
        state.fighters[0].stats = state.fighters[0].stats.map(|v| v * 10);
        let asked = GameState::asked_terms(&state.fighters[0], &data);
        // Not in its last season yet.
        assert!(!state.may_move(Move::Renew(0, asked), &data));
        let calendar = &data.calendar;
        // The great event's last week: the off-season starts next week.
        state.week = u64::from(calendar.competition_weeks + calendar.great_event_weeks);
        state.fighters[0].contract.as_mut().unwrap().ends = 1;
        let renew = |terms| Decisions {
            moves: vec![Move::Renew(0, terms)],
            ..plan(Activity::Rest, false)
        };
        // A happy fighter takes less than it asks, a bonus making up for it.
        let haggled = Terms {
            wage: percent_of(asked.wage, 100 - rules.haggle_percent),
            win_bonus: asked.wage,
            seasons: 1,
        };
        let mut happy = state.clone();
        happy.fighters[0].morale = Morale::new(100);
        let summary = next_week(&mut happy, &renew(haggled), &data);
        assert_eq!(summary.market, [MarketNews::Renewed(0)]);
        assert_eq!(happy.fighters[0].contract, Some(haggled.signed(1)));
        next_week(&mut happy, &plan(Activity::Rest, false), &data);
        assert_eq!(happy.fighters[0].status, Status::Active);
        // Unhappy, it wants all it asks, and resents being offered less.
        state.fighters[0].morale = Morale::new(rules.renewal_morale);
        let low = Terms {
            win_bonus: 0,
            ..haggled
        };
        let summary = next_week(&mut state, &renew(low), &data);
        assert_eq!(summary.market, [MarketNews::TermsRefused(0)]);
        assert!(state.fighters[0].morale.value() < rules.renewal_morale);
        // Now too unhappy to renew at all: it leaves as the off-season starts.
        let summary = next_week(&mut state, &renew(asked), &data);
        assert_eq!(
            summary.market[..2],
            [MarketNews::RenewalRefused(0), MarketNews::ContractEnded(0)]
        );
        assert_eq!(state.fighters[0].status, Status::Released);
        assert!(!state.may_move(Move::Renew(0, asked), &data));
    }

    #[test]
    fn upkeep_and_training_are_paid() {
        let data = data();
        let economy = &data.economy;
        let mut state = game(7, &data);
        let summary = next_week(&mut state, &plan(Activity::Train, false), &data);
        let cost = i64::from(economy.training_cost);
        let upkeep = i64::from(economy.upkeep);
        let wage = i64::from(state.fighters[0].contract.unwrap().wage);
        let modest = &economy.wealth.modest;
        assert_eq!(
            flows(&summary),
            [
                (Flow::Training, -cost),
                (Flow::Upkeep, -upkeep),
                (Flow::Wage, -wage)
            ]
        );
        assert_eq!(
            state.money,
            i64::from(modest.starting_money) - cost - upkeep - wage
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
        let mut state = game(7, &data);
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
        let mut state = game(7, &data);
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
        let mut alone = game(7, &data);
        // A nonchalant would lose more form, and spare more of it.
        alone.fighters[0].traits.clear();
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
        let mut state = game(7, &data);
        state.money = 0;
        state.fighters[0].injuries.push(Injury {
            location: Location::Body,
            severity: Severity::Medium,
            weeks_left: 4,
        });
        let summary = next_week(&mut state, &plan(Activity::Train, true), &data);
        let report = summary.reports[0].as_ref().unwrap();
        assert_eq!(report.plan, REST);
        let wage = state.fighters[0].contract.unwrap().wage;
        assert_eq!(state.money, -i64::from(data.economy.upkeep + wage));
    }

    #[test]
    fn each_post_is_hired_once_a_week_and_paid_in_turn() {
        let data = data();
        let mut state = game(7, &data);
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
        let mut state = game(7, &data);
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
        let mut state = game(7, &data);
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
    fn a_fight_brings_receipts_and_the_winner_s_prize_and_costs_the_win_bonus() {
        let data = data();
        let colosseum = &data.world.colosseums[0].purses;
        // Even a broke club can fight: the first colosseum is free.
        let mut state = game(2, &data);
        state.money = 0;
        // Twice as strong as a generated fighter, so that it wins some.
        state.fighters[0].stats = state.fighters[0].stats.map(|v| v * 2);
        state.fighters[0].contract.as_mut().unwrap().win_bonus = 7;
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
            let bonus = (Flow::WinBonus, -7);
            assert_eq!(flows.contains(&bonus), fight.result == FightResult::Won);
            won |= fight.result == FightResult::Won;
        }
        assert!(won, "a strong fighter wins a fight in 20 weeks");
    }

    #[test]
    fn a_fight_meets_a_rival_fighter_and_moves_both_elo() {
        let data = data();
        let rules = &data.world;
        let mut state = game(7, &data);
        // Past the season's opening, whose melees keep the rival fighters busy.
        state.week = 2;
        let rome = ColosseumId(1);
        // Shown before the week: the opponent is one of them.
        let (c, likely) = state
            .likely_opponents(0, Activity::Fight(rome), &data)
            .unwrap();
        assert_eq!(c, 1);
        let likely: Vec<_> = (likely.iter())
            .map(|&o| state.rosters().fighter(o).unwrap().potential)
            .collect();
        assert_eq!(state.likely_opponents(0, Activity::Rest, &data), None);
        let summary = next_week(&mut state, &plan(Activity::Fight(rome), false), &data);
        let fight = summary.reports[0].as_ref().unwrap().fight.as_ref().unwrap();
        assert!(likely.contains(&fight.opponent().unwrap().potential));
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
        // Rome's ranking before the fight is kept, to set against the one after it.
        let [(id, before)] = summary.rankings_before.as_slice() else {
            panic!("{:?}", summary.rankings_before);
        };
        assert_eq!(*id, ColosseumId(1));
        assert_ne!(*before, state.colosseums[1].ranking);
    }

    /// A game of seed 7 whose fighter leads Rome's ranking.
    fn top_of_rome(data: &DesignData) -> GameState {
        let mut state = game(1, data);
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
        // Its club holds it against the player.
        let Contender::Rival(held) = champion else {
            panic!("a rival champion, not {champion:?}");
        };
        let taken = Reason::TitleTaken {
            fighter: 0,
            colosseum: 1,
        };
        assert_eq!(reasons(&state, held.club).contains(&taken), won);
        // Only a title changing hands is news.
        let told = (state.news.iter())
            .filter(|n| matches!(n.what, WorldNews::Title(..)))
            .map(|n| (n.week, n.what));
        let changed = won.then_some((1, WorldNews::Title(1, news)));
        assert_eq!(told.collect::<Vec<_>>(), Vec::from_iter(changed));
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
        // No title fight for whoever may not have one: a week of rest (on a fresh world:
        // the defense may have left the fighter dead).
        let mut state = top_of_rome(&data);
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
        let mut state = game(7, &data);
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
        let mut state = game(2, &data);
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
        // Beaten or killed, its club holds it against the player.
        let why = match (fight.result, state.rival_fighter(rival).unwrap().status) {
            (_, Status::Dead) => vec![Reason::Killed {
                fighter: 0,
                victim: rival,
            }],
            (FightResult::Won, _) => vec![Reason::Beaten { fighter: 0, rival }],
            _ => Vec::new(),
        };
        assert_eq!(reasons(&state, rival.club), why);
        // Far ahead in Elo, no rival fighter accepts: a week of rest.
        let other = ranked(&state, 3);
        state.colosseums[1].ranking[0].elo = 2000;
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
    fn an_outsider_meets_one_club_fighter_in_a_title_fight_or_a_challenge() {
        let data = data();
        let rome = ColosseumId(1);
        // Two club fighters at the top of Rome's ranking.
        let two_at_the_top = || {
            let mut state = top_of_rome(&data);
            let twin = state.fighters[0].clone();
            state.fighters.push(twin);
            let rating = crate::colosseum::Rating {
                contender: Contender::Club(1),
                elo: 1100,
            };
            state.colosseums[1].ranking.insert(1, rating);
            state
        };
        let both = |activity| Decisions {
            plans: vec![plan(activity, false).plans[0]; 2],
            ..Decisions::default()
        };
        let mut state = two_at_the_top();
        assert!(state.title_opponent(1, rome, &data).is_some());
        let summary = next_week(&mut state, &both(Activity::TitleFight(rome)), &data);
        let fights = (summary.titles.iter())
            .filter(|(c, n)| *c == 1 && matches!(n, TitleNews::Fight { .. }))
            .count();
        assert_eq!(fights, 1);
        let second = summary.reports[1].as_ref().unwrap();
        assert_eq!(second.plan.activity, Activity::Rest);

        let mut state = two_at_the_top();
        let rival = ranked(&state, 8);
        assert!((0..2).all(|i| state.may_challenge(i, rome, rival, &data)));
        let summary = next_week(&mut state, &both(Activity::Challenge(rome, rival)), &data);
        let first = summary.reports[0].as_ref().unwrap();
        assert_eq!(first.plan.activity, Activity::Challenge(rome, rival));
        let second = summary.reports[1].as_ref().unwrap();
        assert_eq!(second.plan.activity, Activity::Rest);
        assert!(second.fight.is_none());
    }

    #[test]
    fn a_club_that_cannot_pay_rome_rests() {
        let data = data();
        let mut state = game(7, &data);
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
        let mut state = game(7, &data);
        state.fighters[0].status = Status::Dead;
        state.money = -100;
        let agent = state.free_agents[2].clone();
        let signing = moves(&[sign(&state, 2, &data)]);
        let summary = next_week(&mut state, &signing, &data);
        assert_eq!(summary.market, [MarketNews::Signed(1)]);
        assert_eq!(state.fighters.len(), 2);
        assert!(summary.reports[1].is_some());
        assert_eq!(state.fighters[1].status, Status::Active);
        assert_eq!(state.fighters[1].potential, agent.potential);
        // Off the market, its entry kept.
        assert_eq!(state.free_agents[2].status, Status::Sold);
        assert!(!state.may_move(sign(&state, 2, &data), &data));
    }

    #[test]
    fn a_full_club_signs_no_one() {
        let data = data();
        let mut state = game(7, &data);
        // The first free agent on the market.
        let hire = |state: &GameState| {
            let a = (0..state.free_agents.len())
                .find(|&a| state.free_agents[a].status == Status::Active)
                .unwrap();
            moves(&[sign(state, a, &data)])
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
        let mut state = game(7, &data);
        let (first, third) = (state.free_agents[0].clone(), state.free_agents[2].clone());
        let signings = moves(&[sign(&state, 2, &data), sign(&state, 0, &data)]);
        next_week(&mut state, &signings, &data);
        assert_eq!(state.fighters[1].potential, third.potential);
        assert_eq!(state.fighters[2].potential, first.potential);
    }

    #[test]
    fn a_rival_fighter_is_bought_by_negotiation() {
        let data = data();
        let mut state = game(7, &data);
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
        // decides. Too tired to fight that week, so that it does not die before.
        state.rivals[2].fighters[0].form = crate::fighter::form::Form::new(10);
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
                assert_eq!(reasons(&state, 2), [Reason::Bought { fighter: 1 }]);
                // A merchant forgives when the price is right.
                let merchant = data.world.profiles.get(Profile::Merchant);
                let points =
                    data.world.relations.deal * i32::try_from(merchant.deal_percent).unwrap() / 100;
                let decay = i32::try_from(data.world.relations.decay).unwrap();
                assert_eq!(state.relation(2), points - decay);
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
        let mut state = game(7, &data);
        let list = moves(&[Move::List(0)]);
        let train = Decisions {
            plans: vec![WeekPlan {
                activity: Activity::Train,
                care: false,
                refused: Refused::NONE,
                loadout: 0,
                focus: None,
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
        let roster = &full.rivals[1].fighters;
        let extra = roster
            .iter()
            .find(|f| f.status == Status::Active)
            .unwrap()
            .clone();
        full.rivals[1].fighters.extend(vec![extra; 6]);
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
        assert_eq!(reasons(&state, 1), [Reason::Sold { fighter: 0 }]);
        assert!(
            state.rivals[1]
                .fighters
                .iter()
                .any(|f| f.potential == sold.potential)
        );
    }

    #[test]
    fn a_club_offers_unasked_for_a_fighter_not_kept() {
        let mut data = data();
        let market = &mut data.economy.market;
        market.unsolicited_percent = 100;
        market.unsolicited_max_percent = 100;
        let mut state = game(7, &data);
        // Too tired to fight, so that it does not die before the offers.
        state.fighters[0].form = crate::fighter::form::Form::new(10);
        next_week(&mut state, &Decisions::default(), &data);
        // Not listed: a single club offers, its gamble on top of its price.
        let [offer] = state.sale_offers[..] else {
            panic!("{:?}", state.sale_offers);
        };
        let profile = state.rivals[offer.rival].rules(&data);
        let worth = value(&state.fighters[0], &data.economy.market);
        let price = percent_of(
            percent_of(worth, profile.buy_percent),
            profile.unsolicited_percent,
        );
        assert_eq!(offer.price, price);
        // Kept: no offer, and it may not be listed until for sale again.
        next_week(&mut state, &moves(&[Move::Keep(0)]), &data);
        assert_eq!(state.kept, [0]);
        assert_eq!(state.sale_offers, []);
        assert!(!state.may_move(Move::List(0), &data));
        next_week(&mut state, &moves(&[Move::Unkeep(0)]), &data);
        assert_eq!(state.kept, Vec::<usize>::new());
        assert_eq!(state.sale_offers.len(), 1);
    }

    #[test]
    fn a_club_s_standing_sets_its_prices_and_challenges() {
        let data = data();
        let standings = &data.world.relations.standings;
        let mut state = game(7, &data);
        let target = RivalId {
            club: 2,
            fighter: 0,
        };
        let asking = state.asking_price(target, &data).unwrap();
        assert_eq!(state.price_to_player(target, &data), Some(asking));
        let set = |state: &mut GameState, standing: Standing| {
            // Above its lowest, so that a week fading it keeps it.
            state.relations.entry(2).or_default().score = standings.get(standing).from + 5;
        };
        // A friend asks less of the player, and offers more for its fighters.
        set(&mut state, Standing::Friend);
        let friend = standings.get(Standing::Friend).price_percent;
        assert_eq!(
            state.price_to_player(target, &data),
            Some(percent_of(asking, friend))
        );
        let mut listed = state.clone();
        next_week(&mut listed, &moves(&[Move::List(0)]), &data);
        let offer = listed.sale_offers.iter().find(|o| o.rival == 2).unwrap();
        let worth = value(&listed.fighters[0], &data.economy.market);
        let merchant = data.world.profiles.get(Profile::Merchant);
        let price = percent_of(worth, merchant.buy_percent);
        assert_eq!(
            u64::from(offer.price),
            u64::from(price) * 100 / u64::from(friend)
        );
        // A rival trades nothing with the player.
        set(&mut state, Standing::Rival);
        assert_eq!(state.price_to_player(target, &data), None);
        state.money = 10_000;
        assert!(!state.may_move(Move::Offer(target, asking * 2), &data));
        next_week(&mut state, &moves(&[Move::List(0)]), &data);
        assert!(state.sale_offers.iter().all(|o| o.rival != 2));
        assert!(state.sale_offers.iter().any(|o| o.rival != 2));
    }

    #[test]
    fn a_hostile_club_accepts_the_player_s_challenges_more_willingly() {
        let data = data();
        let mut state = top_of_rome(&data);
        let rome = ColosseumId(1);
        let rival = ranked(&state, 3);
        let mut hostile = state.clone();
        let points = data.world.relations.standings.rival.from;
        hostile.relations.entry(rival.club).or_default().score = points;
        // Some Elo gap refused by a neutral club is accepted by a rival one, never the
        // other way round.
        let mut accepted = false;
        for elo in (1100..2000).step_by(10) {
            state.colosseums[1].ranking[0].elo = elo;
            hostile.colosseums[1].ranking[0].elo = elo;
            let neutral = state.may_challenge(0, rome, rival, &data);
            let angry = hostile.may_challenge(0, rome, rival, &data);
            assert!(angry || !neutral, "at {elo}");
            accepted |= angry && !neutral;
        }
        assert!(accepted);
    }

    #[test]
    fn a_club_declares_a_rivalry_whose_fights_draw_a_crowd() {
        let data = data();
        let mut state = top_of_rome(&data);
        let rome = ColosseumId(1);
        let rival = ranked(&state, 7);
        let mut rivals = state.clone();
        rivals.relations.entry(rival.club).or_default().score = -70;
        let week = rivals.week;
        let challenge = plan(Activity::Challenge(rome, rival), false);
        // Declared at the end of a week: that week's fight is still an ordinary one.
        let summary = next_week(&mut rivals, &challenge, &data);
        let first = next_week(&mut state, &challenge, &data);
        assert_eq!(flows(&summary), flows(&first));
        assert!(rivals.is_rival(rival.club));
        assert_eq!(rivals.standing(rival.club, &data), Standing::Rival);
        let declared = News {
            week,
            what: WorldNews::Rivalry(rival.club, true),
        };
        assert!(rivals.news.contains(&declared));
        // The next fight against its fighters brings more receipts and fame.
        let mut neutral = rivals.clone();
        neutral.relations.remove(&rival.club);
        let other = (1..rivals.colosseums[1].ranking.len())
            .map(|rank| ranked(&rivals, rank))
            .find(|&id| id.club == rival.club && neutral.may_challenge(0, rome, id, &data))
            .unwrap();
        let challenge = plan(Activity::Challenge(rome, other), false);
        let receipts = |summary: &WeekSummary| {
            flows(summary)
                .into_iter()
                .find(|&(flow, _)| flow == Flow::Receipts)
                .map(|(_, amount)| u32::try_from(amount).unwrap())
        };
        let fame_change = |summary: &WeekSummary| {
            let report = summary.reports[0].as_ref().unwrap();
            report.fight.as_ref().unwrap().fame_change
        };
        let crowd = next_week(&mut rivals, &challenge, &data);
        let usual = next_week(&mut neutral, &challenge, &data);
        let rivalry = &data.world.relations.rivalry;
        assert_eq!(
            receipts(&crowd),
            receipts(&usual).map(|r| percent_of(r, rivalry.receipts_percent))
        );
        let bonus = i32::try_from(rivalry.fame).unwrap();
        assert_eq!(fame_change(&crowd), fame_change(&usual) + bonus);
        // Forgiven well above a rival's score, it ends the rivalry.
        rivals.relations.get_mut(&rival.club).unwrap().score = crate::relations::MAX_SCORE;
        let week = rivals.week;
        next_week(&mut rivals, &plan(Activity::Rest, false), &data);
        assert!(!rivals.is_rival(rival.club));
        let ended = News {
            week,
            what: WorldNews::Rivalry(rival.club, false),
        };
        assert!(rivals.news.contains(&ended));
    }

    #[test]
    fn a_pact_between_the_club_s_fighter_and_a_rival_one_moves_its_club() {
        let rival = RivalId {
            club: 2,
            fighter: 4,
        };
        let entries = [
            Contender::Rival(rival),
            Contender::FreeAgent(0),
            Contender::Club(1),
            Contender::Club(0),
        ];
        let pact = |a, b, betrayer| Pact { a, b, betrayer };
        let kept = Reason::PactKept { fighter: 1, rival };
        let betrayed = Reason::Betrayed { fighter: 1, rival };
        assert_eq!(pact_reason(&entries, pact(0, 2, None)), Some((2, kept)));
        assert_eq!(
            pact_reason(&entries, pact(2, 0, Some(2))),
            Some((2, betrayed))
        );
        assert_eq!(pact_reason(&entries, pact(0, 2, Some(0))), None);
        assert_eq!(pact_reason(&entries, pact(1, 2, None)), None);
        assert_eq!(pact_reason(&entries, pact(2, 3, None)), None);
    }

    #[test]
    fn a_melee_with_an_unknown_entry_begins_nobody_s_week() {
        let data = data();
        let mut state = game(7, &data);
        state.fighters[0].injuries.push(Injury {
            location: Location::Body,
            severity: Severity::Medium,
            weeks_left: 4,
        });
        let before = state.clone();
        let wanted = plan(Activity::Melee(ColosseumId(0)), true).plans;
        let entries = [Contender::Club(0), Contender::FreeAgent(usize::MAX)];
        let played = melee_entries(
            &mut state,
            &entries,
            (&wanted, ArenaKind::Sand),
            StaffEffects::default(),
            &data,
        );
        assert!(played.is_none());
        assert_eq!(state, before);
    }

    #[test]
    fn a_released_fighter_becomes_a_free_agent() {
        let data = data();
        let mut state = game(7, &data);
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
        let mut state = game(7, &data);
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
        let mut data = data();
        // Nobody retires, so that the release alone decides.
        data.fighter.retirement.per_year = 0;
        data.fighter.retirement.per_permanent_injury = 0;
        let market = &data.economy.market;
        let mut state = game(3, &data);
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

    #[test]
    fn everyone_ages_at_the_start_of_the_off_season_whenever_they_arrived() {
        let data = data();
        let mut state = game(3, &data);
        let calendar = &data.calendar;
        let off_season = u64::from(calendar.competition_weeks + calendar.great_event_weeks + 1);
        while state.week < off_season / 2 {
            next_week(&mut state, &Decisions::default(), &data);
        }
        // A free agent arrived mid-season, a year away from its birthday by its own count.
        let agent = crate::market::free_agent(&mut state.rng, &data);
        state.free_agents.push(agent.clone());
        while state.week < off_season {
            let summary = next_week(&mut state, &Decisions::default(), &data);
            assert!(summary.reports.iter().flatten().all(|r| !r.birthday));
        }
        let before = state.clone();
        let summary = next_week(&mut state, &Decisions::default(), &data);
        let fighters = before.fighters.iter().zip(&state.fighters);
        for ((was, is), report) in fighters.zip(&summary.reports) {
            if was.status == Status::Active && is.status != Status::Dead {
                assert_eq!(is.age, was.age + 1);
                assert!(report.as_ref().is_some_and(|r| r.birthday));
            }
        }
        // Signed or not, it lives on with its potential and its look.
        let now = state
            .free_agents
            .iter()
            .chain(state.rivals.iter().flat_map(|r| &r.fighters));
        let same: Vec<_> = now
            .filter(|f| f.potential == agent.potential && f.appearance == agent.appearance)
            .collect();
        assert_eq!(same.len(), 1);
        assert_eq!(same[0].age, agent.age + 1);
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
    fn wealth_follows_money_at_the_off_season_with_no_patron() {
        let data = data();
        let rules = &data.economy.wealth;
        let mut state = game_of(7, Wealth::Poor, &data);
        assert_eq!(state.money, i64::from(rules.poor.starting_money));
        state.fighters[0].status = Status::Retired;
        let summary = next_week(&mut state, &Decisions::default(), &data);
        // Without fighters, no money comes in.
        assert_eq!(flows(&summary), []);
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
        assert_eq!(flows(&summary), []);
        assert_eq!(summary.wealth, None);
    }

    #[test]
    fn a_club_ruined_two_off_seasons_closes_and_a_new_one_is_founded() {
        let mut data = data();
        data.world.clubs.founded = 1;
        data.world.clubs.ruined_seasons = 2;
        let mut state = game(7, &data);
        let poor = 1;
        // Ruined at the last off-season already.
        state.rivals[poor].ruined_seasons = 1;
        up_to_the_off_season(&mut state, &data);
        state.rivals[poor].money = 0;
        // A rich club without money is not ruined: it becomes modest first.
        state.rivals[0].money = 0;
        let leaving = state.rivals[poor].active();
        let week = state.week;
        next_week(&mut state, &Decisions::default(), &data);
        let closed = &state.rivals[poor];
        assert_eq!(closed.closed, Some(week));
        assert_eq!(closed.active(), 0);
        // Released, unless it retired at its birthday or died in a fight the same week.
        let released = closed
            .fighters
            .iter()
            .filter(|f| matches!(f.status, Status::Released | Status::Retired | Status::Dead));
        assert!(released.count() >= leaving);
        assert_eq!(closed.staff, Staff::default());
        assert!(state.rivals[0].is_open());
        let founded = &state.rivals[3];
        assert!(founded.is_open());
        // A rich one may buy one more the same week.
        assert!(founded.active() >= usize::try_from(data.world.roster.wanted).unwrap());
        let told: Vec<_> = state.news.iter().map(|n| (n.week, n.what)).collect();
        assert!(told.contains(&(week, WorldNews::Closed(poor))));
        assert!(told.contains(&(week, WorldNews::Founded(3))));
        // Closed, it hires no one, signs no one and is offered no fighter.
        state.rivals[poor].money = 100_000;
        next_week(&mut state, &Decisions::default(), &data);
        assert_eq!(state.rivals[poor].active(), 0);
        assert_eq!(state.rivals[poor].staff, Staff::default());
        assert!(!state.rival_has_place(poor, &data));
        // The news fade after `news_weeks`.
        let kept = u64::from(data.world.news_weeks);
        while state.week <= week + kept {
            next_week(&mut state, &Decisions::default(), &data);
        }
        assert!(state.news.iter().all(|n| n.week > week));
    }

    #[test]
    fn a_poor_club_signs_the_free_agent_of_the_best_potential_its_recruiter_sees() {
        let data = data();
        let mut state = game(7, &data);
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

    #[test]
    fn a_fighter_let_go_comes_back_only_once_its_grudge_fades() {
        let data = data();
        let mut state = game(7, &data);
        let modest = 2;
        // The best free agent there is, once the modest club's own.
        star(&mut state.rivals[modest].fighters[0]);
        let id = state.rivals[modest].fighters[0].id;
        let club = RivalId {
            club: modest,
            fighter: 0,
        };
        release_rival(&mut state, club, &data);
        let agent = state.free_agents.len() - 1;
        let ties = &data.world.ties;
        assert_eq!(
            state.free_agents[agent].tie(Club::Rival(modest)),
            ties.released
        );
        let mut soon = state.clone();
        rival_signings(&mut soon, &data);
        assert_ne!(soon.rivals[modest].fighters.last().unwrap().id, id);
        assert_eq!(soon.free_agents[agent].status, Status::Active);
        // Weeks later, the grudge has faded: the club takes it back.
        while state.free_agents[agent].tie(Club::Rival(modest)) < ties.refuses_below {
            state.free_agents[agent].tie_week(None, ties);
        }
        rival_signings(&mut state, &data);
        assert_eq!(state.rivals[modest].fighters.last().unwrap().id, id);
    }

    /// Makes `fighter` the one every club wants, and one who never refuses a famous club.
    fn star(fighter: &mut CareerFighter) {
        fighter.stats = fighter.stats.map(|_| 500);
        fighter.fame = Fame::default();
        fighter.personality.ego = crate::fighter::personality::TraitValue::new(0);
    }

    #[test]
    fn a_rich_club_buys_better_fighters_from_poorer_clubs_and_lets_its_weakest_go() {
        let data = data();
        let max = usize::try_from(data.world.roster.max).unwrap();
        let mut state = game(7, &data);
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
        state.colosseums[0].champion = Some(Contender::Rival(id));
        let asked = state.asking_price(id, &data).unwrap();
        let price = i64::from(asked);
        rich_purchases(&mut state, &data);
        let bought_champion = WorldNews::ChampionBought {
            colosseum: 0,
            fighter: id,
            buyer: rich,
            price: asked,
        };
        let told: Vec<_> = state.news.iter().map(|n| n.what).collect();
        assert_eq!(told, [bought_champion]);
        assert_eq!(state.rival_fighter(id).unwrap().status, Status::Sold);
        let bought = state.rivals[rich].fighters.last().unwrap();
        assert!(bought.stats.to_array().iter().all(|&s| s == 500));
        let to = RivalId {
            club: rich,
            fighter: state.rivals[rich].fighters.len() - 1,
        };
        let left = state.rival_fighter(id).unwrap();
        assert_eq!(left.moved_to, Some(Contender::Rival(to)));
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
        // Rich too, it may, once the star has played half a season for its new club.
        state.rivals[modest].wealth = Wealth::Rich;
        let mut too_soon = state.clone();
        assert_eq!(
            buy_from_rival(&mut too_soon, modest, star_value, &data),
            None
        );
        let bought = state.rivals[rich].fighters.last().unwrap();
        state.week = GameState::sellable_from(bought, &data);
        assert_eq!(state.week, 1 + u64::from(data.calendar.season_weeks() / 2));
        assert_eq!(
            buy_from_rival(&mut state, modest, star_value, &data),
            Some(rich)
        );
    }

    #[test]
    fn a_fighter_who_joined_the_club_is_sold_only_after_half_a_season() {
        let data = data();
        let mut state = game(7, &data);
        let signing = sign(&state, 0, &data);
        market_moves(&mut state, &[signing], &data, &mut Vec::new());
        let signed = state.fighters.len() - 1;
        assert_eq!(state.fighters[signed].joined, Some(1));
        // The fighter there from the start may be listed, not the one just signed.
        assert!(state.may_move(Move::List(0), &data));
        assert!(!state.may_move(Move::List(signed), &data));
        state.week = GameState::sellable_from(&state.fighters[signed], &data);
        assert!(state.may_move(Move::List(signed), &data));
    }

    #[test]
    fn at_the_off_season_a_rich_club_lets_go_at_any_age_whoever_no_longer_performs() {
        let data = data();
        let mut state = game(3, &data);
        state.fighters[0].status = Status::Retired;
        let (rich, poor) = (0, 1);
        up_to_the_off_season(&mut state, &data);
        state.rivals[rich].money = 100_000;
        state.rivals[poor].money = 0;
        // A young fighter worth little in each club, too tired to fight that week.
        let mut weak = Vec::new();
        for club in [rich, poor] {
            let roster = &mut state.rivals[club].fighters;
            let j = (0..roster.len())
                .find(|&j| roster[j].status == Status::Active)
                .unwrap();
            roster[j].stats = roster[j].stats.map(|_| 1);
            roster[j].form = crate::fighter::form::Form::new(10);
            roster[j].age = 22;
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
        let mut state = game(7, &data);
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
            let newcomer = Contender::Rival(RivalId {
                club: 0,
                fighter: state.rivals[0].fighters.len() - 1,
            });
            let from = (1..state.rivals.len())
                .flat_map(|club| {
                    let fighters = &state.rivals[club].fighters;
                    (0..fighters.len()).map(move |fighter| RivalId { club, fighter })
                })
                .find(|&id| {
                    let f = state.rival_fighter(id).unwrap();
                    f.status == Status::Sold && f.moved_to == Some(newcomer)
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
        let mut poor = game(7, &data);
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
        let mut state = game(3, &data);
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
            let mut state = game(9, &data);
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
        let mut state = game(5, &data);
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
            // Those who lose often die before they sink far: a bit below the start.
            assert!(
                elos.iter().any(|&e| e < data.world.elo_start - 25),
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

    /// The clubs founded in the last `weeks` weeks, by the news.
    fn founded_within(state: &GameState, weeks: u64) -> Vec<usize> {
        (state.news.iter())
            .filter_map(|n| match n.what {
                WorldNews::Founded(club) if n.week + weeks > state.week => Some(club),
                _ => None,
            })
            .collect()
    }

    /// Whether rival club `club` has someone on its staff, or could hire no one on the
    /// staff market as it is.
    fn staffed(state: &GameState, club: usize, data: &DesignData) -> bool {
        let rival = &state.rivals[club];
        let staff = &rival.staff;
        let shares = i64::from(data.economy.staff.salary_shares(rival.active()));
        let rules = (rival.rules(data), rivals::staff_posts(rival.wealth));
        let money = rival.money / shares;
        staff.trainer.is_some()
            || staff.doctor.is_some()
            || staff.recruiter.is_some()
            || rivals::staff_moves(staff, money, rules, &state.staff_market).is_empty()
    }

    /// Every fighter in activity has a name, and no two share it.
    fn assert_names_apart(state: &GameState, week: u64) {
        let rivals = state.rivals.iter().flat_map(|r| &r.fighters);
        let mut names: Vec<_> = state
            .fighters
            .iter()
            .chain(rivals)
            .chain(&state.free_agents)
            .filter(|f| f.status == Status::Active && f.moved_to.is_none())
            .map(|f| &f.name)
            .collect();
        assert!(names.iter().all(|n| !n.is_empty()), "week {week}");
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "week {week}");
    }

    /// The milestone of phase 5: seasons follow one another where rankings, champions and
    /// markets change without the player.
    #[test]
    fn three_seasons_live_without_the_player() {
        let data = DesignData::load().unwrap();
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
            // A club that loses fighters after its signings (sold to another, killed in a
            // melee or a title defense, several in a hard week) recruits the next week; a
            // poor one may run into debt, and closes once ruined (`club_lives`).
            for rival in state.rivals.iter().filter(|r| r.is_open()) {
                assert!(
                    rival.active() + 4 >= wanted,
                    "week {week}: {:?}",
                    rival.name
                );
            }
            assert_names_apart(&state, week);
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
        // Most titles changed hands (a great champion may keep one three seasons long),
        // and the first ranks are not those of the first season.
        let changed = title_changes.iter().filter(|&&n| n > 0).count();
        assert!(changed * 4 >= title_changes.len() * 3, "{title_changes:?}");
        assert_ne!(leaders[0], leaders[2], "{leaders:?}");
        // Ruined clubs closed, their fighters gone to the market, and new ones were
        // founded at the off-seasons, the last ones still in the news.
        let told = state.news.iter().map(|n| n.what);
        let closed: Vec<usize> = (0..state.rivals.len())
            .filter(|&k| !state.rivals[k].is_open())
            .collect();
        assert_ne!(closed, Vec::<usize>::new());
        assert!(closed.iter().all(|&k| state.rivals[k].active() == 0));
        assert!(state.rivals.len() > start.rivals.len());
        assert!(told.clone().any(|n| matches!(n, WorldNews::Founded(_))));
        // The clubs replaced those who died, retired or were released in the off-season.
        // Fighters released at `release_age` are worth signing, so the clubs seldom buy
        // from one another (none here).
        let open = state.rivals.iter().filter(|r| r.is_open());
        for (rival, before) in open.zip(&start.rivals) {
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
        // not afford a trainer besides, nor find one when forty clubs share the
        // market); their salaries, paid for every three fighters, keep their money
        // within bounds (up to 26 000 without them).
        // A club founded at the last off-season may not have found one yet, and one
        // near ruin (ruined last season, or short of the candidates' pay) hires none.
        let founded = founded_within(&state, season);
        for (club, rival) in state.rivals.iter().enumerate().filter(|(_, r)| r.is_open()) {
            let spared = founded.contains(&club) || rival.ruined_seasons > 0;
            assert!(spared || staffed(&state, club, &data), "{:?}", rival.staff);
            assert!(rival.money < 25_000, "{}", rival.money);
        }
        assert!(
            state
                .rivals
                .iter()
                .any(|r| r.wealth == Wealth::Poor && r.staff.recruiter.is_some())
        );
        // Poor and modest clubs live side by side, each one's wealth following its money.
        // ponytail: no rich club left after three seasons since a fighter giving up must
        // reach the door, the dead cost too much to replace; rebalance the economy.
        let mut wealths: Vec<Wealth> = state.rivals.iter().map(|r| r.wealth).collect();
        wealths.sort();
        wealths.dedup();
        assert!(wealths.starts_with(&[Wealth::Poor, Wealth::Modest]));
        // Free agents fought too, ranked in the colosseum free to enter; some were
        // signed with their record, and some died in the arena.
        let fights = |f: &CareerFighter| f.record.wins + f.record.losses + f.record.draws;
        let agents = &state.free_agents;
        assert!(
            agents
                .iter()
                .any(|f| f.status == Status::Active && fights(f) > 0)
        );
        assert!(agents.iter().any(|f| {
            let signed = f.moved_to.and_then(|to| state.rosters().fighter(to));
            f.status == Status::Sold && signed.is_some_and(|s| fights(s) > 0)
        }));
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
        let mut state = game(7, &data);
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
        let mut state = game(7, &data);
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
            ..moves(&[sign(&state, a, &data)])
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
        let mut state = game(3, &data);
        assert_eq!(state.rivals[0].profile, Profile::Cautious);
        assert_eq!(state.rivals[1].profile, Profile::Aggressive);
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
            &data.world,
        );
        let me = &state.fighters[0];
        let capua = (capua, 0, me);
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
            opponent(&mut rng, world, (challenge, 0, me), &only, &fought),
            Some((0, Contender::Rival(four)))
        );
        // A free agent registered there is met too.
        let agent = Contender::FreeAgent(2);
        only.set_activity(agent, capua.0);
        // The club's fighter meets the closest in Elo of those registered.
        let mut ranked = state.colosseums.clone();
        let elo = data.world.elo_start;
        ranked[0].ranking = [
            (Contender::Club(0), elo),
            (one, elo + 400),
            (agent, elo + 30),
        ]
        .map(|(contender, elo)| crate::colosseum::Rating { contender, elo })
        .to_vec();
        let near = (&ranked[..], world.1, world.2, world.3);
        for _ in 0..8 {
            assert_eq!(
                opponent(&mut rng, near, capua, &only, &fought),
                Some((0, agent))
            );
        }
        // A spiteful fighter meets first the one it bears a grudge against.
        let mut spiteful = me.clone();
        spiteful.traits = vec![Trait::Spiteful];
        let theirs = &state.free_agents[2];
        spiteful.grudges = vec![(theirs.id, theirs.name.clone())];
        for _ in 0..8 {
            assert_eq!(
                opponent(&mut rng, world, (capua.0, 0, &spiteful), &only, &fought),
                Some((0, agent))
            );
        }
        fought.push(one);
        assert_eq!(
            opponent(&mut rng, world, capua, &only, &fought),
            Some((0, agent))
        );
        fought.push(agent);
        assert!(opponent(&mut rng, world, capua, &only, &fought).is_some_and(|(_, r)| r != one));
    }
}
