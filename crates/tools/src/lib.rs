//! Shared by the command-line tools: argument parsing and number formatting.
//! The duel setup itself lives in `lanista_core::duel`, shared with the game.

/// Value of the `--name value` option among `args` (program name excluded), or `default`.
///
/// # Errors
/// On an unknown argument, a missing value or a value that is not a whole number.
pub fn option(
    args: impl IntoIterator<Item = String>,
    name: &str,
    default: u64,
) -> Result<u64, String> {
    let mut value = default;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg != name {
            return Err(format!(
                "unknown argument: {arg} (expected {name} <number>)"
            ));
        }
        let Some(text) = args.next() else {
            return Err(format!("{name} needs a value"));
        };
        value = text
            .parse()
            .map_err(|_| format!("{name}: not a whole number: {text}"))?;
    }
    Ok(value)
}

/// `numerator / denominator` with one decimal, rounded down, in integers only.
#[must_use]
pub fn one_decimal(numerator: u64, denominator: u64) -> String {
    if denominator == 0 {
        return "-".into();
    }
    let tenths = numerator * 10 / denominator;
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|&s| s.to_owned()).collect()
    }

    #[test]
    fn reads_an_option() {
        assert_eq!(option(args(&[]), "--seed", 42), Ok(42));
        assert_eq!(option(args(&["--seed", "7"]), "--seed", 42), Ok(7));
        assert!(option(args(&["--seed"]), "--seed", 42).is_err());
        assert!(option(args(&["--seed", "x"]), "--seed", 42).is_err());
        assert!(option(args(&["--graine", "7"]), "--seed", 42).is_err());
    }

    #[test]
    fn formats_one_decimal() {
        assert_eq!(one_decimal(1, 3), "0.3");
        assert_eq!(one_decimal(127, 30), "4.2");
        assert_eq!(one_decimal(1, 0), "-");
    }
}
