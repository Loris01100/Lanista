use crate::ai::Action;
use crate::combat::Vec2;

/// Index of a fighter in its arena; fighters refer to each other by ID, never by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FighterId(pub u32);

/// Index of a class in the design data (`data/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClassId(pub u32);

/// The six stats of the design plan; its French abbreviation is given for each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Center, in meters.
    pub position: Vec2,
    /// Last decision of its AI, applied every tick until the next one.
    pub action: Action,
}

impl Fighter {
    /// A fighter at full health, mana and endurance.
    #[must_use]
    pub fn new(
        id: FighterId,
        class: ClassId,
        stats: Stats,
        max_mana: u32,
        max_endurance: u32,
        position: Vec2,
    ) -> Self {
        Self {
            id,
            class,
            stats,
            hp: stats.hp,
            mana: Gauge::full(max_mana),
            endurance: Gauge::full(max_endurance),
            position,
            action: Action::Wait,
        }
    }

    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }

    /// Takes `damage` hit points off, stopping at 0.
    pub fn take_damage(&mut self, damage: u32) {
        self.hp = self.hp.saturating_sub(damage);
    }
}
