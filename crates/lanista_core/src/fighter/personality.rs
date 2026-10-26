//! Personality: a temperament, fixed for life, and two hidden axes drawn around it.
//! Boldness sets the surrender threshold, which the player never controls; ego, the
//! taste for the crowd. The temperament is what the player sees, and each brings its
//! own trade-off ([`TemperamentRules`]); the berserker and the proud also fight their way.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::fighter::{percent_of, pick};

/// One personality axis, always within 0 to [`Personality::MAX`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(from = "u32", into = "u32")]
pub struct TraitValue(u32);

impl TraitValue {
    /// Axis `value`, capped at [`Personality::MAX`].
    #[must_use]
    pub fn new(value: u32) -> Self {
        Self(value.min(Personality::MAX))
    }

    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Temperament {
    /// No excess: the standard duel's and the tests', never drawn for a new fighter
    /// with the shipped weights.
    Balanced,
    Reckless,
    Showman,
    Methodical,
    Survivor,
    /// Never afraid, and presses harder the more it is hurt.
    Berserker,
    /// Never gives up against a weaker opponent.
    Proud,
}

impl Temperament {
    pub const ALL: [Self; 7] = [
        Self::Balanced,
        Self::Reckless,
        Self::Showman,
        Self::Methodical,
        Self::Survivor,
        Self::Berserker,
        Self::Proud,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Personality {
    pub temperament: Temperament,
    /// High: presses, fights on at critical HP and may take a fatal blow. Low: wary,
    /// gives up early, rarely dies.
    pub boldness: TraitValue,
    /// High: plays to the crowd and clings to the arena. Low: discreet.
    pub ego: TraitValue,
}

impl Personality {
    pub const MAX: u32 = 100;
    /// An axis at this value has no effect on the AI.
    pub const NEUTRAL: u32 = 50;

    /// Balanced, both axes at [`Personality::NEUTRAL`].
    #[must_use]
    pub fn neutral() -> Self {
        let neutral = TraitValue(Self::NEUTRAL);
        Self {
            temperament: Temperament::Balanced,
            boldness: neutral,
            ego: neutral,
        }
    }

    /// A temperament drawn by its weight, each axis within `rules.spread` of its value.
    pub fn random<R: Rng + ?Sized>(rng: &mut R, rules: &PersonalityRules) -> Self {
        let choices = Temperament::ALL.map(|t| (rules.temperaments.get(t).weight, t));
        let temperament = pick(rng, &choices);
        let centre = rules.temperaments.get(temperament);
        let mut around = |value: u32| {
            let low = value.saturating_sub(rules.spread);
            let high = (value + rules.spread).min(Self::MAX);
            TraitValue(rng.random_range(low..=high.max(low)))
        };
        Self {
            temperament,
            boldness: around(centre.boldness),
            ego: around(centre.ego),
        }
    }

    /// Hit points at or below which this fighter gives up, out of `max_hp`.
    /// The percentage goes linearly from `rules.surrender_percent_most_timid`
    /// at boldness 0 to `rules.surrender_percent_boldest` at the maximum.
    #[must_use]
    pub fn surrender_hp(&self, max_hp: u32, rules: &PersonalityRules) -> u32 {
        let low = i64::from(rules.surrender_percent_most_timid);
        let high = i64::from(rules.surrender_percent_boldest);
        let boldness = i64::from(self.boldness.value());
        let percent = low + (high - low) * boldness / i64::from(Self::MAX);
        percent_of(max_hp, u32::try_from(percent.clamp(0, 100)).unwrap_or(0))
    }
}

/// Surrender thresholds and temperaments, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalityRules {
    /// Percentage of max HP at which a fighter with boldness 0 gives up.
    pub surrender_percent_most_timid: u32,
    /// Percentage of max HP at which a fighter with full boldness gives up;
    /// 0 never gives up.
    pub surrender_percent_boldest: u32,
    /// Largest gap between an axis and its temperament's value, either way.
    pub spread: u32,
    pub temperaments: Temperaments,
}

/// A temperament's place on the two axes and its trade-off, the percentages applied
/// to what a fighter of its own goes through.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemperamentRules {
    /// Chance of being drawn for a new fighter, against the others' weights.
    pub weight: u32,
    pub boldness: u32,
    pub ego: u32,
    /// Of every fame change, gained or lost.
    pub fame_percent: u32,
    /// Of every morale loss.
    pub morale_loss_percent: u32,
    /// Of the chance of an injury after a fight.
    pub injury_percent: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Temperaments {
    pub balanced: TemperamentRules,
    pub reckless: TemperamentRules,
    pub showman: TemperamentRules,
    pub methodical: TemperamentRules,
    pub survivor: TemperamentRules,
    pub berserker: TemperamentRules,
    pub proud: TemperamentRules,
}

impl Temperaments {
    #[must_use]
    pub fn get(&self, temperament: Temperament) -> &TemperamentRules {
        match temperament {
            Temperament::Balanced => &self.balanced,
            Temperament::Reckless => &self.reckless,
            Temperament::Showman => &self.showman,
            Temperament::Methodical => &self.methodical,
            Temperament::Survivor => &self.survivor,
            Temperament::Berserker => &self.berserker,
            Temperament::Proud => &self.proud,
        }
    }
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
    use crate::design_data::DesignData;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn rules() -> PersonalityRules {
        PersonalityRules {
            surrender_percent_most_timid: 50,
            surrender_percent_boldest: 0,
            ..DesignData::load().unwrap().fighter.personality
        }
    }

    fn bold(boldness: u32) -> Personality {
        Personality {
            boldness: TraitValue::new(boldness),
            ..Personality::neutral()
        }
    }

    #[test]
    fn boldness_lowers_the_surrender_threshold() {
        assert_eq!(bold(0).surrender_hp(200, &rules()), 100);
        assert_eq!(bold(50).surrender_hp(200, &rules()), 50);
        assert_eq!(bold(100).surrender_hp(200, &rules()), 0);
    }

    #[test]
    fn axes_are_capped() {
        assert_eq!(TraitValue::new(250).value(), Personality::MAX);
    }

    #[test]
    fn random_personalities_are_reproducible() {
        let draw = |seed| Personality::random(&mut ChaCha8Rng::seed_from_u64(seed), &rules());
        assert_eq!(draw(5), draw(5));
        assert_ne!(draw(5), draw(6));
    }

    #[test]
    fn every_drawn_temperament_but_the_balanced_one_shows_up() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let drawn: Vec<_> = (0..500)
            .map(|_| Personality::random(&mut rng, &rules()).temperament)
            .collect();
        for t in Temperament::ALL {
            assert_eq!(drawn.contains(&t), t != Temperament::Balanced, "{t:?}");
        }
    }

    proptest! {
        #[test]
        fn bolder_never_gives_up_sooner(a in 0..=100u32, b in 0..=100u32, hp in 1..1000u32) {
            let (less, more) = (bold(a.min(b)), bold(a.max(b)));
            prop_assert!(more.surrender_hp(hp, &rules()) <= less.surrender_hp(hp, &rules()));
            prop_assert!(less.surrender_hp(hp, &rules()) <= hp);
        }

        #[test]
        fn axes_stay_near_their_temperament(seed: u64) {
            let rules = rules();
            let p = Personality::random(&mut ChaCha8Rng::seed_from_u64(seed), &rules);
            let centre = rules.temperaments.get(p.temperament);
            for (axis, value) in [(p.boldness, centre.boldness), (p.ego, centre.ego)] {
                prop_assert!(axis.value() <= Personality::MAX);
                prop_assert!(axis.value().abs_diff(value) <= rules.spread);
            }
        }
    }
}
