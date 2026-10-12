//! Reference fights: a given setup and seed always gives the same log.
//! A changed snapshot means the fight changed: review it with `cargo insta review`.

use lanista_core::ai::{CombatAi, UtilityAi};
use lanista_core::combat::{Arena, Vec2, run};
use lanista_core::design_data::DesignData;
use lanista_core::fighter::{ClassId, Fighter, FighterId, Stats};
use lanista_core::log::{Report, log_text};

/// One minute of fight.
const MAX_TICKS: u32 = 1800;

const STATS: Stats = Stats {
    hp: 100,
    melee_attack: 20,
    ranged_attack: 20,
    speed: 30,
    melee_defense: 10,
    ranged_defense: 10,
};

/// A duel between utility AIs on a 20 × 20 m floor, starting 14 m apart.
fn duel(classes: [u32; 2], seed: u64) -> String {
    let data = DesignData::from_ron(include_str!("../../../data/classes.ron")).unwrap();
    let fighter = |id: u32, x: f64| {
        let class = ClassId(classes[usize::try_from(id).unwrap()]);
        let class_data = data.class(class).unwrap();
        Fighter::new(FighterId(id), class, class_data, STATS, Vec2::new(x, 10.0))
    };
    let mut arena = Arena::new(20.0, 20.0, vec![fighter(0, 3.0), fighter(1, 17.0)], seed);
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
