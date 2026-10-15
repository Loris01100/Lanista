use rand::RngExt;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use serde::Deserialize;

use crate::ai::{Action, CombatAi};
use crate::design_data::SpellData;
use crate::fighter::injury::Location;
use crate::fighter::personality::Personality;
use crate::fighter::{Fighter, FighterId, Stats};
use crate::log::{Event, EventKind};

pub const TICKS_PER_SECOND: u32 = 30;
/// Duration of a tick, in seconds.
pub const DT: f64 = 1.0 / TICKS_PER_SECOND as f64;
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
    /// How often attacks hit; always, unless set with [`Arena::with_accuracy`].
    pub accuracy: AccuracyRules,
    /// How hits stun; never, unless set with [`Arena::with_rules`].
    pub stun: StunRules,
}

/// Combat rules, from `data/combat.ron`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRules {
    pub accuracy: AccuracyRules,
    pub stun: StunRules,
}

/// How a hit may stun its target (see [`stun_chance`]), and how a stunned fighter gets
/// back up (see [`rise_percent`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StunRules {
    /// Chance a blunt hit stuns, in percent per tenth of the target's max HP it takes.
    pub blunt_percent: u32,
    /// The same for a cutting hit; a neutral one never stuns.
    pub cutting_percent: u32,
    /// Each injury to the target's head, after-effect included, raises the chance by
    /// this percentage of it.
    pub head_injury_percent: u32,
    /// Ticks a stunned fighter stays down; its opponents wait for it.
    pub ticks: u32,
    /// Chance to get back up, in percent, from determination 0 to determination 100.
    pub rise_percent_least_determined: u32,
    pub rise_percent_most_determined: u32,
    /// Taken off that chance for every stun after the first in the same fight.
    pub rise_percent_lost_per_stun: u32,
}

impl StunRules {
    /// No hit ever stuns.
    pub const NEVER: Self = Self {
        blunt_percent: 0,
        cutting_percent: 0,
        head_injury_percent: 0,
        ticks: 0,
        rise_percent_least_determined: 100,
        rise_percent_most_determined: 100,
        rise_percent_lost_per_stun: 0,
    };
}

/// What a spell hits with: a blunt blow stuns more often than a cutting one, a neutral
/// one (spells without an element yet) never.
// ponytail: cutting is meant to favor lost limbs and deaths too (roadmap, phase 5 bis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum DamageKind {
    Blunt,
    Cutting,
    Neutral,
}

/// Chance that an attack hits, from the attacker's accuracy against the target's speed
/// (see [`hit_percent`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccuracyRules {
    /// Chance to hit, in percent, when the attacker's accuracy equals the target's speed.
    pub even_percent: u32,
    /// Lowest and highest chance to hit, in percent.
    pub min_percent: u32,
    pub max_percent: u32,
}

impl AccuracyRules {
    /// Every attack hits.
    pub const ALWAYS: Self = Self {
        even_percent: 100,
        min_percent: 100,
        max_percent: 100,
    };
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
            accuracy: AccuracyRules::ALWAYS,
            stun: StunRules::NEVER,
        }
    }

    /// This arena, where attacks hit following `rules`.
    #[must_use]
    pub fn with_accuracy(mut self, rules: AccuracyRules) -> Self {
        self.accuracy = rules;
        self
    }

    /// This arena, where attacks hit and stun following `rules`, as in real fights.
    #[must_use]
    pub fn with_rules(mut self, rules: CombatRules) -> Self {
        self.accuracy = rules.accuracy;
        self.stun = rules.stun;
        self
    }

    /// At most one fighter still fighting: the others are dead, gave up or were knocked out.
    #[must_use]
    pub fn is_over(&self) -> bool {
        self.fighters.iter().filter(|f| f.is_fighting()).count() <= 1
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
    come_round(arena);
    if arena.tick.is_multiple_of(TICKS_PER_DECISION) {
        for (i, ai) in ais.iter_mut().enumerate() {
            let fighter = &arena.fighters[i];
            if fighter.is_standing() {
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

/// Chance in percent that an attack hits: `even_percent` × 2 × accuracy / (accuracy +
/// target's speed), within `min_percent` and `max_percent`: `even_percent` when the
/// attacker's accuracy equals the target's speed, up to twice that for a far better aim,
/// down to 0 for none.
#[must_use]
pub fn hit_percent(attacker: &Stats, defender: &Stats, rules: &AccuracyRules) -> u32 {
    let accuracy = u64::from(attacker.accuracy);
    let even = u64::from(rules.even_percent);
    let percent = (even * 2 * accuracy)
        .checked_div(accuracy + u64::from(defender.speed))
        .unwrap_or(even);
    // At most twice `even_percent`, so it fits in a u32.
    let percent = u32::try_from(percent).unwrap_or(u32::MAX);
    percent.clamp(rules.min_percent, rules.max_percent.max(rules.min_percent))
}

/// Chance, in hundredths of a percent (10 000 for sure), that a hit taking `damage` HP
/// with a `kind` weapon stuns `target`: the kind's percentage per tenth of the target's
/// max HP the hit takes, raised by `head_injury_percent` of it per head injury.
#[must_use]
pub fn stun_chance(damage: u32, kind: DamageKind, target: &Fighter, rules: &StunRules) -> u32 {
    let percent = match kind {
        DamageKind::Blunt => rules.blunt_percent,
        DamageKind::Cutting => rules.cutting_percent,
        DamageKind::Neutral => 0,
    };
    let heads = target
        .condition
        .injuries
        .iter()
        .filter(|injury| injury.location == Location::Head)
        .count();
    let heads = u64::try_from(heads).unwrap_or(u64::MAX);
    let raise = heads
        .saturating_mul(u64::from(rules.head_injury_percent))
        .saturating_add(100);
    // percent × damage / (max HP / 10) × raise / 100, in hundredths of a percent.
    let chance = (u64::from(percent) * u64::from(damage) * 10)
        .saturating_mul(raise)
        .checked_div(u64::from(target.stats.hp))
        .unwrap_or(0);
    u32::try_from(chance.min(10_000)).unwrap_or(10_000)
}

/// Chance, in percent, that `fighter`, down after its latest stun, gets back up: from
/// `rise_percent_least_determined` at determination 0 to `rise_percent_most_determined`
/// at 100, less `rise_percent_lost_per_stun` for each of its stuns after the first.
#[must_use]
pub fn rise_percent(fighter: &Fighter, rules: &StunRules) -> u32 {
    let low = i64::from(rules.rise_percent_least_determined);
    let high = i64::from(rules.rise_percent_most_determined);
    let determination = i64::from(fighter.condition.personality.determination.value());
    let percent = low + (high - low) * determination / i64::from(Personality::MAX);
    let lost =
        i64::from(rules.rise_percent_lost_per_stun) * i64::from(fighter.stuns.saturating_sub(1));
    u32::try_from((percent - lost).clamp(0, 100)).unwrap_or(0)
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

/// Counts down the ticks of every fighter down: at the last one, it gets back up with
/// [`rise_percent`], drawn only below 100, or is knocked out of the fight.
fn come_round(arena: &mut Arena) {
    for f in &mut arena.fighters {
        if !f.is_fighting() || f.stunned_ticks == 0 {
            continue;
        }
        f.stunned_ticks -= 1;
        if f.stunned_ticks > 0 {
            continue;
        }
        let chance = rise_percent(f, &arena.stun);
        let rises = chance >= 100 || arena.rng.random_range(0..100) < chance;
        f.knocked_out = !rises;
        let fighter = f.id;
        arena.log.push(Event {
            tick: arena.tick,
            kind: if rises {
                EventKind::Rise { fighter }
            } else {
                EventKind::KnockedOut { fighter }
            },
        });
    }
}

fn cool_down(arena: &mut Arena) {
    for f in &mut arena.fighters {
        for c in &mut f.cooldowns {
            *c = c.saturating_sub(1);
        }
    }
}

/// Order in which the fighters casting this tick strike: the fastest first, so of two
/// fighters killing each other on the same tick, the faster survives. Fighters of the
/// same speed are shuffled on the arena's generator, drawn only for such a tie.
fn cast_order(arena: &mut Arena) -> Vec<usize> {
    let fighters = &arena.fighters;
    let mut order: Vec<usize> = (0..fighters.len())
        .filter(|&i| {
            let f = &fighters[i];
            f.is_standing() && matches!(f.action, Action::Cast { spell, .. } if f.can_cast(spell))
        })
        .collect();
    order.sort_by_key(|&i| std::cmp::Reverse(fighters[i].stats.speed));
    for tie in order.chunk_by_mut(|&a, &b| fighters[a].stats.speed == fighters[b].stats.speed) {
        // Fisher-Yates on u32 draws, the same on every platform.
        for k in (1..tie.len()).rev() {
            let Ok(top) = u32::try_from(k) else { continue };
            let j = arena.rng.random_range(0..=top);
            tie.swap(k, usize::try_from(j).unwrap_or(k));
        }
    }
    order
}

/// Casts the spell each fighter standing chose, if it is ready, affordable and the
/// target is standing and in reach, in `cast_order`; a cast fighter then waits for
/// its next decision. The spell hits with `hit_percent`, drawn only when below 100; the
/// target dies at 0 HP, gives up at its surrender threshold, or else may be stunned with
/// `stun_chance`, drawn only above 0, staying down for `StunRules::ticks`.
fn cast_spells(arena: &mut Arena) {
    for i in cast_order(arena) {
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
        if !caster.is_standing() || !defender.is_standing() || caster.id == target {
            continue;
        }
        if !caster.can_cast(spell) || edge_distance(caster, defender) > caster.spells[spell].reach {
            continue;
        }
        let chance = hit_percent(&caster.stats, &defender.stats, &arena.accuracy);
        let missed = chance < 100 && arena.rng.random_range(0..100) >= chance;
        let amount = if missed {
            0
        } else {
            spell_damage(&caster.stats, &defender.stats, &caster.spells[spell])
        };
        let kind = caster.spells[spell].damage_kind;
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
            missed,
        });
        let died = !defender.is_alive();
        let surrendered = defender.surrendered;
        let chance = if died || surrendered {
            0
        } else {
            stun_chance(hp_before - defender.hp, kind, defender, &arena.stun)
        };
        let stunned = chance > 0 && arena.rng.random_range(0..10_000) < chance;
        if stunned {
            defender.stunned_ticks = arena.stun.ticks.max(1);
            defender.stuns += 1;
            defender.action = Action::Wait;
        }
        arena.log.push(cast);
        if died {
            arena.log.push(event(EventKind::Death { fighter: target }));
        } else if surrendered {
            arena
                .log
                .push(event(EventKind::Surrender { fighter: target }));
        } else if stunned {
            arena.log.push(event(EventKind::Stun { fighter: target }));
        }
    }
}

fn move_fighters(arena: &mut Arena) {
    for f in arena.fighters.iter_mut().filter(|f| f.is_standing()) {
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

/// Pushes overlapping fighters still fighting apart, each by half the overlap.
// ponytail: one pass in index order, a crowded melee may keep small overlaps; iterate if visible.
fn separate(arena: &mut Arena) {
    let fighters = &mut arena.fighters;
    for i in 0..fighters.len() {
        for j in i + 1..fighters.len() {
            let (a, b) = (&fighters[i], &fighters[j]);
            if !a.is_fighting() || !b.is_fighting() {
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
            accuracy: 30,
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

    const ACCURACY: AccuracyRules = AccuracyRules {
        even_percent: 80,
        min_percent: 25,
        max_percent: 95,
    };

    fn aim(accuracy: u32, speed: u32) -> u32 {
        let attacker = Stats {
            accuracy,
            ..stats(0, 0, 0, 0)
        };
        let defender = Stats {
            speed,
            ..stats(0, 0, 0, 0)
        };
        hit_percent(&attacker, &defender, &ACCURACY)
    }

    #[test]
    fn accuracy_against_speed_sets_the_chance_to_hit() {
        assert_eq!(aim(30, 30), 80);
        // 80 × 2 × 40 / 70, rounded down.
        assert_eq!(aim(40, 30), 91);
        assert_eq!(aim(20, 30), 64);
        assert_eq!(aim(300, 30), 95);
        assert_eq!(aim(0, 30), 25);
        assert_eq!(aim(0, 0), 80);
        assert_eq!(aim(u32::MAX, u32::MAX), 80);
        assert_eq!(
            hit_percent(
                &stats(0, 0, 0, 0),
                &stats(0, 0, 0, 0),
                &AccuracyRules::ALWAYS
            ),
            100
        );
    }

    #[test]
    fn a_miss_spends_the_spell_and_deals_nothing() {
        let mut arena = duel(0.5).with_accuracy(AccuracyRules {
            even_percent: 50,
            min_percent: 50,
            max_percent: 50,
        });
        // 5 casts, each one tick after the last cooldown.
        for _ in 0..5 {
            play(&mut arena, &[HIT, Action::Wait], 30);
        }
        let missed = arena
            .log
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Cast { missed: true, .. }))
            .count();
        assert_eq!(arena.log.len(), 5);
        assert!((1..5).contains(&missed), "{missed} missed");
        let hits = u32::try_from(5 - missed).unwrap();
        assert_eq!(arena.fighters[1].hp, 100 - hits * HIT_DAMAGE);
        assert_eq!(arena.fighters[0].endurance.current, 0);
        for event in &arena.log {
            if let EventKind::Cast {
                missed: true,
                damage,
                ..
            } = event.kind
            {
                assert_eq!(damage, 0);
            }
        }
    }

    #[test]
    fn misses_follow_the_seed() {
        let rules = AccuracyRules {
            even_percent: 50,
            min_percent: 50,
            max_percent: 50,
        };
        let fight = |seed| {
            let fighters = vec![fighter(0, 5.0, 5.0), fighter(1, 6.5, 5.0)];
            let mut arena = Arena::new(20.0, 20.0, fighters, seed).with_accuracy(rules);
            play(&mut arena, &[HIT, Action::Wait], 150);
            arena.log
        };
        assert_eq!(fight(1), fight(1));
    }

    const HIT_BACK: Action = Action::Cast {
        spell: 0,
        target: FighterId(0),
    };

    /// Two fighters one hit from death strike each other on the first tick, fighter 1 at
    /// `speed_1`: the winner's index.
    fn trade(speed_1: u32, seed: u64) -> usize {
        let mut fighters = vec![fighter(0, 5.0, 5.0), fighter(1, 6.5, 5.0)];
        fighters[1].stats.speed = speed_1;
        for f in &mut fighters {
            f.hp = HIT_DAMAGE;
        }
        let mut arena = Arena::new(20.0, 20.0, fighters, seed);
        play(&mut arena, &[HIT, HIT_BACK], 1);
        let alive: Vec<usize> = (0..2).filter(|&i| arena.fighters[i].is_alive()).collect();
        assert_eq!(alive.len(), 1, "exactly one strikes first");
        alive[0]
    }

    #[test]
    fn the_faster_strikes_first_on_the_same_tick() {
        assert_eq!(trade(31, 0), 1);
        assert_eq!(trade(29, 0), 0);
    }

    #[test]
    fn a_draw_decides_between_equal_speeds() {
        let winners: Vec<usize> = (0..20).map(|seed| trade(30, seed)).collect();
        assert!(winners.contains(&0) && winners.contains(&1), "{winners:?}");
        assert_eq!(
            winners,
            (0..20).map(|seed| trade(30, seed)).collect::<Vec<_>>()
        );
    }

    const STUN: StunRules = StunRules {
        blunt_percent: 5,
        cutting_percent: 1,
        head_injury_percent: 50,
        ticks: 60,
        rise_percent_least_determined: 30,
        rise_percent_most_determined: 90,
        rise_percent_lost_per_stun: 20,
    };

    #[test]
    fn heavy_blunt_hits_stun_most_and_head_injuries_more_still() {
        use crate::fighter::injury::{Injury, Severity};

        // 100 HP: 10 damage is a tenth of them.
        let mut target = fighter(1, 0.0, 0.0);
        let chance = |damage, kind, target: &Fighter| stun_chance(damage, kind, target, &STUN);
        assert_eq!(chance(10, DamageKind::Blunt, &target), 500);
        assert_eq!(chance(20, DamageKind::Blunt, &target), 1000);
        assert_eq!(chance(20, DamageKind::Cutting, &target), 200);
        assert_eq!(chance(20, DamageKind::Neutral, &target), 0);
        assert_eq!(chance(1000, DamageKind::Blunt, &target), 10_000);
        let head = Injury {
            location: Location::Head,
            severity: Severity::Permanent,
            weeks_left: 0,
        };
        target.condition.injuries = vec![head, head];
        assert_eq!(chance(20, DamageKind::Blunt, &target), 2000);
        target.stats.hp = 0;
        assert_eq!(chance(20, DamageKind::Blunt, &target), 0);
    }

    #[test]
    fn the_determined_get_up_more_often_and_every_stun_makes_it_harder() {
        use crate::fighter::personality::TraitValue;

        let mut f = fighter(0, 0.0, 0.0);
        let rise = |f: &Fighter, determination| {
            let mut f = f.clone();
            f.condition.personality.determination = TraitValue::new(determination);
            rise_percent(&f, &STUN)
        };
        f.stuns = 1;
        assert_eq!((rise(&f, 0), rise(&f, 50), rise(&f, 100)), (30, 60, 90));
        f.stuns = 3;
        assert_eq!((rise(&f, 0), rise(&f, 100)), (0, 50));
    }

    /// `duel(0.5)` where fighter 0's first hit surely stuns, and a fighter gets up with
    /// `rise_percent`.
    fn stunning_duel(rise_percent: u32) -> Arena {
        let mut arena = duel(0.5);
        arena.stun = StunRules {
            blunt_percent: 10_000,
            rise_percent_least_determined: rise_percent,
            rise_percent_most_determined: rise_percent,
            ..STUN
        };
        arena
    }

    #[test]
    fn a_stunned_fighter_stays_down_unhit_then_gets_up() {
        let mut arena = stunning_duel(100);
        play(&mut arena, &[HIT, Action::Wait], 1);
        let down = &arena.fighters[1];
        assert_eq!((down.hp, down.stuns, down.stunned_ticks), (90, 1, 60));
        assert!(down.is_fighting() && !down.is_standing());
        assert_eq!(
            arena.log.last().map(|e| e.kind),
            Some(EventKind::Stun {
                fighter: FighterId(1)
            })
        );
        // Fighter 0's cooldown ends at tick 30, but nobody hits a fighter down,
        // and the fighter down neither hits back nor moves.
        play(&mut arena, &[HIT, HIT_BACK], 59);
        assert_eq!(arena.fighters[1].hp, 90);
        assert_eq!(arena.fighters[0].hp, 100);
        assert!(!arena.is_over());
        // Tick 60: it gets up, and is hit again at once.
        play(&mut arena, &[HIT, Action::Wait], 1);
        let kinds: Vec<EventKind> = arena.log.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&EventKind::Rise {
            fighter: FighterId(1)
        }));
        assert_eq!(arena.fighters[1].hp, 80);
    }

    #[test]
    fn a_fighter_who_does_not_get_up_is_knocked_out_alive() {
        let mut arena = stunning_duel(0);
        play(&mut arena, &[HIT, Action::Wait], 61);
        let out = &arena.fighters[1];
        assert!(out.knocked_out && out.is_alive() && !out.is_fighting());
        assert!(arena.is_over());
        assert_eq!(
            arena.log.last().map(|e| e.kind),
            Some(EventKind::KnockedOut {
                fighter: FighterId(1)
            })
        );
        let report = crate::log::Report::new(&arena);
        assert_eq!(report.winner, Some(FighterId(0)));
        assert!(report.fighters[1].knocked_out && report.fighters[1].stuns == 1);
    }

    #[test]
    fn without_stun_rules_nobody_is_stunned() {
        let mut arena = duel(0.5);
        play(&mut arena, &[HIT, Action::Wait], 150);
        assert_eq!(arena.fighters[1].stuns, 0);
        assert_eq!(arena.fighters[1].hp, 100 - 5 * HIT_DAMAGE);
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
