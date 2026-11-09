//! `batch --seeds 100 --min 40 --max 60 --matchup 70`: plays every loadout against every
//! other one, both ways, in each arena kind on each element of the floor, `--seeds` duels
//! each (seeds 0 to n - 1), all with the standard stats and set up as a colosseum's duel
//! (`duel::loadout_duel`). Prints each class's win rate against the other classes, the
//! win rate of each loadout against each other one, and each loadout's by arena kind and
//! by element. Fails if a class's win rate is outside `--min` to `--max` %, or if a loadout
//! wins more than `--matchup` % against another class's loadout.

use std::process::ExitCode;

use lanista_core::combat::{ArenaKind, Element, Floor, TICKS_PER_SECOND};
use lanista_core::design_data::DesignData;
use lanista_core::duel::{loadout_duel, play};
use lanista_core::fighter::{ClassId, Fighter};
use rayon::prelude::*;
use tools::{one_decimal, options};

/// One class in one of its loadouts.
struct Entry {
    class: ClassId,
    loadout: usize,
    name: String,
}

/// One duel to play: the indices of its two entries, of its arena kind and element in
/// their `ALL`, and its seed.
#[derive(Clone, Copy)]
struct Job {
    entries: [usize; 2],
    kind: usize,
    element: usize,
    seed: u64,
}

/// A duel's winner, its index among the two fighters or None (unfinished, or no
/// survivor), and its duration in ticks.
type Outcome = (Option<usize>, u32);

/// Wins out of fights.
#[derive(Clone, Copy, Default)]
struct Tally {
    wins: u64,
    fights: u64,
}

impl Tally {
    fn add(&mut self, won: bool) {
        self.fights += 1;
        self.wins += u64::from(won);
    }

    fn percent(self) -> String {
        one_decimal(self.wins * 100, self.fights)
    }

    /// Whether the win rate is below `min` or above `max` %.
    fn outside(self, min: u64, max: u64) -> bool {
        self.wins * 100 < min * self.fights || self.wins * 100 > max * self.fights
    }
}

fn main() -> ExitCode {
    let defaults = [
        ("--seeds", 100),
        ("--min", 40),
        ("--max", 60),
        ("--matchup", 70),
    ];
    let [seeds, min, max, matchup] = match options(std::env::args().skip(1), defaults) {
        Ok([0, ..]) => {
            eprintln!("--seeds must be at least 1");
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

    let entries = entries(&data);
    let jobs = jobs(entries.len(), seeds);
    let outcomes = play_all(&data, &entries, &jobs);
    let tallies = Tallies::count(&data, &entries, &jobs, &outcomes);
    let fights = u64::try_from(jobs.len()).unwrap_or(u64::MAX);
    println!(
        "{fights} fights ({seeds} seeds per loadout pair, arena kind and element), \
         average duration {} s, without a winner after one minute: {}",
        one_decimal(tallies.ticks, fights * u64::from(TICKS_PER_SECOND)),
        tallies.no_winner,
    );
    let failures =
        tallies.print_classes(&data, (min, max)) + tallies.print_matchups(&entries, matchup);
    tallies.print_floors(&entries);
    if failures > 0 {
        println!("\n{failures} win rate(s) out of balance");
        return ExitCode::FAILURE;
    }
    println!("\nevery win rate in balance");
    ExitCode::SUCCESS
}

/// Every class in each of its loadouts, in the order of `data/classes.ron`.
fn entries(data: &DesignData) -> Vec<Entry> {
    data.classes
        .iter()
        .zip(0..)
        .flat_map(|(class, id)| {
            class
                .loadouts
                .iter()
                .enumerate()
                .map(move |(loadout, l)| Entry {
                    class: ClassId(id),
                    loadout,
                    name: format!("{}/{}", class.key, l.key),
                })
        })
        .collect()
}

/// Every pair of different entries among `n`, both ways, in each arena kind on each
/// element, `seeds` times.
fn jobs(n: usize, seeds: u64) -> Vec<Job> {
    let mut jobs = Vec::new();
    for a in 0..n {
        for b in (0..n).filter(|&b| b != a) {
            for kind in 0..ArenaKind::ALL.len() {
                for element in 0..Element::ALL.len() {
                    jobs.extend((0..seeds).map(|seed| Job {
                        entries: [a, b],
                        kind,
                        element,
                        seed,
                    }));
                }
            }
        }
    }
    jobs
}

/// Each job's outcome, in the order of `jobs`.
fn play_all(data: &DesignData, entries: &[Entry], jobs: &[Job]) -> Vec<Outcome> {
    // Each fight is independent: parallel across fights, never inside one.
    jobs.par_iter()
        .map(|job| {
            let sides = job.entries.map(|e| (entries[e].class, entries[e].loadout));
            let floor = Floor::new(ArenaKind::ALL[job.kind], Element::ALL[job.element]);
            let mut arena = loadout_duel(data, sides, floor, job.seed);
            play(&mut arena);
            let winner = arena
                .is_over()
                .then(|| arena.fighters.iter().position(Fighter::is_fighting))
                .flatten();
            (winner, arena.tick)
        })
        .collect()
}

/// The wins added up, by entry index (by class index for `classes`).
struct Tallies {
    /// Against the other classes only.
    classes: Vec<Tally>,
    against: Vec<Vec<Tally>>,
    by_kind: Vec<[Tally; ArenaKind::ALL.len()]>,
    by_element: Vec<[Tally; Element::ALL.len()]>,
    ticks: u64,
    no_winner: u64,
}

impl Tallies {
    fn count(data: &DesignData, entries: &[Entry], jobs: &[Job], outcomes: &[Outcome]) -> Self {
        let n = entries.len();
        let mut t = Self {
            classes: vec![Tally::default(); data.classes.len()],
            against: vec![vec![Tally::default(); n]; n],
            by_kind: vec![[Tally::default(); ArenaKind::ALL.len()]; n],
            by_element: vec![[Tally::default(); Element::ALL.len()]; n],
            ticks: 0,
            no_winner: 0,
        };
        for (job, &(winner, duration)) in jobs.iter().zip(outcomes) {
            t.ticks += u64::from(duration);
            t.no_winner += u64::from(winner.is_none());
            for side in 0..2 {
                let (me, other) = (job.entries[side], job.entries[1 - side]);
                let won = winner == Some(side);
                t.against[me][other].add(won);
                t.by_kind[me][job.kind].add(won);
                t.by_element[me][job.element].add(won);
                if entries[me].class != entries[other].class {
                    let class = usize::try_from(entries[me].class.0).unwrap_or(usize::MAX);
                    t.classes[class].add(won);
                }
            }
        }
        t
    }

    /// Prints each class's win rate; how many are outside `min` to `max` %.
    fn print_classes(&self, data: &DesignData, (min, max): (u64, u64)) -> usize {
        let mut failures = 0;
        println!("\nwin rate against the other classes (aim: {min} to {max} %):");
        for (class, tally) in data.classes.iter().zip(&self.classes) {
            let flag = if tally.outside(min, max) {
                failures += 1;
                "  out of range"
            } else {
                ""
            };
            println!("  {:<16} {:>5} %{flag}", class.key, tally.percent());
        }
        failures
    }

    /// Prints the win rate of each entry against each other one, then those beating
    /// another class's entry more than `matchup` % of the time; how many these are.
    fn print_matchups(&self, entries: &[Entry], matchup: u64) -> usize {
        let width = name_width(entries);
        println!("\nwin rate of each row against each column (%):");
        print!("{:width$}   ", "");
        for i in 0..entries.len() {
            print!(" {i:>5}");
        }
        println!();
        let mut lopsided = Vec::new();
        for (a, entry) in entries.iter().enumerate() {
            print!("{a:>2} {:<width$}", entry.name);
            for (b, tally) in self.against[a].iter().enumerate() {
                if a == b {
                    print!(" {:>5}", ".");
                    continue;
                }
                print!(" {:>5}", tally.percent());
                if entry.class != entries[b].class && tally.outside(0, matchup) {
                    lopsided.push(format!(
                        "  {} beats {} {} % of the time",
                        entry.name,
                        entries[b].name,
                        tally.percent()
                    ));
                }
            }
            println!();
        }
        if !lopsided.is_empty() {
            println!("\nloadouts winning over {matchup} % against another class's:");
            for line in &lopsided {
                println!("{line}");
            }
        }
        lopsided.len()
    }

    /// Prints each entry's win rate by arena kind and by element of the floor.
    fn print_floors(&self, entries: &[Entry]) {
        let width = name_width(entries);
        println!("\nwin rate by arena kind and by element of the floor (%):");
        print!("{:width$}", "");
        for kind in ArenaKind::ALL {
            print!(" {:>8}", format!("{kind:?}"));
        }
        print!("  |");
        for element in Element::ALL {
            print!(" {:>8}", format!("{element:?}"));
        }
        println!();
        for (a, entry) in entries.iter().enumerate() {
            print!("{:<width$}", entry.name);
            for tally in self.by_kind[a] {
                print!(" {:>8}", tally.percent());
            }
            print!("  |");
            for tally in self.by_element[a] {
                print!(" {:>8}", tally.percent());
            }
            println!();
        }
    }
}

/// The length of the longest entry name, to line the tables up.
fn name_width(entries: &[Entry]) -> usize {
    entries.iter().map(|e| e.name.len()).max().unwrap_or(0)
}
