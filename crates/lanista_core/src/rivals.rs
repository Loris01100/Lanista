//! Rival clubs: managers playing by simple weekly rules whose thresholds come from
//! their profile (`data/world.ron`), each with its own roster of fighters and its
//! wealth, which sets how it hires and recruits.

use std::cmp::Reverse;

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::colosseum::{ColosseumId, Continent};
use crate::design_data::DesignData;
use crate::economy::Wealth;
use crate::fighter::career::{Activity, CareerFighter, Status, WeekPlan, badly_hurt_or_ill};
use crate::fighter::equipment::Refused;
use crate::fighter::{percent_of, pick};
use crate::market::{MarketRules, value};
use crate::names;
use crate::staff::{Candidate, Role, Staff, StaffMove};

/// How a rival manager plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Profile {
    Cautious,
    Aggressive,
    /// Keeps its money, sells cheap and buys dear.
    Merchant,
    /// Recruits young fighters to raise them.
    Mentor,
    /// Recruits fighters whose stats are grown.
    Veteran,
    /// Recruits the fighters who draw the crowd, whatever their stats.
    Showman,
}

/// How a club rates a fighter it might recruit or keep (`Eye::appeal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Eye {
    /// By its value (`market::value`).
    #[default]
    Worth,
    /// Its value, times `percent` up to `until` years old.
    Youth { until: u32, percent: u32 },
    /// Its value, times `percent` from `from` years old.
    Grown { from: u32, percent: u32 },
    /// By its fame alone, at the market's `price_per_fame`.
    Crowd,
}

impl Eye {
    /// What `fighter` is worth to a club of this eye, on the scale of `market::value`.
    #[must_use]
    pub fn appeal(self, fighter: &CareerFighter, market: &MarketRules) -> u32 {
        let worth = || value(fighter, market);
        match self {
            Eye::Youth { until, percent } if fighter.age <= until => percent_of(worth(), percent),
            Eye::Grown { from, percent } if fighter.age >= from => percent_of(worth(), percent),
            Eye::Worth | Eye::Youth { .. } | Eye::Grown { .. } => worth(),
            Eye::Crowd => fighter.fame.value().saturating_mul(market.price_per_fame),
        }
    }
}

/// A profile's thresholds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRules {
    /// Below this form, a fighter rests.
    pub rest_form: u32,
    /// Care is given, and a fighter bought, only if the club keeps this much money
    /// after paying.
    pub care_reserve: u32,
    /// A fit fighter fights in the colosseum where Elo gives it the best chance
    /// against the others, if at least this chance, and trains otherwise.
    pub fight_percent: u32,
    /// The club accepts the player's challenge if Elo gives its fighter at least this
    /// chance of winning.
    pub accept_percent: u32,
    /// Each week, the chance a club's fighter ranked in a colosseum is challenged by
    /// the rival fighter closest in Elo.
    pub challenge_percent: u32,
    /// Price asked for one of its fighters, and offered for one of the player's, in
    /// percent of its value (`market::value`).
    pub sell_percent: u32,
    pub buy_percent: u32,
    /// Its offer for one of the player's fighters not for sale, in percent of its
    /// `buy_percent` price: how much it gambles to tempt the player.
    pub unsolicited_percent: u32,
    /// Each post of the staff (`staff_posts`) is held at the best level whose salary,
    /// with those of the posts before it, the club could pay this many weeks from its
    /// money; someone already there stays while it could pay half as many.
    pub staff_weeks: u32,
    /// Weight of what lowers its relation with the player's club, and of a deal
    /// concluded with it, in percent (`relations::Relation::change`).
    pub anger_percent: u32,
    pub deal_percent: u32,
    /// How it rates the fighters it recruits, buys and lets go; by value if left out.
    #[serde(default)]
    pub recruits: Eye,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    pub cautious: ProfileRules,
    pub aggressive: ProfileRules,
    pub merchant: ProfileRules,
    pub mentor: ProfileRules,
    pub veteran: ProfileRules,
    pub showman: ProfileRules,
}

impl Profiles {
    #[must_use]
    pub fn get(&self, profile: Profile) -> &ProfileRules {
        match profile {
            Profile::Cautious => &self.cautious,
            Profile::Aggressive => &self.aggressive,
            Profile::Merchant => &self.merchant,
            Profile::Mentor => &self.mentor,
            Profile::Veteran => &self.veteran,
            Profile::Showman => &self.showman,
        }
    }
}

/// How the rival clubs are drawn when the world opens, from `data/world.ron`: each
/// list holds (weight, value) pairs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClubRules {
    /// How many when the world opens; at least one.
    pub count: u32,
    /// Most clubs open at once: no club is founded beyond it.
    pub max: u32,
    /// Clubs founded at each off-season's start: as many as closed, at least this many.
    pub founded: u32,
    /// A poor club with less money than this at an off-season's start is ruined for
    /// the season; ruined `ruined_seasons` seasons in a row, it closes.
    pub ruined_below: u32,
    pub ruined_seasons: u32,
    pub profiles: Vec<(u32, Profile)>,
    /// At the start of a game.
    pub wealth: Vec<(u32, Wealth)>,
    /// A club's home, drawn first.
    pub continents: Vec<(u32, Continent)>,
    /// Keys of `data/names.ron`'s cultures, drawn among those of the club's continent.
    pub cultures: Vec<(u32, String)>,
}

/// The size of the rival clubs' rosters.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RosterRules {
    /// A club below this many fighters in activity signs free agents.
    pub wanted: u32,
    /// Most fighters in activity a club may have: a full club buys no one.
    pub max: u32,
}

/// A rival club's fighter: the club's index in `GameState::rivals`, and the fighter's
/// in its roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RivalId {
    pub club: usize,
    pub fighter: usize,
}

/// A rival club in the game, drawn when the world opens (`random_club`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rival {
    /// No two clubs share it.
    pub name: names::ClubName,
    /// The key of its culture in `data/names.ron`, which named it.
    pub culture: String,
    /// Its home, its culture's: it fights mostly in the colosseums there.
    pub continent: Continent,
    pub profile: Profile,
    /// Below 0 when upkeep ran it into debt, like the player's.
    pub money: i64,
    /// Its fighters; like the player's, one who leaves keeps its place here, as a
    /// moved entry if it fights on elsewhere (`CareerFighter::moved_to`).
    #[serde(with = "crate::fighter::career::entries")]
    pub fighters: Vec<CareerFighter>,
    /// Its trainer and doctor, and a poor club's recruiter, paid every week
    /// (`staff_moves`).
    pub staff: Staff,
    /// Changes with its money at each off-season (`Wealth::next`).
    pub wealth: Wealth,
    /// Off-seasons in a row it started ruined (`ClubRules::ruined_below`).
    pub ruined_seasons: u32,
    /// The week it closed, ruined: its fighters went to the market, it plays no more.
    pub closed: Option<u64>,
}

impl Rival {
    /// Its profile's thresholds.
    #[must_use]
    pub fn rules<'a>(&self, data: &'a DesignData) -> &'a ProfileRules {
        data.world.profiles.get(self.profile)
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        self.closed.is_none()
    }

    /// Fighters neither retired, dead nor gone.
    #[must_use]
    pub fn active(&self) -> usize {
        self.fighters
            .iter()
            .filter(|f| f.status == Status::Active)
            .count()
    }
}

/// The key of a culture of `data/world.ron`'s `clubs.cultures` drawn by its weight
/// among those of `continent`, or of any continent without one or when `continent`
/// has none; `None` when no culture has a weight.
pub fn culture_on<R: Rng + ?Sized>(
    rng: &mut R,
    continent: Option<Continent>,
    data: &DesignData,
) -> Option<String> {
    let rules = &data.world.clubs;
    let of = |continent: Option<Continent>| -> Vec<(u32, usize)> {
        (0..)
            .zip(&rules.cultures)
            .filter(|(_, (_, key))| {
                continent.is_none_or(|home| data.culture(key).is_some_and(|c| c.continent == home))
            })
            .map(|(i, (weight, _))| (*weight, i))
            .collect()
    };
    let weighed = |cultures: &[(u32, usize)]| cultures.iter().any(|&(w, _)| w > 0);
    let mut cultures = of(continent);
    if !weighed(&cultures) {
        cultures = of(None);
    }
    weighed(&cultures).then(|| rules.cultures[pick(rng, &cultures)].1.clone())
}

/// A new rival club: profile and wealth drawn by `data/world.ron`'s `clubs` weights,
/// the continent furthest below its share of the clubs (`home`), then a culture of
/// that continent drawn by its weight, named by its culture unlike any open club of
/// `others` (a closed club's name may come back) and the player's club, `player`,
/// with its wealth's starting money, neither fighters nor staff.
pub fn random_club<R: Rng + ?Sized>(
    rng: &mut R,
    (others, player): (&[Rival], &names::ClubName),
    data: &DesignData,
) -> Rival {
    let rules = &data.world.clubs;
    let profile = pick(rng, &rules.profiles);
    let wealth = pick(rng, &rules.wealth);
    let continent = home(others, data);
    // Loading makes sure each continent drawn has a culture.
    let key = culture_on(rng, Some(continent), data).unwrap_or_default();
    let taken: Vec<&names::ClubName> = others
        .iter()
        .filter(|r| r.is_open())
        .map(|r| &r.name)
        .chain([player])
        .collect();
    let name = data
        .culture(&key)
        .map(|c| names::club_name(rng, c, &taken))
        .unwrap_or_default();
    Rival {
        name,
        culture: key,
        continent,
        profile,
        money: i64::from(data.economy.wealth.get(wealth).starting_money),
        fighters: Vec::new(),
        staff: Staff::default(),
        wealth,
        ruined_seasons: 0,
        closed: None,
    }
}

/// The home of a new club among `others`: the continent furthest below its share of
/// the open clubs by `clubs.continents`' weights, the first of equals, so that each
/// continent has its share whatever the draws and closures.
fn home(others: &[Rival], data: &DesignData) -> Continent {
    let rules = &data.world.clubs;
    let total: i64 = rules.continents.iter().map(|&(w, _)| i64::from(w)).sum();
    let open = others.iter().filter(|r| r.is_open());
    let count = i64::try_from(open.clone().count() + 1).unwrap_or(i64::MAX);
    (0..)
        .zip(&rules.continents)
        .filter(|(_, (weight, _))| *weight > 0)
        .max_by_key(|&(i, &(weight, continent))| {
            let have = open.clone().filter(|r| r.continent == continent).count();
            let have = i64::try_from(have).unwrap_or(i64::MAX);
            // Its share less what it has, both times `total`.
            (i64::from(weight) * count - have * total, Reverse(i))
        })
        .map_or(Continent::Europe, |(_, &(_, continent))| continent)
}

/// The posts a rival club of `wealth` fills, in order. A poor club, which bets on
/// finding talent among rookies and cheap free agents, hires a recruiter first to see
/// their potential; the others none, as they judge a fighter by what it shows.
#[must_use]
pub fn staff_posts(wealth: Wealth) -> &'static [Role] {
    match wealth {
        Wealth::Poor => &[Role::Recruiter, Role::Trainer, Role::Doctor],
        Wealth::Modest | Wealth::Rich => &[Role::Trainer, Role::Doctor],
    }
}

/// A rival club's changes of staff this week, with `money`, from `held` and the
/// candidates on `market`, by its profile's `staff_weeks`, post after post (`posts`):
/// the candidate of the best level, then the cheapest, whose salary, with those of the
/// posts before it, it could pay `staff_weeks` weeks; the one already there stays while
/// it could pay half as many, unless that candidate is of a better level. A post with
/// no one paid is left empty, and one held out of `posts` is dismissed.
#[must_use]
pub fn staff_moves(
    held: &Staff,
    money: i64,
    (rules, posts): (&ProfileRules, &[Role]),
    market: &[Candidate],
) -> Vec<StaffMove> {
    let weeks = i64::from(rules.staff_weeks);
    // Whether `salary`, with `paid` before, can be paid `weeks` weeks from `money`.
    let fits = |paid: i64, salary: u32, weeks: i64| (paid + i64::from(salary)) * weeks <= money;
    let mut moves = Vec::new();
    let mut paid = 0;
    for &role in posts {
        let best = market
            .iter()
            .enumerate()
            .filter(|(_, c)| c.person.role() == role && fits(paid, c.person.salary(), weeks))
            .max_by_key(|&(c, candidate)| {
                let person = &candidate.person;
                (person.level(), Reverse(person.salary()), Reverse(c))
            });
        let kept = held
            .level(role)
            .zip(held.salary(role))
            .filter(|&(_, salary)| fits(paid, salary, weeks / 2));
        let salary = match (kept, best) {
            (Some((level, salary)), best)
                if best.is_none_or(|(_, c)| c.person.level() <= level) =>
            {
                salary
            }
            (_, Some((c, candidate))) => {
                moves.push(StaffMove::Hire(c));
                candidate.person.salary()
            }
            (_, None) => {
                if held.level(role).is_some() {
                    moves.push(StaffMove::Dismiss(role));
                }
                0
            }
        };
        paid += i64::from(salary);
    }
    for role in [Role::Trainer, Role::Doctor, Role::Recruiter] {
        if !posts.contains(&role) && held.level(role).is_some() {
            moves.push(StaffMove::Dismiss(role));
        }
    }
    moves
}

/// A colosseum as a rival manager sees it for one fighter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Venue {
    /// The chance, from 0 to 1, Elo gives the fighter against the others there.
    pub chance: f64,
    /// What registering costs, the journey included.
    pub fee: u32,
    /// Whether it is on the club's continent.
    pub home: bool,
}

/// A rival manager's week for one fighter, with `money` left to spend this week, which
/// the plan's costs come out of: rest below the profile's form or when badly hurt or
/// ill, with care if the club keeps its reserve after paying; otherwise a fight in the
/// colosseum (`venues`, in colosseum order) where Elo gives it the best chance against
/// the others, among those it can pay the fee of where that chance reaches
/// `fight_percent`, at home if one is, abroad only otherwise; else training if it can
/// pay for it; else, if it `earns` its receipts, a fight in the best free colosseum at
/// home whatever its chance, as receipts are a club's only income; else rest. Care and
/// training are at the prices of the club's `staff`.
#[must_use]
pub fn plan(
    fighter: &CareerFighter,
    venues: &[Venue],
    money: &mut i64,
    (rules, staff, earns): (&ProfileRules, &Staff, bool),
    data: &DesignData,
) -> WeekPlan {
    let hurt = badly_hurt_or_ill(fighter);
    let care = staff.effects().medicine.care_rules(&data.fighter.care);
    let care_cost = i64::from(fighter.care_cost(&care));
    let care = hurt && care_cost > 0 && *money - care_cost >= i64::from(rules.care_reserve);
    if care {
        *money -= care_cost;
    }
    let best = |chosen: &dyn Fn(&Venue) -> bool| {
        venues
            .iter()
            .zip(0..)
            .filter(|(v, _)| chosen(v))
            .reduce(|best, next| {
                let better = (next.0.home, next.0.chance) > (best.0.home, best.0.chance);
                if better { next } else { best }
            })
    };
    let chosen =
        best(&|v| i64::from(v.fee) <= *money && v.chance * 100.0 >= f64::from(rules.fight_percent));
    let free = best(&|v| v.home && v.fee == 0);
    let training = i64::from(staff.training_cost(data.economy.training_cost));
    let activity = if hurt || fighter.form.value() < rules.rest_form {
        Activity::Rest
    } else if let Some((venue, id)) = chosen {
        *money -= i64::from(venue.fee);
        Activity::Fight(ColosseumId(id))
    } else if training <= *money {
        *money -= training;
        Activity::Train
    } else if let Some((_, id)) = free.filter(|_| earns) {
        Activity::Fight(ColosseumId(id))
    } else {
        Activity::Rest
    };
    WeekPlan {
        activity,
        care,
        refused: Refused::NONE,
        loadout: fighter.preferred_loadout(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fighter::form::Form;
    use crate::fighter::injury::{Injury, Location, Severity};
    use crate::staff::Person;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    /// The colosseums of `data` at these chances, all at home, at their entry fees.
    fn at(chances: &[f64], data: &DesignData) -> Vec<Venue> {
        chances
            .iter()
            .zip(&data.world.colosseums)
            .map(|(&chance, c)| Venue {
                chance,
                fee: c.purses.entry_fee,
                home: true,
            })
            .collect()
    }

    #[test]
    fn each_eye_rates_a_fighter_its_way() {
        let data = DesignData::load().unwrap();
        let market = &data.economy.market;
        let profiles = &data.world.profiles;
        let mut fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        fighter.fame = crate::fighter::fame::Fame::new(10);
        let worth = value(&fighter, market);
        let [mentor, veteran, showman] =
            [&profiles.mentor, &profiles.veteran, &profiles.showman].map(|p| p.recruits);
        assert_eq!(profiles.cautious.recruits.appeal(&fighter, market), worth);
        fighter.age = 20;
        assert!(mentor.appeal(&fighter, market) > worth);
        assert_eq!(veteran.appeal(&fighter, market), worth);
        fighter.age = 32;
        assert_eq!(mentor.appeal(&fighter, market), worth);
        assert!(veteran.appeal(&fighter, market) > worth);
        // Stats do not count for the crowd, fame alone.
        let crowd = showman.appeal(&fighter, market);
        assert_eq!(crowd, 10 * market.price_per_fame);
        fighter.stats.hp += 50;
        assert_eq!(showman.appeal(&fighter, market), crowd);
    }

    #[test]
    fn each_profile_rests_fights_or_trains_by_its_thresholds() {
        let data = DesignData::load().unwrap();
        let profiles = &data.world.profiles;
        let none = Staff::default();
        let mut fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        let rome = Activity::Fight(ColosseumId(1));
        let mut money = 1000;
        // Fit, and better off in Rome: everyone fights there, and the fee is set aside.
        for profile in [Profile::Cautious, Profile::Aggressive, Profile::Merchant] {
            let plan = plan(
                &fighter,
                &at(&[0.4, 0.5], &data),
                &mut money,
                (profiles.get(profile), &none, true),
                &data,
            );
            assert_eq!(plan.activity, rome);
        }
        assert_eq!(money, 1000 - 3 * 20);
        // Behind in Elo: the cautious club trains, the aggressive one still fights.
        let cautious = &profiles.cautious;
        let behind = [0.3, 0.2];
        assert_eq!(
            plan(
                &fighter,
                &at(&behind, &data),
                &mut money,
                (cautious, &none, true),
                &data
            )
            .activity,
            Activity::Train
        );
        let aggressive = &profiles.aggressive;
        assert_eq!(
            plan(
                &fighter,
                &at(&behind, &data),
                &mut money,
                (aggressive, &none, true),
                &data
            )
            .activity,
            Activity::Fight(ColosseumId(0))
        );
        // Tired: only the aggressive club sends it in.
        fighter.form = Form::new(50);
        assert_eq!(
            plan(
                &fighter,
                &at(&[0.5, 0.5], &data),
                &mut money,
                (cautious, &none, true),
                &data
            )
            .activity,
            Activity::Rest
        );
        assert_eq!(
            plan(
                &fighter,
                &at(&[0.5, 0.5], &data),
                &mut money,
                (aggressive, &none, true),
                &data
            )
            .activity,
            Activity::Fight(ColosseumId(0))
        );
    }

    #[test]
    fn without_money_a_club_fights_for_the_receipts() {
        let data = DesignData::load().unwrap();
        let none = Staff::default();
        let aggressive = &data.world.profiles.aggressive;
        let fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        // Without the fee, the free colosseum; without money, training only if paid
        // for, else a fight for the receipts in the free colosseum, whatever the chance;
        // a free agent, who earns none, rests.
        let mut five = 5;
        assert_eq!(
            plan(
                &fighter,
                &at(&[0.4, 0.5], &data),
                &mut five,
                (aggressive, &none, true),
                &data
            )
            .activity,
            Activity::Fight(ColosseumId(0))
        );
        let mut broke = 4;
        assert_eq!(
            plan(
                &fighter,
                &at(&[0.1, 0.1], &data),
                &mut broke,
                (aggressive, &none, true),
                &data
            )
            .activity,
            Activity::Fight(ColosseumId(0))
        );
        assert_eq!(
            plan(
                &fighter,
                &at(&[0.1, 0.1], &data),
                &mut broke,
                (aggressive, &none, false),
                &data
            )
            .activity,
            Activity::Rest
        );
        assert_eq!(broke, 4);
    }

    #[test]
    fn a_club_fights_abroad_only_when_nothing_at_home_suits_it() {
        let data = DesignData::load().unwrap();
        let fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        let rules = (&data.world.profiles.cautious, &Staff::default(), true);
        let venue = |chance, fee, home| Venue { chance, fee, home };
        // Better off abroad, but a fight at home suits it: at home.
        let mut money = 1000;
        let venues = [venue(0.5, 0, true), venue(0.9, 60, false)];
        let week = plan(&fighter, &venues, &mut money, rules, &data);
        assert_eq!(week.activity, Activity::Fight(ColosseumId(0)));
        // Nothing at home suits it: abroad, journey paid.
        let venues = [venue(0.3, 0, true), venue(0.6, 60, false)];
        let week = plan(&fighter, &venues, &mut money, rules, &data);
        assert_eq!(
            (week.activity, money),
            (Activity::Fight(ColosseumId(1)), 940)
        );
        // Unless it cannot pay the journey.
        let mut money = 50;
        let week = plan(&fighter, &venues, &mut money, rules, &data);
        assert_eq!(week.activity, Activity::Train);
    }

    #[test]
    fn care_is_given_only_above_the_reserve() {
        let data = DesignData::load().unwrap();
        let profiles = &data.world.profiles;
        let none = Staff::default();
        let mut fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        fighter.injuries.push(Injury {
            location: Location::Body,
            severity: Severity::Severe,
            weeks_left: 5,
        });
        let cost = i64::from(fighter.care_cost(&data.fighter.care));
        assert!(cost > 0);
        let even = [0.5, 0.5];
        let mut money = cost + 100;
        let cautious = plan(
            &fighter,
            &at(&even, &data),
            &mut money,
            (&profiles.cautious, &none, true),
            &data,
        );
        assert_eq!(
            cautious,
            WeekPlan {
                activity: Activity::Rest,
                care: true,
                refused: Refused::NONE,
                loadout: fighter.loadout,
            }
        );
        assert_eq!(money, 100);
        let mut money = cost + 100;
        let merchant = plan(
            &fighter,
            &at(&even, &data),
            &mut money,
            (&profiles.merchant, &none, true),
            &data,
        );
        assert_eq!(
            merchant,
            WeekPlan {
                activity: Activity::Rest,
                care: false,
                refused: Refused::NONE,
                loadout: fighter.loadout,
            }
        );
        // A doctor's care costs more, at its price.
        let doctor = Staff::of_levels(&[(Role::Doctor, 5)], &data.economy.staff);
        let medicine = doctor.effects().medicine;
        let dear = i64::from(fighter.care_cost(&medicine.care_rules(&data.fighter.care)));
        assert!(dear > cost);
        let mut money = dear + 100;
        let cared = plan(
            &fighter,
            &at(&even, &data),
            &mut money,
            (&profiles.cautious, &doctor, true),
            &data,
        );
        assert!(cared.care);
        assert_eq!(money, 100);
    }

    #[test]
    fn a_trainer_sets_the_price_of_training() {
        let data = DesignData::load().unwrap();
        let fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        let trainer = Staff::of_levels(&[(Role::Trainer, 5)], &data.economy.staff);
        let price = i64::from(data.economy.staff.trainer(5).unwrap().training_cost);
        let mut money = 1000;
        let rules = (&data.world.profiles.cautious, &trainer, true);
        let week = plan(&fighter, &at(&[0.1, 0.1], &data), &mut money, rules, &data);
        assert_eq!(week.activity, Activity::Train);
        assert_eq!(money, 1000 - price);
    }

    #[test]
    fn a_rival_club_hires_the_best_candidates_its_money_can_keep() {
        let data = DesignData::load().unwrap();
        let staff = &data.economy.staff;
        let profile = ProfileRules {
            staff_weeks: 10,
            ..data.world.profiles.cautious.clone()
        };
        let rules = (&profile, staff_posts(Wealth::Modest));
        let person = |role, level| Person::of_level(role, level, staff).unwrap();
        let candidate = |role, level| Candidate {
            person: person(role, level),
            weeks_left: 1,
        };
        let salary = |role, level| i64::from(person(role, level).salary());
        // A cheaper trainer of level 5 comes after the usual one.
        let mut cheap = candidate(Role::Trainer, 5);
        let Person::Trainer(m) = &mut cheap.person else {
            unreachable!()
        };
        m.rules.salary -= 1;
        let market = [
            candidate(Role::Trainer, 1),
            candidate(Role::Trainer, 5),
            cheap,
            candidate(Role::Doctor, 2),
            candidate(Role::Doctor, 5),
            candidate(Role::Recruiter, 5),
        ];
        let none = Staff::default();
        // Nothing to pay ten weeks of the cheapest trainer with.
        let poor = salary(Role::Trainer, 1) * 10 - 1;
        assert_eq!(staff_moves(&none, poor, rules, &market), []);
        // Rich: the best of both, the cheaper of equals, never a recruiter.
        let rich = (salary(Role::Trainer, 5) + salary(Role::Doctor, 5)) * 10;
        assert_eq!(
            staff_moves(&none, rich, rules, &market),
            [StaffMove::Hire(2), StaffMove::Hire(4)]
        );
        // The trainer first, the doctor with what is left.
        let trainer_only = salary(Role::Trainer, 5) * 10;
        assert_eq!(
            staff_moves(&none, trainer_only, rules, &market),
            [StaffMove::Hire(2)]
        );
        let doctor_2 = (salary(Role::Trainer, 5) - 1 + salary(Role::Doctor, 2)) * 10;
        assert_eq!(
            staff_moves(&none, doctor_2, rules, &market),
            [StaffMove::Hire(2), StaffMove::Hire(3)]
        );
        // Those held stay, as no candidate is better, and the recruiter goes.
        let full = Staff::of_levels(
            &[(Role::Trainer, 5), (Role::Doctor, 5), (Role::Recruiter, 1)],
            staff,
        );
        assert_eq!(
            staff_moves(&full, rich, rules, &market),
            [StaffMove::Dismiss(Role::Recruiter)]
        );
        // A better candidate takes the place of the one held.
        let low = Staff::of_levels(&[(Role::Trainer, 1)], staff);
        assert_eq!(
            staff_moves(&low, rich, rules, &market),
            [StaffMove::Hire(2), StaffMove::Hire(4)]
        );
        // Poorer, the staff stays while half as many weeks are paid; then the last
        // post goes first, for the best candidate still paid, or no one.
        let both = Staff::of_levels(&[(Role::Trainer, 5), (Role::Doctor, 5)], staff);
        let half = rich / 2;
        assert_eq!(staff_moves(&both, half, rules, &market), []);
        assert_eq!(
            staff_moves(&both, half - 1, rules, &market),
            [StaffMove::Dismiss(Role::Doctor)]
        );
        let doctor_5 = Staff::of_levels(&[(Role::Doctor, 5)], staff);
        let low_staff = (salary(Role::Trainer, 1) + salary(Role::Doctor, 2)) * 10;
        assert_eq!(
            staff_moves(&doctor_5, low_staff, rules, &market),
            [StaffMove::Hire(0), StaffMove::Hire(3)]
        );
        // A poor club hires its recruiter first, then the trainer with what is left.
        let poor = (&profile, staff_posts(Wealth::Poor));
        let recruiter = salary(Role::Recruiter, 5) * 10;
        assert_eq!(
            staff_moves(&none, recruiter, poor, &market),
            [StaffMove::Hire(5)]
        );
        assert_eq!(
            staff_moves(&none, recruiter + trainer_only, poor, &market),
            [StaffMove::Hire(5), StaffMove::Hire(2)]
        );
    }
}
