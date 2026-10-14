//! Personality: four native traits, fixed for life, that shape a fighter in combat.
//! Determination sets the surrender threshold, which the player never controls.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::fighter::percent_of;

/// One personality trait, always within 0 to [`Personality::MAX`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(from = "u32", into = "u32")]
pub struct TraitValue(u32);

impl TraitValue {
    /// Trait `value`, capped at [`Personality::MAX`].
    #[must_use]
    pub fn new(value: u32) -> Self {
        Self(value.min(Personality::MAX))
    }

    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Personality {
    pub aggression: TraitValue,
    pub caution: TraitValue,
    pub showmanship: TraitValue,
    /// High: fights on at critical HP and may take a fatal blow. Low: gives up early, rarely dies.
    pub determination: TraitValue,
}

impl Personality {
    pub const MAX: u32 = 100;
    /// A trait at this value has no effect on the AI.
    pub const NEUTRAL: u32 = 50;

    /// Every trait at [`Personality::NEUTRAL`].
    #[must_use]
    pub fn neutral() -> Self {
        let neutral = TraitValue(Self::NEUTRAL);
        Self {
            aggression: neutral,
            caution: neutral,
            showmanship: neutral,
            determination: neutral,
        }
    }

    /// A personality with every trait drawn evenly from 0 to [`Personality::MAX`].
    pub fn random<R: Rng + ?Sized>(rng: &mut R) -> Self {
        let mut roll = || TraitValue(rng.random_range(0..=Self::MAX));
        Self {
            aggression: roll(),
            caution: roll(),
            showmanship: roll(),
            determination: roll(),
        }
    }

    /// Hit points at or below which this fighter gives up, out of `max_hp`.
    /// The percentage goes linearly from `rules.surrender_percent_least_determined`
    /// at determination 0 to `rules.surrender_percent_most_determined` at the maximum.
    #[must_use]
    pub fn surrender_hp(&self, max_hp: u32, rules: &PersonalityRules) -> u32 {
        let low = i64::from(rules.surrender_percent_least_determined);
        let high = i64::from(rules.surrender_percent_most_determined);
        let determination = i64::from(self.determination.value());
        let percent = low + (high - low) * determination / i64::from(Self::MAX);
        percent_of(max_hp, u32::try_from(percent.clamp(0, 100)).unwrap_or(0))
    }
}

/// Surrender thresholds, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalityRules {
    /// Percentage of max HP at which a fighter with determination 0 gives up.
    pub surrender_percent_least_determined: u32,
    /// Percentage of max HP at which a fighter with full determination gives up;
    /// 0 never gives up.
    pub surrender_percent_most_determined: u32,
}

// Saves go through `new`: an edited value stays within its bounds.
impl From<u32> for TraitValue {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl From<TraitValue> for u32 {
    fn from(value: TraitValue) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    const RULES: PersonalityRules = PersonalityRules {
        surrender_percent_least_determined: 50,
        surrender_percent_most_determined: 0,
    };

    fn determined(determination: u32) -> Personality {
        Personality {
            determination: TraitValue::new(determination),
            ..Personality::neutral()
        }
    }

    #[test]
    fn determination_lowers_the_surrender_threshold() {
        assert_eq!(determined(0).surrender_hp(200, &RULES), 100);
        assert_eq!(determined(50).surrender_hp(200, &RULES), 50);
        assert_eq!(determined(100).surrender_hp(200, &RULES), 0);
    }

    #[test]
    fn traits_are_capped() {
        assert_eq!(TraitValue::new(250).value(), Personality::MAX);
    }

    #[test]
    fn random_personalities_are_reproducible() {
        let draw = |seed| Personality::random(&mut ChaCha8Rng::seed_from_u64(seed));
        assert_eq!(draw(5), draw(5));
        assert_ne!(draw(5), draw(6));
    }

    proptest! {
        #[test]
        fn more_determined_never_gives_up_sooner(a in 0..=100u32, b in 0..=100u32, hp in 1..1000u32) {
            let (less, more) = (determined(a.min(b)), determined(a.max(b)));
            prop_assert!(more.surrender_hp(hp, &RULES) <= less.surrender_hp(hp, &RULES));
            prop_assert!(less.surrender_hp(hp, &RULES) <= hp);
        }

        #[test]
        fn random_traits_stay_in_range(seed: u64) {
            let p = Personality::random(&mut ChaCha8Rng::seed_from_u64(seed));
            for t in [p.aggression, p.caution, p.showmanship, p.determination] {
                prop_assert!(t.value() <= Personality::MAX);
            }
        }
    }
}
