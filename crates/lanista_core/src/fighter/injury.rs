//! Injuries: a location, a severity and the weeks left to heal; permanent ones never heal.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::fighter::form::{Form, FormRules};
use crate::fighter::{Stats, percent_of, pick};

/// Left or right: a fighter's dominant hand, or the side of an arm or leg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}

/// Where an injury is. Arms and legs have a side; whether an arm is the dominant one
/// depends on the fighter's dominant hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Location {
    Head,
    Body,
    Arm(Side),
    Leg(Side),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Light,
    Medium,
    Severe,
    /// Never heals: a lasting after-effect.
    Permanent,
    /// The limb is gone, severed by a cutting hit: never heals, no doctor spares it,
    /// and it takes off more than an after-effect (`InjuryRules::lost`).
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Injury {
    pub location: Location,
    pub severity: Severity,
    /// Weeks before it heals; always 0 for a permanent injury.
    pub weeks_left: u32,
}

impl Injury {
    /// Never heals: an after-effect or a lost limb.
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        matches!(self.severity, Severity::Permanent | Severity::Lost)
    }

    /// A limb the fighter no longer has.
    #[must_use]
    pub fn is_lost(&self) -> bool {
        self.severity == Severity::Lost
    }

    /// Percentage of each stat this injury takes off, for a fighter whose dominant hand is `dominant`.
    #[must_use]
    pub fn malus(&self, dominant: Side, rules: &InjuryRules) -> Stats {
        if self.is_lost() {
            let lost = &rules.lost;
            return match self.location {
                Location::Arm(side) if side == dominant => lost.dominant_arm,
                Location::Arm(_) => lost.off_arm,
                Location::Leg(_) => lost.leg,
                Location::Head | Location::Body => Stats::default(),
            };
        }
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
            Severity::Permanent | Severity::Lost => 100,
        };
        base.map(|value| value.saturating_mul(percent) / 100)
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
    let location = roll_location(rng, rules);
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
        Severity::Permanent | Severity::Lost => (0, 0),
    };
    Some(Injury {
        location,
        severity,
        weeks_left: rng.random_range(weeks.0..=weeks.1),
    })
}

/// The severe injury of a fighter who survives a fatal blow, at a drawn location.
pub fn fatal_blow_wound<R: Rng + ?Sized>(rng: &mut R, rules: &InjuryRules) -> Injury {
    let location = roll_location(rng, rules);
    let (low, high) = rules.healing_weeks.severe;
    Injury {
        location,
        severity: Severity::Severe,
        weeks_left: rng.random_range(low..=high),
    }
}

/// The severe head injury a stun may leave (see `InjuryRules::stun_wound_chance`).
pub fn stun_wound<R: Rng + ?Sized>(rng: &mut R, rules: &InjuryRules) -> Injury {
    let (low, high) = rules.healing_weeks.severe;
    Injury {
        location: Location::Head,
        severity: Severity::Severe,
        weeks_left: rng.random_range(low..=high),
    }
}

/// What a cutting hit leaves of a limb it severed: the limb lost.
#[must_use]
pub fn severed_limb_wound(location: Location) -> Injury {
    Injury {
        location,
        severity: Severity::Lost,
        weeks_left: 0,
    }
}

/// Where an injury lands, by `rules.location_weights`.
fn roll_location<R: Rng + ?Sized>(rng: &mut R, rules: &InjuryRules) -> Location {
    let side = if rng.random_bool(0.5) {
        Side::Left
    } else {
        Side::Right
    };
    let weights = &rules.location_weights;
    pick(
        rng,
        &[
            (weights.head, Location::Head),
            (weights.body, Location::Body),
            (weights.arm, Location::Arm(side)),
            (weights.leg, Location::Leg(side)),
        ],
    )
}

/// `injury` as a doctor leaves it: one that would leave an after-effect is only severe
/// with a chance of `spared_percent`, healing in the weeks of a severe one. Draws
/// nothing for a temporary injury, a lost limb or without a doctor (`spared_percent` 0).
pub fn spare_after_effect<R: Rng + ?Sized>(
    rng: &mut R,
    injury: Injury,
    spared_percent: u32,
    rules: &InjuryRules,
) -> Injury {
    if injury.severity != Severity::Permanent
        || spared_percent == 0
        || rng.random_range(0..100) >= spared_percent
    {
        return injury;
    }
    let (min, max) = rules.healing_weeks.severe;
    Injury {
        severity: Severity::Severe,
        weeks_left: rng.random_range(min..=max),
        ..injury
    }
}

/// Whether `injuries` count `part` as a lost limb.
#[must_use]
pub fn has_lost(injuries: &[Injury], part: Location) -> bool {
    injuries.iter().any(|i| i.is_lost() && i.location == part)
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
            sum.zip(malus, u32::saturating_add)
        });
    stats.zip(total, |value, percent| {
        percent_of(value, 100 - percent.min(rules.max_malus).min(100))
    })
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
    /// Percentage of each stat an after-effect takes off, by location.
    pub permanent: LocationMaluses,
    /// Percentage of each stat a lost limb takes off.
    pub lost: LimbMaluses,
    /// Highest percentage of a stat all injuries together can take off.
    pub max_malus: u32,
    /// Chance that a fighter brought to 0 HP survives, with a severe injury.
    pub fatal_blow_survival: f64,
    /// Chance that a fighter stunned in a fight comes out of it with a severe head
    /// injury, instead of the usual draw.
    pub stun_wound_chance: f64,
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

/// Stat maluses of a lost limb, in percent; the arms depend on the dominant hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimbMaluses {
    pub dominant_arm: Stats,
    pub off_arm: Stats,
    pub leg: Stats,
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
            lost: LimbMaluses {
                dominant_arm: only(0, 70, 0),
                off_arm: only(0, 10, 0),
                leg: only(0, 0, 50),
            },
            max_malus: 80,
            fatal_blow_survival: 0.5,
            stun_wound_chance: 0.5,
        }
    }

    const FORM_RULES: FormRules = FormRules {
        risk_factor: 2.0,
        fight: 20,
        title_defense: 30,
        melee: 30,
        training: 5,
        rest: 15,
    };

    const STATS: Stats = Stats {
        hp: 100,
        melee_attack: 20,
        ranged_attack: 20,
        accuracy: 30,
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
        let crippled = injury(Location::Arm(Side::Right), Severity::Permanent, 0);
        assert_eq!(crippled.malus(Side::Right, &rules()).melee_attack, 60);
    }

    #[test]
    fn a_lost_limb_takes_off_more_than_an_after_effect_and_never_heals() {
        let rules = rules();
        let lost_leg = severed_limb_wound(Location::Leg(Side::Left));
        assert!(lost_leg.is_lost() && lost_leg.is_permanent());
        assert_eq!(lost_leg.malus(Side::Right, &rules).speed, 50);
        let limp = injury(Location::Leg(Side::Left), Severity::Permanent, 0);
        assert_eq!(limp.malus(Side::Right, &rules).speed, 20);
        let lost_arm = severed_limb_wound(Location::Arm(Side::Right));
        assert_eq!(lost_arm.malus(Side::Right, &rules).melee_attack, 70);
        assert_eq!(lost_arm.malus(Side::Left, &rules).melee_attack, 10);
        let mut injuries = vec![lost_leg];
        heal_week(&mut injuries);
        assert_eq!(injuries, vec![lost_leg]);
        // No doctor gives a limb back.
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(
            spare_after_effect(&mut rng, lost_arm, 100, &rules),
            lost_arm
        );
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
    fn a_doctor_spares_after_effects_only() {
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let rules = rules();
        let permanent = Injury {
            location: Location::Head,
            severity: Severity::Permanent,
            weeks_left: 0,
        };
        assert_eq!(
            spare_after_effect(&mut rng, permanent, 0, &rules),
            permanent
        );
        let spared = spare_after_effect(&mut rng, permanent, 100, &rules);
        assert_eq!(
            (spared.location, spared.severity),
            (Location::Head, Severity::Severe)
        );
        let (min, max) = rules.healing_weeks.severe;
        assert!((min..=max).contains(&spared.weeks_left));
        let light = Injury {
            severity: Severity::Light,
            weeks_left: 2,
            ..permanent
        };
        assert_eq!(spare_after_effect(&mut rng, light, 100, &rules), light);
        let kept = (0..1000)
            .filter(|_| spare_after_effect(&mut rng, permanent, 30, &rules).is_permanent())
            .count();
        assert!(kept.abs_diff(700) < 50, "{kept}");
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
                    Severity::Permanent | Severity::Lost => (0, 0),
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
