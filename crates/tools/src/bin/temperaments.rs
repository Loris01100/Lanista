//! `temperaments --worlds 8 --seasons 3`: lives worlds without the player in parallel,
//! then prints, by temperament, what the careers of the rival fighters and free agents
//! came to: fights, wins, deaths, retirements, fame and after-effects; and how many
//! clubs went bankrupt, which the cost of injuries and deaths drives; then the same by
//! trait, and for the fighters without one.

use std::process::ExitCode;

use lanista_core::design_data::DesignData;
use lanista_core::fighter::career::{CareerFighter, Status};
use lanista_core::fighter::personality::Temperament;
use lanista_core::fighter::traits::Trait;
use lanista_core::state::{Decisions, GameState, next_week};
use rayon::prelude::*;
use tools::{one_decimal, options};

/// What one temperament's fighters came to, added up.
#[derive(Default, Clone, Copy)]
struct Tally {
    fighters: u64,
    fights: u64,
    wins: u64,
    dead: u64,
    retired: u64,
    fame: u64,
    morale: u64,
    after_effects: u64,
}

impl Tally {
    fn add(&mut self, f: &CareerFighter) {
        let r = f.record;
        self.fighters += 1;
        self.fights += u64::from(r.wins + r.losses + r.draws);
        self.wins += u64::from(r.wins);
        self.dead += u64::from(f.status == Status::Dead);
        self.retired += u64::from(f.status == Status::Retired);
        self.fame += u64::from(f.fame.value());
        self.morale += u64::from(f.morale.value());
        self.after_effects += u64::from(f.permanent_injuries());
    }

    fn sum(mut self, w: Self) -> Self {
        self.fighters += w.fighters;
        self.fights += w.fights;
        self.wins += w.wins;
        self.dead += w.dead;
        self.retired += w.retired;
        self.fame += w.fame;
        self.morale += w.morale;
        self.after_effects += w.after_effects;
        self
    }

    fn print(&self, name: &str) {
        if self.fighters == 0 {
            return;
        }
        println!(
            "{:<12} {:>8} {:>7} {:>6} {:>9} {:>6} {:>8} {:>5} {:>7} {:>7}",
            name,
            self.fighters,
            one_decimal(self.fights, self.fighters),
            one_decimal(100 * self.wins, self.fights),
            one_decimal(100 * self.dead, self.fights),
            one_decimal(100 * self.dead, self.fighters),
            one_decimal(100 * self.retired, self.fighters),
            one_decimal(self.fame, self.fighters),
            one_decimal(self.morale, self.fighters),
            one_decimal(self.after_effects, self.fighters),
        );
    }
}

/// Every trait, then none.
const TRAITS: usize = Trait::ALL.len() + 1;

/// A world's tallies: by temperament, by trait then without one; and its closed clubs.
type World = ([Tally; 7], [Tally; TRAITS], u64);

fn main() -> ExitCode {
    let [worlds, seasons] = match options(
        std::env::args().skip(1),
        [("--worlds", 8), ("--seasons", 3)],
    ) {
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
    let weeks = seasons * u64::from(data.calendar.season_weeks());
    // Each world is independent: parallel across worlds, never inside one.
    let tallies: Vec<World> = (0..worlds)
        .into_par_iter()
        .map(|seed| {
            let mut state = GameState::new(seed, &data);
            for f in &mut state.fighters {
                f.status = Status::Retired;
            }
            for _ in 0..weeks {
                next_week(&mut state, &Decisions::default(), &data);
            }
            let mut tally = [Tally::default(); 7];
            let mut by_trait = [Tally::default(); TRAITS];
            // A fighter who moved left a blank entry behind: its career is where it went.
            let careers = state.rivals.iter().flat_map(|r| &r.fighters);
            for f in careers
                .chain(&state.free_agents)
                .filter(|f| f.moved_to.is_none())
            {
                tally[index(f.personality.temperament)].add(f);
                for &t in &f.traits {
                    by_trait[trait_index(t)].add(f);
                }
                if f.traits.is_empty() {
                    by_trait[Trait::ALL.len()].add(f);
                }
            }
            let closed = state.rivals.iter().filter(|r| !r.is_open()).count();
            (tally, by_trait, u64::try_from(closed).unwrap_or(u64::MAX))
        })
        .collect();
    let closed: u64 = tallies.iter().map(|(.., c)| c).sum();

    println!(
        "{worlds} worlds (seeds 0 to {}), {seasons} seasons each, {} clubs closed a world",
        worlds - 1,
        one_decimal(closed, worlds)
    );
    println!(
        "{:<12} {:>8} {:>7} {:>6} {:>9} {:>6} {:>8} {:>5} {:>7} {:>7}",
        "temperament",
        "fighters",
        "fights",
        "win %",
        "deaths %",
        "dead %",
        "retired%",
        "fame",
        "morale",
        "after-e"
    );
    let total = |pick: &dyn Fn(&World) -> Tally| {
        tallies.iter().fold(Tally::default(), |s, w| s.sum(pick(w)))
    };
    for t in Temperament::ALL {
        total(&|w| w.0[index(t)]).print(&format!("{t:?}"));
    }
    println!("{:<12}", "trait");
    for t in Trait::ALL {
        total(&|w| w.1[trait_index(t)]).print(&format!("{t:?}"));
    }
    total(&|w| w.1[Trait::ALL.len()]).print("none");
    ExitCode::SUCCESS
}

fn index(temperament: Temperament) -> usize {
    Temperament::ALL
        .iter()
        .position(|&t| t == temperament)
        .unwrap_or(0)
}

fn trait_index(t: Trait) -> usize {
    Trait::ALL.iter().position(|&a| a == t).unwrap_or(0)
}
