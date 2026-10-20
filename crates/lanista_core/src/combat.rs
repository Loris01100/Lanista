use rand::RngExt;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use serde::Deserialize;

use crate::ai::{Action, CombatAi};
use crate::design_data::SpellData;
use crate::fighter::injury::{Location, Side};
use crate::fighter::personality::Personality;
use crate::fighter::{Fighter, FighterId, Stats, pick};
use crate::log::{Event, EventKind};
use crate::melee::{EliminationRules, MeleeRules, PactRules, eliminate, update_pacts};

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
    /// How cutting hits sever; never, unless set with [`Arena::with_rules`].
    pub cut: CutRules,
    /// Percentage of their speed fighters walk at: 100, unless set with
    /// [`Arena::with_layout`].
    pub speed_percent: u32,
    /// What stands on the floor; nothing, unless set with [`Arena::with_layout`].
    pub pillars: Vec<Pillar>,
    /// How fighters make and break pacts; never, unless set with [`Arena::with_pacts`].
    pub pact_rules: PactRules,
    /// The pairs of allies, who do not aim at each other; each fighter has one at most.
    pub pacts: Vec<(FighterId, FighterId)>,
    /// Who goes for whom first, hunter then prey: in a melee, a rival club's fighters
    /// after the player's.
    pub hunts: Vec<(FighterId, FighterId)>,
    /// Points added to the chance that two fighters make a pact, whichever way round:
    /// in a melee, the player's fighters and those of a club by its standing.
    pub pact_points: Vec<(FighterId, FighterId, i32)>,
    /// Who is put out for dealing too little damage, and when; nobody, unless set with
    /// [`Arena::with_elimination`].
    pub elimination: EliminationRules,
    /// Ticks the fight lasts at most: a duel's minute (`duel::MAX_TICKS`), unless set
    /// with [`Arena::with_max_ticks`].
    pub max_ticks: u32,
}

/// Combat rules, from `data/combat.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRules {
    pub accuracy: AccuracyRules,
    pub stun: StunRules,
    pub cut: CutRules,
    pub arenas: ArenaLayouts,
    pub melee: MeleeRules,
}

/// What a colosseum's arena does to the fight (see [`ArenaLayout`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum ArenaKind {
    /// Slows everyone.
    Sand,
    /// Pillars block ranged spells and the way.
    Pillars,
    /// A small floor, where melee fighters close in fast.
    Narrow,
}

impl ArenaKind {
    pub const ALL: [Self; 3] = [Self::Sand, Self::Pillars, Self::Narrow];
}

/// A round pillar: fighters walk around it, and ranged spells do not go through it.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pillar {
    /// Center, in meters.
    pub x: f64,
    pub y: f64,
    /// In meters.
    pub radius: f64,
}

/// The floor of an arena kind, from `data/combat.ron`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaLayout {
    /// In meters.
    pub width: f64,
    pub height: f64,
    /// Percentage of their speed fighters walk at.
    pub speed_percent: u32,
    pub pillars: Vec<Pillar>,
}

/// The floor of each arena kind.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArenaLayouts {
    pub sand: ArenaLayout,
    pub pillars: ArenaLayout,
    pub narrow: ArenaLayout,
}

impl ArenaLayouts {
    #[must_use]
    pub fn get(&self, kind: ArenaKind) -> &ArenaLayout {
        match kind {
            ArenaKind::Sand => &self.sand,
            ArenaKind::Pillars => &self.pillars,
            ArenaKind::Narrow => &self.narrow,
        }
    }
}

/// How a hit may stun its target (see [`stun_chance`]), and how a stunned fighter gets
/// back up (see [`rise_percent`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StunRules {
    /// Chance a blunt hit stuns, in hundredths of a percent per tenth of the target's
    /// max HP it takes.
    pub blunt_hundredths: u32,
    /// The same for a cutting hit; a neutral one never stuns.
    pub cutting_hundredths: u32,
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
        blunt_hundredths: 0,
        cutting_hundredths: 0,
        head_injury_percent: 0,
        ticks: 0,
        rise_percent_least_determined: 100,
        rise_percent_most_determined: 100,
        rise_percent_lost_per_stun: 0,
    };
}

/// How a cutting hit may sever a limb or the head (see [`severed_part`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutRules {
    /// Chance a cutting hit the target survives standing severs one of its limbs, in
    /// hundredths of a percent per tenth of the target's max HP the hit takes.
    pub limb_hundredths: u32,
    /// The same for the head, on any cutting hit that lands: the target dies.
    pub head_hundredths: u32,
    /// Chance, in percent, that a cutting hit bringing its target to 0 HP takes its
    /// head off: a fatal blow nobody survives.
    pub fatal_head_percent: u32,
    /// Relative chances that a severed limb is an arm or a leg; the side is drawn 50/50.
    pub arm_weight: u32,
    pub leg_weight: u32,
}

impl CutRules {
    /// No hit ever severs anything.
    pub const NEVER: Self = Self {
        limb_hundredths: 0,
        head_hundredths: 0,
        fatal_head_percent: 0,
        arm_weight: 1,
        leg_weight: 1,
    };
}

/// What a spell hits with: a blunt blow stuns more often than a cutting one, a cutting
/// one may sever a limb or the head, a neutral one (spells without an element yet) does
/// neither.
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
            cut: CutRules::NEVER,
            speed_percent: 100,
            pillars: Vec::new(),
            pact_rules: PactRules::NEVER,
            pacts: Vec::new(),
            hunts: Vec::new(),
            pact_points: Vec::new(),
            elimination: EliminationRules::NEVER,
            max_ticks: crate::duel::MAX_TICKS,
        }
    }

    /// This arena, whose fight lasts at most `ticks`: a melee.
    #[must_use]
    pub fn with_max_ticks(mut self, ticks: u32) -> Self {
        self.max_ticks = ticks;
        self
    }

    /// This arena, where attacks hit following `rules`.
    #[must_use]
    pub fn with_accuracy(mut self, rules: AccuracyRules) -> Self {
        self.accuracy = rules;
        self
    }

    /// This arena, where attacks hit, stun and sever following `rules`, as in real fights.
    #[must_use]
    pub fn with_rules(mut self, rules: &CombatRules) -> Self {
        self.accuracy = rules.accuracy;
        self.stun = rules.stun;
        self.cut = rules.cut;
        self
    }

    /// This arena, where fighters make and break pacts following `rules`: a melee.
    #[must_use]
    pub fn with_pacts(mut self, rules: PactRules) -> Self {
        self.pact_rules = rules;
        self
    }

    /// This arena, where those who deal the least damage are put out at each check,
    /// following `rules`: a melee.
    #[must_use]
    pub fn with_elimination(mut self, rules: EliminationRules) -> Self {
        self.elimination = rules;
        self
    }

    /// The lowest and the highest center a fighter may stand at, inside the walls.
    #[must_use]
    pub fn reachable(&self) -> (Vec2, Vec2) {
        (
            Vec2::new(RADIUS, RADIUS),
            Vec2::new(self.width - RADIUS, self.height - RADIUS),
        )
    }

    /// Whether `a` and `b` are allies.
    #[must_use]
    pub fn allied(&self, a: FighterId, b: FighterId) -> bool {
        self.pacts
            .iter()
            .any(|&pact| pact == (a, b) || pact == (b, a))
    }

    /// This arena on `layout`'s floor: its size, its pillars, and its ground slowing
    /// fighters down to its speed percentage.
    #[must_use]
    pub fn with_layout(mut self, layout: &ArenaLayout) -> Self {
        self.width = layout.width;
        self.height = layout.height;
        self.speed_percent = layout.speed_percent;
        self.pillars.clone_from(&layout.pillars);
        self
    }

    /// At most one fighter still fighting: the others are dead, gave up, were knocked
    /// out or lost a limb.
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
        update_pacts(arena);
        for (i, ai) in ais.iter_mut().enumerate() {
            let fighter = &arena.fighters[i];
            if fighter.is_standing() {
                let action = ai.decide(arena, fighter.id);
                arena.fighters[i].action = action;
            }
        }
    }
    cool_down(arena);
    recover(arena);
    move_fighters(arena);
    separate(arena);
    keep_inside(arena);
    cast_spells(arena);
    eliminate(arena);
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

/// `hundredths` of a percent per tenth of `max_hp` that `damage` takes, in hundredths
/// of a percent, at most 10 000 (sure); 0 for no HP.
fn per_tenth(hundredths: u32, damage: u32, max_hp: u32) -> u64 {
    (u64::from(hundredths) * u64::from(damage) * 10)
        .checked_div(u64::from(max_hp))
        .unwrap_or(0)
        .min(10_000)
}

/// Chance, in hundredths of a percent (10 000 for sure), that a hit taking `damage` HP
/// with a `kind` weapon stuns `target`: the kind's hundredths per tenth of the target's
/// max HP the hit takes, raised by `head_injury_percent` of it per head injury.
#[must_use]
pub fn stun_chance(damage: u32, kind: DamageKind, target: &Fighter, rules: &StunRules) -> u32 {
    let hundredths = match kind {
        DamageKind::Blunt => rules.blunt_hundredths,
        DamageKind::Cutting => rules.cutting_hundredths,
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
    // hundredths × damage / (max HP / 10) × raise / 100.
    let chance = (u64::from(hundredths) * u64::from(damage) * 10)
        .saturating_mul(raise)
        .checked_div(u64::from(target.stats.hp).saturating_mul(100))
        .unwrap_or(0);
    u32::try_from(chance.min(10_000)).unwrap_or(10_000)
}

/// What a cutting hit taking `damage` HP severs of `target`, as the hit left it: its
/// head with `fatal_head_percent` if the hit brought it to 0 HP, else with
/// `head_hundredths` per tenth of its max HP the hit takes; then, if it still fights,
/// a limb with `limb_hundredths` per tenth (see [`severed_limb`]); a limb it already
/// lost is a stump, and nothing is severed. Each chance is drawn only above 0.
pub fn severed_part<R: rand::Rng + ?Sized>(
    rng: &mut R,
    damage: u32,
    target: &Fighter,
    rules: &CutRules,
) -> Option<Location> {
    let draw = |rng: &mut R, chance: u64| {
        chance > 0 && u64::from(rng.random_range(0..10_000_u32)) < chance
    };
    let head = if target.is_alive() {
        per_tenth(rules.head_hundredths, damage, target.stats.hp)
    } else {
        u64::from(rules.fatal_head_percent.min(100)) * 100
    };
    if draw(rng, head) {
        return Some(Location::Head);
    }
    let limb = per_tenth(rules.limb_hundredths, damage, target.stats.hp);
    (target.is_fighting() && draw(rng, limb))
        .then(|| severed_limb(rng, rules))
        .filter(|&part| !target.has_lost(part))
}

/// Which limb a cutting hit severs: an arm or a leg by `arm_weight` and `leg_weight`,
/// on a side drawn 50/50.
pub fn severed_limb<R: rand::Rng + ?Sized>(rng: &mut R, rules: &CutRules) -> Location {
    let side = if rng.random_bool(0.5) {
        Side::Left
    } else {
        Side::Right
    };
    pick(
        rng,
        &[
            (rules.arm_weight, Location::Arm(side)),
            (rules.leg_weight, Location::Leg(side)),
        ],
    )
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

/// Whether a pillar stands across the line from `from` to `to`, blocking a ranged spell.
#[must_use]
pub fn shot_blocked(pillars: &[Pillar], from: Vec2, to: Vec2) -> bool {
    pillars
        .iter()
        .any(|p| segment_distance(Vec2::new(p.x, p.y), from, to) < p.radius)
}

/// Distance from `point` to the segment from `a` to `b`.
fn segment_distance(point: Vec2, a: Vec2, b: Vec2) -> f64 {
    let along = Vec2::new(b.x - a.x, b.y - a.y);
    let squared = along.x * along.x + along.y * along.y;
    let t = if squared > 0.0 {
        (((point.x - a.x) * along.x + (point.y - a.y) * along.y) / squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Vec2::new(a.x + t * along.x - point.x, a.y + t * along.y - point.y).length()
}

/// Extra turn, in radians, past the edge of a pillar in the way: enough to clear it.
const AROUND_PILLAR: f64 = 0.1;

/// Which way a fighter at `from` walks to reach `to`: straight there, or, when a pillar
/// it cannot walk through stands ahead in the way, past the edge of the nearest such
/// pillar, on the side of `to`.
#[must_use]
pub fn path_direction(pillars: &[Pillar], from: Vec2, to: Vec2) -> Vec2 {
    let straight = Vec2::new(to.x - from.x, to.y - from.y);
    let length = straight.length();
    if length <= 0.0 {
        return straight;
    }
    // The pillar's center seen from `from`, how close one may walk by, how far ahead.
    let mut nearest: Option<(Vec2, f64, f64)> = None;
    for p in pillars {
        let clearance = p.radius + RADIUS;
        let center = Vec2::new(p.x - from.x, p.y - from.y);
        let ahead = (center.x * straight.x + center.y * straight.y) / length;
        let in_way = segment_distance(Vec2::new(p.x, p.y), from, to) < clearance;
        if ahead > 0.0 && in_way && nearest.is_none_or(|(_, _, a)| ahead < a) {
            nearest = Some((center, clearance, ahead));
        }
    }
    let Some((center, clearance, _)) = nearest else {
        return straight;
    };
    let distance = center.length();
    let edge = if distance > clearance {
        libm::asin(clearance / distance)
    } else {
        std::f64::consts::FRAC_PI_2
    };
    let turned = |side: f64| {
        let angle = side * (edge + AROUND_PILLAR);
        let (sin, cos) = (libm::sin(angle), libm::cos(angle));
        Vec2::new(
            center.x * cos - center.y * sin,
            center.x * sin + center.y * cos,
        )
    };
    // Turn towards the side of the line to the pillar where `to` lies. Right behind
    // it, towards higher y, or higher x for a pillar straight up or down: of two
    // fighters facing each other across a pillar, both go the same way round and
    // meet, instead of chasing each other around it.
    let cross = center.x * straight.y - center.y * straight.x;
    if cross.abs() >= 1e-9 * length * length {
        return turned(if cross > 0.0 { 1.0 } else { -1.0 });
    }
    let (left, right) = (turned(1.0), turned(-1.0));
    if (left.y, left.x) >= (right.y, right.x) {
        left
    } else {
        right
    }
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

/// At the end of each second, every fighter still in the fight recovers mana and
/// endurance: none is left without a spell for long.
fn recover(arena: &mut Arena) {
    if arena.tick == 0 || !arena.tick.is_multiple_of(TICKS_PER_SECOND) {
        return;
    }
    for f in arena.fighters.iter_mut().filter(|f| f.is_fighting()) {
        f.mana.recover();
        f.endurance.recover();
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
/// its next decision. The spell hits with `hit_percent`, drawn only when below 100; a
/// cutting hit may then sever the target's head, killing it, or a limb, taking it out of
/// the fight alive (`severed_part`); the target dies at 0 HP, gives up at its surrender
/// threshold, or else may be stunned with `stun_chance`, drawn only above 0, staying
/// down for `StunRules::ticks`.
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
        if arena.allied(caster.id, target) {
            continue;
        }
        if !caster.can_cast(spell) || edge_distance(caster, defender) > caster.spells[spell].reach {
            continue;
        }
        let ranged = caster.spells[spell].range == Range::Ranged;
        if ranged && shot_blocked(&arena.pillars, caster.position, defender.position) {
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
        let damage = hp_before - defender.hp;
        let severed = if kind == DamageKind::Cutting && !missed {
            severed_part(&mut arena.rng, damage, defender, &arena.cut)
        } else {
            None
        };
        if let Some(part) = severed {
            defender.severed = Some(part);
            defender.action = Action::Wait;
            if part == Location::Head {
                defender.hp = 0;
                defender.surrendered = false;
            }
        }
        let died = !defender.is_alive();
        let surrendered = defender.surrendered;
        let chance = if died || surrendered || severed.is_some() {
            0
        } else {
            stun_chance(damage, kind, defender, &arena.stun)
        };
        let stunned = chance > 0 && arena.rng.random_range(0..10_000) < chance;
        if stunned {
            defender.stunned_ticks = arena.stun.ticks.max(1);
            defender.stuns += 1;
            defender.action = Action::Wait;
        }
        arena.log.push(cast);
        if let Some(part) = severed {
            arena.log.push(event(EventKind::Sever {
                fighter: target,
                part,
            }));
        }
        if died {
            arena.log.push(event(EventKind::Death { fighter: target }));
        } else if surrendered {
            arena
                .log
                .push(event(EventKind::Surrender { fighter: target }));
        } else if stunned {
            arena.log.push(event(EventKind::Stun { fighter: target }));
        }
        let caster = &mut arena.fighters[i];
        caster.round_damage = caster.round_damage.saturating_add(damage);
    }
}

/// Moves every fighter standing that chose to, at its speed slowed by the ground.
fn move_fighters(arena: &mut Arena) {
    let ground = f64::from(arena.speed_percent) / 100.0;
    for f in arena.fighters.iter_mut().filter(|f| f.is_standing()) {
        if let Action::Move(direction) = f.action {
            let length = direction.length();
            if length > 0.0 {
                let step = f64::from(f.stats.speed) * METERS_PER_SPEED * ground * DT / length;
                f.position.x += direction.x * step;
                f.position.y += direction.y * step;
            }
        }
    }
}

/// Pushes overlapping fighters still fighting apart, each by half the overlap; a fighter
/// down stays where it lies and the one standing takes the whole push.
// ponytail: one pass in index order, a crowded melee may keep small overlaps; iterate if visible.
fn separate(arena: &mut Arena) {
    let fighters = &mut arena.fighters;
    for i in 0..fighters.len() {
        for j in i + 1..fighters.len() {
            let (a, b) = (&fighters[i], &fighters[j]);
            if !a.is_fighting() || !b.is_fighting() || !a.is_standing() && !b.is_standing() {
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
            // A fighter down does not move: the one standing takes the whole push.
            let (push_a, push_b) = match (a.is_standing(), b.is_standing()) {
                (true, false) => (overlap, 0.0),
                (false, true) => (0.0, overlap),
                _ => (overlap / 2.0, overlap / 2.0),
            };
            fighters[i].position.x -= normal.x * push_a;
            fighters[i].position.y -= normal.y * push_a;
            fighters[j].position.x += normal.x * push_b;
            fighters[j].position.y += normal.y * push_b;
        }
    }
}

/// Pushes every fighter still in the fight out of the pillars, then back inside the
/// walls; the others lie where they fell.
fn keep_inside(arena: &mut Arena) {
    let (low, high) = arena.reachable();
    for f in arena.fighters.iter_mut().filter(|f| f.is_fighting()) {
        for p in &arena.pillars {
            let offset = Vec2::new(f.position.x - p.x, f.position.y - p.y);
            let distance = offset.length();
            let clearance = p.radius + RADIUS;
            if distance >= clearance {
                continue;
            }
            // On the center: no direction to push along, pick one.
            let normal = if distance > 0.0 {
                Vec2::new(offset.x / distance, offset.y / distance)
            } else {
                Vec2::new(1.0, 0.0)
            };
            f.position.x = p.x + normal.x * clearance;
            f.position.y = p.y + normal.y * clearance;
        }
        f.position.x = f.position.x.clamp(low.x, high.x);
        f.position.y = f.position.y.clamp(low.y, high.y);
    }
}

#[cfg(test)]
mod tests {
    use rand_chacha::rand_core::Rng;

    use super::*;
    use crate::fighter::{FighterId, Gauge, Stats, test_fighter as fighter};

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
    fn mana_and_endurance_come_back_each_second_up_to_the_max() {
        let mut arena = duel(2.0);
        for f in &mut arena.fighters {
            f.mana = Gauge::full(50, 4);
            f.endurance = Gauge::full(50, 3);
            f.mana.current = 0;
            f.endurance.current = 48;
        }
        arena.fighters[1].surrendered = true;
        play(&mut arena, &[Action::Wait, Action::Wait], TICKS_PER_SECOND);
        assert_eq!(arena.fighters[0].mana.current, 0, "not a whole second yet");
        play(&mut arena, &[Action::Wait, Action::Wait], 1);
        assert_eq!(arena.fighters[0].mana.current, 4);
        assert_eq!(arena.fighters[0].endurance.current, 50);
        // Out of the fight, it recovers nothing.
        assert_eq!(arena.fighters[1].mana.current, 0);
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

    #[test]
    fn a_fighter_walking_into_one_down_is_pushed_back_alone() {
        let mut arena = Arena::new(
            20.0,
            20.0,
            vec![fighter(0, 5.0, 5.0), fighter(1, 6.5, 5.0)],
            0,
        );
        arena.fighters[1].stunned_ticks = 1_000;
        play(
            &mut arena,
            &[Action::Move(Vec2::new(1.0, 0.0)), Action::Wait],
            60,
        );
        let (a, b) = (arena.fighters[0].position, arena.fighters[1].position);
        assert_eq!(b, Vec2::new(6.5, 5.0));
        assert!(b.x - a.x >= 2.0 * RADIUS - 1e-9, "{}", b.x - a.x);
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
        blunt_hundredths: 500,
        cutting_hundredths: 167,
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
        // A third of the blunt chance.
        assert_eq!(chance(20, DamageKind::Cutting, &target), 334);
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
            blunt_hundredths: 10_000,
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

    const CUT: CutRules = CutRules {
        limb_hundredths: 20,
        head_hundredths: 5,
        fatal_head_percent: 30,
        arm_weight: 1,
        leg_weight: 1,
    };

    /// `duel(0.5)` where fighter 0 strikes with a cutting weapon, severing by `cut`.
    fn cutting_duel(cut: CutRules) -> Arena {
        let mut arena = duel(0.5);
        arena.fighters[0].spells[0].damage_kind = DamageKind::Cutting;
        arena.cut = cut;
        arena
    }

    #[test]
    fn a_fatal_cut_may_behead_and_a_lighter_one_sever_a_limb() {
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let mut target = fighter(1, 0.0, 0.0);
        let sure = CutRules {
            limb_hundredths: 10_000,
            fatal_head_percent: 100,
            ..CUT
        };
        // Standing after the hit: the limb, never the head without head_hundredths.
        let limb = severed_part(
            &mut rng,
            10,
            &target,
            &CutRules {
                head_hundredths: 0,
                ..sure
            },
        );
        assert!(matches!(limb, Some(Location::Arm(_) | Location::Leg(_))));
        // Gave up: no limb is cut off a fighter out of the fight.
        target.surrendered = true;
        assert_eq!(
            severed_part(
                &mut rng,
                10,
                &target,
                &CutRules {
                    head_hundredths: 0,
                    ..sure
                }
            ),
            None
        );
        // Brought to 0 HP: the head, by fatal_head_percent.
        target.hp = 0;
        assert_eq!(
            severed_part(&mut rng, 10, &target, &sure),
            Some(Location::Head)
        );
        let never = CutRules {
            fatal_head_percent: 0,
            ..sure
        };
        assert_eq!(severed_part(&mut rng, 10, &target, &never), None);
        let beheaded = (0..10_000)
            .filter(|_| severed_part(&mut rng, 10, &target, &CUT).is_some())
            .count();
        assert!(beheaded.abs_diff(3_000) < 150, "{beheaded}");
        // Standing, 10 % of the HP: 0.05 % for the head, then 0.2 % for a limb.
        let mut target = fighter(1, 0.0, 0.0);
        target.hp = 90;
        let cut = (0..100_000)
            .filter(|_| severed_part(&mut rng, 10, &target, &CUT).is_some())
            .count();
        assert!(cut.abs_diff(250) < 60, "{cut}");
    }

    #[test]
    fn a_limb_lost_before_is_a_stump_nothing_is_cut_off() {
        use crate::fighter::injury::severed_limb_wound;

        let arms_only = CutRules {
            limb_hundredths: 10_000,
            head_hundredths: 0,
            leg_weight: 0,
            ..CUT
        };
        let mut target = fighter(1, 0.0, 0.0);
        target.condition.injuries = vec![severed_limb_wound(Location::Arm(Side::Left))];
        assert!(target.has_lost(Location::Arm(Side::Left)));
        assert!(!target.has_lost(Location::Arm(Side::Right)));
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let cuts: Vec<Option<Location>> = (0..200)
            .map(|_| severed_part(&mut rng, 10, &target, &arms_only))
            .collect();
        assert!(cuts.contains(&Some(Location::Arm(Side::Right))));
        assert!(cuts.contains(&None));
        assert!(!cuts.contains(&Some(Location::Arm(Side::Left))));
    }

    #[test]
    fn arms_and_legs_are_severed_by_their_weights() {
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let arms_only = CutRules {
            leg_weight: 0,
            ..CUT
        };
        for _ in 0..100 {
            assert!(matches!(
                severed_limb(&mut rng, &arms_only),
                Location::Arm(_)
            ));
        }
        let left = (0..1000)
            .filter(|_| {
                matches!(
                    severed_limb(&mut rng, &CUT),
                    Location::Arm(Side::Left) | Location::Leg(Side::Left)
                )
            })
            .count();
        assert!(left.abs_diff(500) < 60, "{left}");
    }

    #[test]
    fn a_fighter_that_lost_a_limb_is_out_alive_and_never_hit_again() {
        let mut arena = cutting_duel(CutRules {
            limb_hundredths: 10_000,
            head_hundredths: 0,
            ..CUT
        });
        play(&mut arena, &[HIT, HIT_BACK], 1);
        let out = &arena.fighters[1];
        assert!(matches!(
            out.severed,
            Some(Location::Arm(_) | Location::Leg(_))
        ));
        assert!(out.is_alive() && !out.is_fighting() && !out.surrendered);
        assert_eq!(out.hp, 100 - HIT_DAMAGE);
        assert!(arena.is_over());
        assert!(matches!(
            arena.log.last().map(|e| e.kind),
            Some(EventKind::Sever { fighter: FighterId(1), part }) if Some(part) == out.severed
        ));
        // Not a single stun: the cut took it out first.
        assert_eq!(out.stuns, 0);
        let report = crate::log::Report::new(&arena);
        assert_eq!(report.winner, Some(FighterId(0)));
        assert_eq!(report.fighters[1].severed, out.severed);
        assert!(crate::log::log_text(&arena).contains("fighter 1 loses its "));
    }

    #[test]
    fn a_beheading_kills_even_a_light_hit() {
        let mut arena = cutting_duel(CutRules {
            head_hundredths: 10_000,
            ..CUT
        });
        // A threshold the hit crosses: the beheaded did not give up, it died.
        arena.fighters[1].surrender_hp = 95;
        play(&mut arena, &[HIT, Action::Wait], 1);
        let dead = &arena.fighters[1];
        assert_eq!((dead.hp, dead.severed), (0, Some(Location::Head)));
        assert!(!dead.surrendered);
        let kinds: Vec<EventKind> = arena.log.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds[1..],
            [
                EventKind::Sever {
                    fighter: FighterId(1),
                    part: Location::Head
                },
                EventKind::Death {
                    fighter: FighterId(1)
                },
            ]
        );
        assert!(crate::log::log_text(&arena).contains("fighter 1 is beheaded"));
    }

    #[test]
    fn only_cutting_hits_sever() {
        let sure = CutRules {
            limb_hundredths: 10_000,
            head_hundredths: 10_000,
            ..CUT
        };
        // The test spell is blunt.
        let mut arena = duel(0.5);
        arena.cut = sure;
        play(&mut arena, &[HIT, Action::Wait], 150);
        assert_eq!(arena.fighters[1].severed, None);
        // Without cut rules, a cutting hit severs nothing either.
        let mut arena = cutting_duel(CutRules::NEVER);
        play(&mut arena, &[HIT, Action::Wait], 150);
        assert_eq!(arena.fighters[1].severed, None);
        assert_eq!(arena.fighters[1].hp, 100 - 5 * HIT_DAMAGE);
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

    fn pillar(x: f64, y: f64, radius: f64) -> Pillar {
        Pillar { x, y, radius }
    }

    #[test]
    fn sand_slows_fighters_down() {
        let floor = ArenaLayout {
            width: 20.0,
            height: 20.0,
            speed_percent: 50,
            pillars: Vec::new(),
        };
        let mut arena = Arena::new(1.0, 1.0, vec![fighter(0, 5.0, 5.0)], 0).with_layout(&floor);
        assert_eq!((arena.width, arena.height), (20.0, 20.0));
        play(&mut arena, &[Action::Move(Vec2::new(1.0, 0.0))], 30);
        // Speed 30 → 3 m/s, halved, for one second.
        assert!((arena.fighters[0].position.x - 6.5).abs() < 1e-9);
    }

    #[test]
    fn a_fighter_cannot_walk_through_a_pillar() {
        let mut arena = Arena::new(20.0, 20.0, vec![fighter(0, 5.0, 5.0)], 0);
        arena.pillars = vec![pillar(8.0, 5.0, 1.0)];
        play(&mut arena, &[Action::Move(Vec2::new(1.0, 0.0))], 60);
        let position = arena.fighters[0].position;
        assert!(
            (position.x - (8.0 - 1.0 - RADIUS)).abs() < 1e-9,
            "{position:?}"
        );
    }

    #[test]
    fn a_pillar_blocks_a_shot_but_not_one_beside_it() {
        let pillars = [pillar(10.0, 10.0, 1.0)];
        assert!(shot_blocked(
            &pillars,
            Vec2::new(3.0, 10.0),
            Vec2::new(17.0, 10.0)
        ));
        assert!(shot_blocked(
            &pillars,
            Vec2::new(3.0, 9.5),
            Vec2::new(17.0, 10.5)
        ));
        assert!(!shot_blocked(
            &pillars,
            Vec2::new(3.0, 12.0),
            Vec2::new(17.0, 12.0)
        ));
        // Both on the same side, the pillar behind one of them.
        assert!(!shot_blocked(
            &pillars,
            Vec2::new(3.0, 10.0),
            Vec2::new(8.0, 10.0)
        ));
        assert!(!shot_blocked(
            &[],
            Vec2::new(3.0, 10.0),
            Vec2::new(17.0, 10.0)
        ));
    }

    #[test]
    fn a_ranged_spell_does_not_go_through_a_pillar() {
        let mut fighters = vec![fighter(0, 5.0, 5.0), fighter(1, 9.0, 5.0)];
        fighters[0].spells[0].range = Range::Ranged;
        fighters[0].spells[0].reach = 10.0;
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.pillars = vec![pillar(7.0, 5.0, 0.5)];
        play(&mut arena, &[HIT, Action::Wait], 1);
        assert_eq!(arena.fighters[1].hp, 100);
        arena.pillars.clear();
        play(&mut arena, &[HIT, Action::Wait], 1);
        assert_eq!(arena.fighters[1].hp, 100 - HIT_DAMAGE);
    }

    #[test]
    fn the_way_goes_straight_or_around_the_pillar_in_the_way() {
        let (from, to) = (Vec2::new(3.0, 10.0), Vec2::new(17.0, 11.0));
        assert_eq!(path_direction(&[], from, to), Vec2::new(14.0, 1.0));
        // Behind the fighter, or off the way: straight.
        let behind = [pillar(1.5, 10.0, 0.4)];
        assert_eq!(path_direction(&behind, from, to), Vec2::new(14.0, 1.0));
        let aside = [pillar(10.0, 15.0, 1.0)];
        assert_eq!(path_direction(&aside, from, to), Vec2::new(14.0, 1.0));
        // In the way, the target a little above it: round its top, clear of its edge.
        let center = pillar(10.0, 10.0, 1.0);
        let way = path_direction(&[center], from, to);
        assert!(way.x > 0.0 && way.y > 0.0, "{way:?}");
        let ahead = Vec2::new(from.x + way.x * 10.0, from.y + way.y * 10.0);
        let closest = segment_distance(Vec2::new(center.x, center.y), from, ahead);
        assert!(closest >= center.radius + RADIUS, "{closest}");
    }

    #[test]
    fn fighters_facing_each_other_across_a_pillar_go_round_the_same_way() {
        let pillars = [pillar(10.0, 10.0, 1.0)];
        let (a, b) = (Vec2::new(3.0, 10.0), Vec2::new(17.0, 10.0));
        let (from_a, from_b) = (
            path_direction(&pillars, a, b),
            path_direction(&pillars, b, a),
        );
        assert!(from_a.y > 0.0 && from_b.y > 0.0, "{from_a:?} {from_b:?}");
        let (c, d) = (Vec2::new(10.0, 3.0), Vec2::new(10.0, 17.0));
        let (from_c, from_d) = (
            path_direction(&pillars, c, d),
            path_direction(&pillars, d, c),
        );
        assert!(from_c.x > 0.0 && from_d.x > 0.0, "{from_c:?} {from_d:?}");
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
