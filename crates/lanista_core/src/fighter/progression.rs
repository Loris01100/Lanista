//! Progression without levels: stats change right after each training session and
//! each fight, capped by a hidden potential, at a pace set by age and morale.
//! From a given age, stats decline every year, speed first.

use rand::{Rng, RngExt};
use serde::Deserialize;

use crate::fighter::Stats;
use crate::fighter::morale::Morale;

/// What can make a fighter progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressEvent {
    /// A session whose trainer adds this percentage of the chance to it (0 without one).
    Training {
        coaching_percent: u32,
    },
    Fight,
}

/// A hidden potential: the highest value each stat can reach by progressing.
/// Drawn above `stats`, `rules.potential_headroom_percent` higher per stat.
pub fn random_potential<R: Rng + ?Sized>(
    rng: &mut R,
    stats: Stats,
    rules: &ProgressionRules,
) -> Stats {
    let (min, max) = rules.potential_headroom_percent;
    Stats::from_array(stats.to_array().map(|value| {
        let headroom = u64::from(value) * u64::from(rng.random_range(min..=max)) / 100;
        value.saturating_add(u32::try_from(headroom).unwrap_or(u32::MAX))
    }))
}

/// Percentage of the usual progression chance at `age`: the first bracket of
/// `rules.age_brackets` whose last age is at least `age`, 0 past the last one.
#[must_use]
pub fn age_percent(age: u32, rules: &ProgressionRules) -> u32 {
    rules
        .age_brackets
        .iter()
        .find(|&&(last_age, _)| age <= last_age)
        .map_or(0, |&(_, percent)| percent)
}

/// Stats after `event`: each stat below its potential has a chance to gain
/// its `rules.gain`, never past the potential. The chance is the event's,
/// scaled by the age bracket, by morale (± `rules.morale_percent` at 100 and 0) and,
/// for training, raised by the trainer's coaching.
pub fn progress<R: Rng + ?Sized>(
    rng: &mut R,
    stats: Stats,
    potential: Stats,
    event: ProgressEvent,
    age: u32,
    morale: Morale,
    rules: &ProgressionRules,
) -> Stats {
    let (base, coaching_percent) = match event {
        ProgressEvent::Training { coaching_percent } => (rules.training_chance, coaching_percent),
        ProgressEvent::Fight => (rules.fight_chance, 0),
    };
    let morale_offset = i64::from(morale.value()) - i64::from(Morale::NEUTRAL);
    let morale_percent =
        100 + morale_offset * i64::from(rules.morale_percent) / i64::from(Morale::NEUTRAL);
    // Chance out of 1 000 000: base % × age % × morale % × coaching %.
    let chance = i64::from(base)
        * i64::from(age_percent(age, rules))
        * morale_percent.max(0)
        * (100 + i64::from(coaching_percent))
        / 100;
    let chance = u32::try_from(chance.clamp(0, 1_000_000)).unwrap_or(0);
    let mut stats = stats.to_array();
    let caps = potential.to_array();
    let gains = rules.gain.to_array();
    for ((value, cap), gain) in stats.iter_mut().zip(caps).zip(gains) {
        // Always drawn, so a capped stat doesn't change the draws of the others.
        let lucky = rng.random_range(0..1_000_000) < chance;
        if lucky && *value < cap {
            *value = value.saturating_add(gain).min(cap);
        }
    }
    Stats::from_array(stats)
}

/// Stats after a year at `age`: from `rules.decline_from_age`, each stat loses
/// `rules.decline_percent` of its value, rounded up, so the decline always shows.
#[must_use]
pub fn age_decline(stats: Stats, age: u32, rules: &ProgressionRules) -> Stats {
    if age < rules.decline_from_age {
        return stats;
    }
    stats.zip(rules.decline_percent, |value, percent| {
        let loss = (u64::from(value) * u64::from(percent.min(100))).div_ceil(100);
        value - u32::try_from(loss).unwrap_or(value)
    })
}

/// Progression pace, potential and decline, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressionRules {
    /// Chance, in percent, that each stat gains after a training session,
    /// in the first age bracket at neutral morale.
    pub training_chance: u32,
    /// Same after a fight.
    pub fight_chance: u32,
    /// Points a stat gains when it progresses.
    pub gain: Stats,
    /// `(last age, percent of the progression chance)`, by increasing age;
    /// no progression past the last one.
    pub age_brackets: Vec<(u32, u32)>,
    /// Progression chance bonus at morale 100, malus at 0, in percent of the chance.
    pub morale_percent: u32,
    /// Lowest and highest percentage a generated potential sits above each stat.
    pub potential_headroom_percent: (u32, u32),
    /// First age at which stats decline every year.
    pub decline_from_age: u32,
    /// Percentage of each stat lost per year from `decline_from_age`.
    pub decline_percent: Stats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn rules() -> ProgressionRules {
        ProgressionRules {
            training_chance: 30,
            fight_chance: 20,
            gain: Stats {
                hp: 2,
                melee_attack: 1,
                ranged_attack: 1,
                speed: 1,
                melee_defense: 1,
                ranged_defense: 1,
            },
            age_brackets: vec![(27, 100), (35, 40), (39, 10)],
            morale_percent: 20,
            potential_headroom_percent: (20, 80),
            decline_from_age: 40,
            decline_percent: Stats {
                hp: 2,
                melee_attack: 2,
                ranged_attack: 2,
                speed: 5,
                melee_defense: 2,
                ranged_defense: 2,
            },
        }
    }

    const STATS: Stats = Stats {
        hp: 100,
        melee_attack: 20,
        ranged_attack: 20,
        speed: 30,
        melee_defense: 10,
        ranged_defense: 10,
    };

    /// Total points gained over `weeks` training sessions, from `STATS`, far from potential.
    fn gained(age: u32, morale: u32, weeks: u32) -> u32 {
        coached(age, morale, weeks, 0)
    }

    /// The same with a trainer adding `coaching_percent`.
    fn coached(age: u32, morale: u32, weeks: u32, coaching_percent: u32) -> u32 {
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let potential = STATS.map(|v| v * 10);
        let mut stats = STATS;
        for _ in 0..weeks {
            let event = ProgressEvent::Training { coaching_percent };
            stats = progress(
                &mut rng,
                stats,
                potential,
                event,
                age,
                Morale::new(morale),
                &rules(),
            );
        }
        stats.to_array().iter().sum::<u32>() - STATS.to_array().iter().sum::<u32>()
    }

    #[test]
    fn age_brackets_set_the_pace() {
        assert_eq!(age_percent(20, &rules()), 100);
        assert_eq!(age_percent(27, &rules()), 100);
        assert_eq!(age_percent(28, &rules()), 40);
        assert_eq!(age_percent(39, &rules()), 10);
        assert_eq!(age_percent(40, &rules()), 0);
        let (young, plateau, old) = (
            gained(22, 50, 100),
            gained(30, 50, 100),
            gained(37, 50, 100),
        );
        assert!(young > plateau && plateau > old, "{young} {plateau} {old}");
        assert_eq!(gained(41, 50, 100), 0);
    }

    #[test]
    fn morale_modulates_progression() {
        let (low, high) = (gained(22, 0, 200), gained(22, 100, 200));
        assert!(high > low, "{low} {high}");
    }

    #[test]
    fn a_trainer_raises_training_gains() {
        let (alone, coached) = (gained(22, 50, 200), coached(22, 50, 200, 75));
        assert!(coached > alone, "{alone} {coached}");
    }

    #[test]
    fn potential_caps_progression() {
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let potential = Stats { hp: 101, ..STATS };
        let mut stats = STATS;
        for _ in 0..100 {
            stats = progress(
                &mut rng,
                stats,
                potential,
                ProgressEvent::Fight,
                20,
                Morale::neutral(),
                &rules(),
            );
        }
        assert_eq!(stats, potential);
    }

    #[test]
    fn decline_starts_at_40_and_hits_speed_hardest() {
        assert_eq!(age_decline(STATS, 39, &rules()), STATS);
        let old = age_decline(STATS, 40, &rules());
        assert_eq!((old.hp, old.melee_attack, old.speed), (98, 19, 28));
    }

    proptest! {
        #[test]
        fn progress_stays_between_stats_and_potential(seed: u64, age in 18..50u32, morale in 0..=100u32) {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let potential = random_potential(&mut rng, STATS, &rules());
            let after = progress(&mut rng, STATS, potential, ProgressEvent::Training { coaching_percent: 0 }, age, Morale::new(morale), &rules());
            for ((before, after), cap) in STATS.to_array().into_iter().zip(after.to_array()).zip(potential.to_array()) {
                prop_assert!(before <= after && after <= cap);
            }
        }

        #[test]
        fn potential_sits_in_its_headroom(seed: u64) {
            let potential = random_potential(&mut ChaCha8Rng::seed_from_u64(seed), STATS, &rules());
            for (stat, cap) in STATS.to_array().into_iter().zip(potential.to_array()) {
                prop_assert!((stat + stat / 5..=stat + stat * 4 / 5).contains(&cap));
            }
        }
    }
}
