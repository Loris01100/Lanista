//! Design data (classes, spells, fighter rules, calendar, economy, colosseums) read from the RON files of `data/`.

use serde::Deserialize;

use crate::calendar::CalendarRules;
use crate::colosseum::WorldRules;
use crate::combat::Range;
use crate::economy::EconomyRules;
use crate::fighter::ClassId;
use crate::fighter::care::CareRules;
use crate::fighter::career::CareerRules;
use crate::fighter::fame::FameRules;
use crate::fighter::form::FormRules;
use crate::fighter::illness::IllnessRules;
use crate::fighter::injury::InjuryRules;
use crate::fighter::morale::MoraleRules;
use crate::fighter::personality::PersonalityRules;
use crate::fighter::progression::ProgressionRules;
use crate::fighter::retirement::RetirementRules;

/// Everything read from `data/`. `ClassId(i)` is the class at index `i` of `classes`.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignData {
    pub classes: Vec<ClassData>,
    pub fighter: FighterRules,
    pub calendar: CalendarRules,
    pub economy: EconomyRules,
    pub world: WorldRules,
}

/// Rules of the fighter's physical state, from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FighterRules {
    pub form: FormRules,
    pub injury: InjuryRules,
    pub illness: IllnessRules,
    pub care: CareRules,
    pub personality: PersonalityRules,
    pub morale: MoraleRules,
    pub fame: FameRules,
    pub progression: ProgressionRules,
    pub retirement: RetirementRules,
    pub career: CareerRules,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassData {
    /// Shown to the player.
    pub name: String,
    pub max_mana: u32,
    pub max_endurance: u32,
    pub spells: Vec<SpellData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellData {
    /// Shown to the player.
    pub name: String,
    /// Melee uses melee attack against melee defense, ranged the ranged ones.
    pub range: Range,
    /// Farthest distance between the two fighters' edges, in meters.
    pub reach: f64,
    /// Damage, as a percentage of the `damage` formula's result.
    pub power: u32,
    /// Ticks before the spell can be cast again (30 ticks per second).
    pub cooldown: u32,
    pub cost: Cost,
}

/// Resource spent to cast a spell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Cost {
    Mana(u32),
    Endurance(u32),
}

const CLASSES_FILE: &str = "data/classes.ron";
const FIGHTER_FILE: &str = "data/fighter.ron";
const CALENDAR_FILE: &str = "data/calendar.ron";
const ECONOMY_FILE: &str = "data/economy.ron";
const WORLD_FILE: &str = "data/world.ron";

/// Why the design data could not be loaded; `file` is the file at fault.
#[derive(Debug)]
pub enum LoadError {
    Io {
        file: &'static str,
        source: std::io::Error,
    },
    /// Not valid RON, or not the expected fields.
    Ron {
        file: &'static str,
        source: ron::error::SpannedError,
    },
    /// Parsed, but a value would break the game, e.g. a range whose lowest is above its highest.
    Invalid {
        file: &'static str,
        field: &'static str,
        problem: &'static str,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { file, source } => write!(f, "{file}: {source}"),
            Self::Ron { file, source } => write!(f, "{file}: {source}"),
            Self::Invalid {
                file,
                field,
                problem,
            } => write!(f, "{file}: {field}: {problem}"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Ron { source, .. } => Some(source),
            Self::Invalid { .. } => None,
        }
    }
}

impl DesignData {
    /// Reads the content of `data/classes.ron`, `data/fighter.ron`, `data/calendar.ron`,
    /// `data/economy.ron` and `data/world.ron`.
    ///
    /// # Errors
    /// If a text is not valid RON or does not match the expected fields, or if a value
    /// would make the game panic later (no class, a range upside down, a chance outside 0 to 1,
    /// a season of no week, no free colosseum).
    pub fn from_ron(
        classes: &str,
        fighter: &str,
        calendar: &str,
        economy: &str,
        world: &str,
    ) -> Result<Self, LoadError> {
        let data = Self {
            classes: ron::from_str(classes).map_err(|source| LoadError::Ron {
                file: CLASSES_FILE,
                source,
            })?,
            fighter: ron::from_str(fighter).map_err(|source| LoadError::Ron {
                file: FIGHTER_FILE,
                source,
            })?,
            calendar: ron::from_str(calendar).map_err(|source| LoadError::Ron {
                file: CALENDAR_FILE,
                source,
            })?,
            economy: ron::from_str(economy).map_err(|source| LoadError::Ron {
                file: ECONOMY_FILE,
                source,
            })?,
            world: ron::from_str(world).map_err(|source| LoadError::Ron {
                file: WORLD_FILE,
                source,
            })?,
        };
        data.validate()?;
        Ok(data)
    }

    /// Reads the files of `data/` at run time, so balancing needs no rebuild.
    ///
    /// # Errors
    /// If a file cannot be read, or [`DesignData::from_ron`] rejects it.
    // ponytail: path of the source tree; resolve next to the executable when
    // packaging for Steam.
    pub fn load() -> Result<Self, LoadError> {
        let read = |file: &'static str| {
            let path = format!("{}/../../{file}", env!("CARGO_MANIFEST_DIR"));
            std::fs::read_to_string(path).map_err(|source| LoadError::Io { file, source })
        };
        Self::from_ron(
            &read(CLASSES_FILE)?,
            &read(FIGHTER_FILE)?,
            &read(CALENDAR_FILE)?,
            &read(ECONOMY_FILE)?,
            &read(WORLD_FILE)?,
        )
    }

    /// Rejects the values the rest of the game would panic on.
    fn validate(&self) -> Result<(), LoadError> {
        if self.classes.is_empty() {
            return Err(LoadError::Invalid {
                file: CLASSES_FILE,
                field: "classes",
                problem: "no class",
            });
        }
        if self.economy.max_fighters == 0 {
            return Err(LoadError::Invalid {
                file: ECONOMY_FILE,
                field: "max_fighters",
                problem: "a club of no fighter",
            });
        }
        let world = |field, problem| LoadError::Invalid {
            file: WORLD_FILE,
            field,
            problem,
        };
        if !self
            .world
            .colosseums
            .iter()
            .any(|c| c.purses.entry_fee == 0)
        {
            return Err(world("colosseums", "none free to enter"));
        }
        if self.world.colosseums.iter().any(|c| c.residents == 0) {
            return Err(world("residents", "a colosseum without opponents"));
        }
        if self.world.rivals.is_empty() {
            return Err(world("rivals", "no rival club to own the residents"));
        }
        if self.world.roster.max < self.world.roster.wanted {
            return Err(world("roster", "fewer places than fighters wanted"));
        }
        let market = &self.economy.market;
        if market.max_free_agents < market.min_free_agents {
            return Err(LoadError::Invalid {
                file: ECONOMY_FILE,
                field: "market",
                problem: "fewer free agents at most than at least",
            });
        }
        if self.calendar.season_weeks() == 0 {
            return Err(LoadError::Invalid {
                file: CALENDAR_FILE,
                field: "season",
                problem: "no week",
            });
        }
        let rules = &self.fighter;
        let ranges = [
            ("career.start_age", rules.career.start_age),
            (
                "injury.healing_weeks.light",
                rules.injury.healing_weeks.light,
            ),
            (
                "injury.healing_weeks.medium",
                rules.injury.healing_weeks.medium,
            ),
            (
                "injury.healing_weeks.severe",
                rules.injury.healing_weeks.severe,
            ),
            ("illness.weeks.mild", rules.illness.weeks.mild),
            ("illness.weeks.moderate", rules.illness.weeks.moderate),
            ("illness.weeks.severe", rules.illness.weeks.severe),
            ("illness.worsening_weeks", rules.illness.worsening_weeks),
            (
                "progression.potential_headroom_percent",
                rules.progression.potential_headroom_percent,
            ),
        ];
        let chances = [
            ("injury.chance_per_fight", rules.injury.chance_per_fight),
            ("illness.chance_per_week", rules.illness.chance_per_week),
            ("illness.worsening_chance", rules.illness.worsening_chance),
        ];
        let invalid = |field, problem| LoadError::Invalid {
            file: FIGHTER_FILE,
            field,
            problem,
        };
        if let Some(&(field, _)) = ranges.iter().find(|(_, (min, max))| min > max) {
            return Err(invalid(field, "lowest above highest"));
        }
        if let Some(&(field, _)) = chances
            .iter()
            .find(|(_, chance)| !(0.0..=1.0).contains(chance))
        {
            return Err(invalid(field, "not a chance from 0 to 1"));
        }
        // A negative factor would turn a tired fighter's chances negative.
        if rules.form.risk_factor.is_nan() || rules.form.risk_factor < 0.0 {
            return Err(invalid("form.risk_factor", "below 0"));
        }
        Ok(())
    }

    #[must_use]
    pub fn class(&self, id: ClassId) -> Option<&ClassData> {
        self.classes.get(usize::try_from(id.0).ok()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSES: &str = include_str!("../../../data/classes.ron");
    const FIGHTER: &str = include_str!("../../../data/fighter.ron");
    const CALENDAR: &str = include_str!("../../../data/calendar.ron");
    const ECONOMY: &str = include_str!("../../../data/economy.ron");
    const WORLD: &str = include_str!("../../../data/world.ron");

    #[test]
    fn shipped_data_loads() {
        let data = DesignData::from_ron(CLASSES, FIGHTER, CALENDAR, ECONOMY, WORLD).unwrap();
        assert_eq!(data.classes.len(), 2);
        for class in &data.classes {
            assert_eq!(class.spells.len(), 3, "{}", class.name);
        }
        assert_eq!(data.calendar.season_weeks(), 30);
        // Two colosseums, the first free to enter: the safety net.
        assert_eq!(data.world.colosseums.len(), 2);
        assert_eq!(data.world.colosseums[0].purses.entry_fee, 0);
    }

    #[test]
    fn finds_a_class_by_id() {
        let data = DesignData::from_ron(CLASSES, FIGHTER, CALENDAR, ECONOMY, WORLD).unwrap();
        assert_eq!(data.class(ClassId(1)), data.classes.get(1));
        assert_eq!(data.class(ClassId(2)), None);
    }

    #[test]
    fn rejects_unknown_fields() {
        let typo = r#"[(name: "X", max_mana: 0, max_endurance: 0, spels: [])]"#;
        assert!(matches!(
            DesignData::from_ron(typo, FIGHTER, CALENDAR, ECONOMY, WORLD),
            Err(LoadError::Ron {
                file: CLASSES_FILE,
                ..
            })
        ));
    }

    #[test]
    fn rejects_values_the_game_would_panic_on() {
        let field = |classes: &str, fighter: &str| match DesignData::from_ron(
            classes, fighter, CALENDAR, ECONOMY, WORLD,
        ) {
            Err(LoadError::Invalid { field, .. }) => field,
            other => panic!("expected an invalid field, got {other:?}"),
        };
        assert_eq!(field("[]", FIGHTER), "classes");
        let upside_down = FIGHTER.replacen("start_age: (20, 30)", "start_age: (30, 20)", 1);
        assert_eq!(field(CLASSES, &upside_down), "career.start_age");
        let not_a_chance = FIGHTER.replacen("worsening_chance: 0.5", "worsening_chance: 1.5", 1);
        assert_eq!(field(CLASSES, &not_a_chance), "illness.worsening_chance");
        let no_week = "(competition_weeks: 0, great_event_weeks: 0, off_season_weeks: 0)";
        assert!(matches!(
            DesignData::from_ron(CLASSES, FIGHTER, no_week, ECONOMY, WORLD),
            Err(LoadError::Invalid {
                field: "season",
                ..
            })
        ));
        let not_free = WORLD.replacen("entry_fee: 0", "entry_fee: 5", 1);
        assert!(matches!(
            DesignData::from_ron(CLASSES, FIGHTER, CALENDAR, ECONOMY, &not_free),
            Err(LoadError::Invalid {
                field: "colosseums",
                ..
            })
        ));
    }

    #[test]
    fn shipped_form_rules_triple_risk_at_form_0() {
        let data = DesignData::from_ron(CLASSES, FIGHTER, CALENDAR, ECONOMY, WORLD).unwrap();
        assert!((data.fighter.form.risk_factor - 2.0).abs() < f64::EPSILON);
    }
}
