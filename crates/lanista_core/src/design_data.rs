//! Design data (classes, spells) read from the RON files of `data/`.

use serde::Deserialize;

use crate::combat::Range;
use crate::fighter::ClassId;

/// Everything read from `data/`. `ClassId(i)` is the class at index `i` of `classes`.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignData {
    pub classes: Vec<ClassData>,
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
    /// Reads the content of `data/classes.ron`.
    ///
    /// # Errors
    /// If the text is not valid RON or does not match the expected fields.
    pub fn from_ron(classes: &str) -> Result<Self, ron::error::SpannedError> {
        Ok(Self {
            classes: ron::from_str(classes)?,
        })
    }

    /// Reads `data/classes.ron` at run time, so balancing needs no rebuild.
    ///
    /// # Errors
    /// If the file cannot be read or parsed.
    // ponytail: path of the source tree; resolve next to the executable when
    // packaging for Steam.
    pub fn load() -> Result<Self, String> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/classes.ron");
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        Self::from_ron(&text).map_err(|e| format!("{path}: {e}"))
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

    #[test]
    fn shipped_data_loads() {
        let data = DesignData::from_ron(CLASSES).unwrap();
        assert_eq!(data.classes.len(), 2);
        for class in &data.classes {
            assert_eq!(class.spells.len(), 3, "{}", class.name);
        }
    }

    #[test]
    fn finds_a_class_by_id() {
        let data = DesignData::from_ron(CLASSES).unwrap();
        assert_eq!(data.class(ClassId(1)), data.classes.get(1));
        assert_eq!(data.class(ClassId(2)), None);
    }

    #[test]
    fn rejects_unknown_fields() {
        let typo = r#"[(name: "X", max_mana: 0, max_endurance: 0, spels: [])]"#;
        assert!(DesignData::from_ron(typo).is_err());
    }
}
