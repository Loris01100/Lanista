//! Runs the `temperaments` binary: one short world tells each drawn temperament's
//! careers, and a bad argument is refused.

use std::process::Command;

fn temperaments(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_temperaments"))
        .args(args)
        .output()
        .expect("run temperaments")
}

#[test]
fn every_drawn_temperament_gets_its_line() {
    let output = temperaments(&["--worlds", "1", "--seasons", "1"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("UTF-8 output");
    for t in [
        "Reckless",
        "Showman",
        "Methodical",
        "Survivor",
        "Berserker",
        "Proud",
    ] {
        assert!(text.contains(t), "no {t} in the output");
    }
    assert!(!text.contains("Balanced"));
}

#[test]
fn an_unknown_argument_is_refused() {
    assert_eq!(temperaments(&["--nope"]).status.code(), Some(2));
}
