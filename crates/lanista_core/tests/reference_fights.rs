//! Reference fights: a given setup and seed always gives the same log.
//! A changed snapshot means the fight changed: review it with `cargo insta review`.

use lanista_core::ai::{CombatAi, UtilityAi};
use lanista_core::combat::{Arena, ArenaKind, Vec2, run};
use lanista_core::design_data::DesignData;
use lanista_core::duel::{MAX_TICKS, STATS, play, random_duel};
use lanista_core::fighter::personality::TraitValue;
use lanista_core::fighter::{ClassId, Fighter, FighterId};
use lanista_core::log::{Report, log_text, places};
use lanista_core::melee::melee_arena;

/// A duel between utility AIs on a 20 × 20 m floor, starting 14 m apart.
fn duel(classes: [u32; 2], seed: u64) -> String {
    let data = DesignData::load().unwrap();
    let fighter = |id: u32, x: f64| {
        let class = ClassId(classes[usize::try_from(id).unwrap()]);
        let class_data = data.class(class).unwrap();
        Fighter::new(FighterId(id), class, class_data, STATS, Vec2::new(x, 10.0))
    };
    let mut arena = Arena::new(20.0, 20.0, vec![fighter(0, 3.0), fighter(1, 17.0)], seed)
        .with_rules(&data.combat);
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

/// The standard duel of seed 128: a hunter's first shot stuns the first fighter, who
/// does not get up.
#[test]
fn a_standard_duel_with_a_knock_out() {
    insta::assert_snapshot!(standard_duel(128));
}

/// The standard duel of seed 87: a cutting hit takes off the second fighter's arm.
#[test]
fn a_standard_duel_with_a_lost_limb() {
    insta::assert_snapshot!(standard_duel(87));
}

/// The standard duel of seed 97: a gladiator makes a hunter's arm bleed, then takes
/// its head off.
#[test]
fn a_standard_duel_with_a_beheading() {
    insta::assert_snapshot!(standard_duel(97));
}

/// A murmillo against an augur, both in their first loadout, on the floor of `kind`:
/// see [`loadout_duel`].
fn arena_duel(kind: ArenaKind, seed: u64) -> String {
    loadout_duel(kind, [(0, 0), (1, 0)], seed)
}

/// Two fighters of the given (class, loadout) on the floor of `kind`, starting at 15 %
/// and 85 % of its width, mid-height, as career duels do.
fn loadout_duel(kind: ArenaKind, fighters: [(u32, usize); 2], seed: u64) -> String {
    let data = DesignData::load().unwrap();
    let layout = data.combat.arenas.get(kind);
    let middle = layout.height / 2.0;
    let fighter = |id: u32, x: f64| {
        let (class, loadout) = fighters[usize::try_from(id).unwrap()];
        let class = ClassId(class);
        let class_data = data.class(class).unwrap();
        Fighter::in_loadout(
            FighterId(id),
            class,
            (class_data, loadout),
            STATS,
            Vec2::new(x, middle),
        )
    };
    let fighters = vec![
        fighter(0, layout.width * 0.15),
        fighter(1, layout.width * 0.85),
    ];
    let mut arena = Arena::new(layout.width, layout.height, fighters, seed)
        .with_rules(&data.combat)
        .with_layout(layout);
    play(&mut arena);
    format!("{}\n{}", log_text(&arena), Report::new(&arena))
}

#[test]
fn a_duel_on_sand() {
    insta::assert_snapshot!(arena_duel(ArenaKind::Sand, 42));
}

#[test]
fn a_duel_among_pillars() {
    insta::assert_snapshot!(arena_duel(ArenaKind::Pillars, 42));
}

#[test]
fn a_duel_in_a_narrow_arena() {
    insta::assert_snapshot!(arena_duel(ArenaKind::Narrow, 42));
}

/// A gladiator with two swords (Lacération's two blows) against a field master with
/// two daggers (two mines a cast), gathering and throwing sand.
#[test]
fn two_swords_against_two_daggers_on_sand() {
    insta::assert_snapshot!(loadout_duel(ArenaKind::Sand, [(2, 1), (3, 1)], 42));
}

/// A hunter with a heavy bow against a field master with a dagger and a shield among
/// pillars: its rain of arrows flies over them, its trap catches the thrower.
#[test]
fn a_heavy_bow_against_a_dagger_and_shield_among_pillars() {
    insta::assert_snapshot!(loadout_duel(ArenaKind::Pillars, [(4, 0), (3, 0)], 42));
}

/// A melee of 8, murmillos and augurs in turn with the standard duel's stats, timid
/// enough to make pacts, bold enough to break some, on sand: its log and places.
fn reference_melee(seed: u64) -> String {
    let data = DesignData::load().unwrap();
    let fighters = (0..8)
        .map(|id| {
            let class = ClassId(id % 2);
            let class_data = data.class(class).unwrap();
            let mut f = Fighter::new(FighterId(id), class, class_data, STATS, Vec2::new(0.0, 0.0));
            f.condition.personality.boldness = TraitValue::new(10 + 5 * id);
            f
        })
        .collect();
    let mut arena = melee_arena(fighters, ArenaKind::Sand, seed, &data.combat);
    play(&mut arena);
    format!(
        "{}\n{}\nplaces: {:?}",
        log_text(&arena),
        Report::new(&arena),
        places(&arena)
    )
}

#[test]
fn a_melee_of_seed_3() {
    insta::assert_snapshot!(reference_melee(3));
}

#[test]
fn a_melee_of_seed_11() {
    insta::assert_snapshot!(reference_melee(11));
}
