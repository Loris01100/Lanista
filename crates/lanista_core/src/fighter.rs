pub mod appearance;
pub mod care;
pub mod career;
pub mod equipment;
pub mod fame;
pub mod form;
pub mod illness;
pub mod injury;
pub mod melee;
pub mod morale;
pub mod personality;
pub mod progression;
pub mod retirement;

use std::sync::Arc;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::ai::Action;
use crate::combat::Vec2;
use crate::design_data::{ClassData, Cost, SpellData};
use crate::fighter::appearance::Appearance;
use crate::fighter::equipment::Refused;
use crate::fighter::form::Form;
use crate::fighter::illness::Illness;
use crate::fighter::injury::{Injury, Location};
use crate::fighter::morale::Morale;
use crate::fighter::personality::Personality;
use crate::learned::Model;

/// Index of a fighter in its arena; fighters refer to each other by ID, never by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FighterId(pub u32);

/// Index of a class in the design data (`data/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ClassId(pub u32);

/// Number of stats in [`Stats`].
pub const STAT_COUNT: usize = 7;

/// The stats of the design plan; its French abbreviation is given for each.
/// Read from `data/` as percentages (injury and illness maluses), where a missing stat is 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Stats {
    /// PV: max hit points.
    pub hp: u32,
    /// AC: melee damage.
    pub melee_attack: u32,
    /// AL: ranged damage (weapons and spells).
    pub ranged_attack: u32,
    /// PRÉ: chance to hit, against the target's speed (see `combat::hit_percent`).
    pub accuracy: u32,
    /// VIT: movement speed, attack rate and dodging.
    pub speed: u32,
    /// DC: melee damage reduction.
    pub melee_defense: u32,
    /// DL: ranged damage reduction.
    pub ranged_defense: u32,
}

impl Stats {
    /// The stats in declaration order: hp, melee attack, ranged attack, accuracy, speed,
    /// melee defense, ranged defense.
    #[must_use]
    pub fn to_array(self) -> [u32; STAT_COUNT] {
        [
            self.hp,
            self.melee_attack,
            self.ranged_attack,
            self.accuracy,
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
            accuracy,
            speed,
            melee_defense,
            ranged_defense,
        ]: [u32; STAT_COUNT],
    ) -> Self {
        Self {
            hp,
            melee_attack,
            ranged_attack,
            accuracy,
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
            accuracy: f(self.accuracy, other.accuracy),
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
    /// Points recovered each second of the fight, up to `max`.
    pub per_second: u32,
}

impl Gauge {
    #[must_use]
    pub fn full(max: u32, per_second: u32) -> Self {
        Self {
            current: max,
            max,
            per_second,
        }
    }

    /// One second's recovery.
    pub fn recover(&mut self) {
        self.current = self.current.saturating_add(self.per_second).min(self.max);
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
    /// Index in `spells` of its weapon's auto attack, the last one, cast only once it can
    /// pay for none of the others (`can_cast`); none for a class without one.
    pub auto_attack: Option<usize>,
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
    /// Ticks left down, stunned: still in the fight, but neither acting nor hit; 0 standing.
    pub stunned_ticks: u32,
    /// Times stunned in this fight.
    pub stuns: u32,
    /// Did not get up after a stun: out of the fight, alive.
    pub knocked_out: bool,
    /// What a cutting hit severed: its head, dead, or a limb, out of the fight alive.
    pub severed: Option<Location>,
    /// Hits taken from spells that sever for sure after some (`SpellData::sure_cut_hits`).
    pub cleaving_hits: u32,
    /// What lasts on it for a while: bleeding parts, lost accuracy.
    pub effects: Vec<Lasting>,
    /// Dealt too little damage before a melee's check: out of the fight alive, a defeat
    /// (see `melee::QuotaRules`).
    pub eliminated: bool,
    /// Damage dealt since the melee's last check.
    pub round_damage: u32,
    /// The fighter's state outside the fight, read by the AI so that an injured or
    /// fearful fighter fights differently. `stats` already include its effects.
    pub condition: Condition,
    /// Equipment it fights without; `stats` and its spells already include the effects.
    pub refused: Refused,
    /// How it is drawn; no effect on the fight.
    pub appearance: Appearance,
    /// The learned AI that plays it; the utility AI without one (see `duel::ais`).
    pub model: Option<Arc<Model>>,
}

/// What a hit leaves on a fighter for a while (see `combat::wear_off`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// `part` loses HP each second, dealt by `source`; one per part at most.
    Bleeding { part: Location, source: FighterId },
    /// Its attacks hit less often: `percent` of its accuracy lost, adding up.
    Blinded { percent: u32 },
}

/// An effect and the ticks it lasts still.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lasting {
    pub effect: Effect,
    pub ticks_left: u32,
}

/// Everything about a fighter's state that its combat AI reads, beyond HP and resources.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub personality: Personality,
    pub morale: Morale,
    pub form: Form,
    pub injuries: Vec<Injury>,
    pub illness: Option<Illness>,
}

impl Default for Condition {
    /// Neutral personality and morale, full form, healthy: the AI behaves as with no condition.
    fn default() -> Self {
        Self {
            personality: Personality::neutral(),
            morale: Morale::neutral(),
            form: Form::full(),
            injuries: Vec::new(),
            illness: None,
        }
    }
}

impl Fighter {
    /// A fighter at full health, mana and endurance, all spells ready, who fights to the death,
    /// in a neutral condition, in its class's first loadout.
    #[must_use]
    pub fn new(
        id: FighterId,
        class: ClassId,
        class_data: &ClassData,
        stats: Stats,
        position: Vec2,
    ) -> Self {
        Self::in_loadout(id, class, (class_data, 0), stats, position)
    }

    /// A fighter of `class` in loadout `loadout` of `class_data`, the first one if it has
    /// no such loadout: the loadout's spell in the second slot, its weapon's auto attack
    /// after the others.
    #[must_use]
    pub fn in_loadout(
        id: FighterId,
        class: ClassId,
        (class_data, loadout): (&ClassData, usize),
        stats: Stats,
        position: Vec2,
    ) -> Self {
        let mut spells = class_data.spells_in(loadout);
        let attack = class_data
            .loadout(loadout)
            .and_then(|l| l.auto_attack.clone());
        let auto_attack = attack.map(|attack| {
            spells.push(attack);
            spells.len() - 1
        });
        Self {
            id,
            class,
            stats,
            hp: stats.hp,
            mana: Gauge::full(class_data.max_mana, class_data.mana_per_second),
            endurance: Gauge::full(class_data.max_endurance, class_data.endurance_per_second),
            cooldowns: vec![0; spells.len()],
            spells,
            auto_attack,
            position,
            action: Action::Wait,
            surrender_hp: 0,
            surrendered: false,
            stunned_ticks: 0,
            stuns: 0,
            knocked_out: false,
            severed: None,
            cleaving_hits: 0,
            effects: Vec::new(),
            eliminated: false,
            round_damage: 0,
            condition: Condition::default(),
            refused: Refused::NONE,
            appearance: Appearance::default(),
            model: None,
        }
    }

    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }

    /// Alive, has not given up, was not knocked out nor eliminated and has all its
    /// limbs; maybe down for now.
    #[must_use]
    pub fn is_fighting(&self) -> bool {
        self.is_alive()
            && !self.surrendered
            && !self.knocked_out
            && !self.eliminated
            && self.severed.is_none()
    }

    /// Whether it no longer has `part`: cut off in this fight, or a limb lost before.
    #[must_use]
    pub fn has_lost(&self, part: Location) -> bool {
        self.severed == Some(part) || injury::has_lost(&self.condition.injuries, part)
    }

    /// Whether `part` is bleeding.
    #[must_use]
    pub fn bleeds(&self, part: Location) -> bool {
        self.effects
            .iter()
            .any(|l| matches!(l.effect, Effect::Bleeding { part: p, .. } if p == part))
    }

    /// Its accuracy, less what it lost for now (`Effect::Blinded`), down to 0.
    #[must_use]
    pub fn accuracy(&self) -> u32 {
        let lost: u32 = self
            .effects
            .iter()
            .map(|l| match l.effect {
                Effect::Blinded { percent } => percent,
                Effect::Bleeding { .. } => 0,
            })
            .sum();
        percent_of(self.stats.accuracy, 100 - lost.min(100))
    }

    /// Fighting and not down: acts, and can be hit.
    #[must_use]
    pub fn is_standing(&self) -> bool {
        self.is_fighting() && self.stunned_ticks == 0
    }

    /// Takes `damage` hit points off, stopping at 0: a blow that reaches 0 kills.
    /// A survivor left at or below `surrender_hp` gives up.
    pub fn take_damage(&mut self, damage: u32) {
        self.hp = self.hp.saturating_sub(damage);
        if self.is_alive() && self.hp <= self.surrender_hp {
            self.surrendered = true;
        }
    }

    /// Whether spell `spell` exists, is off cooldown and is affordable; the auto attack
    /// only once none of the other spells is affordable, a last resort.
    #[must_use]
    pub fn can_cast(&self, spell: usize) -> bool {
        let Some(data) = self.spells.get(spell) else {
            return false;
        };
        let last_resort = || {
            (0..self.spells.len())
                .filter(|&s| Some(s) != self.auto_attack)
                .all(|s| !self.can_afford(s))
        };
        self.cooldowns[spell] == 0
            && self.can_afford_spell(data)
            && (Some(spell) != self.auto_attack || last_resort())
    }

    /// Whether spell `spell` exists and its cost can be paid now, cooldown aside.
    #[must_use]
    pub fn can_afford(&self, spell: usize) -> bool {
        self.spells
            .get(spell)
            .is_some_and(|data| self.can_afford_spell(data))
    }

    fn can_afford_spell(&self, data: &SpellData) -> bool {
        match data.cost {
            Cost::Mana(n) => self.mana.current >= n,
            Cost::Endurance(n) => self.endurance.current >= n,
        }
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

/// A fighter for tests: 100 HP, 10 in every other stat but accuracy and speed 30,
/// and one blunt melee spell (reach 1 m, power 100, cooldown 30, 10 endurance out of 50,
/// none recovered).
#[cfg(test)]
pub(crate) fn test_fighter(id: u32, x: f64, y: f64) -> Fighter {
    use crate::combat::{DamageKind, Range};

    let class = ClassData {
        key: "test".into(),
        max_mana: 50,
        max_endurance: 50,
        mana_per_second: 0,
        endurance_per_second: 0,
        spells: vec![SpellData {
            name: "Coup".into(),
            range: Range::Melee,
            reach: 1.0,
            power: 100,
            cooldown: 30,
            cost: Cost::Endurance(10),
            damage_kind: DamageKind::Blunt,
            accuracy_percent: 100,
            sure_cut_hits: None,
            bleed_percent: 0,
            accuracy_drop: None,
        }],
        loadouts: Vec::new(),
    };
    let stats = Stats {
        hp: 100,
        melee_attack: 10,
        ranged_attack: 10,
        accuracy: 30,
        speed: 30,
        melee_defense: 10,
        ranged_defense: 10,
    };
    Fighter::new(FighterId(id), ClassId(0), &class, stats, Vec2::new(x, y))
}
