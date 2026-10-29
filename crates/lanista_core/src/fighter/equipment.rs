//! The colosseum's equipment, the same for everyone: a fighter may refuse its armour,
//! never its weapons, fighting weaker for a fame bonus.
// ponytail: only the player's fighters refuse; rival fighters and free agents keep
// everything, as the statistical model has no such input.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::fighter::fame::FameRules;
use crate::fighter::{Stats, percent_of};

/// The equipment a fighter refuses for a fight: its armour or nothing, by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused {
    /// Fights without armour: melee and ranged defense fall.
    pub armour: bool,
}

impl Refused {
    /// Everything worn.
    pub const NONE: Self = Self { armour: false };

    /// `stats` with defenses down to `armourless_defense_percent` without armour.
    #[must_use]
    pub fn stats(self, stats: Stats, rules: &EquipmentRules) -> Stats {
        let mut stats = stats;
        if self.armour {
            stats.melee_defense = percent_of(stats.melee_defense, rules.armourless_defense_percent);
            stats.ranged_defense =
                percent_of(stats.ranged_defense, rules.armourless_defense_percent);
        }
        stats
    }

    /// The fame the risk brings, whatever the result.
    #[must_use]
    pub fn fame_bonus(self, rules: &FameRules) -> u32 {
        if self.armour { rules.refused_armour } else { 0 }
    }
}

/// What a loadout's off hand holds (`design_data::LoadoutData`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum OffHand {
    Free,
    /// None: the weapon takes both hands (a bow).
    BothHands,
    /// A defense bonus; it does not parry.
    Shield,
    /// A second weapon, which strikes weaker unless the fighter is ambidextrous.
    Weapon,
}

impl OffHand {
    /// `stats` with what the off hand brings: melee and ranged defenses up to
    /// `shield_defense_percent` with a shield, melee attack down to
    /// `off_hand_attack_percent` with a second weapon, unless `ambidextrous`.
    #[must_use]
    pub fn stats(self, stats: Stats, ambidextrous: bool, rules: &EquipmentRules) -> Stats {
        let mut stats = stats;
        match self {
            Self::Shield => {
                stats.melee_defense = percent_of(stats.melee_defense, rules.shield_defense_percent);
                stats.ranged_defense =
                    percent_of(stats.ranged_defense, rules.shield_defense_percent);
            }
            Self::Weapon if !ambidextrous => {
                stats.melee_attack = percent_of(stats.melee_attack, rules.off_hand_attack_percent);
            }
            Self::Weapon | Self::Free | Self::BothHands => {}
        }
        stats
    }
}

/// What refusing the armour costs, and what the off hand brings, from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentRules {
    /// Melee and ranged defense left without armour, in percent.
    pub armourless_defense_percent: u32,
    /// Melee and ranged defense with a shield, in percent.
    pub shield_defense_percent: u32,
    /// Melee attack with two weapons, for a fighter not ambidextrous, in percent.
    pub off_hand_attack_percent: u32,
}

/// How at ease a fighter is with each loadout of its class, from 0 to 100, from
/// `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EaseRules {
    /// A recruit's ease with every loadout when it is good at all of them; the more
    /// lopsided, the closer to `best` with one and to `worst` with the others.
    pub good: u32,
    pub best: u32,
    pub worst: u32,
    /// The ease from which a loadout costs nothing.
    pub comfortable: u32,
    /// Percentage of the attacks, defenses and accuracy lost at ease 0, less the closer
    /// to `comfortable`.
    pub malus_percent: u32,
    /// Ease gained with a loadout for each fight in it.
    pub per_fight: u32,
}

impl EaseRules {
    /// A recruit's ease with each of `loadouts` loadouts: one drawn as its strong one,
    /// a lopsidedness drawn from 0 (`good` everywhere) to 100 (`best` with the strong
    /// one, `worst` with the others).
    pub fn random<R: Rng + ?Sized>(&self, rng: &mut R, loadouts: usize) -> Vec<u32> {
        let lopsided = rng.random_range(0..=100);
        let strong = rng.random_range(0..loadouts.max(1));
        let good = self.good.min(100);
        let best = good + percent_of(self.best.min(100).saturating_sub(good), lopsided);
        let worst = good - percent_of(good.saturating_sub(self.worst), lopsided);
        (0..loadouts)
            .map(|i| if i == strong { best } else { worst })
            .collect()
    }

    /// `stats` in a loadout the fighter is at `ease` with: attacks, defenses and
    /// accuracy down by up to `malus_percent` below `comfortable`.
    #[must_use]
    pub fn stats(&self, stats: Stats, ease: u32) -> Stats {
        let missing = self.comfortable.saturating_sub(ease);
        let lost = self.malus_percent * missing / self.comfortable.max(1);
        let kept = 100 - lost.min(100);
        let mut stats = stats;
        for stat in [
            &mut stats.melee_attack,
            &mut stats.ranged_attack,
            &mut stats.accuracy,
            &mut stats.melee_defense,
            &mut stats.ranged_defense,
        ] {
            *stat = percent_of(*stat, kept);
        }
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_data::DesignData;

    const RULES: EquipmentRules = EquipmentRules {
        armourless_defense_percent: 40,
        shield_defense_percent: 120,
        off_hand_attack_percent: 80,
    };

    const STATS: Stats = Stats {
        hp: 100,
        melee_attack: 20,
        ranged_attack: 30,
        accuracy: 30,
        speed: 30,
        melee_defense: 10,
        ranged_defense: 50,
    };

    #[test]
    fn the_refused_armour_lowers_the_defenses_only() {
        assert_eq!(Refused::NONE.stats(STATS, &RULES), STATS);
        let naked = Refused { armour: true }.stats(STATS, &RULES);
        assert_eq!((naked.melee_defense, naked.ranged_defense), (4, 20));
        assert_eq!((naked.melee_attack, naked.ranged_attack), (20, 30));
    }

    #[test]
    fn a_shield_defends_and_a_second_weapon_weakens_who_is_not_ambidextrous() {
        assert_eq!(OffHand::Free.stats(STATS, false, &RULES), STATS);
        let shield = OffHand::Shield.stats(STATS, false, &RULES);
        assert_eq!((shield.melee_defense, shield.ranged_defense), (12, 60));
        assert_eq!(shield.melee_attack, 20);
        let two = OffHand::Weapon.stats(STATS, false, &RULES);
        assert_eq!((two.melee_attack, two.ranged_attack), (16, 30));
        assert_eq!(OffHand::Weapon.stats(STATS, true, &RULES), STATS);
    }

    #[test]
    fn ease_is_drawn_good_or_lopsided_and_costs_below_comfortable() {
        use rand::SeedableRng;
        let rules = EaseRules {
            good: 70,
            best: 100,
            worst: 10,
            comfortable: 70,
            malus_percent: 20,
            per_fight: 4,
        };
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
        for _ in 0..50 {
            let ease = rules.random(&mut rng, 2);
            // The further one goes up, the further the other goes down.
            let (high, low) = (ease[0].max(ease[1]), ease[0].min(ease[1]));
            assert!((70..=100).contains(&high) && (10..=70).contains(&low));
            assert!((70 - low).abs_diff(2 * (high - 70)) <= 1);
        }
        assert_eq!(rules.stats(STATS, 70), STATS);
        assert_eq!(rules.stats(STATS, 100), STATS);
        let awkward = rules.stats(STATS, 0);
        assert_eq!((awkward.melee_attack, awkward.ranged_defense), (16, 40));
        assert_eq!((awkward.hp, awkward.speed), (100, 30));
        assert_eq!(rules.stats(STATS, 35).melee_attack, 18);
    }

    #[test]
    fn the_refused_armour_brings_fame() {
        let rules = DesignData::load().unwrap().fighter.fame;
        assert_eq!(Refused::NONE.fame_bonus(&rules), 0);
        assert_eq!(
            Refused { armour: true }.fame_bonus(&rules),
            rules.refused_armour
        );
        assert!(0 < rules.refused_armour);
    }
}
