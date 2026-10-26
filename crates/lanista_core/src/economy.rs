//! Money: what comes in (prizes, receipts, sales: only the fighters bring it) and goes
//! out (upkeep, the fighters' wages, care, training, entry fees, staff salaries), the
//! contracts of the club's fighters, and a club's wealth: poor, modest or rich, which
//! sets its starting money and changes with its money at each off-season.

use serde::{Deserialize, Serialize};

use crate::fighter::career::CareerFighter;
use crate::fighter::fame::Fame;
use crate::fighter::melee::place_share;
use crate::fighter::percent_of;
use crate::market::{MarketRules, value};
use crate::staff::StaffRules;

/// Prices and purses, from `data/economy.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EconomyRules {
    /// A club's starting money by wealth, and when its wealth changes.
    pub wealth: WealthRules,
    /// Most fighters in activity a club may have.
    pub max_fighters: u32,
    /// Paid each week for each active fighter, even into debt.
    pub upkeep: u32,
    /// Paid for each training session without a trainer.
    pub training_cost: u32,
    pub contracts: ContractRules,
    pub staff: StaffRules,
    pub market: MarketRules,
}

/// How rich a club is: a richer club starts with more money, and a rival club plays by
/// it (`rivals`). It changes at the start of each off-season (`Wealth::next`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Wealth {
    /// Bets on scouting rookies and cheap free agents, hoping to find a gem to sell.
    Poor,
    Modest,
    /// Buys at will, and lets go any fighter who no longer performs.
    Rich,
}

impl Wealth {
    pub const ALL: [Self; 3] = [Self::Poor, Self::Modest, Self::Rich];

    /// The wealth of a club with `money` at the start of an off-season: one step up from
    /// the next level's `from`, one step down below half of its own, else the same.
    #[must_use]
    pub fn next(self, money: i64, rules: &WealthRules) -> Self {
        let up = match self {
            Self::Poor => Some(Self::Modest),
            Self::Modest => Some(Self::Rich),
            Self::Rich => None,
        };
        let down = match self {
            Self::Poor => None,
            Self::Modest => Some(Self::Poor),
            Self::Rich => Some(Self::Modest),
        };
        if let Some(up) = up.filter(|&up| money >= i64::from(rules.get(up).from)) {
            up
        } else if let Some(down) = down.filter(|_| money * 2 < i64::from(rules.get(self).from)) {
            down
        } else {
            self
        }
    }
}

/// One level of wealth.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WealthLevel {
    /// A club's money at the start of a game.
    pub starting_money: u32,
    /// A club with this much money at the start of an off-season rises to this level;
    /// one at this level with less than half falls to the one below.
    pub from: u32,
}

/// From `data/economy.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WealthRules {
    pub poor: WealthLevel,
    pub modest: WealthLevel,
    pub rich: WealthLevel,
}

impl WealthRules {
    #[must_use]
    pub fn get(&self, wealth: Wealth) -> &WealthLevel {
        match wealth {
            Wealth::Poor => &self.poor,
            Wealth::Modest => &self.modest,
            Wealth::Rich => &self.rich,
        }
    }
}

/// The contracts of the player's club's fighters, from `data/economy.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractRules {
    /// The weekly wage a fighter asks, in percent of its value (`market::value`).
    pub wage_percent: u32,
    /// How much its ego moves it: by this percentage at an ego of 100 or 0, linear
    /// in between.
    pub ego_percent: u32,
    /// Seasons a contract signed or renewed runs, the current one apart.
    pub seasons: u64,
    /// Least morale at which a fighter agrees to renew.
    pub renewal_morale: u32,
}

impl ContractRules {
    /// The weekly wage `fighter` asks for its value and ego, at least 1.
    #[must_use]
    pub fn asked_wage(&self, fighter: &CareerFighter, market: &MarketRules) -> u32 {
        let ego = i64::from(fighter.personality.ego.value()) - 50;
        let percent = 100 + ego * i64::from(self.ego_percent) / 50;
        let wage = percent_of(value(fighter, market), self.wage_percent);
        percent_of(wage, u32::try_from(percent.max(0)).unwrap_or(0)).max(1)
    }

    /// The contract `fighter` signs in `season`: its asked wage until the start of the
    /// off-season `seasons` later.
    #[must_use]
    pub fn signed(&self, fighter: &CareerFighter, season: u64, market: &MarketRules) -> Contract {
        Contract {
            wage: self.asked_wage(fighter, market),
            ends: season.saturating_add(self.seasons),
        }
    }
}

/// What a fighter of the player's club is paid each week, and until when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub wage: u32,
    /// The season at whose off-season start the fighter leaves, a free agent, unless
    /// renewed during it.
    pub ends: u64,
}

/// A colosseum's fee and purses.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Purses {
    /// Paid to register a fighter; 0 for the free colosseum, the safety net.
    pub entry_fee: u32,
    /// Paid to the winner's club.
    pub win_prize: u32,
    /// Receipts paid to each fighter's club, win or lose.
    pub receipts_base: u32,
    /// Added to the receipts for each fame point of the two fighters.
    pub receipts_per_fame: u32,
    /// Paid for each title defense the club's champion survives.
    pub champion_bonus: u32,
    /// A melee pays the club of its last one standing this much, and of each other
    /// fighter its place's share, down to nothing for the first one out.
    pub melee_prize: u32,
}

impl Purses {
    /// What place `place` of a melee of `count`, the first `paid` places paid, pays: the
    /// same for every fighter whatever its fame or club (see [`place_share`]).
    #[must_use]
    pub fn melee_receipts(&self, place: u32, (count, paid): (usize, u32)) -> u32 {
        place_share(self.melee_prize, place, (count, paid))
    }

    /// Gate receipts of a fight: famous fighters draw a crowd.
    #[must_use]
    pub fn receipts(&self, fighter: Fame, opponent: Fame) -> u32 {
        let fame = fighter.value().saturating_add(opponent.value());
        self.receipts_base
            .saturating_add(self.receipts_per_fame.saturating_mul(fame))
    }
}

/// Why money moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Flow {
    Upkeep,
    Care,
    Training,
    EntryFee,
    WinPrize,
    Receipts,
    ChampionBonus,
    /// A rival's fighter bought.
    Purchase,
    /// The club's fighter sold to a rival.
    Sale,
    /// A staff member's weekly salary.
    Salary,
    /// A fighter's weekly wage, by its contract.
    Wage,
}

/// Money in (positive) or out (negative) for one of the club's fighters, or for the club.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// Index of the fighter in the club; `None` for the club's own: its staff.
    pub fighter: Option<usize>,
    pub flow: Flow,
    pub amount: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipts_grow_with_both_fighters_fame() {
        let rules = Purses {
            entry_fee: 0,
            win_prize: 50,
            receipts_base: 20,
            receipts_per_fame: 1,
            champion_bonus: 0,
            melee_prize: 150,
        };
        assert_eq!(rules.receipts(Fame::default(), Fame::default()), 20);
        let famous = Fame::default().changed_by(30);
        assert_eq!(rules.receipts(famous, Fame::default().changed_by(10)), 60);
        assert_eq!(rules.melee_receipts(1, (20, 12)), 150);
        assert_eq!(rules.melee_receipts(12, (20, 12)), 150 / 12);
        assert_eq!(rules.melee_receipts(13, (20, 12)), 0);
    }

    #[test]
    fn wealth_rises_from_the_next_level_and_falls_below_half_its_own() {
        let level = |from| WealthLevel {
            starting_money: 0,
            from,
        };
        let rules = WealthRules {
            poor: level(0),
            modest: level(1000),
            rich: level(5000),
        };
        let next = |wealth: Wealth, money| wealth.next(money, &rules);
        assert_eq!(next(Wealth::Poor, 999), Wealth::Poor);
        assert_eq!(next(Wealth::Poor, 1000), Wealth::Modest);
        // One step a season, even with the money of the rich.
        assert_eq!(next(Wealth::Poor, 9000), Wealth::Modest);
        assert_eq!(next(Wealth::Poor, -100), Wealth::Poor);
        assert_eq!(next(Wealth::Modest, 500), Wealth::Modest);
        assert_eq!(next(Wealth::Modest, 499), Wealth::Poor);
        assert_eq!(next(Wealth::Modest, 5000), Wealth::Rich);
        assert_eq!(next(Wealth::Rich, 2500), Wealth::Rich);
        assert_eq!(next(Wealth::Rich, 2499), Wealth::Modest);
    }
}
