//! `simulate --seed 42`: plays one duel and prints how it went.

use std::process::ExitCode;

use lanista_core::log::{Report, log_text};
use tools::{design_data, option, play, random_duel};

fn main() -> ExitCode {
    let seed = match option(std::env::args().skip(1), "--seed", 42) {
        Ok(seed) => seed,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let data = match design_data() {
        Ok(data) => data,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let mut arena = random_duel(&data, seed);
    println!("seed {seed}");
    for f in &arena.fighters {
        let class = data.class(f.class).map_or("?", |c| c.name.as_str());
        println!(
            "fighter {}: {class}, starts at ({:.1}, {:.1})",
            f.id.0, f.position.x, f.position.y
        );
    }
    play(&mut arena);
    print!("\n{}\n{}", log_text(&arena), Report::new(&arena));
    ExitCode::SUCCESS
}
