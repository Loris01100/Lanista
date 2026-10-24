//! How a fighter looks: drawn at random when it is generated, so that 20 fighters in a
//! melee can be told apart, and kept for life. The game builds it from layers drawn in
//! light grey and tinted (see `docs/Formats_graphiques.md`); `lanista_core` only keeps
//! which shape and which tints.

use std::collections::BTreeMap;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::fighter::equipment::OffHand;

/// A fighter's look.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    /// Built from the layers of `AppearanceRules::build`: indices of its hair shape in
    /// `AppearanceRules::hair` and of its tints in `skins`, `hair_tints` and `tunics`.
    Layered {
        hair: u32,
        skin: u32,
        hair_tint: u32,
        tunic: u32,
    },
    /// A unique character, drawn whole: `assets/uniques/<name>.png`.
    Unique(String),
}

impl Default for Appearance {
    /// The first shape and tints.
    fn default() -> Self {
        Self::Layered {
            hair: 0,
            skin: 0,
            hair_tint: 0,
            tunic: 0,
        }
    }
}

impl Appearance {
    /// A layered look with a shape and tints drawn among `rules`' (the first of a list
    /// left empty).
    pub fn random<R: Rng + ?Sized>(rng: &mut R, rules: &AppearanceRules) -> Self {
        let mut index = |count: usize| {
            let count = u32::try_from(count).unwrap_or(u32::MAX);
            if count > 1 {
                rng.random_range(0..count)
            } else {
                0
            }
        };
        Self::Layered {
            hair: index(rules.hair.len()),
            skin: index(rules.skins.len()),
            hair_tint: index(rules.hair_tints.len()),
            tunic: index(rules.tunics.len()),
        }
    }
}

/// A tint, in RGB from 0 to 255, that the game multiplies into a layer drawn in grey.
pub type Tint = (u8, u8, u8);

/// What a layer is multiplied by: one of the fighter's tints, or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Paint {
    Skin,
    /// The hair tint; the layer is drawn from the fighter's hair shape.
    Hair,
    Tunic,
    /// Drawn in its own colours (metal, leather, wood).
    Plain,
}

/// The part of the body a layer goes with, seen from the fighter facing right: it is
/// not drawn once that part is lost, and a `Head` layer is one image that follows the
/// head instead of a spritesheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Part {
    Body,
    Head,
    ArmFront,
    ArmBack,
    LegFront,
    LegBack,
}

/// One layer of a look: `assets/fighters/<name>/<variant>.png` and its optional
/// `_detail`, drawn in its paint, only with the off hand `with` if there is one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerRule {
    pub name: String,
    pub paint: Paint,
    pub part: Part,
    #[serde(default)]
    pub with: Option<OffHand>,
}

impl LayerRule {
    /// Whether a fighter with `off_hand` has this layer.
    #[must_use]
    pub fn drawn_with(&self, off_hand: OffHand) -> bool {
        self.with.is_none_or(|with| with == off_hand)
    }
}

/// The shapes and tints of the layered looks, and how the head follows the body, from
/// `data/fighter.ron`. `lanista_core` only draws indices among them; the game draws them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceRules {
    /// The variant of the layers of a class without its own look:
    /// `assets/fighters/<layer>/<build>.png`.
    pub build: String,
    /// The layers of a class without its own look, in drawing order.
    pub layers: Vec<LayerRule>,
    /// The layers of each class with its own drawings, by class key, in drawing order;
    /// their variant is the class key.
    #[serde(default)]
    pub classes: BTreeMap<String, Vec<LayerRule>>,
    /// Hair shapes: `assets/fighters/hair/<shape>.png`.
    pub hair: Vec<String>,
    pub skins: Vec<Tint>,
    pub hair_tints: Vec<Tint>,
    pub tunics: Vec<Tint>,
    /// The point the head and hair images turn around, in pixels of their 32 × 32 image:
    /// the neck.
    pub head_pivot: (f64, f64),
    /// For each image of the spritesheet, in order: how far the head and hair move from
    /// their place in the first idle image, in pixels (right and down, the fighter
    /// facing right), and how much they turn, in degrees (clockwise).
    pub head_frames: Vec<(f64, f64, f64)>,
}

impl AppearanceRules {
    /// The variant and the layers a fighter of the class `class_key` is drawn with: its
    /// class's own, or the common ones.
    #[must_use]
    pub fn look(&self, class_key: &str) -> (&str, &[LayerRule]) {
        match self.classes.get_key_value(class_key) {
            Some((key, layers)) => (key.as_str(), layers.as_slice()),
            None => self.default_look(),
        }
    }

    /// The variant and the layers of a class without its own look.
    #[must_use]
    pub fn default_look(&self) -> (&str, &[LayerRule]) {
        (self.build.as_str(), self.layers.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_data::DesignData;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn looks_are_drawn_among_the_shapes_and_tints() {
        let rules = DesignData::load().unwrap().fighter.appearance;
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let looks: Vec<Appearance> = (0..200)
            .map(|_| Appearance::random(&mut rng, &rules))
            .collect();
        let within = |index: u32, count: usize| usize::try_from(index).unwrap() < count;
        for look in &looks {
            let Appearance::Layered {
                hair,
                skin,
                hair_tint,
                tunic,
            } = *look
            else {
                panic!("a unique look drawn at random");
            };
            assert!(within(hair, rules.hair.len()) && within(skin, rules.skins.len()));
            assert!(within(hair_tint, rules.hair_tints.len()));
            assert!(within(tunic, rules.tunics.len()));
        }
        // Every hair shape and skin comes up.
        for shape in 0..rules.hair.len() {
            let shape = u32::try_from(shape).unwrap();
            assert!(
                looks
                    .iter()
                    .any(|l| matches!(l, Appearance::Layered { hair, .. } if *hair == shape))
            );
        }
        let mut again = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(Appearance::random(&mut again, &rules), looks[0]);
    }

    #[test]
    fn a_look_is_saved_and_read_back() {
        for look in [
            Appearance::default(),
            Appearance::Unique("spartacus".into()),
        ] {
            let json = serde_json::to_string(&look).unwrap();
            assert_eq!(serde_json::from_str::<Appearance>(&json).unwrap(), look);
        }
    }

    #[test]
    fn a_class_is_drawn_with_its_own_layers_or_the_common_ones() {
        let rules = DesignData::load().unwrap().fighter.appearance;
        let (variant, layers) = rules.look("gladiateur");
        assert_eq!(variant, "gladiateur");
        let names = |off_hand| -> Vec<&str> {
            layers
                .iter()
                .filter(|l| l.drawn_with(off_hand))
                .map(|l| l.name.as_str())
                .collect()
        };
        let shielded = names(OffHand::Shield);
        assert!(shielded.contains(&"shield") && !shielded.contains(&"weapon_back"));
        let armed = names(OffHand::Weapon);
        assert!(armed.contains(&"weapon_back") && !armed.contains(&"shield"));
        assert!(armed.contains(&"weapon") && armed.contains(&"crest"));
        assert_eq!(rules.look("augure"), rules.default_look());
        assert_eq!(rules.default_look().0, rules.build);
    }
}
