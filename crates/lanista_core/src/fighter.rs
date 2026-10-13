pub mod care;
pub mod fame;
pub mod form;
pub mod illness;
pub mod injury;
pub mod morale;
pub mod personality;
pub mod progression;
pub mod retirement;

use rand::{Rng, RngExt};
use serde::Deserialize;

use crate::ai::Action;
use crate::combat::Vec2;
use crate::design_data::{ClassData, Cost, SpellData};

/// Index of a fighter in its arena; fighters refer to each other by ID, never by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FighterId(pub u32);

/// Index of a class in the design data (`data/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClassId(pub u32);

/// The six stats of the design plan; its French abbreviation is given for each.
/// Read from `data/` as percentages (injury and illness maluses), where a missing stat is 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Stats {
    /// PV: max hit points.
    pub hp: u32,
    /// AC: melee damage.
    pub melee_attack: u32,
    /// AL: ranged damage (weapons and spells).
    pub ranged_attack: u32,
    /// VIT: movement speed and attack rate.
    pub speed: u32,
    /// DC: melee damage reduction.
    pub melee_defense: u32,
    /// DL: ranged damage reduction.
    pub ranged_defense: u32,
}

impl Stats {
    /// The stats in declaration order: hp, melee attack, ranged attack, speed,
    /// melee defense, ranged defense.
    #[must_use]
    pub fn to_array(self) -> [u32; 6] {
        [
            self.hp,
            self.melee_attack,
            self.ranged_attack,
            self.speed,
            self.melee_defense,
            self.ranged_defense,
        ]
    }

    /// The inverse of [`Stats::to_array`].
    #[must_use]
    pub fn from_array(
        [
            hp,
            melee_attack,
            ranged_attack,
            speed,
            melee_defense,
            ranged_defense,
        ]: [u32; 6],
    ) -> Self {
        Self {
            hp,
            melee_attack,
            ranged_attack,
            speed,
            melee_defense,
            ranged_defense,
        }
    }

    /// `f` applied to every stat.
    #[must_use]
    pub fn map(self, f: impl Fn(u32) -> u32) -> Self {
        self.zip(self, |value, _| f(value))
    }

    /// `f` applied to each stat of `self` and the same stat of `other`.
    #[must_use]
    pub fn zip(self, other: Self, f: impl Fn(u32, u32) -> u32) -> Self {
        Self {
            hp: f(self.hp, other.hp),
            melee_attack: f(self.melee_attack, other.melee_attack),
            ranged_attack: f(self.ranged_attack, other.ranged_attack),
            speed: f(self.speed, other.speed),
            melee_defense: f(self.melee_defense, other.melee_defense),
            ranged_defense: f(self.ranged_defense, other.ranged_defense),
        }
    }
}

/// `percent` % of `value`, rounded down, in integers so every platform agrees.
pub(crate) fn percent_of(value: u32, percent: u32) -> u32 {
    u32::try_from(u64::from(value) * u64::from(percent) / 100).unwrap_or(u32::MAX)
}

/// A value drawn among `choices` with a chance proportional to its weight.
/// Falls back to the last choice when all weights are 0.
pub(crate) fn pick<R: Rng + ?Sized, T: Copy>(rng: &mut R, choices: &[(u32, T)]) -> T {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gauge {
    pub current: u32,
    pub max: u32,
}

impl Gauge {
    #[must_use]
    pub fn full(max: u32) -> Self {
        Self { current: max, max }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fighter {
    pub id: FighterId,
    pub class: ClassId,
    pub stats: Stats,
    /// Current hit points; the max is `stats.hp`.
    pub hp: u32,
    pub mana: Gauge,
    pub endurance: Gauge,
    /// Its class's spells, copied from the design data.
    pub spells: Vec<SpellData>,
    /// Ticks left before each spell of `spells` can be cast again; 0 when ready.
    pub cooldowns: Vec<u32>,
    /// Center, in meters.
    pub position: Vec2,
    /// Last decision of its AI, applied every tick until the next one.
    pub action: Action,
    /// Hit points at or below which it gives up (see `Personality::surrender_hp`);
    /// 0 fights to the death.
    pub surrender_hp: u32,
    /// Gave up: out of the fight, alive.
    pub surrendered: bool,
}

impl Fighter {
    /// A fighter at full health, mana and endurance, all spells ready, who fights to the death.
    #[must_use]
    pub fn new(
        id: FighterId,
        class: ClassId,
        class_data: &ClassData,
        stats: Stats,
        position: Vec2,
    ) -> Self {
        Self {
            id,
            class,
            stats,
            hp: stats.hp,
            mana: Gauge::full(class_data.max_mana),
            endurance: Gauge::full(class_data.max_endurance),
            spells: class_data.spells.clone(),
            cooldowns: vec![0; class_data.spells.len()],
            position,
            action: Action::Wait,
            surrender_hp: 0,
            surrendered: false,
        }
    }

    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }

    /// Alive and has not given up.
    #[must_use]
    pub fn is_fighting(&self) -> bool {
        self.is_alive() && !self.surrendered
    }

    /// Takes `damage` hit points off, stopping at 0: a blow that reaches 0 kills.
    /// A survivor left at or below `surrender_hp` gives up.
    pub fn take_damage(&mut self, damage: u32) {
        self.hp = self.hp.saturating_sub(damage);
        if self.is_alive() && self.hp <= self.surrender_hp {
            self.surrendered = true;
        }
    }

    /// Whether spell `spell` exists, is off cooldown and is affordable.
    #[must_use]
    pub fn can_cast(&self, spell: usize) -> bool {
        let Some(data) = self.spells.get(spell) else {
            return false;
        };
        let affordable = match data.cost {
            Cost::Mana(n) => self.mana.current >= n,
            Cost::Endurance(n) => self.endurance.current >= n,
        };
        self.cooldowns[spell] == 0 && affordable
    }

    /// Spends the cost of spell `spell` and starts its cooldown.
    /// Call only after `can_cast`.
    pub fn spend(&mut self, spell: usize) {
        let data = &self.spells[spell];
        match data.cost {
            Cost::Mana(n) => self.mana.current = self.mana.current.saturating_sub(n),
            Cost::Endurance(n) => self.endurance.current = self.endurance.current.saturating_sub(n),
        }
        self.cooldowns[spell] = data.cooldown;
    }
}

/// A fighter for tests: 100 HP, 10 in every other stat but speed 30,
/// and one melee spell (reach 1 m, power 100, cooldown 30, 10 endurance out of 50).
#[cfg(test)]
pub(crate) fn test_fighter(id: u32, x: f64, y: f64) -> Fighter {
    use crate::combat::Range;

    let class = ClassData {
        name: "Test".into(),
        max_mana: 50,
        max_endurance: 50,
        spells: vec![SpellData {
            name: "Coup".into(),
            range: Range::Melee,
            reach: 1.0,
            power: 100,
            cooldown: 30,
            cost: Cost::Endurance(10),
        }],
    };
    let stats = Stats {
        hp: 100,
        melee_attack: 10,
        ranged_attack: 10,
        speed: 30,
        melee_defense: 10,
        ranged_defense: 10,
    };
    Fighter::new(FighterId(id), ClassId(0), &class, stats, Vec2::new(x, y))
}
