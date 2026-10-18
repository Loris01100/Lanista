//! How a fighter looks: drawn at random when it is generated, so that 20 fighters in a
//! melee can be told apart, and kept for life. The game builds it from layers drawn in
//! light grey and tinted (see `docs/Formats_graphiques.md`); `lanista_core` only keeps
//! which shape and which tints.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

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

/// The shapes and tints of the layered looks, and how the head follows the body, from
/// `data/fighter.ron`. `lanista_core` only draws indices among them; the game draws them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceRules {
    /// The variant of every layer but the hair: `assets/fighters/<layer>/<build>.png`.
    pub build: String,
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
}
