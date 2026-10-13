//! Design data (classes, spells, fighter rules) read from the RON files of `data/`.

use serde::Deserialize;

use crate::combat::Range;
use crate::fighter::ClassId;
use crate::fighter::care::CareRules;
use crate::fighter::fame::FameRules;
use crate::fighter::form::FormRules;
use crate::fighter::illness::IllnessRules;
use crate::fighter::injury::InjuryRules;
use crate::fighter::morale::MoraleRules;
use crate::fighter::personality::PersonalityRules;

/// Everything read from `data/`. `ClassId(i)` is the class at index `i` of `classes`.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignData {
    pub classes: Vec<ClassData>,
    pub fighter: FighterRules,
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

impl DesignData {
    /// Reads the content of `data/classes.ron` and `data/fighter.ron`.
    ///
    /// # Errors
    /// If a text is not valid RON or does not match the expected fields.
    pub fn from_ron(classes: &str, fighter: &str) -> Result<Self, ron::error::SpannedError> {
        Ok(Self {
            classes: ron::from_str(classes)?,
            fighter: ron::from_str(fighter)?,
        })
    }

    /// Reads `data/classes.ron` and `data/fighter.ron` at run time, so balancing needs no rebuild.
    ///
    /// # Errors
    /// If a file cannot be read or parsed.
    // ponytail: path of the source tree; resolve next to the executable when
    // packaging for Steam.
    pub fn load() -> Result<Self, String> {
        let read = |path: &str| std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"));
        let classes = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/classes.ron");
        let fighter = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/fighter.ron");
        Self::from_ron(&read(classes)?, &read(fighter)?)
            .map_err(|e| format!("{classes} or {fighter}: {e}"))
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

    #[test]
    fn shipped_data_loads() {
        let data = DesignData::from_ron(CLASSES, FIGHTER).unwrap();
        assert_eq!(data.classes.len(), 2);
        for class in &data.classes {
            assert_eq!(class.spells.len(), 3, "{}", class.name);
        }
    }

    #[test]
    fn finds_a_class_by_id() {
        let data = DesignData::from_ron(CLASSES, FIGHTER).unwrap();
        assert_eq!(data.class(ClassId(1)), data.classes.get(1));
        assert_eq!(data.class(ClassId(2)), None);
    }

    #[test]
    fn rejects_unknown_fields() {
        let typo = r#"[(name: "X", max_mana: 0, max_endurance: 0, spels: [])]"#;
        assert!(DesignData::from_ron(typo, FIGHTER).is_err());
    }

    #[test]
    fn shipped_form_rules_triple_risk_at_form_0() {
        let data = DesignData::from_ron(CLASSES, FIGHTER).unwrap();
        assert!((data.fighter.form.risk_factor - 2.0).abs() < f64::EPSILON);
    }
}
