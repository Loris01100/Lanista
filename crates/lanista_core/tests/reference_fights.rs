//! Reference fights: a given setup and seed always gives the same log.
//! A changed snapshot means the fight changed: review it with `cargo insta review`.

use lanista_core::ai::{CombatAi, UtilityAi};
use lanista_core::combat::{Arena, Vec2, run};
use lanista_core::design_data::DesignData;
use lanista_core::duel::{MAX_TICKS, STATS, play, random_duel};
use lanista_core::fighter::{ClassId, Fighter, FighterId};
use lanista_core::log::{Report, log_text};

/// A duel between utility AIs on a 20 × 20 m floor, starting 14 m apart.
fn duel(classes: [u32; 2], seed: u64) -> String {
    let data = DesignData::load().unwrap();
    let fighter = |id: u32, x: f64| {
        let class = ClassId(classes[usize::try_from(id).unwrap()]);
        let class_data = data.class(class).unwrap();
        Fighter::new(FighterId(id), class, class_data, STATS, Vec2::new(x, 10.0))
    };
    let mut arena = Arena::new(20.0, 20.0, vec![fighter(0, 3.0), fighter(1, 17.0)], seed)
        .with_rules(data.combat);
    let mut ais: Vec<Box<dyn CombatAi>> = vec![Box::new(UtilityAi), Box::new(UtilityAi)];
    run(&mut arena, &mut ais, MAX_TICKS);
    format!("{}\n{}", log_text(&arena), Report::new(&arena))
}

#[test]
fn murmillo_against_augur() {
    insta::assert_snapshot!(duel([0, 1], 42));
}

#[test]
fn murmillo_against_murmillo() {
    insta::assert_snapshot!(duel([0, 0], 42));
}

/// The standard duel of `seed`: its log and report.
fn standard_duel(seed: u64) -> String {
    let mut arena = random_duel(&DesignData::load().unwrap(), seed);
    play(&mut arena);
    format!(
        "{}
{}",
        log_text(&arena),
        Report::new(&arena)
    )
}

/// The standard duel of seed 15: the first fighter is stunned twice, gets up once, then
/// is knocked out.
#[test]
fn a_standard_duel_with_a_knock_out() {
    insta::assert_snapshot!(standard_duel(15));
}

/// The standard duel of seed 62: a cutting hit takes off the second fighter's leg.
#[test]
fn a_standard_duel_with_a_lost_limb() {
    insta::assert_snapshot!(standard_duel(62));
}

/// The standard duel of seed 13: the killing blow, a cutting one, takes the head off.
#[test]
fn a_standard_duel_with_a_beheading() {
    insta::assert_snapshot!(standard_duel(13));
}
