//! Care, always decided by the player: paying now to shorten an injury or illness,
//! or waiting for it to heal on its own. The price depends only on the severity,
//! so care late in a recovery buys little.

use serde::Deserialize;

use crate::fighter::illness::{ByLevel, Illness};
use crate::fighter::injury::{BySeverity, Injury, Severity};

/// Care prices and effect, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CareRules {
    /// Price of caring for an injury, by severity; permanent injuries can't be cared for.
    pub injury_cost: BySeverity<u32>,
    /// Price of caring for an illness, by level.
    pub illness_cost: ByLevel<u32>,
    /// Percentage of the weeks left that remain after care, rounded up, at least 1.
    pub weeks_kept_percent: u32,
}

impl CareRules {
    /// Weeks left after caring for something with `weeks` left.
    #[must_use]
    pub fn weeks_after_care(&self, weeks: u32) -> u32 {
        let kept = u64::from(weeks) * u64::from(self.weeks_kept_percent.min(100));
        u32::try_from(kept.div_ceil(100)).unwrap_or(weeks).max(1)
    }
}

impl Injury {
    /// Price of caring for this injury, or `None` if care would change nothing
    /// (a permanent injury, or one too close to healed).
    #[must_use]
    pub fn care_cost(&self, rules: &CareRules) -> Option<u32> {
        let cost = match self.severity {
            Severity::Light => rules.injury_cost.light,
            Severity::Medium => rules.injury_cost.medium,
            Severity::Severe => rules.injury_cost.severe,
            Severity::Permanent | Severity::Lost => return None,
        };
        (rules.weeks_after_care(self.weeks_left) < self.weeks_left).then_some(cost)
    }

    /// Cares for this injury and returns the price to pay, or `None` and changes nothing
    /// if care would be useless (see [`Injury::care_cost`]).
    pub fn treat(&mut self, rules: &CareRules) -> Option<u32> {
        let cost = self.care_cost(rules)?;
        self.weeks_left = rules.weeks_after_care(self.weeks_left);
        Some(cost)
    }
}

impl Illness {
    /// Price of caring for this illness, or `None` if it is too close to healed for care to help.
    #[must_use]
    pub fn care_cost(&self, rules: &CareRules) -> Option<u32> {
        (rules.weeks_after_care(self.weeks_left) < self.weeks_left)
            .then_some(rules.illness_cost.get(self.level))
    }

    /// Cares for this illness and returns the price to pay, or `None` and changes nothing
    /// if care would be useless (see [`Illness::care_cost`]).
    pub fn treat(&mut self, rules: &CareRules) -> Option<u32> {
        let cost = self.care_cost(rules)?;
        self.weeks_left = rules.weeks_after_care(self.weeks_left);
        Some(cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fighter::illness::IllnessLevel;
    use crate::fighter::injury::{Location, Side};
    use proptest::prelude::*;

    const RULES: CareRules = CareRules {
        injury_cost: BySeverity {
            light: 20,
            medium: 60,
            severe: 150,
        },
        illness_cost: ByLevel {
            mild: 15,
            moderate: 50,
            severe: 120,
        },
        weeks_kept_percent: 50,
    };

    fn injury(severity: Severity, weeks_left: u32) -> Injury {
        Injury {
            location: Location::Leg(Side::Left),
            severity,
            weeks_left,
        }
    }

    #[test]
    fn care_halves_the_weeks_left_rounded_up() {
        let mut severe = injury(Severity::Severe, 7);
        assert_eq!(severe.treat(&RULES), Some(150));
        assert_eq!(severe.weeks_left, 4);

        let mut ill = Illness {
            level: IllnessLevel::Moderate,
            weeks_left: 4,
        };
        assert_eq!(ill.treat(&RULES), Some(50));
        assert_eq!(ill.weeks_left, 2);
    }

    #[test]
    fn care_late_costs_the_same_and_saves_less() {
        assert_eq!(injury(Severity::Medium, 5).care_cost(&RULES), Some(60));
        assert_eq!(injury(Severity::Medium, 2).care_cost(&RULES), Some(60));
    }

    #[test]
    fn useless_care_is_not_offered_and_changes_nothing() {
        let mut nearly_healed = injury(Severity::Light, 1);
        assert_eq!(nearly_healed.treat(&RULES), None);
        assert_eq!(nearly_healed, injury(Severity::Light, 1));

        let mut lost_leg = injury(Severity::Permanent, 0);
        assert_eq!(lost_leg.treat(&RULES), None);
        assert_eq!(lost_leg, injury(Severity::Permanent, 0));
    }

    proptest! {
        #[test]
        fn care_shortens_but_never_heals_outright(weeks in 0..100u32, percent in 0..=100u32) {
            let rules = CareRules { weeks_kept_percent: percent, ..RULES };
            let after = rules.weeks_after_care(weeks);
            prop_assert!(after >= 1);
            prop_assert!(after <= weeks.max(1));
        }
    }
}
