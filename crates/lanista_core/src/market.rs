//! The market: free agents signed for nothing (rookies each off-season, fighters released
//! by the clubs), rival fighters bought by negotiation, the club's fighters sold from a
//! transfer list, and what the club sees of a fighter's stats from outside.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::design_data::DesignData;
use crate::fighter::career::CareerFighter;
use crate::fighter::fame::Fame;
use crate::fighter::{STAT_COUNT, percent_of};
use crate::rivals::RivalId;
use crate::staff::Scouting;

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
    /// A fighter's value: so much per point of trained stats and of fame, per fight won
    /// and drawn, less so much per fight lost; never below a tenth of its stats' part.
    pub price_per_point: u32,
    pub price_per_fame: u32,
    pub price_per_win: u32,
    pub price_per_draw: u32,
    pub price_per_loss: u32,
    /// At the start of each off-season, a rival club releases its fighters this old
    /// worth less than its average fighter.
    pub release_age: u32,
    /// A rich rival club also releases, at any age, its fighters worth less than this
    /// percentage of its average fighter.
    pub rich_release_percent: u32,
    /// Each week, a rich rival club tries to buy a fighter of another club worth at
    /// least this percentage of its weakest.
    pub rich_upgrade_percent: u32,
    /// A club counters an offer of at least this percentage of its asking price, and
    /// refuses a lower one.
    pub counter_percent: u32,
    /// A fighter's chance of refusing to join the player's club, before its fame, the
    /// club's and its caution.
    pub refusal_percent: u32,
    /// Stats of a fighter out of the club seen without observing it, and how many more
    /// each week of observation reveals; once all are seen, a further week narrows
    /// their ranges from `range_percent` to `narrowed_range_percent` of the stat.
    pub shown_stats: u32,
    pub revealed_per_week: u32,
    pub range_percent: u32,
    pub narrowed_range_percent: u32,
}

/// A fighter out of the club the player may observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    FreeAgent(usize),
    Rival(RivalId),
}

/// One of the player's market moves for the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Observe a fighter out of the club this week.
    Observe(Target),
    /// Sign the free agent at this index, for nothing.
    Sign(usize),
    /// Let the club's fighter at this index go, for nothing: a free agent.
    Release(usize),
    /// Put the club's fighter at this index on the transfer list, or take it off: the
    /// rival clubs make offers for it while it fights on for the club.
    List(usize),
    Unlist(usize),
    /// Accept a rival club's offer (`SaleOffer`) for the club's fighter.
    AcceptOffer {
        fighter: usize,
        rival: usize,
    },
    /// Offer this price for a rival club's fighter; the club answers at the end of the
    /// week, then the fighter.
    Offer(RivalId, u32),
}

/// A rival club's offer for one of the player's listed fighters, open for a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleOffer {
    /// The club's fighter at this index.
    pub fighter: usize,
    /// The rival club at this index.
    pub rival: usize,
    pub price: u32,
}

/// What happened on the market for the club in a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketNews {
    /// The free agent became the club's fighter at this index.
    Signed(usize),
    Released(usize),
    Sold(SaleOffer),
    /// The offer was accepted: the rival fighter became the club's at index `fighter`.
    Bought {
        from: RivalId,
        fighter: usize,
        price: u32,
    },
    /// The club asks this price instead, for the next week.
    Countered(RivalId, u32),
    /// The club refused the offer, too low.
    ClubRefused(RivalId),
    /// The club accepted, but the fighter refused to join.
    FighterRefused(RivalId),
    /// Accepted, but the club had no place or not enough money, or the fighter was gone.
    Failed(RivalId),
}

/// What `fighter` is worth on the market: its stats, fame and record.
#[must_use]
pub fn value(fighter: &CareerFighter, rules: &MarketRules) -> u32 {
    let points: u32 = fighter.stats.to_array().iter().sum();
    let stats = i64::from(points) * i64::from(rules.price_per_point);
    let record = fighter.record;
    let worth = stats
        + i64::from(fighter.fame.value()) * i64::from(rules.price_per_fame)
        + i64::from(record.wins) * i64::from(rules.price_per_win)
        + i64::from(record.draws) * i64::from(rules.price_per_draw)
        - i64::from(record.losses) * i64::from(rules.price_per_loss);
    u32::try_from(worth.max(stats / 10)).unwrap_or(u32::MAX)
}

/// What the club sees of each trained stat of a fighter out of it, in `Stats::to_array`
/// order: `None` while hidden, else a range holding it. `shown_stats` are seen at first,
/// `revealed_per_week` more for each week observed, then the ranges narrow; the
/// recruiter's `scouting` reveals more each week and narrows every range. The order
/// stats are revealed in, and the place of a stat in its range (so that the middle does
/// not give it away), come from the hidden potential.
// ponytail: order and place derived from the potential rather than drawn and saved; draw
// them once players learn to read them.
#[must_use]
pub fn stat_ranges(
    fighter: &CareerFighter,
    rules: &MarketRules,
    scouting: &Scouting,
) -> [Option<(u32, u32)>; STAT_COUNT] {
    let potential = fighter.potential.to_array();
    let mix = |i: usize| (potential[i] * 37 + 11 * u32::try_from(i).unwrap_or(0)) % 101;
    let mut order: Vec<usize> = (0..STAT_COUNT).collect();
    order.sort_by_key(|&i| mix(i));
    let weeks = fighter.observed_weeks;
    let per_week = rules
        .revealed_per_week
        .saturating_add(scouting.revealed_per_week);
    let shown = rules
        .shown_stats
        .saturating_add(per_week.saturating_mul(weeks));
    let to_see_all = u32::try_from(STAT_COUNT)
        .unwrap_or(u32::MAX)
        .saturating_sub(rules.shown_stats)
        .div_ceil(per_week.max(1));
    let width = if weeks > to_see_all {
        rules.narrowed_range_percent
    } else {
        rules.range_percent
    };
    let width = percent_of(width, scouting.range_width_percent);
    let stats = fighter.stats.to_array();
    let mut ranges = [None; STAT_COUNT];
    for &i in order
        .iter()
        .take(usize::try_from(shown).unwrap_or(STAT_COUNT))
    {
        let wide = percent_of(stats[i], width);
        let low = stats[i] - percent_of(wide, mix(i));
        ranges[i] = Some((low, low + wide));
    }
    ranges
}

/// The recruiter's estimate of the total of `fighter`'s hidden potential, a range of
/// `scouting.potential_range_percent` of it holding it, once observed for a week;
/// `None` before, or without a recruiter able to tell.
#[must_use]
pub fn potential_range(fighter: &CareerFighter, scouting: &Scouting) -> Option<(u32, u32)> {
    if fighter.observed_weeks == 0 {
        return None;
    }
    scouted_potential(fighter, scouting)
}

/// What a rival club's recruiter makes of `fighter`'s potential: the middle of the range
/// `potential_range` gives, as a club on the market watches every free agent; `None`
/// without a recruiter able to tell.
#[must_use]
pub fn estimated_potential(fighter: &CareerFighter, scouting: &Scouting) -> Option<u32> {
    scouted_potential(fighter, scouting).map(|(low, high)| low + (high - low) / 2)
}

/// `potential_range` once observed.
fn scouted_potential(fighter: &CareerFighter, scouting: &Scouting) -> Option<(u32, u32)> {
    if scouting.potential_range_percent == 0 {
        return None;
    }
    let total: u32 = fighter.potential.to_array().iter().sum();
    let wide = percent_of(total, scouting.potential_range_percent);
    // Its place in the range, from the stats, so that the middle does not give it away.
    let stats: u32 = fighter.stats.to_array().iter().sum();
    let low = total - percent_of(wide, (stats * 37) % 101);
    Some((low, low + wide))
}

/// Whether the recruiter's `scouting` sees the personality of `fighter`, out of the
/// club: once observed for a week, by a recruiter able to tell.
#[must_use]
pub fn personality_seen(fighter: &CareerFighter, scouting: &Scouting) -> bool {
    scouting.personality && fighter.observed_weeks > 0
}

/// Whether `fighter` refuses to join the player's club, whose best fame is
/// `club_fame`: `refusal_percent`, plus half its fame above the club's (less below),
/// plus a fifth of its caution above the middle (less below), between 0 and 90 %.
pub fn refuses_to_join<R: Rng + ?Sized>(
    rng: &mut R,
    fighter: &CareerFighter,
    club_fame: Fame,
    rules: &MarketRules,
) -> bool {
    let fame = i64::from(fighter.fame.value()) - i64::from(club_fame.value());
    let caution = i64::from(fighter.personality.caution.value()) - 50;
    let percent = (i64::from(rules.refusal_percent) + fame / 2 + caution / 5).clamp(0, 90);
    i64::from(rng.random_range(0..100_u32)) < percent
}

/// A new free agent, as strong as the residents of a colosseum drawn at random.
pub fn free_agent<R: Rng + ?Sized>(rng: &mut R, data: &DesignData) -> CareerFighter {
    let fighter = CareerFighter::random(rng, data);
    let colosseums = &data.world.colosseums;
    let prestige = if colosseums.is_empty() {
        1
    } else {
        colosseums[rng.random_range(0..colosseums.len())].prestige
    };
    with_prestige(fighter, prestige, data)
}

/// A new fighter of `rookie_age` years, as strong as anyone at the start.
pub fn rookie<R: Rng + ?Sized>(rng: &mut R, data: &DesignData) -> CareerFighter {
    let mut fighter = CareerFighter::random(rng, data);
    fighter.age = data.economy.market.rookie_age;
    fighter
}

/// `fighter` with its stats and potential grown for a colosseum of `prestige`.
#[must_use]
pub fn with_prestige(
    mut fighter: CareerFighter,
    prestige: u32,
    data: &DesignData,
) -> CareerFighter {
    let percent = 100 + data.world.prestige_stats_percent * prestige.saturating_sub(1);
    fighter.stats = fighter.stats.map(|v| percent_of(v, percent).max(1));
    fighter.potential = fighter.potential.map(|v| percent_of(v, percent).max(1));
    fighter
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fighter::career::Record;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    #[test]
    fn value_follows_stats_fame_and_record() {
        let data = data();
        let rules = &data.economy.market;
        let mut fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        let points: u32 = fighter.stats.to_array().iter().sum();
        let stats = points * rules.price_per_point;
        assert_eq!(value(&fighter, rules), stats);
        fighter.fame = fighter.fame.changed_by(10);
        fighter.record = Record {
            wins: 3,
            losses: 1,
            draws: 2,
        };
        assert_eq!(
            value(&fighter, rules),
            stats + 10 * rules.price_per_fame + 3 * rules.price_per_win + 2 * rules.price_per_draw
                - rules.price_per_loss
        );
        // A long losing run costs no more than nine tenths of the stats' part.
        fighter.record.losses = 1000;
        assert_eq!(value(&fighter, rules), stats / 10);
    }

    #[test]
    fn stats_are_revealed_week_after_week_then_narrowed() {
        let data = data();
        let rules = &data.economy.market;
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        for _ in 0..20 {
            let mut fighter = CareerFighter::random(&mut rng, &data);
            let stats = fighter.stats.to_array();
            let mut seen = Vec::new();
            for weeks in 0..5 {
                fighter.observed_weeks = weeks;
                let ranges = stat_ranges(&fighter, rules, &Scouting::default());
                seen.push(ranges.iter().flatten().count());
                for (&stat, range) in stats.iter().zip(ranges) {
                    let Some((low, high)) = range else { continue };
                    assert!(low <= stat && stat <= high, "{stat} in {low}-{high}");
                    let width = if weeks >= 4 {
                        rules.narrowed_range_percent
                    } else {
                        rules.range_percent
                    };
                    assert_eq!(high - low, percent_of(stat, width));
                }
            }
            // Two at first, two more a week, then all of them, narrower from week 4.
            assert_eq!(seen, [2, 4, 6, 7, 7]);
            // A stat once seen stays seen.
            fighter.observed_weeks = 0;
            let first = stat_ranges(&fighter, rules, &Scouting::default());
            fighter.observed_weeks = 1;
            let second = stat_ranges(&fighter, rules, &Scouting::default());
            assert!(
                first
                    .iter()
                    .zip(second)
                    .all(|(a, b)| a.is_none() || b.is_some())
            );
        }
    }

    #[test]
    fn a_recruiter_reveals_more_narrower_and_the_potential() {
        let data = data();
        let rules = &data.economy.market;
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        let mut fighter = CareerFighter::random(&mut rng, &data);
        let scouting = Scouting {
            revealed_per_week: 2,
            range_width_percent: 50,
            potential_range_percent: 20,
            personality: true,
        };
        let alone = Scouting::default();
        fighter.observed_weeks = 1;
        let seen = |s: &Scouting| stat_ranges(&fighter, rules, s).iter().flatten().count();
        assert_eq!((seen(&alone), seen(&scouting)), (4, 6));
        let stats = fighter.stats.to_array();
        for (&stat, range) in stats.iter().zip(stat_ranges(&fighter, rules, &scouting)) {
            let Some((low, high)) = range else { continue };
            assert!(low <= stat && stat <= high);
            let usual = percent_of(stat, rules.range_percent);
            assert_eq!(high - low, percent_of(usual, 50));
        }
        let total: u32 = fighter.potential.to_array().iter().sum();
        let (low, high) = potential_range(&fighter, &scouting).unwrap();
        assert!(low <= total && total <= high);
        assert_eq!(high - low, percent_of(total, 20));
        assert!(potential_range(&fighter, &alone).is_none());
        assert!(personality_seen(&fighter, &scouting) && !personality_seen(&fighter, &alone));
        fighter.observed_weeks = 0;
        assert!(potential_range(&fighter, &scouting).is_none());
        assert!(!personality_seen(&fighter, &scouting));
        // A rival club's recruiter needs no observation, and takes the middle.
        assert_eq!(
            estimated_potential(&fighter, &scouting),
            Some(low + (high - low) / 2)
        );
        assert_eq!(estimated_potential(&fighter, &alone), None);
    }

    #[test]
    fn famous_and_cautious_fighters_refuse_more_often() {
        let data = data();
        let rules = &data.economy.market;
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let mut fighter = CareerFighter::random(&mut rng, &data);
        fighter.personality.caution = 50.into();
        let refusals = |fighter: &CareerFighter, club: Fame, rng: &mut ChaCha8Rng| {
            (0..1000)
                .filter(|_| refuses_to_join(rng, fighter, club, rules))
                .count()
        };
        let base = refusals(&fighter, fighter.fame, &mut rng);
        let expected = usize::try_from(rules.refusal_percent * 10).unwrap();
        assert!(base.abs_diff(expected) < 50, "{base}");
        fighter.fame = Fame::default().changed_by(60);
        assert!(refusals(&fighter, Fame::default(), &mut rng) > base + 200);
        // A famous club tempts it.
        fighter.personality.caution = 0.into();
        assert_eq!(
            refusals(&fighter, Fame::default().changed_by(100), &mut rng),
            0
        );
    }

    #[test]
    fn rookies_are_all_the_same_age() {
        let data = data();
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        assert!((0..10).all(|_| rookie(&mut rng, &data).age == data.economy.market.rookie_age));
    }
}
