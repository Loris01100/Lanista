//! `calibrate --fights 20000`: plays real duels between varied fighters (seeds 0 to
//! n - 1, in parallel), fits the statistical model on them and writes it to
//! `data/stat_model.ron`, read by `lanista_core::stat_model`.

use std::process::ExitCode;

use lanista_core::design_data::{DesignData, STAT_MODEL_FILE};
use lanista_core::fighter::fame::FightResult;
use rayon::prelude::*;
use tools::calibration::{Sample, fit, record};
use tools::{one_decimal, option};

fn main() -> ExitCode {
    let fights = match option(std::env::args().skip(1), "--fights", 20_000) {
        Ok(0) => {
            eprintln!("--fights must be at least 1");
            return ExitCode::from(2);
        }
        Ok(n) => n,
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
    let samples: Vec<Sample> = (0..fights)
        .into_par_iter()
        .map(|seed| record(seed, &data))
        .collect();
    let model = fit(&samples, data.classes.len());

    let count = |f: &dyn Fn(&Sample) -> bool| samples.iter().filter(|s| f(s)).count() as u64;
    let with_winner = count(&|s| s.sides.iter().any(|d| d.result == FightResult::Won));
    let unfinished = count(&|s| s.sides[0].result == FightResult::Unfinished);
    let deaths = count(&|s| {
        s.sides
            .iter()
            .any(|d| d.died && d.result != FightResult::Won)
    });
    let knock_outs = count(&|s| s.sides.iter().any(|d| d.knocked_out));
    let stuns = count(&|s| s.sides.iter().any(|d| d.stunned));
    let foreseen = count(&|s| {
        let chance = model.winner.chance(&s.fighters[0], &s.fighters[1]);
        match (s.sides[0].result, s.sides[1].result) {
            (FightResult::Won, _) => chance > 0.5,
            (_, FightResult::Won) => chance < 0.5,
            _ => false,
        }
    });
    println!("{fights} real duels (seeds 0 to {})", fights - 1);
    println!(
        "  with a winner {} %, unfinished {} %, a death {} %",
        one_decimal(with_winner * 100, fights),
        one_decimal(unfinished * 100, fights),
        one_decimal(deaths * 100, fights),
    );
    println!(
        "  a stun {} %, a knock-out {} %",
        one_decimal(stuns * 100, fights),
        one_decimal(knock_outs * 100, fights),
    );
    println!(
        "  the regression foresees the winner of {} % of the fights with one",
        one_decimal(foreseen * 100, with_winner),
    );
    println!("winner weights: {:?}", model.winner);
    println!("unfinished by classes: {:?}", model.unfinished);
    println!("knocked out by the winner's class: {:?}", model.knocked_out);
    println!("death by determination: {:?}", model.death);
    println!("stunned by the opponent's class: {:?}", model.stunned);

    let text = match model.to_ron() {
        Ok(text) => text,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = std::fs::write(DesignData::path(STAT_MODEL_FILE), text) {
        eprintln!("{STAT_MODEL_FILE}: {e}");
        return ExitCode::FAILURE;
    }
    println!("written to {STAT_MODEL_FILE}");
    ExitCode::SUCCESS
}
