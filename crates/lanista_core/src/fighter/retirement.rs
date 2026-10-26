//! Retirement, decided only by the fighter, from their personality, age and condition,
//! unless it lost too many limbs to fight on. The player can't force it; releasing a
//! fighter sends them to the market instead.

use rand::{Rng, RngExt};
use serde::Deserialize;

use crate::fighter::injury::{Injury, Location};
use crate::fighter::personality::Personality;

/// Chance, in percent, that a fighter retires at this year's check (once a year, off-season).
/// None before `rules.from_age`; then `rules.per_year` per year since, plus
/// `rules.per_permanent_injury` per permanent injury, scaled by ego:
/// ± `rules.ego_percent` at ego 0 and 100, the crowd holding the vain in the arena.
#[must_use]
pub fn retirement_chance(
    age: u32,
    personality: &Personality,
    permanent_injuries: u32,
    rules: &RetirementRules,
) -> u32 {
    if age < rules.from_age {
        return 0;
    }
    let years = i64::from(age - rules.from_age + 1);
    let base = years * i64::from(rules.per_year)
        + i64::from(permanent_injuries) * i64::from(rules.per_permanent_injury);
    let half = i64::from(Personality::MAX / 2);
    let offset = half - i64::from(personality.ego.value());
    let scaled = base * (100 + offset * i64::from(rules.ego_percent) / half) / 100;
    u32::try_from(scaled.clamp(0, 100)).unwrap_or(0)
}

/// Whether the fighter decides to retire at this year's check.
pub fn decides_to_retire<R: Rng + ?Sized>(
    rng: &mut R,
    age: u32,
    personality: &Personality,
    permanent_injuries: u32,
    rules: &RetirementRules,
) -> bool {
    rng.random_range(0..100) < retirement_chance(age, personality, permanent_injuries, rules)
}

/// Whether `injuries` leave the fighter more limbs lost than it fights on without:
/// more than `max_lost_arms` arms or `max_lost_legs` legs. It then retires at once.
#[must_use]
pub fn too_maimed(injuries: &[Injury], rules: &RetirementRules) -> bool {
    let lost = |limb: fn(Location) -> bool| {
        let count = injuries
            .iter()
            .filter(|i| i.is_lost() && limb(i.location))
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    };
    lost(|l| matches!(l, Location::Arm(_))) > rules.max_lost_arms
        || lost(|l| matches!(l, Location::Leg(_))) > rules.max_lost_legs
}

/// Retirement odds, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetirementRules {
    /// First age at which a fighter may retire.
    pub from_age: u32,
    /// Chance added per year from `from_age` (counting that year), in percent.
    pub per_year: u32,
    /// Chance added per permanent injury, in percent.
    pub per_permanent_injury: u32,
    /// How much ego moves the chance, in percent of it.
    pub ego_percent: u32,
    /// Most arms, then legs, a fighter fights on without; one more and it retires.
    pub max_lost_arms: u32,
    pub max_lost_legs: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fighter::personality::TraitValue;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    const RULES: RetirementRules = RetirementRules {
        from_age: 30,
        per_year: 5,
        per_permanent_injury: 15,
        ego_percent: 50,
        max_lost_arms: 1,
        max_lost_legs: 1,
    };

    #[test]
    fn an_arm_and_a_leg_lost_are_the_most_a_fighter_fights_on_without() {
        use crate::fighter::injury::{Side, severed_limb_wound};

        let lost = |l| severed_limb_wound(l);
        let arm = lost(Location::Arm(Side::Left));
        let leg = lost(Location::Leg(Side::Right));
        assert!(!too_maimed(&[], &RULES));
        assert!(!too_maimed(&[arm, leg], &RULES));
        assert!(too_maimed(&[arm, lost(Location::Arm(Side::Right))], &RULES));
        assert!(too_maimed(&[leg, lost(Location::Leg(Side::Left))], &RULES));
        // A crippled limb is not a lost one.
        let crippled = Injury {
            severity: crate::fighter::injury::Severity::Permanent,
            ..lost(Location::Arm(Side::Right))
        };
        assert!(!too_maimed(&[arm, crippled], &RULES));
    }

    fn determined(ego: u32) -> Personality {
        Personality {
            ego: TraitValue::new(ego),
            ..Personality::neutral()
        }
    }

    #[test]
    fn nobody_retires_before_30() {
        assert_eq!(retirement_chance(29, &determined(0), 3, &RULES), 0);
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        assert!((0..1000).all(|_| !decides_to_retire(&mut rng, 29, &determined(0), 3, &RULES)));
    }

    #[test]
    fn age_injuries_and_personality_set_the_chance() {
        let average = determined(50);
        assert_eq!(retirement_chance(30, &average, 0, &RULES), 5);
        assert_eq!(retirement_chance(35, &average, 0, &RULES), 30);
        assert_eq!(retirement_chance(35, &average, 1, &RULES), 45);
        assert_eq!(retirement_chance(35, &determined(100), 0, &RULES), 15);
        assert_eq!(retirement_chance(35, &determined(0), 0, &RULES), 45);
        assert_eq!(retirement_chance(60, &average, 0, &RULES), 100);
    }

    proptest! {
        #[test]
        fn older_never_less_likely_to_retire(a in 0..80u32, b in 0..80u32, determination in 0..=100u32, injuries in 0..4u32) {
            let p = determined(determination);
            let (young, old) = (a.min(b), a.max(b));
            prop_assert!(retirement_chance(young, &p, injuries, &RULES) <= retirement_chance(old, &p, injuries, &RULES));
        }
    }
}
