//! The world's cultures (`data/names.ron`): the names a club of each may take and the
//! patterns it is named by, so that a club's name follows its culture.

use rand::{Rng, RngExt};
use serde::Deserialize;

use crate::colosseum::Continent;

/// A culture of the world, from `data/names.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Culture {
    /// How `data/world.ron` and the saves name it.
    pub key: String,
    /// Shown to the player.
    pub name: String,
    /// Where its clubs are at home.
    pub continent: Continent,
    /// Names a club may take; at least one.
    pub names: Vec<String>,
    /// Patterns of a club's name, each with `{name}`; at least one.
    pub clubs: Vec<String>,
}

/// What a pattern of `Culture::clubs` puts a name in place of.
pub const NAME: &str = "{name}";

/// A club's name in `culture`: a drawn pattern with a drawn name, "de" before a vowel
/// becoming "d'". When that name is in `taken`, another draw, up to a few, then the
/// first draw numbered ("Ludus Magnus II"), so that no two clubs share a name.
pub fn club_name<R: Rng + ?Sized>(rng: &mut R, culture: &Culture, taken: &[&str]) -> String {
    let mut first = None;
    for _ in 0..20 {
        let name = draw_name(rng, culture);
        if !taken.contains(&name.as_str()) {
            return name;
        }
        first.get_or_insert(name);
    }
    let first = first.unwrap_or_default();
    // One of the first `taken.len() + 1` numbers is free.
    (2..)
        .take(taken.len() + 1)
        .map(|n| format!("{first} {}", roman(n)))
        .find(|name| !taken.contains(&name.as_str()))
        .unwrap_or(first)
}

fn draw_name<R: Rng + ?Sized>(rng: &mut R, culture: &Culture) -> String {
    let pattern = drawn(rng, &culture.clubs);
    let name = drawn(rng, &culture.names);
    let elided = name.starts_with(is_vowel);
    let pattern = if elided {
        pattern.replace("de {name}", "d'{name}")
    } else {
        pattern.to_owned()
    };
    pattern.replace(NAME, name)
}

fn drawn<'a, R: Rng + ?Sized>(rng: &mut R, among: &'a [String]) -> &'a str {
    match among.len().checked_sub(1) {
        Some(top) => &among[rng.random_range(0..=top)],
        None => "",
    }
}

fn is_vowel(c: char) -> bool {
    "AEIOUYÂÄÀÉÈÊËÎÏÔÖÛÜŪĀĒĪŌ".contains(c)
}

/// `n` in Roman numerals, from 1 to 39 (beyond, a figure).
fn roman(n: u32) -> String {
    if n >= 40 {
        return n.to_string();
    }
    let units = ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"];
    let tens = "X".repeat(usize::try_from(n / 10).unwrap_or(0));
    format!("{tens}{}", units[usize::try_from(n % 10).unwrap_or(0)])
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    fn culture(names: &[&str], clubs: &[&str]) -> Culture {
        Culture {
            key: "test".into(),
            name: "Test".into(),
            continent: Continent::Europe,
            names: names.iter().map(|&n| n.into()).collect(),
            clubs: clubs.iter().map(|&c| c.into()).collect(),
        }
    }

    #[test]
    fn a_club_is_named_by_its_culture_eliding_before_a_vowel() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let greek = culture(&["Achille"], &["Palestre de {name}"]);
        assert_eq!(club_name(&mut rng, &greek, &[]), "Palestre d'Achille");
        let roman = culture(&["Magnus"], &["Ludus {name}"]);
        assert_eq!(club_name(&mut rng, &roman, &[]), "Ludus Magnus");
        let mongol = culture(&["Batu"], &["Ordu de {name}"]);
        assert_eq!(club_name(&mut rng, &mongol, &[]), "Ordu de Batu");
    }

    #[test]
    fn no_two_clubs_share_a_name() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let roman = culture(&["Magnus", "Gallicus"], &["Ludus {name}"]);
        let taken = ["Ludus Magnus"];
        assert_eq!(club_name(&mut rng, &roman, &taken), "Ludus Gallicus");
        let taken = ["Ludus Magnus", "Ludus Gallicus", "Ludus Magnus II"];
        let name = club_name(&mut rng, &roman, &taken);
        assert!(["Ludus Gallicus II", "Ludus Magnus III"].contains(&name.as_str()));
    }

    #[test]
    fn roman_numerals() {
        assert_eq!(roman(2), "II");
        assert_eq!(roman(14), "XIV");
        assert_eq!(roman(39), "XXXIX");
        assert_eq!(roman(40), "40");
    }
}
