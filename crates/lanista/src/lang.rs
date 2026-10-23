//! The player's language: each text is a Fluent message of `assets/lang/<language>.ftl`,
//! named by its key and filled with its arguments.

use std::cell::RefCell;

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};

/// A language the game is played in.
pub struct Language {
    /// Its file's name, `fr` for `fr.ftl`, and its identifier for Fluent.
    pub code: &'static str,
    /// Its name in itself, as the language menu shows it.
    pub name: &'static str,
    source: &'static str,
}

/// The game's languages, the first one by default.
pub const LANGUAGES: &[Language] = &[Language {
    code: "fr",
    name: "Français",
    source: include_str!("../../../assets/lang/fr.ftl"),
}];

thread_local! {
    static BUNDLE: RefCell<(&'static str, FluentBundle<FluentResource>)> =
        RefCell::new((LANGUAGES[0].code, bundle(&LANGUAGES[0])));
}

fn bundle(language: &Language) -> FluentBundle<FluentResource> {
    let resource = FluentResource::try_new(language.source.to_owned())
        .unwrap_or_else(|(_, errors)| panic!("unreadable language file: {errors:?}"));
    let id = language.code.parse().expect("a valid language identifier");
    let mut bundle = FluentBundle::new(vec![id]);
    // No Unicode isolation marks around arguments: the font has no glyph for them.
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource)
        .unwrap_or_else(|errors| panic!("duplicate language keys: {errors:?}"));
    bundle
}

/// The code of the language the texts are in.
pub fn language() -> &'static str {
    BUNDLE.with_borrow(|(code, _)| *code)
}

/// Shows the texts in the language of `code` from now on; returns whether there is one.
pub fn set_language(code: &str) -> bool {
    let Some(language) = LANGUAGES.iter().find(|l| l.code == code) else {
        return false;
    };
    BUNDLE.set((language.code, bundle(language)));
    true
}

/// The text of `key`, its `{ $name }` placeables filled from `args`; the key itself when missing.
pub fn text(key: &str, args: &[(&str, FluentValue)]) -> String {
    BUNDLE.with_borrow(|(_, bundle)| {
        let Some(pattern) = bundle.get_message(key).and_then(|m| m.value()) else {
            return key.to_owned();
        };
        let args: FluentArgs = args.iter().cloned().collect();
        let mut errors = Vec::new();
        let text = bundle
            .format_pattern(pattern, Some(&args), &mut errors)
            .into_owned();
        // A missing argument shows as `{$name}`: the tests stop on it.
        debug_assert!(errors.is_empty(), "{key}: {errors:?}");
        text
    })
}

/// `tr!("key", name = value, …)`: the text of `key` with its arguments, strings or numbers.
macro_rules! tr {
    ($key:expr $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::lang::text(
            $key,
            &[$((stringify!($name), ::fluent_bundle::FluentValue::from($value))),*],
        )
    };
}
pub(crate) use tr;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_filled_with_its_arguments() {
        assert_eq!(
            tr!("fighter-unnamed", class = "Mirmillon", number = 2),
            "Mirmillon n° 2"
        );
        assert_eq!(tr!("weeks", n = 1), "1 semaine");
        assert_eq!(tr!("weeks", n = 3), "3 semaines");
    }

    #[test]
    fn a_club_name_elides_de_before_a_vowel() {
        use lanista_core::names::ClubName;
        let named = |pattern: &str, name: &str, number| {
            crate::texts::named_club(&ClubName {
                pattern: pattern.into(),
                name: name.into(),
                number,
            })
        };
        assert_eq!(named("palestre", "Achille", 0), "Palestre d'Achille");
        assert_eq!(named("ordu", "Batu", 0), "Ordu de Batu");
        assert_eq!(named("ludus", "Magnus", 2), "Ludus Magnus II");
        assert_eq!(named("ludus", "Magnus", 14), "Ludus Magnus XIV");
    }

    #[test]
    fn a_missing_key_shows_itself() {
        assert_eq!(text("no-such-key", &[]), "no-such-key");
    }

    #[test]
    fn a_language_is_chosen_by_its_code() {
        assert!(!set_language("xx"));
        assert_eq!(language(), "fr");
        assert!(set_language("fr"));
        assert_eq!(language(), "fr");
    }

    /// Fails on the keys missing from a language's file.
    fn assert_in_every_language(keys: &[String]) {
        for language in LANGUAGES {
            let bundle = bundle(language);
            let missing: Vec<&String> = keys.iter().filter(|k| !bundle.has_message(k)).collect();
            assert!(
                missing.is_empty(),
                "missing in {}.ftl: {missing:?}",
                language.code
            );
        }
    }

    #[test]
    fn every_key_in_the_code_is_in_every_language() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let keys = keys_in(&source);
        assert!(keys.len() > 100, "{} keys found", keys.len());
        assert_in_every_language(&keys);
    }

    #[test]
    fn every_french_key_is_in_every_language() {
        // A message starts at the line's start; its other lines are indented.
        let keys: Vec<String> = LANGUAGES[0]
            .source
            .lines()
            .filter(|line| !line.starts_with(['#', ' ', '}']))
            .filter_map(|line| line.split_once(" =").map(|(key, _)| key.to_owned()))
            .collect();
        assert!(keys.len() > 100, "{} keys found", keys.len());
        assert_in_every_language(&keys);
    }

    #[test]
    fn every_name_of_the_design_data_is_in_every_language() {
        let data = lanista_core::design_data::DesignData::load().unwrap();
        let mut keys: Vec<String> = data
            .classes
            .iter()
            .map(|c| format!("class-{}", c.key))
            .collect();
        let loadouts = data.classes.iter().flat_map(|c| &c.loadouts);
        keys.extend(loadouts.map(|l| format!("loadout-{}", l.key)));
        keys.extend(
            data.world
                .colosseums
                .iter()
                .map(|c| format!("colosseum-{}", c.key)),
        );
        for culture in &data.cultures {
            keys.push(format!("culture-{}", culture.key));
            keys.extend(culture.clubs.iter().map(|p| format!("club-{p}")));
        }
        assert_in_every_language(&keys);
    }

    #[test]
    fn the_font_has_every_character_of_every_language() {
        use reesch::egui::{Context, FontId, RawInput};
        let ui = Context::default();
        crate::theme::apply(&ui);
        // The fonts are built at the first pass.
        let _ = ui.run(RawInput::default(), |_| {});
        let font = FontId::proportional(18.0);
        let missing = |text: &str| -> Vec<char> {
            ui.fonts(|fonts| {
                let mut missing: Vec<char> = text
                    .chars()
                    .filter(|&c| !c.is_whitespace() && !fonts.has_glyph(&font, c))
                    .collect();
                missing.sort_unstable();
                missing.dedup();
                missing
            })
        };
        for language in LANGUAGES {
            let shown: String = language
                .source
                .lines()
                .filter(|line| !line.starts_with('#'))
                .collect();
            let missing = missing(&shown);
            assert!(missing.is_empty(), "{}.ftl: {missing:?}", language.code);
        }
        // The clubs' names drawn from the data are shown too.
        let data = lanista_core::design_data::DesignData::load().unwrap();
        for culture in &data.cultures {
            let missing = missing(&culture.names.concat());
            assert!(missing.is_empty(), "{}: {missing:?}", culture.key);
        }
    }

    /// The key of each `tr!("key"` in the code under `dir`, but for this file's examples.
    // ponytail: literal keys only, which all the code uses.
    fn keys_in(dir: &std::path::Path) -> Vec<String> {
        let mut keys = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                keys.extend(keys_in(&path));
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("lang.rs") {
                let code = std::fs::read_to_string(&path).unwrap();
                for (at, call) in code.match_indices("tr!(\"") {
                    // Not the end of another macro's name, as `include_str!`.
                    let before = code[..at].chars().next_back();
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let rest = &code[at + call.len()..];
                    keys.extend(rest.split('"').next().map(str::to_owned));
                }
            }
        }
        keys
    }
}
