//! `career --seed 7 --weeks 10`: follows one generated fighter week after week and
//! prints what each week changed (form, morale, fame, stats, injuries, illness).

use std::fmt::Write;
use std::process::ExitCode;

use lanista_core::design_data::DesignData;
use lanista_core::fighter::Stats;
use lanista_core::fighter::career::{Activity, CareerFighter, FightSummary, Status, WeekReport};
use lanista_core::fighter::fame::FightResult;
use lanista_core::fighter::injury::Location;
use lanista_core::fighter::personality::Personality;
use lanista_core::state::{Decisions, GameState, next_week};
use tools::{one_decimal, options};

fn main() -> ExitCode {
    let [seed, weeks] = match options(std::env::args().skip(1), [("--seed", 7), ("--weeks", 10)]) {
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
    let mut state = GameState::new(seed, &data);
    println!("seed {seed}, money {}", state.money);
    print!("{}", describe(&state.fighters[0], &data));
    for week in 1..=weeks {
        let before = state.fighters[0].clone();
        let money = state.money;
        // No plan: the club follows `suggested_plan`.
        let mut summary = next_week(&mut state, &Decisions::default(), &data);
        let Some(report) = summary.reports.swap_remove(0) else {
            break;
        };
        print!(
            "\n{}",
            week_text(week, &before, &state.fighters[0], &report, &data)
        );
        let moves: Vec<String> = summary
            .ledger
            .iter()
            .map(|t| format!("{:?} {:+}", t.flow, t.amount))
            .collect();
        println!("  money {money} -> {}: {}", state.money, moves.join(", "));
    }
    let fighter = &state.fighters[0];
    match fighter.status {
        Status::Active => {}
        Status::Retired => println!("\nretired at {}", fighter.age),
        Status::Dead => println!("\ndied at {}", fighter.age),
        Status::Sold => println!("\nsold at {}", fighter.age),
        Status::Released => println!("\nreleased at {}", fighter.age),
    }
    ExitCode::SUCCESS
}

fn class_name(fighter: &CareerFighter, data: &DesignData) -> String {
    data.class(fighter.class)
        .map_or_else(|| "?".into(), |c| c.name.clone())
}

fn describe(fighter: &CareerFighter, data: &DesignData) -> String {
    let p: &Personality = &fighter.personality;
    let mut text = String::new();
    // Writing to a String cannot fail.
    let _ = writeln!(
        text,
        "{}, {} years old, {:?}-handed",
        class_name(fighter, data),
        fighter.age,
        fighter.dominant_hand
    );
    let _ = writeln!(
        text,
        "personality: aggression {}, caution {}, showmanship {}, determination {} (gives up at {} HP)",
        p.aggression.value(),
        p.caution.value(),
        p.showmanship.value(),
        p.determination.value(),
        p.surrender_hp(fighter.stats.hp, &data.fighter.personality)
    );
    let _ = writeln!(text, "stats:     {}", stats_text(fighter.stats));
    let _ = writeln!(
        text,
        "potential: {} (hidden in the game)",
        stats_text(fighter.potential)
    );
    text
}

/// `n week` or `n weeks`.
fn weeks(n: u32) -> String {
    if n == 1 {
        "1 week".into()
    } else {
        format!("{n} weeks")
    }
}

fn stats_text(s: Stats) -> String {
    format!(
        "HP {} AC {} AL {} VIT {} DC {} DL {}",
        s.hp, s.melee_attack, s.ranged_attack, s.speed, s.melee_defense, s.ranged_defense
    )
}

/// `name before -> after`, or `name value` when it did not change.
fn change(name: &str, before: u32, after: u32) -> String {
    if before == after {
        format!("{name} {after}")
    } else {
        format!("{name} {before} -> {after}")
    }
}

fn week_text(
    week: u64,
    before: &CareerFighter,
    after: &CareerFighter,
    report: &WeekReport,
    data: &DesignData,
) -> String {
    let mut text = String::new();
    let activity = match report.plan.activity {
        Activity::Rest => "rest".to_owned(),
        Activity::Train => "training".to_owned(),
        Activity::Fight(id) => format!(
            "fight in {}",
            data.world.colosseum(id).map_or("?", |c| c.name.as_str())
        ),
        Activity::Melee(id) => format!(
            "melee in {}",
            data.world.colosseum(id).map_or("?", |c| c.name.as_str())
        ),
        Activity::TitleFight(id) => format!(
            "title fight in {}",
            data.world.colosseum(id).map_or("?", |c| c.name.as_str())
        ),
        Activity::Challenge(id, rival) => format!(
            "challenge to rival {} of club {} in {}",
            rival.fighter + 1,
            rival.club + 1,
            data.world.colosseum(id).map_or("?", |c| c.name.as_str())
        ),
    };
    let _ = writeln!(text, "week {week} (age {}): {activity}", before.age);
    if let Some(fight) = &report.fight {
        let _ = writeln!(text, "  {}", fight_text(fight, data));
    }
    if let Some(injury) = &report.new_injury {
        let _ = if injury.is_permanent() {
            writeln!(text, "  injured for good: {:?}", injury.location)
        } else {
            writeln!(
                text,
                "  injured: {:?} {:?}, {}",
                injury.location,
                injury.severity,
                weeks(injury.weeks_left)
            )
        };
    }
    for injury in &report.healed_injuries {
        let _ = writeln!(
            text,
            "  healed: {:?} {:?}",
            injury.location, injury.severity
        );
    }
    if let Some(illness) = &report.new_illness {
        let _ = writeln!(
            text,
            "  fell ill: {:?}, {}",
            illness.level,
            weeks(illness.weeks_left)
        );
    }
    if report.illness_healed {
        let _ = writeln!(text, "  illness healed");
    }
    if report.birthday {
        let _ = writeln!(text, "  one year older: {}", after.age);
        if report.season_gains > 0 {
            let _ = writeln!(text, "  season's record: {} gains", report.season_gains);
        }
    }
    let _ = writeln!(
        text,
        "  {}, {}, {}",
        change("form", before.form.value(), after.form.value()),
        change("morale", before.morale.value(), after.morale.value()),
        change("fame", before.fame.value(), after.fame.value())
    );
    if before.stats != after.stats {
        let _ = writeln!(text, "  stats: {}", stats_text(after.stats));
    }
    let lasting: Vec<String> = after
        .injuries
        .iter()
        .map(|i| {
            if i.is_permanent() {
                format!("{:?} permanent", i.location)
            } else {
                format!(
                    "{:?} {:?} {} left",
                    i.location,
                    i.severity,
                    weeks(i.weeks_left)
                )
            }
        })
        .chain(
            after
                .illness
                .map(|i| format!("ill {:?} {} left", i.level, weeks(i.weeks_left))),
        )
        .collect();
    if !lasting.is_empty() {
        let _ = writeln!(text, "  condition: {}", lasting.join(", "));
    }
    text
}

fn fight_text(fight: &FightSummary, data: &DesignData) -> String {
    let mine = fight.mine();
    let knocked_out = mine.is_some_and(|f| f.knocked_out);
    let severed = mine.and_then(|f| f.severed);
    let result = match (fight.result, fight.died) {
        (_, true) if severed == Some(Location::Head) => "beheaded",
        (_, true) => "killed",
        (FightResult::Lost, _) if severed.is_some() => "lost a limb",
        (FightResult::Lost, _) if knocked_out => "knocked out",
        (FightResult::Won, _) => "won",
        (FightResult::Lost, _) => "lost",
        (FightResult::GaveUp, _) => "gave up",
        (FightResult::Unfinished, _) => "no winner after one minute",
    };
    format!(
        "against {} ({}fight seed {}): {result} in {} s, fame {:+}",
        fight
            .opponent()
            .map_or_else(|| "?".into(), |o| class_name(o, data)),
        if fight.against_stronger {
            "stronger, "
        } else {
            ""
        },
        fight.seed,
        one_decimal(u64::from(fight.ticks), 30),
        fight.fame_change
    )
}
