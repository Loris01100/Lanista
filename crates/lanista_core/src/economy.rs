//! Money: what comes in (prizes, receipts) and goes out (upkeep, care, training,
//! entry fees). Money only comes from fights; there is no sponsor.

use serde::Deserialize;

use crate::fighter::fame::Fame;

/// Prices and purses, from `data/economy.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EconomyRules {
    /// The club's money at the start of a game.
    pub starting_money: u32,
    /// Most fighters in activity a club may have.
    pub max_fighters: u32,
    /// Paid each week for each active fighter, even into debt.
    pub upkeep: u32,
    /// Paid for each training session.
    pub training_cost: u32,
    /// The only colosseum until phase 5.
    pub colosseum: ColosseumRules,
}

/// A colosseum's fee and purses.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColosseumRules {
    /// Paid to register a fighter; 0 for the free colosseum, the safety net.
    pub entry_fee: u32,
    /// Paid to the winner's club.
    pub win_prize: u32,
    /// Receipts paid to each fighter's club, win or lose.
    pub receipts_base: u32,
    /// Added to the receipts for each fame point of the two fighters.
    pub receipts_per_fame: u32,
}

impl ColosseumRules {
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
}

/// Money in (positive) or out (negative) for one of the club's fighters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transaction {
    /// Index of the fighter in the club.
    pub fighter: usize,
    pub flow: Flow,
    pub amount: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipts_grow_with_both_fighters_fame() {
        let rules = ColosseumRules {
            entry_fee: 0,
            win_prize: 50,
            receipts_base: 20,
            receipts_per_fame: 1,
        };
        assert_eq!(rules.receipts(Fame::default(), Fame::default()), 20);
        let famous = Fame::default().changed_by(30);
        assert_eq!(rules.receipts(famous, Fame::default().changed_by(10)), 60);
    }
}
