//! `compare --fights 20000 --tolerance 2`: plays the same fights between varied
//! fighters (seeds from `--from`, 1 000 000 by default, away from the seeds `calibrate`
//! fits on) as real duels and through the statistical model, prints the win, death and
//! injury rates of both, and fails if one gap exceeds the tolerance in percentage points.

use std::process::ExitCode;

use lanista_core::design_data::DesignData;
use rayon::prelude::*;
use tools::comparison::{Pair, play, rates};
use tools::options;

fn main() -> ExitCode {
    let defaults = [
        ("--fights", 20_000),
        ("--from", 1_000_000),
        ("--tolerance", 2),
    ];
    let [fights, from, tolerance] = match options(std::env::args().skip(1), defaults) {
        Ok([0, ..]) => {
            eprintln!("--fights must be at least 1");
            return ExitCode::from(2);
        }
        Ok(values) => values,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let data = match DesignData::load() {
        Ok(data) => data,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    // Each fight is independent: parallel across fights, never inside one.
    let pairs: Vec<Pair> = (from..from.saturating_add(fights))
        .into_par_iter()
        .map(|seed| play(seed, &data))
        .collect();
    let names: Vec<String> = data.classes.iter().map(|c| c.name.clone()).collect();
    let rates = rates(&pairs, &names);

    #[expect(clippy::cast_precision_loss, reason = "a tolerance of a few points")]
    let tolerance = tolerance as f64;
    println!(
        "{fights} fights (seeds {from} to {}), real duels against the model fitted on {} \
         duels",
        from.saturating_add(fights - 1),
        data.stat_model.fights,
    );
    println!(
        "{:<40} {:>7} {:>7} {:>6}",
        "rate (%)", "real", "model", "gap"
    );
    let mut too_far = 0;
    for rate in &rates {
        let flag = if rate.gap() > tolerance {
            too_far += 1;
            "  too far"
        } else {
            ""
        };
        println!(
            "{:<40} {:>7.1} {:>7.1} {:>6.1}{flag}",
            rate.name,
            rate.real,
            rate.model,
            rate.gap(),
        );
    }
    if too_far > 0 {
        println!("{too_far} rate(s) more than {tolerance} points apart: rerun calibrate");
        return ExitCode::FAILURE;
    }
    println!("every rate within {tolerance} points");
    ExitCode::SUCCESS
}
