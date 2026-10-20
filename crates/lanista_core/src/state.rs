//! The whole game: the player's club, the colosseums, the rival clubs, the market and the week
//! to play. One week is one call to [`next_week`]; the interface only builds [`Decisions`] and
//! reads the state.

use std::collections::BTreeMap;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::calendar::{Date, SeasonPhase};
use crate::colosseum::{
    Challenge, Colosseum, ColosseumId, Contender, Continent, Rosters, TitleNews,
};
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction, Wealth};
use crate::fighter::care::CareRules;
use crate::fighter::career::{Activity, CareerFighter, Status, WeekPlan, suggested_plan};
use crate::fighter::equipment::Refused;
use crate::fighter::fame::{Fame, FightResult};
use crate::fighter::percent_of;
use crate::market::{self, Move, SaleOffer, Target, value};
use crate::relations::{Reason, Relation, Standing, StandingRules};
use crate::rivals::{self, Rival, RivalId, Venue};
use crate::staff::{
    self, Candidate, DoctorRules, Medicine, RecruiterRules, Scouting, Staff, StaffEffects,
    StaffMove, StaffNews, TrainerRules,
};

mod colosseums;
mod melee;
mod outsiders;
mod transfers;
mod week;

use outsiders::OutsiderPlans;
use transfers::release_rival;

pub use week::{Decisions, WeekSummary, next_week};

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
    /// What happened in the world, oldest first, over the last `WorldRules::news_weeks`.
    pub news: Vec<News>,
    /// Each rival club's relation with the player's club, by its index; a club
    /// missing has a relation of 0.
    pub relations: BTreeMap<usize, Relation>,
    /// The only source of chance after the start: same seed and decisions, same game.
    pub rng: ChaCha8Rng,
}

impl GameState {
    /// A new game from `seed` with a modest club in Europe (see `start`).
    #[must_use]
    pub fn new(seed: u64, data: &DesignData) -> Self {
        Self::start(seed, (Wealth::Modest, Continent::Europe), data)
    }

    /// A new game from `seed`: a club of `wealth` at home on `continent` with one
    /// generated fighter, the same one `CareerFighter::random` draws first from that
    /// seed, its wealth's starting money, the rival clubs with their fighters, the
    /// colosseums and the free agents.
    #[must_use]
    pub fn start(seed: u64, (wealth, continent): (Wealth, Continent), data: &DesignData) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let fighters = vec![CareerFighter::random(&mut rng, data)];
        let mut state = Self {
            week: 1,
            money: i64::from(data.economy.wealth.get(wealth).starting_money),
            wealth,
            continent,
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
            news: Vec::new(),
            relations: BTreeMap::new(),
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
    /// for each post.
    pub fn open_world(&mut self, data: &DesignData) {
        let drawn = self.rivals.len();
        let count = usize::try_from(data.world.clubs.count).unwrap_or(0);
        while self.rivals.len() < count {
            let club = rivals::random_club(&mut self.rng, &self.rivals, data);
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
            self.free_agents.push(agent);
        }
        staff::fill_market(&mut self.staff_market, &data.economy.staff, &mut self.rng);
    }

    /// Rival club `club` gets generated fighters up to `roster.wanted`, ranked nowhere.
    fn fill_roster(&mut self, club: usize, data: &DesignData) {
        let wanted = usize::try_from(data.world.roster.wanted).unwrap_or(0);
        while self.rivals[club].active() < wanted {
            let fighter = CareerFighter::random(&mut self.rng, data);
            self.rivals[club].fighters.push(fighter);
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

    /// Each colosseum as `contender` sees it, in their order: the chance Elo gives it
    /// there (`Colosseum::chance`), and the fee from `home`, its club's continent, the
    /// journey included; a free agent, `None`, travels for nothing and is at home
    /// everywhere.
    fn venues(
        &self,
        contender: Contender,
        home: Option<Continent>,
        data: &DesignData,
    ) -> Vec<Venue> {
        let world = &data.world;
        self.colosseums
            .iter()
            .zip(&world.colosseums)
            .zip(0..)
            .map(|((c, rules), id)| Venue {
                chance: c.chance(contender, self.rosters(), world),
                fee: home
                    .and_then(|home| world.entry_fee(ColosseumId(id), home))
                    .unwrap_or(rules.purses.entry_fee),
                home: home.is_none_or(|home| home == rules.continent),
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
                            let venues = self.venues(me, Some(rival.continent), data);
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
        (0..)
            .zip(&self.free_agents)
            .map(|(a, f)| {
                if f.status != Status::Active {
                    return REST;
                }
                let venues = self.venues(Contender::FreeAgent(a), None, data);
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
    /// of fighting or training in the last two weeks before its deadline, or else
    /// accepts a challenge it received and the club can pay for.
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
        if matches!(plan.activity, Activity::Fight(_) | Activity::Train)
            && let Some(challenge) = self.challenges.iter().find(|ch| {
                ch.fighter == i
                    && self
                        .entry_fee(ch.colosseum, data)
                        .is_some_and(|fee| i64::from(fee) <= self.money)
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
                .zip(self.rivals.get(rival.club).map(|r| r.rules(data)))
                .is_some_and(|(colosseum, profile)| {
                    let points = self.standing_rules(rival.club, data).challenge_points;
                    let needed = profile.accept_percent.saturating_sub(points);
                    colosseum.accepts_challenge(rival, i, needed, &data.world)
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
            Move::Sign(a) => active(self.free_agents.get(a)) && self.can_hire(data),
            Move::Release(i) => ours(i),
            Move::List(i) => {
                ours(i)
                    && self.may_be_sold(&self.fighters[i], data)
                    && !self.transfer_list.contains(&i)
            }
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

/// At the start of the off-season, each club's wealth follows its money
/// (`Wealth::next`); returns the player's club's new wealth if it changed.
fn wealth_changes(state: &mut GameState, data: &DesignData) -> Option<Wealth> {
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
fn club_lives(state: &mut GameState, data: &DesignData) {
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
        let club = rivals::random_club(&mut state.rng, &state.rivals, data);
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
fn rival_staff(state: &mut GameState, data: &DesignData) {
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
    made
}

/// Whether the week played starts the off-season.
fn off_season_starts(state: &GameState, data: &DesignData) -> bool {
    let today = state.date(data);
    let calendar = &data.calendar;
    today.phase(calendar) == SeasonPhase::OffSeason
        && today.week == calendar.competition_weeks + calendar.great_event_weeks + 1
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
    use super::melee::pact_reason;
    use super::outsiders::opponent;
    use super::transfers::{buy_from_rival, market_moves, rich_purchases, rival_signings, weakest};
    use super::*;
    use crate::colosseum::title_kept;
    use crate::fighter::injury::{Injury, Location, Severity};
    use crate::fighter::melee::Pact;
    use crate::market::MarketNews;
    use crate::rivals::Profile;
    use crate::staff::{Person, Role};
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
        let mut state = GameState::start(seed, (wealth, Continent::Europe), data);
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
        let mut state = game(1, &data);
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
        let state = game(7, &data);
        assert_eq!(state.week, 1);
        assert_eq!(state.date(&data), Date { season: 1, week: 1 });
        let wealth = &data.economy.wealth;
        assert_eq!(state.wealth, Wealth::Modest);
        assert_eq!(state.money, i64::from(wealth.modest.starting_money));
        let fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(7), &data);
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
            assert!(
                culture
                    .names
                    .iter()
                    .any(|n| rival.name.contains(n.as_str())),
                "{}",
                rival.name
            );
            assert!(rival.active() >= wanted);
            assert_eq!(
                rival.money,
                i64::from(data.economy.wealth.get(rival.wealth).starting_money)
            );
            names.push(rival.name.as_str());
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
        let state = GameState::start(7, (Wealth::Modest, Continent::Asia), &data);
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
    fn upkeep_and_training_are_paid() {
        let data = data();
        let economy = &data.economy;
        let mut state = game(7, &data);
        let summary = next_week(&mut state, &plan(Activity::Train, false), &data);
        let cost = i64::from(economy.training_cost);
        let upkeep = i64::from(economy.upkeep);
        let modest = &economy.wealth.modest;
        assert_eq!(
            flows(&summary),
            [(Flow::Training, -cost), (Flow::Upkeep, -upkeep)]
        );
        assert_eq!(
            state.money,
            i64::from(modest.starting_money) - cost - upkeep
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
        assert_eq!(state.money, -i64::from(data.economy.upkeep));
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
    fn a_fight_brings_receipts_and_the_winner_s_prize() {
        let data = data();
        let colosseum = &data.world.colosseums[0].purses;
        // Even a broke club can fight: the first colosseum is free.
        let mut state = game(2, &data);
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
        let mut state = game(7, &data);
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
        let mut state = game(7, data);
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
        let told = state.news.iter().map(|n| (n.week, n.what));
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
        let mut state = game(7, &data);
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
        let refused = Reason::ChallengeRefused {
            fighter: 0,
            rival: received.rival,
        };
        assert_eq!(reasons(&state, received.rival.club), [refused]);
    }

    #[test]
    fn rival_fighters_challenge_the_ranked_fighters_fit_to_fight() {
        let data = data();
        let mut state = top_of_rome(&data);
        // The challenger's club, the first one, challenges 40 % of the weeks.
        state.rivals[0].profile = Profile::Aggressive;
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
        let mut state = game(7, &data);
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
        let mut state = game(7, &data);
        let (first, third) = (state.free_agents[0].clone(), state.free_agents[2].clone());
        next_week(&mut state, &moves(&[Move::Sign(2), Move::Sign(0)]), &data);
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
        next_week(&mut rivals, &plan(Activity::Rest, false), &data);
        assert!(!rivals.is_rival(rival.club));
        assert!(matches!(
            rivals.news.last().map(|n| n.what),
            Some(WorldNews::Rivalry(_, false))
        ));
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
        let data = data();
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
        let released = closed.fighters.iter();
        let released = released.filter(|f| f.status == Status::Released).count();
        assert!(released >= leaving);
        assert_eq!(closed.staff, Staff::default());
        assert!(state.rivals[0].is_open());
        let founded = &state.rivals[3];
        assert!(founded.is_open());
        assert_eq!(
            founded.active(),
            usize::try_from(data.world.roster.wanted).unwrap()
        );
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
        market_moves(&mut state, &[Move::Sign(0)], &data, &mut Vec::new());
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
        let mut state = game(4, &data);
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

    /// The clubs founded in the last `weeks` weeks, by the news.
    fn founded_within(state: &GameState, weeks: u64) -> Vec<usize> {
        (state.news.iter())
            .filter_map(|n| match n.what {
                WorldNews::Founded(club) if n.week + weeks > state.week => Some(club),
                _ => None,
            })
            .collect()
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
            // A club that loses a fighter after its signings (sold to another, killed in
            // a title defense) recruits the next week.
            for rival in state.rivals.iter().filter(|r| r.is_open()) {
                assert!(rival.money >= 0, "week {week}: {}", rival.money);
                assert!(rival.active() + 1 >= wanted, "week {week}: {}", rival.name);
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
        // A club founded at the last off-season may not have found one yet.
        let founded = founded_within(&state, season);
        for (club, rival) in state.rivals.iter().enumerate().filter(|(_, r)| r.is_open()) {
            let staff = &rival.staff;
            let hired = staff.trainer.is_some() || staff.doctor.is_some();
            let new = founded.contains(&club);
            assert!(hired || staff.recruiter.is_some() || new, "{staff:?}");
            assert!(rival.money < 25_000, "{}", rival.money);
        }
        assert!(
            state
                .rivals
                .iter()
                .any(|r| r.wealth == Wealth::Poor && r.staff.recruiter.is_some())
        );
        // Poor, modest and rich clubs live side by side, each one's wealth following its
        // money.
        let mut wealths: Vec<Wealth> = state.rivals.iter().map(|r| r.wealth).collect();
        wealths.sort();
        wealths.dedup();
        assert_eq!(wealths, [Wealth::Poor, Wealth::Modest, Wealth::Rich]);
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
