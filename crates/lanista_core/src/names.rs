//! The world's cultures (`data/names.ron`): the names a club of each may take and the
//! patterns it is named by, so that a club's name follows its culture, and the stage
//! names of its fighters.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::colosseum::Continent;

/// A culture of the world, from `data/names.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Culture {
    /// How `data/world.ron`, the saves and the language files (`culture-<key>`) name it.
    pub key: String,
    /// Where its clubs are at home.
    pub continent: Continent,
    /// Names a club may take; at least one.
    pub names: Vec<String>,
    /// Keys of the patterns of a club's name (`club-<pattern>` in the language files);
    /// at least one.
    pub clubs: Vec<String>,
    /// Stage names its fighters take; at least one. Few enough that they come back,
    /// borne by another once their fighter is gone.
    pub fighters: Vec<String>,
}

/// A club's name: a pattern of its culture with a name, which the language files
/// put together ("Palestre d'Achille").
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ClubName {
    /// One of its culture's `clubs`.
    pub pattern: String,
    /// One of its culture's `names`.
    pub name: String,
    /// 2 or more when that pattern and name were taken ("Ludus Magnus II"), else 0.
    pub number: u32,
}

/// A club's name in `culture`: a drawn pattern with a drawn name. When it is in
/// `taken`, another draw, up to a few, then the first draw numbered, so that no two
/// clubs share a name.
pub fn club_name<R: Rng + ?Sized>(rng: &mut R, culture: &Culture, taken: &[&ClubName]) -> ClubName {
    let mut first = None;
    for _ in 0..20 {
        let name = ClubName {
            pattern: drawn(rng, &culture.clubs).to_owned(),
            name: drawn(rng, &culture.names).to_owned(),
            number: 0,
        };
        if !taken.contains(&&name) {
            return name;
        }
        first.get_or_insert(name);
    }
    let first = first.unwrap_or_default();
    // One of the first `taken.len() + 1` numbers is free.
    (2..)
        .take(taken.len() + 1)
        .map(|number| ClubName {
            number,
            ..first.clone()
        })
        .find(|name| !taken.contains(&name))
        .unwrap_or(first)
}

/// A fighter's stage name, which no other fighter in activity bears.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FighterName {
    /// One of its culture's `fighters`; empty for a fighter not named yet.
    pub name: String,
    /// 2 or more when every name of its culture was borne ("Flamma II"), else 0.
    pub number: u32,
}

impl FighterName {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
    }
}

/// A fighter's name in `culture`, unlike any of `taken`: drawn among the names none
/// of them bears; when all are borne, drawn among those borne least, with the first
/// number free.
pub fn fighter_name<R: Rng + ?Sized>(
    rng: &mut R,
    culture: &Culture,
    taken: &[&FighterName],
) -> FighterName {
    let bearers = |name: &String| taken.iter().filter(|t| t.name == *name).count();
    let fewest = culture.fighters.iter().map(bearers).min().unwrap_or(0);
    let least: Vec<String> = culture
        .fighters
        .iter()
        .filter(|&n| bearers(n) == fewest)
        .cloned()
        .collect();
    let name = drawn(rng, &least).to_owned();
    // One of the first `taken.len() + 1` numbers is free.
    let number = [0]
        .into_iter()
        .chain(2..)
        .take(taken.len() + 1)
        .find(|&number| !taken.iter().any(|t| t.name == name && t.number == number))
        .unwrap_or(0);
    FighterName { name, number }
}

fn drawn<'a, R: Rng + ?Sized>(rng: &mut R, among: &'a [String]) -> &'a str {
    match among.len().checked_sub(1) {
        Some(top) => &among[rng.random_range(0..=top)],
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    fn culture(names: &[&str], clubs: &[&str]) -> Culture {
        Culture {
            key: "test".into(),
            continent: Continent::Europe,
            names: names.iter().map(|&n| n.into()).collect(),
            clubs: clubs.iter().map(|&c| c.into()).collect(),
            fighters: vec!["Flamma".into(), "Spiculus".into()],
        }
    }

    fn fighter(name: &str, number: u32) -> FighterName {
        FighterName {
            name: name.into(),
            number,
        }
    }

    fn named(pattern: &str, name: &str, number: u32) -> ClubName {
        ClubName {
            pattern: pattern.into(),
            name: name.into(),
            number,
        }
    }

    #[test]
    fn a_club_is_named_by_its_culture() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let greek = culture(&["Achille"], &["palestre"]);
        assert_eq!(
            club_name(&mut rng, &greek, &[]),
            named("palestre", "Achille", 0)
        );
    }

    #[test]
    fn no_two_clubs_share_a_name() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let roman = culture(&["Magnus", "Gallicus"], &["ludus"]);
        let magnus = named("ludus", "Magnus", 0);
        let gallicus = named("ludus", "Gallicus", 0);
        assert_eq!(club_name(&mut rng, &roman, &[&magnus]), gallicus);
        let second = named("ludus", "Magnus", 2);
        let name = club_name(&mut rng, &roman, &[&magnus, &gallicus, &second]);
        assert!([named("ludus", "Gallicus", 2), named("ludus", "Magnus", 3)].contains(&name));
    }

    #[test]
    fn a_fighter_takes_a_name_no_one_bears() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let roman = culture(&["Magnus"], &["ludus"]);
        let flamma = fighter("Flamma", 0);
        for _ in 0..10 {
            assert_eq!(
                fighter_name(&mut rng, &roman, &[&flamma]),
                fighter("Spiculus", 0)
            );
        }
    }

    #[test]
    fn when_every_name_is_borne_the_least_borne_is_numbered() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let roman = culture(&["Magnus"], &["ludus"]);
        let (flamma, second) = (fighter("Flamma", 0), fighter("Flamma", 2));
        let spiculus = fighter("Spiculus", 0);
        assert_eq!(
            fighter_name(&mut rng, &roman, &[&flamma, &second, &spiculus]),
            fighter("Spiculus", 2)
        );
        // The first bearer gone, his name comes back bare.
        let other = fighter("Spiculus", 2);
        assert_eq!(
            fighter_name(&mut rng, &roman, &[&second, &spiculus, &other]),
            fighter("Flamma", 0)
        );
    }
}
