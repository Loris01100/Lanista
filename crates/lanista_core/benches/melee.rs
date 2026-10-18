//! How long fights take to play, without drawing: a melee of 20 from start to end, one
//! tick of it (the game plays two to three a frame at x1), and the three fights a club
//! watches at once, one tick each in turn, as the combat screen plays them.
//! `cargo bench -p lanista_core`.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use lanista_core::combat::{Arena, ArenaKind, Vec2, tick};
use lanista_core::design_data::DesignData;
use lanista_core::duel::{random_duel, utility_ais};
use lanista_core::fighter::FighterId;
use lanista_core::fighter::career::CareerFighter;
use lanista_core::fighter::equipment::Refused;
use lanista_core::melee::melee_arena;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// A melee of 20 generated fighters on `kind`'s floor.
fn melee(data: &DesignData, kind: ArenaKind, seed: u64) -> Arena {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let fighters = (0..20)
        .map(|id| {
            let f = CareerFighter::random(&mut rng, data);
            f.combat_fighter(FighterId(id), Refused::NONE, data, Vec2::new(0.0, 0.0))
        })
        .collect();
    melee_arena(fighters, kind, seed, &data.combat)
}

/// Plays `arenas` one tick each in turn until all are over or out of time.
fn play_together(arenas: &mut [Arena]) {
    let mut ais: Vec<_> = arenas.iter().map(utility_ais).collect();
    loop {
        let mut running = false;
        for (arena, ais) in arenas.iter_mut().zip(&mut ais) {
            if !arena.is_over() && arena.tick < arena.max_ticks {
                tick(arena, ais);
                running = true;
            }
        }
        if !running {
            break;
        }
    }
}

fn benches(c: &mut Criterion) {
    let data = DesignData::load().expect("load the design data");
    for kind in ArenaKind::ALL {
        let start = melee(&data, kind, 1);
        c.bench_function(&format!("melee of 20 on {kind:?}, start to end"), |b| {
            b.iter_batched(
                || vec![start.clone()],
                |mut arenas| play_together(black_box(&mut arenas)),
                BatchSize::SmallInput,
            );
        });
    }
    // One tick in the thick of it: the first 10 s played, 20 fighters still around.
    let mut busy = melee(&data, ArenaKind::Pillars, 1);
    let mut ais = utility_ais(&busy);
    for _ in 0..300 {
        tick(&mut busy, &mut ais);
    }
    c.bench_function("one tick of a melee of 20", |b| {
        b.iter_batched(
            || (busy.clone(), utility_ais(&busy)),
            |(mut arena, mut ais)| tick(black_box(&mut arena), &mut ais),
            BatchSize::SmallInput,
        );
    });
    let three = vec![
        melee(&data, ArenaKind::Sand, 2),
        random_duel(&data, 42),
        random_duel(&data, 7),
    ];
    c.bench_function("a melee of 20 and two duels together", |b| {
        b.iter_batched(
            || three.clone(),
            |mut arenas| play_together(black_box(&mut arenas)),
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(melees, benches);
criterion_main!(melees);
