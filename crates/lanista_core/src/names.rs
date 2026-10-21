//! The world's cultures (`data/names.ron`): the names a club of each may take and the
//! patterns it is named by, so that a club's name follows its culture.

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
}
