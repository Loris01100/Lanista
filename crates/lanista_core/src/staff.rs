//! The club's staff, paid every week. For now only the trainer: from level 1 to 5,
//! a better one raises the training gains and spares form, for a higher salary.

use serde::{Deserialize, Serialize};

/// The staff levels on offer, from `data/economy.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffRules {
    /// Trainer levels, from level 1.
    pub trainers: Vec<TrainerRules>,
}

impl StaffRules {
    /// The rules of trainer level `level`, from 1.
    #[must_use]
    pub fn trainer(&self, level: u32) -> Option<&TrainerRules> {
        let i = usize::try_from(level.checked_sub(1)?).ok()?;
        self.trainers.get(i)
    }
}

/// One trainer level.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainerRules {
    /// Paid every week, even into debt.
    pub salary: u32,
    /// Paid for each training session, instead of `EconomyRules::training_cost`.
    pub training_cost: u32,
    pub coaching: Coaching,
}

/// What a trainer brings to each training session; nothing without one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coaching {
    /// Added to each stat's chance to improve, in percent of that chance.
    pub progress_percent: u32,
    /// Form a session costs less, never below nothing.
    pub form_spared: u32,
}

/// The club's staff.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Staff {
    /// The trainer's level, from 1; `None` without a trainer.
    pub trainer: Option<u32>,
}

/// A change of staff, made before the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffMove {
    /// Hires a trainer of this level, in place of the current one.
    HireTrainer(u32),
    DismissTrainer,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_count_from_1() {
        let level = |salary| TrainerRules {
            salary,
            training_cost: 5,
            coaching: Coaching::default(),
        };
        let rules = StaffRules {
            trainers: vec![level(10), level(20)],
        };
        assert_eq!(rules.trainer(0), None);
        assert_eq!(rules.trainer(1).map(|t| t.salary), Some(10));
        assert_eq!(rules.trainer(2).map(|t| t.salary), Some(20));
        assert_eq!(rules.trainer(3), None);
    }
}
