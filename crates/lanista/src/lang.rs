//! The player's language: each text is a Fluent message of `assets/lang/<language>.ftl`,
//! named by its key and filled with its arguments.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};

const FRENCH: &str = include_str!("../../../assets/lang/fr.ftl");

thread_local! {
    // ponytail: French only, built once; a language setting will rebuild it from another file.
    static BUNDLE: FluentBundle<FluentResource> = bundle(FRENCH);
}

fn bundle(source: &str) -> FluentBundle<FluentResource> {
    let resource = FluentResource::try_new(source.to_owned())
        .unwrap_or_else(|(_, errors)| panic!("unreadable language file: {errors:?}"));
    let language = "fr".parse().expect("a valid language identifier");
    let mut bundle = FluentBundle::new(vec![language]);
    // No Unicode isolation marks around arguments: the font has no glyph for them.
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource)
        .unwrap_or_else(|errors| panic!("duplicate language keys: {errors:?}"));
    bundle
}

/// The text of `key`, its `{ $name }` placeables filled from `args`; the key itself when missing.
pub fn text(key: &str, args: &[(&str, FluentValue)]) -> String {
    BUNDLE.with(|bundle| {
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
            tr!("fighter-name", class = "Mirmillon", number = 2),
            "Mirmillon n° 2"
        );
        assert_eq!(tr!("weeks", n = 1), "1 semaine");
        assert_eq!(tr!("weeks", n = 3), "3 semaines");
    }

    #[test]
    fn a_missing_key_shows_itself() {
        assert_eq!(text("no-such-key", &[]), "no-such-key");
    }

    #[test]
    fn every_key_in_the_code_is_in_the_french_file() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let keys = keys_in(&source);
        assert!(keys.len() > 100, "{} keys found", keys.len());
        let missing: Vec<&String> =
            BUNDLE.with(|bundle| keys.iter().filter(|key| !bundle.has_message(key)).collect());
        assert!(missing.is_empty(), "missing in fr.ftl: {missing:?}");
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
