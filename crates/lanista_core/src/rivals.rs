//! Rival clubs: managers playing by simple weekly rules whose thresholds come from
//! their profile (`data/world.ron`), each with its own roster of fighters.

use serde::{Deserialize, Serialize};

use crate::colosseum::ColosseumId;
use crate::design_data::DesignData;
use crate::fighter::career::{Activity, CareerFighter, Status, WeekPlan, badly_hurt_or_ill};
use crate::staff::{Role, Staff, StaffRules};

/// How a rival manager plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Profile {
    Cautious,
    Aggressive,
    /// Keeps its money, sells cheap and buys dear.
    Merchant,
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
    /// Each post of the staff (`STAFF_POSTS`) is held at the best level whose salary,
    /// with those of the posts before it, the club could pay this many weeks from its
    /// money; someone already there stays while it could pay half as many.
    pub staff_weeks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    pub cautious: ProfileRules,
    pub aggressive: ProfileRules,
    pub merchant: ProfileRules,
}

impl Profiles {
    #[must_use]
    pub fn get(&self, profile: Profile) -> &ProfileRules {
        match profile {
            Profile::Cautious => &self.cautious,
            Profile::Aggressive => &self.aggressive,
            Profile::Merchant => &self.merchant,
        }
    }
}

/// A rival club, from `data/world.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RivalData {
    /// Shown to the player.
    pub name: String,
    pub profile: Profile,
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

/// A rival club in the game, in the order of `data/world.ron`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rival {
    /// Below 0 when upkeep ran it into debt, like the player's.
    pub money: i64,
    /// Its fighters; like the player's, one who leaves keeps its place here.
    pub fighters: Vec<CareerFighter>,
    /// Its trainer and doctor, paid every week (`hired_staff`).
    pub staff: Staff,
}

impl Rival {
    /// Fighters neither retired, dead nor gone.
    #[must_use]
    pub fn active(&self) -> usize {
        self.fighters
            .iter()
            .filter(|f| f.status == Status::Active)
            .count()
    }
}

/// The posts a rival club fills, in order: no recruiter, as a rival club already
/// knows every fighter's worth.
pub const STAFF_POSTS: [Role; 2] = [Role::Trainer, Role::Doctor];

/// The staff a rival club with `money` keeps this week, from `held`, by its profile's
/// `staff_weeks`: post after post (`STAFF_POSTS`), the best level whose salary, with
/// those of the posts before it, it could pay `staff_weeks` weeks; or the one already
/// there, if better, while it could pay half as many. A post no level fits is left
/// empty.
#[must_use]
pub fn hired_staff(held: &Staff, money: i64, rules: &ProfileRules, staff: &StaffRules) -> Staff {
    let weeks = i64::from(rules.staff_weeks);
    // Whether `salary`, with `paid` before, can be paid `weeks` weeks from `money`.
    let fits = |paid: i64, salary: u32, weeks: i64| (paid + i64::from(salary)) * weeks <= money;
    let mut hired = Staff::default();
    let mut paid = 0;
    for role in STAFF_POSTS {
        let levels = u32::try_from(staff.levels(role)).unwrap_or(u32::MAX);
        let best = (1..=levels).rev().find(|&level| {
            staff
                .salary(role, level)
                .is_some_and(|s| fits(paid, s, weeks))
        });
        let kept = held.level(role).filter(|&level| {
            best.is_none_or(|best| level > best)
                && staff
                    .salary(role, level)
                    .is_some_and(|s| fits(paid, s, weeks / 2))
        });
        let level = kept.or(best);
        paid += level
            .and_then(|level| staff.salary(role, level))
            .map_or(0, i64::from);
        hired.set(role, level);
    }
    hired
}

/// A rival manager's week for one fighter, with `money` left to spend this week, which
/// the plan's costs come out of: rest below the profile's form or when badly hurt or
/// ill, with care if the club keeps its reserve after paying; otherwise a fight in the
/// colosseum where Elo gives it the best chance against the others (`chances`, from 0
/// to 1, in colosseum order) among those it can pay the fee of, if that chance reaches
/// `fight_percent`; else training if it can pay for it, else rest. Care and training
/// are at the prices of the club's `staff`.
#[must_use]
pub fn plan(
    fighter: &CareerFighter,
    chances: &[f64],
    money: &mut i64,
    (rules, staff): (&ProfileRules, &Staff),
    data: &DesignData,
) -> WeekPlan {
    let hurt = badly_hurt_or_ill(fighter);
    let staff_rules = &data.economy.staff;
    let care = staff
        .effects(staff_rules)
        .medicine
        .care_rules(&data.fighter.care);
    let care_cost = i64::from(fighter.care_cost(&care));
    let care = hurt && care_cost > 0 && *money - care_cost >= i64::from(rules.care_reserve);
    if care {
        *money -= care_cost;
    }
    let best = chances
        .iter()
        .zip(&data.world.colosseums)
        .zip(0..)
        .filter(|((_, c), _)| i64::from(c.purses.entry_fee) <= *money)
        .map(|((&chance, c), id)| (chance, i64::from(c.purses.entry_fee), ColosseumId(id)))
        .reduce(|best, next| if next.0 > best.0 { next } else { best });
    let training = i64::from(staff.training_cost(staff_rules, data.economy.training_cost));
    let activity = if hurt || fighter.form.value() < rules.rest_form {
        Activity::Rest
    } else if let Some((_, fee, id)) =
        best.filter(|&(chance, ..)| chance * 100.0 >= f64::from(rules.fight_percent))
    {
        *money -= fee;
        Activity::Fight(id)
    } else if training <= *money {
        *money -= training;
        Activity::Train
    } else {
        Activity::Rest
    };
    WeekPlan { activity, care }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fighter::form::Form;
    use crate::fighter::injury::{Injury, Location, Severity};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

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
                &[0.4, 0.5],
                &mut money,
                (profiles.get(profile), &none),
                &data,
            );
            assert_eq!(plan.activity, rome);
        }
        assert_eq!(money, 1000 - 3 * 20);
        // Behind in Elo: the cautious club trains, the aggressive one still fights.
        let cautious = &profiles.cautious;
        let behind = [0.3, 0.2];
        assert_eq!(
            plan(&fighter, &behind, &mut money, (cautious, &none), &data).activity,
            Activity::Train
        );
        let aggressive = &profiles.aggressive;
        assert_eq!(
            plan(&fighter, &behind, &mut money, (aggressive, &none), &data).activity,
            Activity::Fight(ColosseumId(0))
        );
        // Tired: only the aggressive club sends it in.
        fighter.form = Form::new(50);
        assert_eq!(
            plan(&fighter, &[0.5, 0.5], &mut money, (cautious, &none), &data).activity,
            Activity::Rest
        );
        assert_eq!(
            plan(
                &fighter,
                &[0.5, 0.5],
                &mut money,
                (aggressive, &none),
                &data
            )
            .activity,
            Activity::Fight(ColosseumId(0))
        );
        // Without the fee, the free colosseum; without money, training only if paid for.
        let mut five = 5;
        assert_eq!(
            plan(&fighter, &[0.4, 0.5], &mut five, (aggressive, &none), &data).activity,
            Activity::Fight(ColosseumId(0))
        );
        let mut broke = 4;
        assert_eq!(
            plan(
                &fighter,
                &[0.1, 0.1],
                &mut broke,
                (aggressive, &none),
                &data
            )
            .activity,
            Activity::Rest
        );
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
            &even,
            &mut money,
            (&profiles.cautious, &none),
            &data,
        );
        assert_eq!(
            cautious,
            WeekPlan {
                activity: Activity::Rest,
                care: true
            }
        );
        assert_eq!(money, 100);
        let mut money = cost + 100;
        let merchant = plan(
            &fighter,
            &even,
            &mut money,
            (&profiles.merchant, &none),
            &data,
        );
        assert_eq!(
            merchant,
            WeekPlan {
                activity: Activity::Rest,
                care: false
            }
        );
        // A doctor's care costs more, at its price.
        let doctor = Staff {
            doctor: Some(5),
            ..Staff::default()
        };
        let medicine = doctor.effects(&data.economy.staff).medicine;
        let dear = i64::from(fighter.care_cost(&medicine.care_rules(&data.fighter.care)));
        assert!(dear > cost);
        let mut money = dear + 100;
        let cared = plan(
            &fighter,
            &even,
            &mut money,
            (&profiles.cautious, &doctor),
            &data,
        );
        assert!(cared.care);
        assert_eq!(money, 100);
    }

    #[test]
    fn a_trainer_sets_the_price_of_training() {
        let data = DesignData::load().unwrap();
        let fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        let trainer = Staff {
            trainer: Some(5),
            ..Staff::default()
        };
        let price = i64::from(data.economy.staff.trainer(5).unwrap().training_cost);
        let mut money = 1000;
        let rules = (&data.world.profiles.cautious, &trainer);
        let week = plan(&fighter, &[0.1, 0.1], &mut money, rules, &data);
        assert_eq!(week.activity, Activity::Train);
        assert_eq!(money, 1000 - price);
    }

    #[test]
    fn a_rival_club_hires_the_staff_its_money_can_keep() {
        let data = DesignData::load().unwrap();
        let staff = &data.economy.staff;
        let rules = ProfileRules {
            staff_weeks: 10,
            ..data.world.profiles.cautious.clone()
        };
        let salary = |role, level| i64::from(staff.salary(role, level).unwrap());
        let both = |trainer, doctor| Staff {
            trainer,
            doctor,
            recruiter: None,
        };
        // Nothing to pay ten weeks of the cheapest trainer with.
        let none = Staff::default();
        let poor = salary(Role::Trainer, 1) * 10 - 1;
        assert_eq!(hired_staff(&none, poor, &rules, staff), none);
        // Rich: the best of both, never a recruiter.
        let full = Staff {
            recruiter: Some(1),
            ..both(Some(5), Some(5))
        };
        let rich = (salary(Role::Trainer, 5) + salary(Role::Doctor, 5)) * 10;
        assert_eq!(
            hired_staff(&none, rich, &rules, staff),
            both(Some(5), Some(5))
        );
        assert_eq!(
            hired_staff(&full, rich, &rules, staff),
            both(Some(5), Some(5))
        );
        // The trainer first, the doctor with what is left.
        let trainer_only = salary(Role::Trainer, 5) * 10;
        assert_eq!(
            hired_staff(&none, trainer_only, &rules, staff),
            both(Some(5), None)
        );
        // Poorer, the staff stays while half as many weeks are paid; then the last
        // post goes first.
        let half = rich / 2;
        assert_eq!(
            hired_staff(&full, half, &rules, staff),
            both(Some(5), Some(5))
        );
        assert_eq!(
            hired_staff(&full, half - 1, &rules, staff),
            both(Some(5), None)
        );
    }
}
