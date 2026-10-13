//! `batch --fights 10000`: plays duels with seeds 0 to n - 1 in parallel, then prints
//! each class's win rate and the average duration.

use std::process::ExitCode;

use lanista_core::combat::TICKS_PER_SECOND;
use lanista_core::design_data::DesignData;
use lanista_core::duel::{play, random_duel};
use lanista_core::fighter::Fighter;
use rayon::prelude::*;
use tools::{one_decimal, option};

/// What `batch` keeps of one fight.
struct Outcome {
    /// Class index of each fighter.
    classes: Vec<usize>,
    /// Index of the winner among the fighters.
    winner: Option<usize>,
    ticks: u32,
    over: bool,
}

fn main() -> ExitCode {
    let fights = match option(std::env::args().skip(1), "--fights", 10_000) {
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
    let outcomes: Vec<Outcome> = (0..fights)
        .into_par_iter()
        .map(|seed| {
            let mut arena = random_duel(&data, seed);
            play(&mut arena);
            let index = |id: u32| usize::try_from(id).unwrap_or(usize::MAX);
            let over = arena.is_over();
            Outcome {
                classes: arena.fighters.iter().map(|f| index(f.class.0)).collect(),
                winner: over
                    .then(|| arena.fighters.iter().position(Fighter::is_alive))
                    .flatten(),
                ticks: arena.tick,
                over,
            }
        })
        .collect();

    let mut appearances = vec![0_u64; data.classes.len()];
    let mut wins = vec![0_u64; data.classes.len()];
    let (mut ticks, mut unfinished, mut no_survivor) = (0_u64, 0_u64, 0_u64);
    for o in &outcomes {
        for &class in &o.classes {
            appearances[class] += 1;
        }
        match (o.over, o.winner) {
            (true, Some(w)) => wins[o.classes[w]] += 1,
            (true, None) => no_survivor += 1,
            (false, _) => unfinished += 1,
        }
        ticks += u64::from(o.ticks);
    }

    let ticks_per_second = u64::from(TICKS_PER_SECOND);
    println!(
        "{fights} fights (seeds 0 to {}), average duration {} s ({} ticks)",
        fights - 1,
        one_decimal(ticks, fights * ticks_per_second),
        one_decimal(ticks, fights),
    );
    println!("win rate per class (a mirror match counts both fighters):");
    for (i, class) in data.classes.iter().enumerate() {
        println!(
            "  {:<12} {:>5} % of {} fights",
            class.name,
            one_decimal(wins[i] * 100, appearances[i]),
            appearances[i],
        );
    }
    println!("unfinished after one minute: {unfinished}, no survivor: {no_survivor}");
    ExitCode::SUCCESS
}
