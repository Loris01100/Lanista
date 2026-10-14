//! The market: free agents signed for nothing (rookies each off-season, fighters released
//! by the clubs), rival fighters bought and the club's sold, and the ranges a fighter's
//! stats are seen within from outside the club.

use rand::Rng;
use serde::Deserialize;

use crate::colosseum::ColosseumId;
use crate::design_data::DesignData;
use crate::fighter::career::CareerFighter;
use crate::fighter::percent_of;

/// From `data/economy.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketRules {
    /// Fewest free agents on the market; missing ones come at the end of the week.
    pub min_free_agents: u32,
    /// Most; beyond, the least valuable leave the market.
    pub max_free_agents: u32,
    /// Free agents arriving at the start of each off-season, `rookie_age` years old.
    pub rookies: u32,
    pub rookie_age: u32,
    /// A fighter's value: this much per point of trained stats, less
    /// `age_discount_percent` per year above `prime_age`, down to a tenth.
    pub price_per_point: u32,
    pub prime_age: u32,
    pub age_discount_percent: u32,
    /// The rival clubs release their fighters this old at the start of each off-season.
    pub release_age: u32,
    /// Width of the range a stat is seen within, in percent of the stat, before and
    /// after a week of observation.
    pub range_percent: u32,
    pub observed_range_percent: u32,
}

/// One market move of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deal {
    /// Signs the free agent at this index, for nothing.
    Sign(usize),
    /// Buys this resident from its club, at its asking price.
    Buy(ColosseumId, usize),
    /// Sells the club's fighter at this index to the rival offering the most.
    Sell(usize),
    /// Lets the club's fighter at this index go, for nothing: a free agent.
    Release(usize),
}

/// A fighter out of the club the player may observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    FreeAgent(usize),
    Resident(ColosseumId, usize),
}

/// What `fighter` is worth on the market.
#[must_use]
pub fn value(fighter: &CareerFighter, rules: &MarketRules) -> u32 {
    let points: u32 = fighter.stats.to_array().iter().sum();
    let years = fighter.age.saturating_sub(rules.prime_age);
    let discount = rules.age_discount_percent.saturating_mul(years).min(90);
    percent_of(points.saturating_mul(rules.price_per_point), 100 - discount)
}

/// The range each trained stat of `fighter` is seen within, in `Stats::to_array`
/// order, narrower once observed. The stat sits at a fixed place in its range, taken
/// from the hidden potential, so the middle does not give it away.
// ponytail: place derived from the potential rather than drawn and saved; draw it once
// players learn to read it.
#[must_use]
pub fn stat_ranges(fighter: &CareerFighter, rules: &MarketRules) -> [(u32, u32); 6] {
    let width = if fighter.observed {
        rules.observed_range_percent
    } else {
        rules.range_percent
    };
    let potential = fighter.potential.to_array();
    let mut i = 0;
    fighter.stats.to_array().map(|stat| {
        let place = (potential[i] * 37 + 11 * u32::try_from(i).unwrap_or(0)) % 101;
        i += 1;
        let wide = percent_of(stat, width);
        let low = stat - percent_of(wide, place);
        (low, low + wide)
    })
}

/// A new fighter of `rules.rookie_age` years.
pub fn rookie<R: Rng + ?Sized>(rng: &mut R, data: &DesignData) -> CareerFighter {
    let mut fighter = CareerFighter::random(rng, data);
    fighter.age = data.economy.market.rookie_age;
    fighter
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn value_follows_stats_and_drops_with_age() {
        let data = DesignData::load().unwrap();
        let rules = &data.economy.market;
        let mut fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        fighter.age = rules.prime_age;
        let points: u32 = fighter.stats.to_array().iter().sum();
        let prime = value(&fighter, rules);
        assert_eq!(prime, points * rules.price_per_point);
        fighter.age += 2;
        assert_eq!(
            value(&fighter, rules),
            percent_of(prime, 100 - 2 * rules.age_discount_percent)
        );
        fighter.age = 90;
        assert_eq!(value(&fighter, rules), percent_of(prime, 10));
    }

    #[test]
    fn stats_are_seen_within_ranges_narrowed_by_observation() {
        let data = DesignData::load().unwrap();
        let rules = &data.economy.market;
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        for _ in 0..20 {
            let mut fighter = CareerFighter::random(&mut rng, &data);
            let stats = fighter.stats.to_array();
            let unseen = stat_ranges(&fighter, rules);
            fighter.observed = true;
            let seen = stat_ranges(&fighter, rules);
            for ((&stat, (low, high)), (seen_low, seen_high)) in stats.iter().zip(unseen).zip(seen)
            {
                assert!(low <= stat && stat <= high, "{stat} in {low}-{high}");
                assert!(seen_low <= stat && stat <= seen_high);
                assert!(seen_high - seen_low <= high - low);
                assert_eq!(high - low, percent_of(stat, rules.range_percent));
            }
        }
    }

    #[test]
    fn rookies_are_all_the_same_age() {
        let data = DesignData::load().unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        assert!((0..10).all(|_| rookie(&mut rng, &data).age == data.economy.market.rookie_age));
    }
}
