use crate::combat::Vec2;
use crate::ia::Action;

/// Index of a fighter in its arena; fighters refer to each other by ID, never by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FighterId(pub u32);

/// Index of a class in the design data (`data/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClasseId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Max hit points.
    pub pv: u32,
    /// Melee damage.
    pub ac: u32,
    /// Ranged damage (weapons and spells).
    pub al: u32,
    /// Movement speed and attack rate.
    pub vit: u32,
    /// Melee damage reduction.
    pub dc: u32,
    /// Ranged damage reduction.
    pub dl: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Jauge {
    pub actuel: u32,
    pub max: u32,
}

impl Jauge {
    #[must_use]
    pub fn pleine(max: u32) -> Self {
        Self { actuel: max, max }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Combattant {
    pub id: FighterId,
    pub classe: ClasseId,
    pub stats: Stats,
    /// Current hit points; the max is `stats.pv`.
    pub pv: u32,
    pub mana: Jauge,
    pub endurance: Jauge,
    /// Center, in meters.
    pub position: Vec2,
    /// Last decision of its AI, applied every tick until the next one.
    pub action: Action,
}

impl Combattant {
    /// A fighter at full health, mana and endurance.
    #[must_use]
    pub fn new(
        id: FighterId,
        classe: ClasseId,
        stats: Stats,
        mana_max: u32,
        endurance_max: u32,
        position: Vec2,
    ) -> Self {
        Self {
            id,
            classe,
            stats,
            pv: stats.pv,
            mana: Jauge::pleine(mana_max),
            endurance: Jauge::pleine(endurance_max),
            position,
            action: Action::Attendre,
        }
    }

    #[must_use]
    pub fn est_vivant(&self) -> bool {
        self.pv > 0
    }
}
