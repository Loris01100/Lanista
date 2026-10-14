//! Rival clubs: managers playing by simple weekly rules whose thresholds come from
//! their profile (`data/world.ron`). The colosseums' residents are their fighters.

use serde::{Deserialize, Serialize};

use crate::colosseum::ColosseumId;
use crate::design_data::DesignData;
use crate::fighter::career::{Activity, CareerFighter, WeekPlan, badly_hurt_or_ill};

/// How a rival manager plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Profile {
    Cautious,
    Aggressive,
    /// Keeps its money.
    // ponytail: thresholds only; buying and selling come with the market.
    Merchant,
}

/// A profile's thresholds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRules {
    /// Below this form, a fighter rests.
    pub rest_form: u32,
    /// Care is given only if the club keeps this much money after paying for it.
    pub care_reserve: u32,
    /// A fit fighter fights if Elo gives it at least this chance against the others
    /// of its colosseum, and trains otherwise.
    pub fight_percent: u32,
    /// The club accepts the player's challenge if Elo gives its fighter at least this
    /// chance of winning.
    pub accept_percent: u32,
    /// Each week, the chance a club's fighter ranked in the colosseum is challenged by
    /// the resident closest in Elo.
    pub challenge_percent: u32,
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

/// A rival club in the game, in the order of `data/world.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rival {
    /// Below 0 when upkeep ran it into debt, like the player's.
    pub money: i64,
}

/// The rival club resident `r` of colosseum `c` belongs to, out of `rivals`: the
/// residents are shared out in turn.
// ponytail: fixed by place, a replaced resident joins the same club; the market
// gives clubs their own rosters.
#[must_use]
pub fn owner(c: usize, r: usize, rivals: usize) -> usize {
    (c + r) % rivals.max(1)
}

/// A rival manager's week for one fighter of colosseum `home`, with `money` left to
/// spend this week, which the plan's costs come out of: rest below the profile's form
/// or when badly hurt or ill, with care if the club keeps its reserve after paying;
/// otherwise a fight there if Elo gives it `fight_percent` chances against the
/// others (`chance`, from 0 to 1) and the club can pay the fee, else training if it
/// can pay for it, else rest.
// ponytail: fights in its own colosseum, the one it is ranked in, until the market
// moves fighters; "where its chances are best" then picks among colosseums.
#[must_use]
pub fn plan(
    fighter: &CareerFighter,
    chance: f64,
    home: ColosseumId,
    money: &mut i64,
    rules: &ProfileRules,
    data: &DesignData,
) -> WeekPlan {
    let hurt = badly_hurt_or_ill(fighter);
    let care_cost = i64::from(fighter.care_cost(&data.fighter));
    let care = hurt && care_cost > 0 && *money - care_cost >= i64::from(rules.care_reserve);
    if care {
        *money -= care_cost;
    }
    let fee = data
        .world
        .colosseum(home)
        .map(|c| i64::from(c.purses.entry_fee));
    let training = i64::from(data.economy.training_cost);
    let activity = if hurt || fighter.form.value() < rules.rest_form {
        Activity::Rest
    } else if chance * 100.0 >= f64::from(rules.fight_percent)
        && let Some(fee) = fee.filter(|&fee| fee <= *money)
    {
        *money -= fee;
        Activity::Fight(home)
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
        let mut fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        let home = ColosseumId(1);
        let mut money = 1000;
        let fight = Activity::Fight(home);
        // Fit and even: everyone fights, and the fee is set aside.
        for profile in [Profile::Cautious, Profile::Aggressive, Profile::Merchant] {
            let plan = plan(
                &fighter,
                0.5,
                home,
                &mut money,
                profiles.get(profile),
                &data,
            );
            assert_eq!(plan.activity, fight);
        }
        assert_eq!(money, 1000 - 3 * 20);
        // Behind in Elo: the cautious club trains, the aggressive one still fights.
        let cautious = &profiles.cautious;
        assert_eq!(
            plan(&fighter, 0.3, home, &mut money, cautious, &data).activity,
            Activity::Train
        );
        let aggressive = &profiles.aggressive;
        assert_eq!(
            plan(&fighter, 0.3, home, &mut money, aggressive, &data).activity,
            fight
        );
        // Tired: only the aggressive club sends it in.
        fighter.form = Form::new(50);
        assert_eq!(
            plan(&fighter, 0.5, home, &mut money, cautious, &data).activity,
            Activity::Rest
        );
        assert_eq!(
            plan(&fighter, 0.5, home, &mut money, aggressive, &data).activity,
            fight
        );
        // No money: training only if paid for, else rest.
        let mut broke = 5;
        assert_eq!(
            plan(&fighter, 0.3, home, &mut broke, cautious, &data).activity,
            Activity::Rest
        );
    }

    #[test]
    fn care_is_given_only_above_the_reserve() {
        let data = DesignData::load().unwrap();
        let profiles = &data.world.profiles;
        let mut fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        fighter.injuries.push(Injury {
            location: Location::Body,
            severity: Severity::Severe,
            weeks_left: 5,
        });
        let cost = i64::from(fighter.care_cost(&data.fighter));
        assert!(cost > 0);
        let mut money = cost + 100;
        let cautious = plan(
            &fighter,
            0.5,
            ColosseumId(0),
            &mut money,
            &profiles.cautious,
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
            0.5,
            ColosseumId(0),
            &mut money,
            &profiles.merchant,
            &data,
        );
        assert_eq!(
            merchant,
            WeekPlan {
                activity: Activity::Rest,
                care: false
            }
        );
    }

    #[test]
    fn residents_are_shared_out_in_turn() {
        assert_eq!(
            (0..4).map(|r| owner(1, r, 3)).collect::<Vec<_>>(),
            [1, 2, 0, 1]
        );
        assert_eq!(owner(0, 5, 0), 0);
    }
}
