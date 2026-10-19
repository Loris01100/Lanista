//! Money: what comes in (prizes, receipts, sales: only the fighters bring it) and goes
//! out (upkeep, care, training, entry fees, staff salaries), and a club's wealth: poor,
//! modest or rich, which sets its starting money and changes with its money at each
//! off-season.

use serde::{Deserialize, Serialize};

use crate::fighter::fame::Fame;
use crate::market::MarketRules;
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
    /// Paid to the club of a melee's last one standing.
    pub melee_prize: u32,
    /// In a melee, added to each fighter's receipts for each other fighter in it.
    pub receipts_per_fighter: u32,
}

impl Purses {
    /// Gate receipts a melee of `fighters` brings the club of a fighter of `fame`:
    /// the bigger the melee and the more famous the fighter, the bigger the crowd.
    #[must_use]
    pub fn melee_receipts(&self, fame: Fame, fighters: usize) -> u32 {
        let others = u32::try_from(fighters.saturating_sub(1)).unwrap_or(u32::MAX);
        self.receipts_base
            .saturating_add(self.receipts_per_fame.saturating_mul(fame.value()))
            .saturating_add(self.receipts_per_fighter.saturating_mul(others))
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// Money in (positive) or out (negative) for one of the club's fighters, or for the club.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            receipts_per_fighter: 3,
        };
        assert_eq!(rules.receipts(Fame::default(), Fame::default()), 20);
        let famous = Fame::default().changed_by(30);
        assert_eq!(rules.receipts(famous, Fame::default().changed_by(10)), 60);
        // A melee of 20 draws a crowd: 19 others at 3 each.
        assert_eq!(rules.melee_receipts(famous, 20), 20 + 30 + 57);
        assert_eq!(rules.melee_receipts(Fame::default(), 1), 20);
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
