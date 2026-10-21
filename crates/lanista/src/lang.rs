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
        bundle
            .format_pattern(pattern, Some(&args), &mut errors)
            .into_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_filled_with_its_arguments() {
        let name = text(
            "fighter-name",
            &[("class", "Mirmillon".into()), ("number", 2.into())],
        );
        assert_eq!(name, "Mirmillon n° 2");
    }

    #[test]
    fn a_missing_key_shows_itself() {
        assert_eq!(text("no-such-key", &[]), "no-such-key");
    }
}
