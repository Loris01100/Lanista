//! Runs the `career` binary: a long career goes through injuries, illnesses and
//! birthdays to retirement, and a bad argument is refused.

use std::process::Command;

fn career(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_career"))
        .args(args)
        .output()
        .expect("run career")
}

#[test]
fn a_long_career_is_printed_week_by_week() {
    let output = career(&["--seed", "7", "--weeks", "400"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("UTF-8 output");
    for expected in [
        "retired at 35",
        "injured",
        "healed",
        "fell ill",
        "one year older",
    ] {
        assert!(text.contains(expected), "no {expected:?} in the output");
    }
}

#[test]
fn an_unknown_argument_is_refused() {
    assert_eq!(career(&["--nope"]).status.code(), Some(2));
}
