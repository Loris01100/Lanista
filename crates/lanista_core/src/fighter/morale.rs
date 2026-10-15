//! Morale: from 0 to 100, neutral at 50. Up with wins and fame, down with defeats,
//! injuries and illnesses; a small bonus or malus in combat. No loyalty.

use serde::{Deserialize, Serialize};

use crate::fighter::{Stats, percent_of};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(from = "u32", into = "u32")]
pub struct Morale(u32);

/// What changes a fighter's morale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoraleEvent {
    Victory,
    /// Lost, gave up included.
    Defeat,
    Injury,
    Illness,
    /// Fame points gained; a loss of fame does not touch morale.
    FameGained(u32),
}

impl Morale {
    pub const MAX: u32 = 100;
    /// No bonus, no malus.
    pub const NEUTRAL: u32 = 50;

    /// Morale `value`, capped at [`Morale::MAX`].
    #[must_use]
    pub fn new(value: u32) -> Self {
        Self(value.min(Self::MAX))
    }

    #[must_use]
    pub fn neutral() -> Self {
        Self(Self::NEUTRAL)
    }

    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }

    /// Morale after `event`, staying within 0 to [`Morale::MAX`].
    #[must_use]
    pub fn after(self, event: MoraleEvent, rules: &MoraleRules) -> Self {
        match event {
            MoraleEvent::Victory => Self::new(self.0.saturating_add(rules.victory)),
            MoraleEvent::Defeat => Self(self.0.saturating_sub(rules.defeat)),
            MoraleEvent::Injury => Self(self.0.saturating_sub(rules.injury)),
            MoraleEvent::Illness => Self(self.0.saturating_sub(rules.illness)),
            MoraleEvent::FameGained(fame) => {
                Self::new(self.0.saturating_add(percent_of(fame, rules.fame_percent)))
            }
        }
    }

    /// Percentage applied to the stats in combat: 100 at neutral morale,
    /// `100 ± rules.combat_percent` at 100 and at 0, linear in between.
    #[must_use]
    pub fn combat_percent(self, rules: &MoraleRules) -> u32 {
        let offset = i64::from(self.0) - i64::from(Self::NEUTRAL);
        let percent = 100 + offset * i64::from(rules.combat_percent) / i64::from(Self::NEUTRAL);
        u32::try_from(percent.max(0)).unwrap_or(0)
    }

    /// `stats` with this morale's combat bonus or malus.
    #[must_use]
    pub fn combat_stats(self, stats: Stats, rules: &MoraleRules) -> Stats {
        let percent = self.combat_percent(rules);
        stats.map(|value| percent_of(value, percent))
    }
}

/// Morale changes and its combat effect, read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoraleRules {
    pub victory: u32,
    pub defeat: u32,
    pub injury: u32,
    pub illness: u32,
    /// Morale gained per fame point gained, in percent.
    pub fame_percent: u32,
    /// Largest bonus or malus on every stat in combat, in percent, at morale 100 or 0.
    pub combat_percent: u32,
}

// Saves go through `new`: an edited value stays within its bounds.
impl From<u32> for Morale {
    fn from(value: u32) -> Self {
        Self::new(value)
    }
}

impl From<Morale> for u32 {
    fn from(value: Morale) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const RULES: MoraleRules = MoraleRules {
        victory: 10,
        defeat: 10,
        injury: 8,
        illness: 5,
        fame_percent: 50,
        combat_percent: 10,
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

    #[test]
    fn events_move_morale() {
        let morale = Morale::neutral();
        assert_eq!(morale.after(MoraleEvent::Victory, &RULES).value(), 60);
        assert_eq!(morale.after(MoraleEvent::Defeat, &RULES).value(), 40);
        assert_eq!(morale.after(MoraleEvent::Injury, &RULES).value(), 42);
        assert_eq!(morale.after(MoraleEvent::Illness, &RULES).value(), 45);
        assert_eq!(
            morale.after(MoraleEvent::FameGained(10), &RULES).value(),
            55
        );
        assert_eq!(
            Morale::new(95).after(MoraleEvent::Victory, &RULES).value(),
            100
        );
        assert_eq!(Morale::new(5).after(MoraleEvent::Defeat, &RULES).value(), 0);
    }

    #[test]
    fn morale_gives_a_small_combat_bonus_or_malus() {
        assert_eq!(Morale::neutral().combat_stats(STATS, &RULES), STATS);
        let high = Morale::new(100).combat_stats(STATS, &RULES);
        assert_eq!((high.hp, high.speed), (110, 33));
        let low = Morale::new(0).combat_stats(STATS, &RULES);
        assert_eq!((low.hp, low.speed), (90, 27));
        assert_eq!(Morale::new(75).combat_percent(&RULES), 105);
    }

    proptest! {
        #[test]
        fn higher_morale_never_fights_worse(a in 0..=100u32, b in 0..=100u32) {
            let (low, high) = (Morale::new(a.min(b)), Morale::new(a.max(b)));
            prop_assert!(low.combat_percent(&RULES) <= high.combat_percent(&RULES));
        }
    }
}
