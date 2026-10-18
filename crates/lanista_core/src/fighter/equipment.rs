//! The colosseum's equipment, the same for everyone: a fighter may refuse its armour,
//! its weapon or both, fighting weaker for a fame bonus that grows with the risk.
// ponytail: only the player's fighters refuse; rival fighters and free agents keep
// everything, as the statistical model has no such input.

use serde::Deserialize;

use crate::combat::DamageKind;
use crate::fighter::fame::FameRules;
use crate::fighter::{Stats, percent_of};

/// The pieces of equipment a fighter refuses for a fight; none by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Refused {
    /// Fights without armour: melee and ranged defense fall.
    pub armour: bool,
    /// Fights bare-handed: melee and ranged attack fall, and cutting hits turn blunt.
    pub weapon: bool,
}

impl Refused {
    /// Everything worn.
    pub const NONE: Self = Self {
        armour: false,
        weapon: false,
    };

    /// Whether any piece is refused.
    #[must_use]
    pub fn any(self) -> bool {
        self.armour || self.weapon
    }

    /// `stats` with the maluses of the refused pieces: defenses down to
    /// `armourless_defense_percent` without armour, attacks down to
    /// `weaponless_attack_percent` without weapon.
    #[must_use]
    pub fn stats(self, stats: Stats, rules: &EquipmentRules) -> Stats {
        let mut stats = stats;
        if self.armour {
            stats.melee_defense = percent_of(stats.melee_defense, rules.armourless_defense_percent);
            stats.ranged_defense =
                percent_of(stats.ranged_defense, rules.armourless_defense_percent);
        }
        if self.weapon {
            stats.melee_attack = percent_of(stats.melee_attack, rules.weaponless_attack_percent);
            stats.ranged_attack = percent_of(stats.ranged_attack, rules.weaponless_attack_percent);
        }
        stats
    }

    /// What a spell of `kind` hits with: bare hands strike blunt instead of cutting.
    #[must_use]
    pub fn damage_kind(self, kind: DamageKind) -> DamageKind {
        match kind {
            DamageKind::Cutting if self.weapon => DamageKind::Blunt,
            kind => kind,
        }
    }

    /// The fame the risk brings, whatever the result: the weapon's bonus, the armour's,
    /// or the bonus of fighting with nothing.
    #[must_use]
    pub fn fame_bonus(self, rules: &FameRules) -> u32 {
        match (self.armour, self.weapon) {
            (true, true) => rules.refused_all,
            (false, true) => rules.refused_weapon,
            (true, false) => rules.refused_armour,
            (false, false) => 0,
        }
    }
}

/// What refusing a piece costs, from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentRules {
    /// Melee and ranged defense left without armour, in percent.
    pub armourless_defense_percent: u32,
    /// Melee and ranged attack left without weapon, in percent.
    pub weaponless_attack_percent: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_data::DesignData;

    const RULES: EquipmentRules = EquipmentRules {
        armourless_defense_percent: 40,
        weaponless_attack_percent: 50,
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
    fn each_piece_refused_lowers_its_stats_only() {
        assert_eq!(Refused::NONE.stats(STATS, &RULES), STATS);
        let armour = Refused {
            armour: true,
            weapon: false,
        };
        let naked = armour.stats(STATS, &RULES);
        assert_eq!((naked.melee_defense, naked.ranged_defense), (4, 20));
        assert_eq!((naked.melee_attack, naked.ranged_attack), (20, 30));
        let weapon = Refused {
            armour: false,
            weapon: true,
        };
        let bare = weapon.stats(STATS, &RULES);
        assert_eq!((bare.melee_attack, bare.ranged_attack), (10, 15));
        assert_eq!((bare.melee_defense, bare.hp, bare.speed), (10, 100, 30));
        let all = Refused {
            armour: true,
            weapon: true,
        };
        assert_eq!(all.stats(STATS, &RULES).melee_attack, 10);
        assert_eq!(all.stats(STATS, &RULES).melee_defense, 4);
    }

    #[test]
    fn bare_hands_strike_blunt() {
        let bare = Refused {
            armour: false,
            weapon: true,
        };
        assert_eq!(bare.damage_kind(DamageKind::Cutting), DamageKind::Blunt);
        assert_eq!(bare.damage_kind(DamageKind::Neutral), DamageKind::Neutral);
        assert_eq!(
            Refused::NONE.damage_kind(DamageKind::Cutting),
            DamageKind::Cutting
        );
    }

    #[test]
    fn the_weapon_brings_more_fame_than_the_armour_and_everything_most() {
        let rules = DesignData::load().unwrap().fighter.fame;
        let bonus = |armour, weapon| Refused { armour, weapon }.fame_bonus(&rules);
        assert_eq!(bonus(false, false), 0);
        assert!(0 < bonus(true, false));
        assert!(bonus(true, false) < bonus(false, true));
        assert!(bonus(false, true) < bonus(true, true));
    }
}
