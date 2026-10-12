use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use serde::Deserialize;

use crate::ai::{Action, CombatAi};
use crate::design_data::SpellData;
use crate::fighter::{Fighter, FighterId, Stats};
use crate::log::{Event, EventKind};

/// Duration of a tick, in seconds: 30 ticks per second.
pub const DT: f64 = 1.0 / 30.0;
/// The AIs decide every 6 ticks, 5 times per second.
pub const TICKS_PER_DECISION: u32 = 6;
// ponytail: same radius and speed scale for everyone, move to `data/` when classes are loaded.
/// Fighter radius, in meters.
pub const RADIUS: f64 = 0.5;
/// Meters per second per point of speed.
pub const METERS_PER_SPEED: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn length(self) -> f64 {
        libm::sqrt(self.x * self.x + self.y * self.y)
    }
}

/// A fight in progress. `FighterId(i)` is the fighter at index `i` of `fighters`.
/// The floor spans `0..width` × `0..height`, in meters.
#[derive(Debug, Clone, PartialEq)]
pub struct Arena {
    pub width: f64,
    pub height: f64,
    /// Ticks elapsed since the start of the fight.
    pub tick: u32,
    pub fighters: Vec<Fighter>,
    /// Replaying the fight with this seed gives the same fight.
    pub seed: u64,
    /// The fight's only source of randomness, created from `seed`.
    pub rng: ChaCha8Rng,
    /// What happened, in order.
    pub log: Vec<Event>,
}

impl Arena {
    #[must_use]
    pub fn new(width: f64, height: f64, fighters: Vec<Fighter>, seed: u64) -> Self {
        Self {
            width,
            height,
            tick: 0,
            fighters,
            seed,
            rng: ChaCha8Rng::seed_from_u64(seed),
            log: Vec::new(),
        }
    }

    /// At most one fighter left alive.
    #[must_use]
    pub fn is_over(&self) -> bool {
        self.fighters.iter().filter(|f| f.is_alive()).count() <= 1
    }

    #[must_use]
    pub fn fighter(&self, id: FighterId) -> Option<&Fighter> {
        self.fighters.get(usize::try_from(id.0).ok()?)
    }
}

/// Advances the fight by one tick. `ais[i]` controls `fighters[i]`.
///
/// # Panics
/// If there is not exactly one AI per fighter.
pub fn tick(arena: &mut Arena, ais: &mut [Box<dyn CombatAi>]) {
    assert_eq!(arena.fighters.len(), ais.len(), "one AI per fighter");
    if arena.tick.is_multiple_of(TICKS_PER_DECISION) {
        for (i, ai) in ais.iter_mut().enumerate() {
            let fighter = &arena.fighters[i];
            if fighter.is_alive() {
                let action = ai.decide(arena, fighter.id);
                arena.fighters[i].action = action;
            }
        }
    }
    cool_down(arena);
    move_fighters(arena);
    separate(arena);
    keep_inside(arena);
    cast_spells(arena);
    arena.tick += 1;
}

/// Plays ticks until the fight is over or `max_ticks` ticks have passed in total.
pub fn run(arena: &mut Arena, ais: &mut [Box<dyn CombatAi>], max_ticks: u32) {
    while !arena.is_over() && arena.tick < max_ticks {
        tick(arena, ais);
    }
}

/// Whether a hit uses melee attack against melee defense (AC against DC)
/// or ranged attack against ranged defense (AL against DL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Range {
    Melee,
    Ranged,
}

/// Damage of a hit: attack × 100 / (100 + defense), rounded up so that
/// a nonzero attack always deals at least 1, whatever the defense.
#[must_use]
pub fn damage(attacker: &Stats, defender: &Stats, range: Range) -> u32 {
    let (attack, defense) = match range {
        Range::Melee => (attacker.melee_attack, defender.melee_defense),
        Range::Ranged => (attacker.ranged_attack, defender.ranged_defense),
    };
    // u64: attack × 100 overflows u32 above 42 million.
    let damage = (u64::from(attack) * 100).div_ceil(100 + u64::from(defense));
    // Never above the attack, so it fits in a u32.
    u32::try_from(damage).unwrap_or(attack)
}

/// Damage of a spell: the `damage` formula scaled by its power, in percent, rounded up.
#[must_use]
pub fn spell_damage(attacker: &Stats, defender: &Stats, spell: &SpellData) -> u32 {
    let base = u64::from(damage(attacker, defender, spell.range));
    let scaled = (base * u64::from(spell.power)).div_ceil(100);
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

/// Distance between the edges of two fighters, in meters; 0 when they touch.
#[must_use]
pub fn edge_distance(a: &Fighter, b: &Fighter) -> f64 {
    let offset = Vec2::new(b.position.x - a.position.x, b.position.y - a.position.y);
    (offset.length() - 2.0 * RADIUS).max(0.0)
}

fn cool_down(arena: &mut Arena) {
    for f in &mut arena.fighters {
        for c in &mut f.cooldowns {
            *c = c.saturating_sub(1);
        }
    }
}

/// Casts the spell each living fighter chose, if it is ready, affordable and the
/// target is alive and in reach; a cast fighter then waits for its next decision.
// ponytail: resolved in index order, so of two fighters killing each other on the
// same tick, the lower index survives.
fn cast_spells(arena: &mut Arena) {
    for i in 0..arena.fighters.len() {
        let caster = &arena.fighters[i];
        let Action::Cast { spell, target } = caster.action else {
            continue;
        };
        let Some(t) = usize::try_from(target.0).ok() else {
            continue;
        };
        let Some(defender) = arena.fighters.get(t) else {
            continue;
        };
        if !caster.is_alive() || !defender.is_alive() || caster.id == target {
            continue;
        }
        if !caster.can_cast(spell) || edge_distance(caster, defender) > caster.spells[spell].reach {
            continue;
        }
        let amount = spell_damage(&caster.stats, &defender.stats, &caster.spells[spell]);
        let caster = &mut arena.fighters[i];
        caster.spend(spell);
        caster.action = Action::Wait;
        let caster = caster.id;
        let defender = &mut arena.fighters[t];
        let hp_before = defender.hp;
        defender.take_damage(amount);
        let event = |kind| Event {
            tick: arena.tick,
            kind,
        };
        let cast = event(EventKind::Cast {
            caster,
            target,
            spell,
            damage: hp_before - defender.hp,
        });
        let died = !defender.is_alive();
        arena.log.push(cast);
        if died {
            arena.log.push(event(EventKind::Death { fighter: target }));
        }
    }
}

fn move_fighters(arena: &mut Arena) {
    for f in arena.fighters.iter_mut().filter(|f| f.is_alive()) {
        if let Action::Move(direction) = f.action {
            let length = direction.length();
            if length > 0.0 {
                let step = f64::from(f.stats.speed) * METERS_PER_SPEED * DT / length;
                f.position.x += direction.x * step;
                f.position.y += direction.y * step;
            }
        }
    }
}

/// Pushes overlapping living fighters apart, each by half the overlap.
// ponytail: one pass in index order, a crowded melee may keep small overlaps; iterate if visible.
fn separate(arena: &mut Arena) {
    let fighters = &mut arena.fighters;
    for i in 0..fighters.len() {
        for j in i + 1..fighters.len() {
            let (a, b) = (&fighters[i], &fighters[j]);
            if !a.is_alive() || !b.is_alive() {
                continue;
            }
            let offset = Vec2::new(b.position.x - a.position.x, b.position.y - a.position.y);
            let distance = offset.length();
            let overlap = 2.0 * RADIUS - distance;
            if overlap <= 0.0 {
                continue;
            }
            // Same center: no direction to push along, pick one.
            let normal = if distance > 0.0 {
                Vec2::new(offset.x / distance, offset.y / distance)
            } else {
                Vec2::new(1.0, 0.0)
            };
            let half = overlap / 2.0;
            fighters[i].position.x -= normal.x * half;
            fighters[i].position.y -= normal.y * half;
            fighters[j].position.x += normal.x * half;
            fighters[j].position.y += normal.y * half;
        }
    }
}

fn keep_inside(arena: &mut Arena) {
    for f in &mut arena.fighters {
        f.position.x = f.position.x.clamp(RADIUS, arena.width - RADIUS);
        f.position.y = f.position.y.clamp(RADIUS, arena.height - RADIUS);
    }
}

#[cfg(test)]
mod tests {
    use rand_chacha::rand_core::Rng;

    use super::*;
    use crate::fighter::{FighterId, Stats, test_fighter as fighter};

    struct Fixed(Action);

    impl CombatAi for Fixed {
        fn decide(&mut self, _: &Arena, _: FighterId) -> Action {
            self.0
        }
    }

    const HIT: Action = Action::Cast {
        spell: 0,
        target: FighterId(1),
    };
    /// Damage of the test spell between two test fighters: 10 × 100 / 110, rounded up.
    const HIT_DAMAGE: u32 = 10;

    /// Fighter 0 hits fighter 1, `gap` meters between their edges.
    fn duel(gap: f64) -> Arena {
        let fighters = vec![fighter(0, 5.0, 5.0), fighter(1, 6.0 + gap, 5.0)];
        Arena::new(20.0, 20.0, fighters, 0)
    }

    #[test]
    fn casts_on_a_target_in_reach() {
        let mut arena = duel(0.5);
        play(&mut arena, &[HIT, Action::Wait], 1);
        assert_eq!(arena.fighters[1].hp, 100 - HIT_DAMAGE);
        assert_eq!(arena.fighters[0].endurance.current, 40);
        assert_eq!(arena.fighters[0].cooldowns[0], 30);
        assert_eq!(arena.fighters[0].action, Action::Wait);
    }

    #[test]
    fn does_not_cast_out_of_reach() {
        let mut arena = duel(2.0);
        play(&mut arena, &[HIT, Action::Wait], 1);
        assert_eq!(arena.fighters[1].hp, 100);
        assert_eq!(arena.fighters[0].endurance.current, 50);
    }

    #[test]
    fn waits_for_the_cooldown() {
        let mut arena = duel(0.5);
        play(&mut arena, &[HIT, Action::Wait], 30);
        assert_eq!(arena.fighters[1].hp, 100 - HIT_DAMAGE);
        // Tick 30: cooldown over and a new decision.
        play(&mut arena, &[HIT, Action::Wait], 1);
        assert_eq!(arena.fighters[1].hp, 100 - 2 * HIT_DAMAGE);
    }

    #[test]
    fn stops_casting_without_resource() {
        let mut arena = duel(0.5);
        play(&mut arena, &[HIT, Action::Wait], 600);
        // 50 endurance, 10 per cast: 5 casts.
        assert_eq!(arena.fighters[1].hp, 100 - 5 * HIT_DAMAGE);
        assert_eq!(arena.fighters[0].endurance.current, 0);
    }

    fn play(arena: &mut Arena, actions: &[Action], ticks: u32) {
        let mut ais: Vec<Box<dyn CombatAi>> = actions
            .iter()
            .map(|&a| Box::new(Fixed(a)) as Box<dyn CombatAi>)
            .collect();
        for _ in 0..ticks {
            tick(arena, &mut ais);
        }
    }

    #[test]
    fn moves_at_its_speed() {
        let mut arena = Arena::new(20.0, 20.0, vec![fighter(0, 5.0, 5.0)], 0);
        // The direction's length must not change the speed.
        play(&mut arena, &[Action::Move(Vec2::new(10.0, 0.0))], 30);
        // Speed 30 → 3 m/s, for one second.
        let position = arena.fighters[0].position;
        assert!((position.x - 8.0).abs() < 1e-9, "{position:?}");
        assert!((position.y - 5.0).abs() < 1e-9, "{position:?}");
    }

    #[test]
    fn two_fighters_do_not_overlap() {
        let mut arena = Arena::new(
            20.0,
            20.0,
            vec![fighter(0, 5.0, 5.0), fighter(1, 8.0, 5.0)],
            0,
        );
        let actions = [
            Action::Move(Vec2::new(1.0, 0.0)),
            Action::Move(Vec2::new(-1.0, 0.0)),
        ];
        play(&mut arena, &actions, 60);
        let (a, b) = (arena.fighters[0].position, arena.fighters[1].position);
        let distance = Vec2::new(b.x - a.x, b.y - a.y).length();
        assert!(distance >= 2.0 * RADIUS - 1e-9, "{distance}");
    }

    fn stats(
        melee_attack: u32,
        ranged_attack: u32,
        melee_defense: u32,
        ranged_defense: u32,
    ) -> Stats {
        Stats {
            hp: 100,
            melee_attack,
            ranged_attack,
            speed: 30,
            melee_defense,
            ranged_defense,
        }
    }

    #[test]
    fn defense_reduces_damage() {
        let attacker = stats(100, 0, 0, 0);
        assert_eq!(damage(&attacker, &stats(0, 0, 0, 0), Range::Melee), 100);
        assert_eq!(damage(&attacker, &stats(0, 0, 100, 0), Range::Melee), 50);
        assert_eq!(damage(&attacker, &stats(0, 0, 300, 0), Range::Melee), 25);
    }

    #[test]
    fn melee_and_ranged_use_their_own_stats() {
        let attacker = stats(100, 60, 0, 0);
        let defender = stats(0, 0, 100, 200);
        assert_eq!(damage(&attacker, &defender, Range::Melee), 50);
        assert_eq!(damage(&attacker, &defender, Range::Ranged), 20);
    }

    #[test]
    fn defense_never_cancels_damage() {
        let attacker = stats(1, 0, 0, 0);
        let defender = stats(0, 0, u32::MAX, 0);
        assert_eq!(damage(&attacker, &defender, Range::Melee), 1);
        assert_eq!(damage(&attacker, &defender, Range::Ranged), 0);
    }

    #[test]
    fn damage_does_not_overflow() {
        let attacker = stats(u32::MAX, 0, 0, 0);
        assert_eq!(
            damage(&attacker, &stats(0, 0, 0, 0), Range::Melee),
            u32::MAX
        );
    }

    #[test]
    fn hp_stop_at_zero() {
        let mut f = fighter(0, 5.0, 5.0);
        f.take_damage(30);
        assert_eq!(f.hp, 70);
        f.take_damage(1000);
        assert_eq!(f.hp, 0);
        assert!(!f.is_alive());
    }

    #[test]
    fn stays_inside_the_arena() {
        let mut arena = Arena::new(10.0, 10.0, vec![fighter(0, 5.0, 5.0)], 0);
        play(&mut arena, &[Action::Move(Vec2::new(-1.0, 1.0))], 300);
        assert_eq!(arena.fighters[0].position, Vec2::new(RADIUS, 10.0 - RADIUS));
    }

    fn draws(arena: &mut Arena) -> [u64; 4] {
        std::array::from_fn(|_| arena.rng.next_u64())
    }

    #[test]
    fn same_seed_same_draws() {
        let mut a = Arena::new(10.0, 10.0, vec![], 42);
        let mut b = Arena::new(10.0, 10.0, vec![], 42);
        assert_eq!(draws(&mut a), draws(&mut b));
    }

    #[test]
    fn other_seed_other_draws() {
        let mut a = Arena::new(10.0, 10.0, vec![], 42);
        let mut b = Arena::new(10.0, 10.0, vec![], 43);
        assert_ne!(draws(&mut a), draws(&mut b));
    }

    #[test]
    fn known_draws_for_a_seed() {
        // Fixed values: fail if a dependency update or a platform changes the sequence.
        let mut arena = Arena::new(10.0, 10.0, vec![], 42);
        assert_eq!(
            draws(&mut arena),
            [
                12_578_764_544_318_200_737,
                17_529_487_244_874_322_312,
                7_886_285_670_807_131_020,
                11_572_758_976_476_374_866,
            ]
        );
    }
}
