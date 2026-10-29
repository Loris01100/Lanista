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
pub mod traits;

use std::sync::Arc;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::ai::Action;
use crate::combat::Vec2;
use crate::design_data::{ClassData, Cost, SpellData};
use crate::fighter::appearance::Appearance;
use crate::fighter::equipment::{OffHand, Refused};
use crate::fighter::form::Form;
use crate::fighter::illness::Illness;
use crate::fighter::injury::{Injury, Location};
use crate::fighter::morale::Morale;
use crate::fighter::personality::Personality;
use crate::learned::Model;

/// Index of a fighter in its arena; fighters refer to each other by ID, never by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent ways out of a fight: given up, fleeing, knocked out, eliminated"
)]
pub struct Fighter {
    pub id: FighterId,
    pub class: ClassId,
    pub stats: Stats,
    /// Current hit points; the max is `stats.hp`.
    pub hp: u32,
    pub mana: Gauge,
    pub endurance: Gauge,
    /// Elements of the floor it gathered and carries, none at first; it recovers none.
    pub elements: Gauge,
    /// Its class's spells, copied from the design data.
    pub spells: Vec<SpellData>,
    /// Index in `spells` of its weapon's auto attack, the last one, cast only once none
    /// of the others is both affordable and of use (`combat::can_cast_on`); none for a
    /// class without one.
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
    /// Gave up in an arena with a door and runs for it: still in the fight, and hit,
    /// until through it (`Arena::door`).
    pub fleeing: bool,
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
    /// What its loadout's off hand holds; only drawn, its effects are in `stats`.
    pub off_hand: OffHand,
    /// Percentage of its melee and ranged defenses its shield brings, in `stats`; 100
    /// without one. A spell that `ignores_shield` hits as if it had none
    /// (`Fighter::stats_against`).
    pub shield_percent: u32,
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
    /// Caught in a trap or held by a throw (`design_data::TrapData`): neither acts nor
    /// moves, but is hit, taking `damage_percent` more; the first hit taking HP frees it.
    Trapped { damage_percent: u32 },
    /// Loses `hp_percent` of its max HP each second, dealt by `source`; one at most.
    Poisoned { source: FighterId, hp_percent: u32 },
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
            elements: Gauge {
                current: 0,
                max: class_data.max_elements,
                per_second: 0,
            },
            cooldowns: vec![0; spells.len()],
            spells,
            auto_attack,
            position,
            action: Action::Wait,
            surrender_hp: 0,
            surrendered: false,
            fleeing: false,
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
            off_hand: class_data
                .loadout(loadout)
                .map_or(OffHand::Free, |l| l.off_hand),
            shield_percent: 100,
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
                Effect::Bleeding { .. } | Effect::Trapped { .. } | Effect::Poisoned { .. } => 0,
            })
            .sum();
        percent_of(self.stats.accuracy, 100 - lost.min(100))
    }

    /// Its stats as `spell` hits them: without its shield's bonus to its defenses, for
    /// a spell that ignores it (`SpellData::ignores_shield`).
    #[must_use]
    pub fn stats_against(&self, spell: &SpellData) -> Stats {
        let mut stats = self.stats;
        if spell.ignores_shield && self.shield_percent > 0 {
            let unshielded = |v: u32| {
                let v = u64::from(v) * 100 / u64::from(self.shield_percent);
                u32::try_from(v).unwrap_or(u32::MAX)
            };
            stats.melee_defense = unshielded(stats.melee_defense);
            stats.ranged_defense = unshielded(stats.ranged_defense);
        }
        stats
    }

    /// Fighting and not down: can be hit, and acts unless caught in a trap.
    #[must_use]
    pub fn is_standing(&self) -> bool {
        self.is_fighting() && self.stunned_ticks == 0
    }

    /// The percentage of damage it takes more, caught in a trap; `None` when free.
    #[must_use]
    pub fn trapped(&self) -> Option<u32> {
        self.effects.iter().find_map(|l| match l.effect {
            Effect::Trapped { damage_percent } => Some(damage_percent),
            Effect::Bleeding { .. } | Effect::Blinded { .. } | Effect::Poisoned { .. } => None,
        })
    }

    /// Standing and not caught in a trap: decides, moves and casts.
    #[must_use]
    pub fn is_free(&self) -> bool {
        self.is_standing() && self.trapped().is_none()
    }

    /// Takes `damage` hit points off, stopping at 0: a blow that reaches 0 kills.
    /// A survivor left at or below `surrender_hp` gives up, unless already on the run.
    pub fn take_damage(&mut self, damage: u32) {
        self.hp = self.hp.saturating_sub(damage);
        if self.is_alive() && !self.fleeing && self.hp <= self.surrender_hp {
            self.surrendered = true;
        }
    }

    /// Whether spell `spell` exists, is off cooldown and is affordable; the auto attack
    /// only once none of the other spells is affordable, a last resort. Out of a fight's
    /// context: `combat::can_cast_on` also lets it strike when no other is of use.
    #[must_use]
    pub fn can_cast(&self, spell: usize) -> bool {
        self.can_cast_when(spell, |_| true)
    }

    /// `can_cast`, where a spell that is not `of_use` leaves the auto attack free as if
    /// it were too dear.
    #[must_use]
    pub fn can_cast_when(&self, spell: usize, of_use: impl Fn(usize) -> bool) -> bool {
        let Some(data) = self.spells.get(spell) else {
            return false;
        };
        let last_resort = || {
            (0..self.spells.len())
                .filter(|&s| Some(s) != self.auto_attack)
                .all(|s| !self.can_afford(s) || !of_use(s))
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
        let Cost {
            mana,
            endurance,
            elements,
        } = data.cost;
        self.mana.current >= mana
            && self.endurance.current >= endurance
            && self.elements.current >= elements
    }

    /// Spends the cost of spell `spell` and starts its cooldown.
    /// Call only after `can_cast`.
    pub fn spend(&mut self, spell: usize) {
        let data = &self.spells[spell];
        self.mana.current = self.mana.current.saturating_sub(data.cost.mana);
        self.endurance.current = self.endurance.current.saturating_sub(data.cost.endurance);
        self.elements.current = self.elements.current.saturating_sub(data.cost.elements);
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
        max_elements: 0,
        spells: vec![SpellData {
            name: "Coup".into(),
            range: Range::Melee,
            reach: 1.0,
            power: 100,
            cooldown: 30,
            cost: Cost {
                endurance: 10,
                ..Cost::default()
            },
            damage_kind: DamageKind::Blunt,
            accuracy_percent: 100,
            sure_cut_hits: None,
            bleed_percent: 0,
            accuracy_drop: None,
            gathers: 0,
            thrown: std::collections::BTreeMap::new(),
            mine: false,
            blows: 1,
            sure_hit: false,
            over_obstacles: false,
            area: 0.0,
            trap: None,
            poison: None,
            hold: None,
            ignores_shield: false,
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

/// [`test_fighter`] carrying 4 elements at most, with two more spells: 1 gathers 3 for
/// 5 endurance, 2 throws them 8 m for 5 endurance and 2 elements, a blunt power 200
/// with stone, power 50 and 20 % accuracy lost for 2 s with sand (set by
/// `Arena::with_layout`).
#[cfg(test)]
pub(crate) fn test_thrower(id: u32, x: f64, y: f64) -> Fighter {
    use crate::combat::{DamageKind, Element, Range};
    use crate::design_data::{AccuracyDrop, Thrown};

    let mut f = test_fighter(id, x, y);
    f.elements.max = 4;
    let strike = f.spells[0].clone();
    let endurance = Cost {
        endurance: 5,
        ..Cost::default()
    };
    f.spells.push(SpellData {
        name: "Récolte".into(),
        power: 0,
        cost: endurance,
        gathers: 3,
        ..strike.clone()
    });
    let thrown = |power, damage_kind, accuracy_drop| Thrown {
        power,
        damage_kind,
        accuracy_drop,
        bleed_percent: 0,
        poison: None,
        hold: None,
        ignores_shield: false,
    };
    let sand = AccuracyDrop {
        percent: 20,
        ticks: 60,
    };
    f.spells.push(SpellData {
        name: "Jette".into(),
        range: Range::Ranged,
        reach: 8.0,
        power: 0,
        cost: Cost {
            elements: 2,
            ..endurance
        },
        thrown: [
            (Element::Stone, thrown(200, DamageKind::Blunt, None)),
            (Element::Sand, thrown(50, DamageKind::Neutral, Some(sand))),
        ]
        .into(),
        ..strike
    });
    f.cooldowns = vec![0; f.spells.len()];
    f
}
