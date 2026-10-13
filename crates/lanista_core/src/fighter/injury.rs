//! Injuries: a location, a severity and the weeks left to heal; permanent ones never heal.

use rand::{Rng, RngExt};
use serde::Deserialize;

use crate::fighter::Stats;
use crate::fighter::form::{Form, FormRules};

/// Left or right: a fighter's dominant hand, or the side of an arm or leg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Left,
    Right,
}

/// Where an injury is. Arms and legs have a side; whether an arm is the dominant one
/// depends on the fighter's dominant hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Location {
    Head,
    Body,
    Arm(Side),
    Leg(Side),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Light,
    Medium,
    Severe,
    /// Never heals: a lasting after-effect, or a lost arm.
    Permanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Injury {
    pub location: Location,
    pub severity: Severity,
    /// Weeks before it heals; always 0 for a permanent injury.
    pub weeks_left: u32,
}

impl Injury {
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        self.severity == Severity::Permanent
    }

    /// Percentage of each stat this injury takes off, for a fighter whose dominant hand is `dominant`.
    #[must_use]
    pub fn malus(&self, dominant: Side, rules: &InjuryRules) -> Stats {
        let maluses = if self.is_permanent() {
            &rules.permanent
        } else {
            &rules.temporary
        };
        let base = match self.location {
            Location::Head => maluses.head,
            Location::Body => maluses.body,
            Location::Arm(side) if side == dominant => maluses.dominant_arm,
            Location::Arm(_) => maluses.off_arm,
            Location::Leg(_) => maluses.leg,
        };
        let percent = match self.severity {
            Severity::Light => rules.severity_percent.light,
            Severity::Medium => rules.severity_percent.medium,
            Severity::Severe => rules.severity_percent.severe,
            Severity::Permanent => 100,
        };
        map_stats(base, |value| value.saturating_mul(percent) / 100)
    }
}

/// Draws whether a fight injures a fighter in form `form`, and if so where and how badly.
/// The chance is `rules.chance_per_fight` raised by the form risk formula.
pub fn roll_injury<R: Rng + ?Sized>(
    rng: &mut R,
    form: Form,
    rules: &InjuryRules,
    form_rules: &FormRules,
) -> Option<Injury> {
    if !rng.random_bool(form.risk(rules.chance_per_fight, form_rules)) {
        return None;
    }
    let side = if rng.random_bool(0.5) {
        Side::Left
    } else {
        Side::Right
    };
    let weights = &rules.location_weights;
    let location = pick(
        rng,
        &[
            (weights.head, Location::Head),
            (weights.body, Location::Body),
            (weights.arm, Location::Arm(side)),
            (weights.leg, Location::Leg(side)),
        ],
    );
    let weights = &rules.severity_weights;
    let severity = pick(
        rng,
        &[
            (weights.light, Severity::Light),
            (weights.medium, Severity::Medium),
            (weights.severe, Severity::Severe),
            (weights.permanent, Severity::Permanent),
        ],
    );
    let weeks = match severity {
        Severity::Light => rules.healing_weeks.light,
        Severity::Medium => rules.healing_weeks.medium,
        Severity::Severe => rules.healing_weeks.severe,
        Severity::Permanent => (0, 0),
    };
    Some(Injury {
        location,
        severity,
        weeks_left: rng.random_range(weeks.0..=weeks.1),
    })
}

/// One week of natural healing: every temporary injury gets a week closer to healed,
/// and healed ones are removed. Permanent injuries stay.
pub fn heal_week(injuries: &mut Vec<Injury>) {
    for injury in injuries.iter_mut() {
        injury.weeks_left = injury.weeks_left.saturating_sub(1);
    }
    injuries.retain(|injury| injury.is_permanent() || injury.weeks_left > 0);
}

/// `stats` lowered by every injury: maluses add up per stat, capped at `rules.max_malus` percent.
#[must_use]
pub fn injured_stats(
    stats: Stats,
    injuries: &[Injury],
    dominant: Side,
    rules: &InjuryRules,
) -> Stats {
    let total = injuries
        .iter()
        .map(|injury| injury.malus(dominant, rules))
        .fold(Stats::default(), |sum, malus| {
            zip_stats(sum, malus, u32::saturating_add)
        });
    zip_stats(stats, total, |value, percent| {
        let kept = u64::from(100 - percent.min(rules.max_malus).min(100));
        u32::try_from(u64::from(value) * kept / 100).unwrap_or(value)
    })
}

/// A value drawn among `choices` with a chance proportional to its weight.
/// Falls back to the last choice when all weights are 0.
fn pick<R: Rng + ?Sized, T: Copy>(rng: &mut R, choices: &[(u32, T)]) -> T {
    let total: u32 = choices.iter().map(|(weight, _)| weight).sum();
    let mut draw = if total == 0 {
        0
    } else {
        rng.random_range(0..total)
    };
    for &(weight, value) in choices {
        if draw < weight {
            return value;
        }
        draw -= weight;
    }
    choices[choices.len() - 1].1
}

fn map_stats(stats: Stats, f: impl Fn(u32) -> u32) -> Stats {
    zip_stats(stats, stats, |value, _| f(value))
}

fn zip_stats(a: Stats, b: Stats, f: impl Fn(u32, u32) -> u32) -> Stats {
    Stats {
        hp: f(a.hp, b.hp),
        melee_attack: f(a.melee_attack, b.melee_attack),
        ranged_attack: f(a.ranged_attack, b.ranged_attack),
        speed: f(a.speed, b.speed),
        melee_defense: f(a.melee_defense, b.melee_defense),
        ranged_defense: f(a.ranged_defense, b.ranged_defense),
    }
}

/// Injury odds, healing times and maluses, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InjuryRules {
    /// Chance of an injury per fight at full form, raised by the form risk formula.
    pub chance_per_fight: f64,
    pub location_weights: LocationWeights,
    pub severity_weights: SeverityWeights,
    /// Lowest and highest weeks to heal, by severity.
    pub healing_weeks: BySeverity<(u32, u32)>,
    /// Temporary maluses as a percentage of `temporary`, by severity.
    pub severity_percent: BySeverity<u32>,
    /// Percentage of each stat a medium injury takes off, by location.
    pub temporary: LocationMaluses,
    /// Percentage of each stat a permanent injury takes off, by location.
    pub permanent: LocationMaluses,
    /// Highest percentage of a stat all injuries together can take off.
    pub max_malus: u32,
}

/// Relative chances of each location; the side of an arm or leg is drawn 50/50.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationWeights {
    pub head: u32,
    pub body: u32,
    pub arm: u32,
    pub leg: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeverityWeights {
    pub light: u32,
    pub medium: u32,
    pub severe: u32,
    pub permanent: u32,
}

/// One value per healing severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BySeverity<T> {
    pub light: T,
    pub medium: T,
    pub severe: T,
}

/// Stat maluses, in percent, by location; the arms depend on the dominant hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocationMaluses {
    pub head: Stats,
    pub body: Stats,
    pub dominant_arm: Stats,
    pub off_arm: Stats,
    pub leg: Stats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn rules() -> InjuryRules {
        let only = |hp, melee_attack, speed| Stats {
            hp,
            melee_attack,
            speed,
            ..Stats::default()
        };
        let maluses = LocationMaluses {
            head: only(0, 10, 0),
            body: only(20, 0, 0),
            dominant_arm: only(0, 30, 0),
            off_arm: only(0, 5, 0),
            leg: only(0, 0, 20),
        };
        InjuryRules {
            chance_per_fight: 0.2,
            location_weights: LocationWeights {
                head: 1,
                body: 1,
                arm: 1,
                leg: 1,
            },
            severity_weights: SeverityWeights {
                light: 1,
                medium: 1,
                severe: 1,
                permanent: 1,
            },
            healing_weeks: BySeverity {
                light: (1, 2),
                medium: (3, 5),
                severe: (6, 8),
            },
            severity_percent: BySeverity {
                light: 50,
                medium: 100,
                severe: 200,
            },
            temporary: maluses,
            permanent: LocationMaluses {
                dominant_arm: only(0, 60, 0),
                ..maluses
            },
            max_malus: 80,
        }
    }

    const FORM_RULES: FormRules = FormRules {
        risk_factor: 2.0,
        fight: 20,
        title_defense: 30,
        training: 5,
        rest: 15,
    };

    const STATS: Stats = Stats {
        hp: 100,
        melee_attack: 20,
        ranged_attack: 20,
        speed: 30,
        melee_defense: 10,
        ranged_defense: 10,
    };

    fn injury(location: Location, severity: Severity, weeks_left: u32) -> Injury {
        Injury {
            location,
            severity,
            weeks_left,
        }
    }

    #[test]
    fn dominant_arm_hurts_more_than_the_other() {
        let left_arm = injury(Location::Arm(Side::Left), Severity::Medium, 3);
        assert_eq!(left_arm.malus(Side::Left, &rules()).melee_attack, 30);
        assert_eq!(left_arm.malus(Side::Right, &rules()).melee_attack, 5);
    }

    #[test]
    fn severity_scales_temporary_maluses() {
        let leg = |severity| injury(Location::Leg(Side::Right), severity, 1);
        let speed = |severity| leg(severity).malus(Side::Right, &rules()).speed;
        assert_eq!(speed(Severity::Light), 10);
        assert_eq!(speed(Severity::Medium), 20);
        assert_eq!(speed(Severity::Severe), 40);
    }

    #[test]
    fn permanent_injuries_use_their_own_maluses() {
        let lost_arm = injury(Location::Arm(Side::Right), Severity::Permanent, 0);
        assert_eq!(lost_arm.malus(Side::Right, &rules()).melee_attack, 60);
    }

    #[test]
    fn maluses_add_up_and_are_capped() {
        let injuries = [
            injury(Location::Body, Severity::Medium, 3),
            injury(Location::Leg(Side::Left), Severity::Medium, 3),
        ];
        let stats = injured_stats(STATS, &injuries, Side::Right, &rules());
        assert_eq!(stats.hp, 80);
        assert_eq!(stats.speed, 24);
        assert_eq!(stats.melee_attack, 20);

        let arm = injury(Location::Arm(Side::Right), Severity::Severe, 6);
        let lost_arm = injury(Location::Arm(Side::Right), Severity::Permanent, 0);
        let stats = injured_stats(STATS, &[arm, lost_arm], Side::Right, &rules());
        assert_eq!(stats.melee_attack, 4, "60 % + 60 % capped at 80 %");
    }

    #[test]
    fn healing_removes_healed_injuries_but_never_permanent_ones() {
        let mut injuries = vec![
            injury(Location::Head, Severity::Light, 1),
            injury(Location::Body, Severity::Medium, 3),
            injury(Location::Leg(Side::Left), Severity::Permanent, 0),
        ];
        heal_week(&mut injuries);
        assert_eq!(
            injuries,
            vec![
                injury(Location::Body, Severity::Medium, 2),
                injury(Location::Leg(Side::Left), Severity::Permanent, 0),
            ]
        );
    }

    #[test]
    fn low_form_injures_more_often() {
        let count = |form| {
            let mut rng = ChaCha8Rng::seed_from_u64(1);
            (0..10_000)
                .filter(|_| roll_injury(&mut rng, form, &rules(), &FORM_RULES).is_some())
                .count()
        };
        let (fresh, exhausted) = (count(Form::full()), count(Form::new(0)));
        assert!((1_800..2_200).contains(&fresh), "{fresh}");
        assert!((5_700..6_300).contains(&exhausted), "{exhausted}");
    }

    #[test]
    fn zero_weights_never_come_out() {
        let mut rules = rules();
        rules.location_weights = LocationWeights {
            head: 0,
            body: 0,
            arm: 1,
            leg: 0,
        };
        rules.severity_weights.permanent = 0;
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        for _ in 0..1_000 {
            if let Some(injury) = roll_injury(&mut rng, Form::new(0), &rules, &FORM_RULES) {
                assert!(matches!(injury.location, Location::Arm(_)));
                assert!(!injury.is_permanent());
            }
        }
    }

    proptest! {
        #[test]
        fn drawn_injuries_heal_in_their_severity_range(seed: u64) {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let rules = rules();
            if let Some(injury) = roll_injury(&mut rng, Form::new(0), &rules, &FORM_RULES) {
                let (min, max) = match injury.severity {
                    Severity::Light => rules.healing_weeks.light,
                    Severity::Medium => rules.healing_weeks.medium,
                    Severity::Severe => rules.healing_weeks.severe,
                    Severity::Permanent => (0, 0),
                };
                prop_assert!((min..=max).contains(&injury.weeks_left));
            }
        }

        #[test]
        fn injuries_never_raise_a_stat(seed: u64, count in 0..6usize) {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let injuries: Vec<Injury> = (0..count)
                .filter_map(|_| roll_injury(&mut rng, Form::new(0), &rules(), &FORM_RULES))
                .collect();
            let stats = injured_stats(STATS, &injuries, Side::Left, &rules());
            prop_assert!(stats.hp <= STATS.hp && stats.hp >= STATS.hp / 5);
            prop_assert!(stats.melee_attack <= STATS.melee_attack);
            prop_assert!(stats.speed <= STATS.speed);
        }
    }
}
