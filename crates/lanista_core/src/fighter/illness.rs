//! Illnesses on three levels: a malus on every stat, slower form recovery,
//! and for a severe illness a risk of getting worse when fighting.

use rand::{Rng, RngExt};
use serde::Deserialize;

use crate::fighter::form::{Form, FormRules};
use crate::fighter::{Stats, percent_of, pick};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IllnessLevel {
    /// Small malus on every stat.
    Mild,
    /// Marked malus, slower form recovery.
    Moderate,
    /// Fighting is not advised: it may make the illness last longer.
    Severe,
}

/// A fighter has at most one illness at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Illness {
    pub level: IllnessLevel,
    /// Weeks before it heals.
    pub weeks_left: u32,
}

impl Illness {
    /// Whether fighting is advised; the player and rival managers decide, the rules don't forbid it.
    #[must_use]
    pub fn fight_advised(&self) -> bool {
        self.level != IllnessLevel::Severe
    }

    /// `stats` lowered by this illness's malus, the same percentage on every stat.
    #[must_use]
    pub fn ill_stats(&self, stats: Stats, rules: &IllnessRules) -> Stats {
        let malus = rules.malus.get(self.level).min(100);
        stats.map(|value| percent_of(value, 100 - malus))
    }

    /// Form after a week of rest while ill: the rest gain is cut to the level's `rest_percent`.
    #[must_use]
    pub fn rest(&self, form: Form, rules: &IllnessRules, form_rules: &FormRules) -> Form {
        let gain = percent_of(form_rules.rest, rules.rest_percent.get(self.level));
        Form::new(form.value().saturating_add(gain))
    }

    /// The illness after its fighter fought: a severe one may get worse and last longer.
    #[must_use]
    pub fn after_fight<R: Rng + ?Sized>(self, rng: &mut R, rules: &IllnessRules) -> Self {
        if self.level != IllnessLevel::Severe || !rng.random_bool(rules.worsening_chance) {
            return self;
        }
        let (min, max) = rules.worsening_weeks;
        Self {
            weeks_left: self.weeks_left.saturating_add(rng.random_range(min..=max)),
            ..self
        }
    }

    /// The illness a week later, or `None` once healed.
    #[must_use]
    pub fn heal_week(self) -> Option<Self> {
        let weeks_left = self.weeks_left.saturating_sub(1);
        (weeks_left > 0).then_some(Self { weeks_left, ..self })
    }
}

/// Draws whether a fighter in form `form`, not already ill, falls ill this week, and how badly.
/// The chance is `rules.chance_per_week` raised by the form risk formula.
pub fn roll_illness<R: Rng + ?Sized>(
    rng: &mut R,
    form: Form,
    rules: &IllnessRules,
    form_rules: &FormRules,
) -> Option<Illness> {
    if !rng.random_bool(form.risk(rules.chance_per_week, form_rules)) {
        return None;
    }
    let weights = &rules.level_weights;
    let level = pick(
        rng,
        &[
            (weights.mild, IllnessLevel::Mild),
            (weights.moderate, IllnessLevel::Moderate),
            (weights.severe, IllnessLevel::Severe),
        ],
    );
    let (min, max) = rules.weeks.get(level);
    Some(Illness {
        level,
        weeks_left: rng.random_range(min..=max),
    })
}

/// Illness odds, durations and effects, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IllnessRules {
    /// Chance of falling ill per week at full form, raised by the form risk formula.
    pub chance_per_week: f64,
    /// Relative chances of each level.
    pub level_weights: ByLevel<u32>,
    /// Lowest and highest weeks to heal (at least 1).
    pub weeks: ByLevel<(u32, u32)>,
    /// Percentage taken off every stat.
    pub malus: ByLevel<u32>,
    /// Percentage of the usual form gain a week of rest gives.
    pub rest_percent: ByLevel<u32>,
    /// Chance a fight makes a severe illness worse.
    pub worsening_chance: f64,
    /// Lowest and highest weeks a worsening adds.
    pub worsening_weeks: (u32, u32),
}

/// One value per illness level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByLevel<T> {
    pub mild: T,
    pub moderate: T,
    pub severe: T,
}

impl<T: Copy> ByLevel<T> {
    #[must_use]
    pub fn get(&self, level: IllnessLevel) -> T {
        match level {
            IllnessLevel::Mild => self.mild,
            IllnessLevel::Moderate => self.moderate,
            IllnessLevel::Severe => self.severe,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    const RULES: IllnessRules = IllnessRules {
        chance_per_week: 0.1,
        level_weights: ByLevel {
            mild: 1,
            moderate: 1,
            severe: 1,
        },
        weeks: ByLevel {
            mild: (1, 2),
            moderate: (3, 5),
            severe: (6, 8),
        },
        malus: ByLevel {
            mild: 5,
            moderate: 15,
            severe: 30,
        },
        rest_percent: ByLevel {
            mild: 100,
            moderate: 50,
            severe: 20,
        },
        worsening_chance: 1.0,
        worsening_weeks: (2, 2),
    };

    const FORM_RULES: FormRules = FormRules {
        risk_factor: 2.0,
        fight: 20,
        title_defense: 30,
        training: 5,
        rest: 20,
    };

    const STATS: Stats = Stats {
        hp: 100,
        melee_attack: 20,
        ranged_attack: 20,
        speed: 30,
        melee_defense: 10,
        ranged_defense: 10,
    };

    fn illness(level: IllnessLevel, weeks_left: u32) -> Illness {
        Illness { level, weeks_left }
    }

    #[test]
    fn malus_hits_every_stat_by_level() {
        let stats = illness(IllnessLevel::Moderate, 3).ill_stats(STATS, &RULES);
        assert_eq!(
            stats,
            Stats {
                hp: 85,
                melee_attack: 17,
                ranged_attack: 17,
                speed: 25,
                melee_defense: 8,
                ranged_defense: 8,
            }
        );
        assert_eq!(
            illness(IllnessLevel::Mild, 1).ill_stats(STATS, &RULES).hp,
            95
        );
    }

    #[test]
    fn illness_slows_form_recovery() {
        let form = Form::new(50);
        assert_eq!(
            illness(IllnessLevel::Mild, 1)
                .rest(form, &RULES, &FORM_RULES)
                .value(),
            70
        );
        assert_eq!(
            illness(IllnessLevel::Moderate, 3)
                .rest(form, &RULES, &FORM_RULES)
                .value(),
            60
        );
        assert_eq!(
            illness(IllnessLevel::Severe, 6)
                .rest(form, &RULES, &FORM_RULES)
                .value(),
            54
        );
        assert_eq!(
            illness(IllnessLevel::Mild, 1).rest(Form::new(95), &RULES, &FORM_RULES),
            Form::full()
        );
    }

    #[test]
    fn only_severe_illness_advises_against_fighting_and_worsens() {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        assert!(illness(IllnessLevel::Moderate, 3).fight_advised());
        assert!(!illness(IllnessLevel::Severe, 6).fight_advised());
        let moderate = illness(IllnessLevel::Moderate, 3);
        assert_eq!(moderate.after_fight(&mut rng, &RULES), moderate);
        let severe = illness(IllnessLevel::Severe, 6).after_fight(&mut rng, &RULES);
        assert_eq!(severe.weeks_left, 8);
    }

    #[test]
    fn heals_after_its_weeks() {
        let ill = illness(IllnessLevel::Mild, 2);
        assert_eq!(ill.heal_week(), Some(illness(IllnessLevel::Mild, 1)));
        assert_eq!(ill.heal_week().and_then(Illness::heal_week), None);
    }

    #[test]
    fn low_form_makes_illness_more_likely() {
        let count = |form| {
            let mut rng = ChaCha8Rng::seed_from_u64(1);
            (0..10_000)
                .filter(|_| roll_illness(&mut rng, form, &RULES, &FORM_RULES).is_some())
                .count()
        };
        let (fresh, exhausted) = (count(Form::full()), count(Form::new(0)));
        assert!((800..1_200).contains(&fresh), "{fresh}");
        assert!((2_700..3_300).contains(&exhausted), "{exhausted}");
    }

    proptest! {
        #[test]
        fn drawn_illnesses_last_their_level_range(seed: u64) {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            if let Some(ill) = roll_illness(&mut rng, Form::new(0), &RULES, &FORM_RULES) {
                let (min, max) = RULES.weeks.get(ill.level);
                prop_assert!((min..=max).contains(&ill.weeks_left));
            }
        }

        #[test]
        fn illness_never_raises_a_stat(weeks in 1..10u32, level in 0..3usize) {
            let level = [IllnessLevel::Mild, IllnessLevel::Moderate, IllnessLevel::Severe][level];
            let stats = illness(level, weeks).ill_stats(STATS, &RULES);
            prop_assert_eq!(stats.zip(STATS, |ill, healthy| u32::from(ill > healthy)), Stats::default());
        }
    }
}
