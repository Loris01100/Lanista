use std::collections::BTreeMap;

use rand::RngExt;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use serde::{Deserialize, Serialize};

use crate::ai::{Action, CombatAi};
use crate::design_data::{AccuracyDrop, SpellData};
use crate::fighter::injury::{Location, LocationWeights, Side, roll_location};
use crate::fighter::personality::Personality;
use crate::fighter::{Effect, Fighter, FighterId, Lasting, Stats, percent_of, pick};
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

#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Where hits land; none is drawn, unless set with [`Arena::with_rules`].
    pub hits: Option<LocationWeights>,
    /// How long and how much a part hit bleeds; never, unless set with
    /// [`Arena::with_rules`].
    pub bleed: BleedRules,
    /// Percentage of their speed fighters walk at: 100, unless set with
    /// [`Arena::with_layout`].
    pub speed_percent: u32,
    /// What stands on the floor; nothing, unless set with [`Arena::with_layout`].
    pub pillars: Vec<Pillar>,
    /// What the floor offers to gather (`SpellData::gathers`); nothing, unless set with
    /// [`Arena::with_layout`].
    pub element: Option<Element>,
    /// The way out, on a wall: a fighter giving up must reach it to be out of the fight,
    /// and runs for it meanwhile (`Fighter::fleeing`). None, unless set with
    /// [`Arena::with_layout`]: a fighter then gives up on the spot.
    pub door: Option<Vec2>,
    /// Percentage of its speed a fighter running for the door walks at: 100, unless set
    /// with [`Arena::with_rules`].
    pub flee_speed_percent: u32,
    /// How mines wait, last and blast; laid mines vanish at once, unless set with
    /// [`Arena::with_rules`].
    pub mine_rules: MineRules,
    /// The mines on the floor, oldest first.
    pub mines: Vec<Mine>,
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
    /// Where a hit lands: relative chances of the head, the body, an arm and a leg.
    pub hits: LocationWeights,
    pub bleed: BleedRules,
    pub mines: MineRules,
    /// Percentage of its speed a fighter running for the door walks at (`Arena::door`).
    pub flee_speed_percent: u32,
    /// Each weapon type's auto attack, by key: what a fighter strikes with once none of
    /// its spells is both affordable and of use (`can_cast_on`, `ClassData::weapon`).
    pub weapons: BTreeMap<String, SpellData>,
    pub arenas: ArenaLayouts,
    pub melee: MeleeRules,
}

/// What a colosseum's arena does to the fight (see [`ArenaLayout`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

/// What a fighter may gather from an arena's floor and throw (`SpellData::thrown`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum Element {
    Sand,
    Stone,
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
    /// What its floor offers to gather.
    pub element: Element,
    pub pillars: Vec<Pillar>,
    /// The way out, on a wall, in meters.
    pub door: Vec2,
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
    /// Chance to get back up, in percent, from boldness 0 to boldness 100.
    pub rise_percent_most_timid: u32,
    pub rise_percent_boldest: u32,
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
        rise_percent_most_timid: 100,
        rise_percent_boldest: 100,
        rise_percent_lost_per_stun: 0,
    };
}

/// How a cutting hit may sever a limb or the head (see [`severed_part`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutRules {
    /// Chance a cutting hit on a limb the target survives standing severs it, in
    /// hundredths of a percent per tenth of the target's max HP the hit takes.
    pub limb_hundredths: u32,
    /// The same for a cutting hit on the head: the target dies.
    pub head_hundredths: u32,
    /// Chance, in percent, that a cutting hit on the head bringing its target to 0 HP
    /// takes it off: a fatal blow nobody survives.
    pub fatal_head_percent: u32,
}

impl CutRules {
    /// No hit ever severs anything.
    pub const NEVER: Self = Self {
        limb_hundredths: 0,
        head_hundredths: 0,
        fatal_head_percent: 0,
    };
}

/// How a part bleeds once a hit made it (`SpellData::bleed_percent`): each second for
/// its part's seconds, it takes `hp_percent_per_second` of the fighter's max HP, at
/// least 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BleedRules {
    pub hp_percent_per_second: u32,
    pub head_seconds: u32,
    pub body_seconds: u32,
    pub arm_seconds: u32,
    pub leg_seconds: u32,
}

impl BleedRules {
    /// Nothing ever bleeds.
    pub const NEVER: Self = Self {
        hp_percent_per_second: 0,
        head_seconds: 0,
        body_seconds: 0,
        arm_seconds: 0,
        leg_seconds: 0,
    };

    /// How long `part` bleeds, in seconds.
    #[must_use]
    pub fn seconds(&self, part: Location) -> u32 {
        match part {
            Location::Head => self.head_seconds,
            Location::Body => self.body_seconds,
            Location::Arm(_) => self.arm_seconds,
            Location::Leg(_) => self.leg_seconds,
        }
    }
}

/// How a mine laid by a spell (`SpellData::mine`) waits, lasts and blasts, from
/// `data/combat.ron`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineRules {
    /// Ticks from its fall until it blasts under whoever touches it.
    pub arm_ticks: u32,
    /// Ticks from its fall until it is gone, blasted or not.
    pub life_ticks: u32,
    /// In meters: a fighter touching it once armed sets it off.
    pub radius: f64,
    /// It hits every fighter whose center is this close, in meters.
    pub blast_radius: f64,
    /// A fighter's mines on the floor at once: a new one takes the oldest's place.
    pub max_per_fighter: usize,
}

impl MineRules {
    /// A mine is gone as soon as it is laid.
    pub const NEVER: Self = Self {
        arm_ticks: 0,
        life_ticks: 0,
        radius: 0.0,
        blast_radius: 0.0,
        max_per_fighter: 0,
    };
}

/// A mine on the floor, seen by everyone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mine {
    pub position: Vec2,
    /// Who laid it, with which of its spells: the blast hits with that spell.
    pub owner: FighterId,
    pub spell: usize,
    /// Tick from which it blasts under whoever touches it.
    pub armed_at: u32,
    /// Tick it is gone at.
    pub gone_at: u32,
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
            hits: None,
            bleed: BleedRules::NEVER,
            speed_percent: 100,
            pillars: Vec::new(),
            element: None,
            door: None,
            flee_speed_percent: 100,
            mine_rules: MineRules::NEVER,
            mines: Vec::new(),
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

    /// This arena, where attacks hit, land, stun and sever following `rules`, as in real
    /// fights.
    #[must_use]
    pub fn with_rules(mut self, rules: &CombatRules) -> Self {
        self.accuracy = rules.accuracy;
        self.stun = rules.stun;
        self.cut = rules.cut;
        self.hits = Some(rules.hits);
        self.bleed = rules.bleed;
        self.mine_rules = rules.mines;
        self.flee_speed_percent = rules.flee_speed_percent;
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

    /// This arena on `layout`'s floor: its size, its pillars, its door, its ground
    /// slowing fighters down to its speed percentage, and its element, which the
    /// fighters' throwing spells now hit with (`SpellData::thrown`).
    #[must_use]
    pub fn with_layout(mut self, layout: &ArenaLayout) -> Self {
        self.width = layout.width;
        self.height = layout.height;
        self.speed_percent = layout.speed_percent;
        self.pillars.clone_from(&layout.pillars);
        self.element = Some(layout.element);
        self.door = Some(layout.door);
        for spell in self.fighters.iter_mut().flat_map(|f| &mut f.spells) {
            if let Some(thrown) = spell.thrown.get(&layout.element) {
                spell.power = thrown.power;
                spell.damage_kind = thrown.damage_kind;
                spell.accuracy_drop = thrown.accuracy_drop;
            }
        }
        self
    }

    /// What fighters walk around: the pillars, and the mines on the floor, armed or not.
    #[must_use]
    pub fn obstacles(&self) -> Vec<Pillar> {
        let mines = self.mines.iter().map(|m| Pillar {
            x: m.position.x,
            y: m.position.y,
            radius: self.mine_rules.radius,
        });
        self.pillars.iter().copied().chain(mines).collect()
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
            if fighter.is_standing() && !fighter.fleeing {
                let action = ai.decide(arena, fighter.id);
                arena.fighters[i].action = action;
            }
        }
    }
    run_for_the_door(arena);
    cool_down(arena);
    recover(arena);
    wear_off(arena);
    move_fighters(arena);
    separate(arena);
    keep_inside(arena);
    escape(arena);
    blast_mines(arena);
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

/// Chance in percent that `spell` hits: `hit_percent` with the accuracy the attacker
/// has for now (`Fighter::accuracy`), times the spell's `accuracy_percent`.
#[must_use]
pub fn spell_hit_percent(
    attacker: &Fighter,
    defender: &Stats,
    spell: &SpellData,
    rules: &AccuracyRules,
) -> u32 {
    let aim = Stats {
        accuracy: attacker.accuracy(),
        ..attacker.stats
    };
    let percent =
        u64::from(hit_percent(&aim, defender, rules)) * u64::from(spell.accuracy_percent) / 100;
    u32::try_from(percent).unwrap_or(u32::MAX)
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

/// Whether a cutting hit on `part` taking `damage` HP severs it from `target`, as the
/// hit left it: the head with `fatal_head_percent` if the hit brought it to 0 HP, else
/// with `head_hundredths` per tenth of its max HP the hit takes; a limb, if it still
/// fights, with `limb_hundredths` per tenth, unless it already lost it (a stump); the
/// body never. `sure` severs the head or a limb that can be. Drawn only above 0.
pub fn severs<R: rand::Rng + ?Sized>(
    rng: &mut R,
    (damage, sure): (u32, bool),
    target: &Fighter,
    part: Location,
    rules: &CutRules,
) -> bool {
    let chance = match part {
        Location::Head if sure => 10_000,
        Location::Head if target.is_alive() => {
            per_tenth(rules.head_hundredths, damage, target.stats.hp)
        }
        Location::Head => u64::from(rules.fatal_head_percent.min(100)) * 100,
        Location::Arm(_) | Location::Leg(_) if target.is_fighting() && !target.has_lost(part) => {
            if sure {
                10_000
            } else {
                per_tenth(rules.limb_hundredths, damage, target.stats.hp)
            }
        }
        Location::Arm(_) | Location::Leg(_) | Location::Body => 0,
    };
    chance > 0 && u64::from(rng.random_range(0..10_000_u32)) < chance
}

/// Which limb a fighter loses: an arm or a leg by `weights`, on a side drawn 50/50.
pub fn severed_limb<R: rand::Rng + ?Sized>(rng: &mut R, weights: &LocationWeights) -> Location {
    let side = if rng.random_bool(0.5) {
        Side::Left
    } else {
        Side::Right
    };
    pick(
        rng,
        &[
            (weights.arm, Location::Arm(side)),
            (weights.leg, Location::Leg(side)),
        ],
    )
}

/// Chance, in percent, that `fighter`, down after its latest stun, gets back up: from
/// `rise_percent_most_timid` at boldness 0 to `rise_percent_boldest`
/// at 100, less `rise_percent_lost_per_stun` for each of its stuns after the first.
#[must_use]
pub fn rise_percent(fighter: &Fighter, rules: &StunRules) -> u32 {
    let low = i64::from(rules.rise_percent_most_timid);
    let high = i64::from(rules.rise_percent_boldest);
    let boldness = i64::from(fighter.condition.personality.boldness.value());
    let percent = low + (high - low) * boldness / i64::from(Personality::MAX);
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

/// Whether `caster`'s spell `spell` would do anything now against `target`, cooldown
/// and cost aside: a gathering spell on a floor with an element, its reserve not full;
/// any other in reach, a ranged one with a clear shot, a mine with the target out of
/// its own blast.
#[must_use]
pub fn of_use(arena: &Arena, caster: &Fighter, spell: usize, target: &Fighter) -> bool {
    let Some(data) = caster.spells.get(spell) else {
        return false;
    };
    if data.gathers > 0 {
        return arena.element.is_some() && caster.elements.current < caster.elements.max;
    }
    let distance = edge_distance(caster, target);
    distance <= data.reach
        && !(data.range == Range::Ranged
            && shot_blocked(&arena.pillars, caster.position, target.position))
        && !(data.mine && distance + 2.0 * RADIUS <= arena.mine_rules.blast_radius)
}

/// `Fighter::can_cast` against `target`: the auto attack as soon as none of the other
/// spells is both affordable and of use (`of_use`), so that a fighter with only a mine
/// too close to lay or nothing to gather still strikes with its weapon.
#[must_use]
pub fn can_cast_on(arena: &Arena, caster: &Fighter, spell: usize, target: &Fighter) -> bool {
    caster.can_cast_when(spell, |s| of_use(arena, caster, s, target))
}

/// Whether `f` chose a cast it can make on its chosen target (`can_cast_on`).
fn chose_castable(arena: &Arena, f: &Fighter) -> bool {
    let Action::Cast { spell, target } = f.action else {
        return false;
    };
    arena
        .fighter(target)
        .map_or_else(|| f.can_cast(spell), |t| can_cast_on(arena, f, spell, t))
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
        .filter(|&i| fighters[i].is_standing() && chose_castable(arena, &fighters[i]))
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
/// its next decision. The spell hits with `hit_percent`, drawn only when below 100, and
/// lands on a part of the target's body drawn by `Arena::hits`; a cutting hit may then
/// sever it: the head, killing, or a limb, taking it out of the fight alive (`severs`);
/// the target dies at 0 HP, gives up at its surrender
/// threshold, or else may be stunned with `stun_chance`, drawn only above 0, staying
/// down for `StunRules::ticks`.
fn cast_spells(arena: &mut Arena) {
    for i in cast_order(arena) {
        if let Some(spell) = ready_gather(arena, i) {
            gather(arena, i, spell);
        } else if let Some((spell, target, t)) = ready_cast(arena, i) {
            if arena.fighters[i].spells[spell].mine {
                lay(arena, i, spell, t);
            } else {
                cast(arena, i, spell, (target, t));
            }
        }
    }
}

/// The gathering spell fighter `i` casts on itself (`SpellData::gathers`), if it may:
/// standing, the spell ready and affordable, on a floor with an element.
fn ready_gather(arena: &Arena, i: usize) -> Option<usize> {
    let caster = &arena.fighters[i];
    let Action::Cast { spell, target } = caster.action else {
        return None;
    };
    let ready = target == caster.id
        && arena.element.is_some()
        && caster.spells.get(spell).is_some_and(|s| s.gathers > 0)
        && caster.is_standing()
        && chose_castable(arena, caster);
    ready.then_some(spell)
}

/// Fighter `i` casts its gathering spell `spell`: what it gathers fills its reserve, up
/// to its max.
fn gather(arena: &mut Arena, i: usize, spell: usize) {
    let caster = &mut arena.fighters[i];
    let before = caster.elements.current;
    caster.elements.current = before
        .saturating_add(caster.spells[spell].gathers)
        .min(caster.elements.max);
    caster.spend(spell);
    caster.action = Action::Wait;
    let (fighter, amount) = (caster.id, caster.elements.current - before);
    arena.log.push(Event {
        tick: arena.tick,
        kind: EventKind::Gather {
            fighter,
            spell,
            amount,
        },
    });
}

/// Fighter `i` lays its `spell`'s mines (`SpellData::blows`): the first at the feet of
/// fighter `t`, each next one `blast_radius` and a mine's radius further aside, across
/// the caster's line, on the other side if that one is past the walls; past
/// `max_per_fighter`, its oldest mines are gone.
// ponytail: the mine falls on its target's feet; a free landing point when an AI needs one.
fn lay(arena: &mut Arena, i: usize, spell: usize, t: usize) {
    let (from, at) = (arena.fighters[i].position, arena.fighters[t].position);
    let caster = &mut arena.fighters[i];
    let blows = caster.spells[spell].blows.max(1);
    caster.spend(spell);
    caster.action = Action::Wait;
    let owner = caster.id;
    let (tick, rules) = (arena.tick, arena.mine_rules);
    let line = Vec2::new(at.x - from.x, at.y - from.y);
    let length = line.length();
    let side = if length > 0.0 {
        Vec2::new(-line.y / length, line.x / length)
    } else {
        Vec2::new(0.0, 1.0)
    };
    let (low, high) = arena.reachable();
    let inside = |p: Vec2| (low.x..=high.x).contains(&p.x) && (low.y..=high.y).contains(&p.y);
    for k in 0..blows {
        let offset = (rules.blast_radius + rules.radius) * f64::from(k);
        let aside =
            |sign: f64| Vec2::new(at.x + sign * side.x * offset, at.y + sign * side.y * offset);
        let position = Some(aside(1.0))
            .filter(|&p| inside(p))
            .unwrap_or_else(|| aside(-1.0));
        arena.mines.push(Mine {
            position,
            owner,
            spell,
            armed_at: tick.saturating_add(rules.arm_ticks),
            gone_at: tick.saturating_add(rules.life_ticks),
        });
    }
    while arena.mines.iter().filter(|m| m.owner == owner).count() > rules.max_per_fighter
        && let Some(oldest) = arena.mines.iter().position(|m| m.owner == owner)
    {
        arena.mines.remove(oldest);
    }
    arena.log.push(Event {
        tick,
        kind: EventKind::Lay {
            fighter: owner,
            spell,
        },
    });
}

/// Distance between two points, in meters.
fn distance(a: Vec2, b: Vec2) -> f64 {
    Vec2::new(b.x - a.x, b.y - a.y).length()
}

/// Mines whose time is up are gone; an armed mine touched by a fighter still in the
/// fight blasts (`blast`).
fn blast_mines(arena: &mut Arena) {
    let tick = arena.tick;
    arena.mines.retain(|m| tick < m.gone_at);
    let reach = RADIUS + arena.mine_rules.radius;
    let mut k = 0;
    while k < arena.mines.len() {
        let mine = arena.mines[k];
        let touched = tick >= mine.armed_at
            && arena
                .fighters
                .iter()
                .any(|f| f.is_fighting() && distance(f.position, mine.position) < reach);
        if touched {
            arena.mines.remove(k);
            blast(arena, mine);
        } else {
            k += 1;
        }
    }
}

/// `mine` blasts: every fighter still in the fight within `blast_radius`, its owner
/// included, takes its spell's damage, never missed, on a part drawn by `Arena::hits`,
/// then the hit's effects (`strike`, `afflict`); the damage to others counts for its owner.
fn blast(arena: &mut Arena, mine: Mine) {
    let Some(o) = usize::try_from(mine.owner.0)
        .ok()
        .filter(|&o| o < arena.fighters.len())
    else {
        return;
    };
    let data = &arena.fighters[o].spells[mine.spell];
    let (kind, bleed_percent, accuracy_drop) =
        (data.damage_kind, data.bleed_percent, data.accuracy_drop);
    for t in 0..arena.fighters.len() {
        let target = &arena.fighters[t];
        if !target.is_fighting()
            || distance(target.position, mine.position) > arena.mine_rules.blast_radius
        {
            continue;
        }
        let owner = &arena.fighters[o];
        let amount = spell_damage(&owner.stats, &target.stats, &owner.spells[mine.spell]);
        let part = arena.hits.map(|w| roll_location(&mut arena.rng, &w));
        let target = &mut arena.fighters[t];
        let hp_before = target.hp;
        target.take_damage(amount);
        let (damage, id) = (hp_before - target.hp, target.id);
        arena.log.push(Event {
            tick: arena.tick,
            kind: EventKind::Blast {
                owner: mine.owner,
                spell: mine.spell,
                target: id,
                damage,
                part,
            },
        });
        let cut = part.filter(|_| kind == DamageKind::Cutting);
        strike(arena, (id, t), (damage, false), kind, cut);
        afflict(arena, (mine.owner, t), (bleed_percent, part), accuracy_drop);
        if t != o {
            let owner = &mut arena.fighters[o];
            owner.round_damage = owner.round_damage.saturating_add(damage);
        }
    }
}

/// The spell, target and target index of fighter `i`'s cast, if it may land: both
/// standing and not allied, the spell ready, affordable and in reach, a shot's line clear.
fn ready_cast(arena: &Arena, i: usize) -> Option<(usize, FighterId, usize)> {
    let caster = &arena.fighters[i];
    let Action::Cast { spell, target } = caster.action else {
        return None;
    };
    let t = usize::try_from(target.0).ok()?;
    let defender = arena.fighters.get(t)?;
    let ready = caster.is_standing()
        && defender.is_standing()
        && caster.id != target
        && !arena.allied(caster.id, target)
        && can_cast_on(arena, caster, spell, defender)
        && caster.spells[spell].gathers == 0
        && edge_distance(caster, defender) <= caster.spells[spell].reach
        && (caster.spells[spell].range != Range::Ranged
            || !shot_blocked(&arena.pillars, caster.position, defender.position));
    ready.then_some((spell, target, t))
}

/// Fighter `i` casts `spell` on the target: one hit roll for all its blows
/// (`SpellData::blows`), then, for each blow while the target still fights, where it
/// lands is drawn, the same damage dealt and logged, and its effects on the target
/// follow (`strike`, `afflict`).
fn cast(arena: &mut Arena, i: usize, spell: usize, (target, t): (FighterId, usize)) {
    let (caster, defender) = (&arena.fighters[i], &arena.fighters[t]);
    let data = &caster.spells[spell];
    let chance = spell_hit_percent(caster, &defender.stats, data, &arena.accuracy);
    let missed = chance < 100 && arena.rng.random_range(0..100) >= chance;
    let blows = if missed { 1 } else { data.blows.max(1) };
    let amount = if missed {
        0
    } else {
        spell_damage(&caster.stats, &defender.stats, data)
    };
    let kind = data.damage_kind;
    let sure_cut_hits = data.sure_cut_hits.filter(|_| !missed);
    let (bleed_percent, accuracy_drop) = (data.bleed_percent, data.accuracy_drop);
    let caster = &mut arena.fighters[i];
    caster.spend(spell);
    caster.action = Action::Wait;
    let caster = caster.id;
    for blow in 0..blows {
        if blow > 0 && !arena.fighters[t].is_fighting() {
            break;
        }
        let part = match arena.hits {
            Some(weights) if !missed => Some(roll_location(&mut arena.rng, &weights)),
            _ => None,
        };
        let defender = &mut arena.fighters[t];
        let hp_before = defender.hp;
        defender.take_damage(amount);
        let damage = hp_before - defender.hp;
        let sure = sure_cut_hits.is_some_and(|hits| {
            defender.cleaving_hits += 1;
            defender.cleaving_hits >= hits
        });
        arena.log.push(Event {
            tick: arena.tick,
            kind: EventKind::Cast {
                caster,
                target,
                spell,
                damage,
                missed,
                part,
            },
        });
        let cut = part.filter(|_| kind == DamageKind::Cutting);
        strike(arena, (target, t), (damage, sure), kind, cut);
        if !missed {
            afflict(arena, (caster, t), (bleed_percent, part), accuracy_drop);
        }
        let caster = &mut arena.fighters[i];
        caster.round_damage = caster.round_damage.saturating_add(damage);
    }
}

/// What a hit from `source` leaves on the target, if it still fights: the part it
/// landed on bleeds with `bleed_percent`, drawn only if that part can and does not
/// already; its accuracy drops by `accuracy_drop`, adding up with the drops it has.
fn afflict(
    arena: &mut Arena,
    (source, t): (FighterId, usize),
    (bleed_percent, part): (u32, Option<Location>),
    accuracy_drop: Option<AccuracyDrop>,
) {
    let defender = &arena.fighters[t];
    if !defender.is_fighting() {
        return;
    }
    let mut effects = Vec::new();
    if let Some(part) = part {
        let seconds = arena.bleed.seconds(part);
        if bleed_percent > 0
            && seconds > 0
            && !defender.bleeds(part)
            && arena.rng.random_range(0..100) < bleed_percent
        {
            let effect = Effect::Bleeding { part, source };
            effects.push((effect, seconds.saturating_mul(TICKS_PER_SECOND)));
        }
    }
    if let Some(drop) = accuracy_drop.filter(|d| d.percent > 0 && d.ticks > 0) {
        let effect = Effect::Blinded {
            percent: drop.percent,
        };
        effects.push((effect, drop.ticks));
    }
    let fighter = arena.fighters[t].id;
    for (effect, ticks_left) in effects {
        arena.fighters[t]
            .effects
            .push(Lasting { effect, ticks_left });
        arena.log.push(Event {
            tick: arena.tick,
            kind: EventKind::Afflicted { fighter, effect },
        });
    }
}

/// Counts down the effects on every fighter still in the fight, which wear off at 0. A
/// bleeding part takes `BleedRules::hp_percent_per_second` of the fighter's max HP, at
/// least 1, at the end of each of its seconds, as damage its source dealt: it may die
/// or give up.
fn wear_off(arena: &mut Arena) {
    let percent = arena.bleed.hp_percent_per_second;
    for t in 0..arena.fighters.len() {
        let f = &mut arena.fighters[t];
        if !f.is_fighting() {
            continue;
        }
        let mut bled = Vec::new();
        for lasting in &mut f.effects {
            lasting.ticks_left = lasting.ticks_left.saturating_sub(1);
            if let Effect::Bleeding { part, source } = lasting.effect
                && lasting.ticks_left.is_multiple_of(TICKS_PER_SECOND)
            {
                bled.push((part, source));
            }
        }
        f.effects.retain(|l| l.ticks_left > 0);
        for (part, source) in bled {
            let f = &mut arena.fighters[t];
            if !f.is_fighting() {
                break;
            }
            let hp_before = f.hp;
            f.take_damage(percent_of(f.stats.hp, percent).max(1));
            let damage = hp_before - f.hp;
            let (fighter, died) = (f.id, !f.is_alive());
            let gave_up = give_up(arena, t);
            if let Some(s) = usize::try_from(source.0)
                .ok()
                .and_then(|s| arena.fighters.get_mut(s))
            {
                s.round_damage = s.round_damage.saturating_add(damage);
            }
            let tick = arena.tick;
            let mut log = |kind| arena.log.push(Event { tick, kind });
            log(EventKind::Bled {
                fighter,
                source,
                part,
                damage,
            });
            if died {
                log(EventKind::Death { fighter });
            } else if let Some(kind) = gave_up {
                log(kind);
            }
        }
    }
}

/// The effects of a hit on the target: a cut on its head may sever it, killing, or on a
/// limb, taking it out of the fight alive (`severs`, for sure if `sure`); it dies at 0 HP, gives up at its
/// surrender threshold, or else may be stunned with `stun_chance`, drawn only above 0.
fn strike(
    arena: &mut Arena,
    (target, t): (FighterId, usize),
    (damage, sure): (u32, bool),
    kind: DamageKind,
    cut: Option<Location>,
) {
    let defender = &mut arena.fighters[t];
    let severed =
        cut.filter(|&part| severs(&mut arena.rng, (damage, sure), defender, part, &arena.cut));
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
    let gave_up = if died { None } else { give_up(arena, t) };
    let tick = arena.tick;
    let mut log = |kind| arena.log.push(Event { tick, kind });
    if let Some(part) = severed {
        log(EventKind::Sever {
            fighter: target,
            part,
        });
    }
    if died {
        log(EventKind::Death { fighter: target });
    } else if let Some(kind) = gave_up {
        log(kind);
    } else if stunned {
        log(EventKind::Stun { fighter: target });
    }
}

/// What fighter `t` giving up on the hit just taken does, if it just reached its
/// surrender threshold: out on the spot (`Surrender`), or, in an arena with a door, on
/// the run for it (`Flee`), still in the fight.
fn give_up(arena: &mut Arena, t: usize) -> Option<EventKind> {
    let f = &mut arena.fighters[t];
    if !f.surrendered || f.fleeing {
        return None;
    }
    let fighter = f.id;
    if arena.door.is_none() || f.severed.is_some() {
        return Some(EventKind::Surrender { fighter });
    }
    f.surrendered = false;
    f.fleeing = true;
    f.action = Action::Wait;
    Some(EventKind::Flee { fighter })
}

/// Every fighter on the run and standing walks for the door, every tick, around the
/// pillars, the mines and the other fighters: one standing in the way blocks it.
// ponytail: goes round the nearest one in the way, no plan to lure a blocker off the door.
fn run_for_the_door(arena: &mut Arena) {
    let Some(door) = arena.door else {
        return;
    };
    for i in 0..arena.fighters.len() {
        let f = &arena.fighters[i];
        if !f.fleeing || !f.is_standing() {
            continue;
        }
        let others = arena
            .fighters
            .iter()
            .filter(|o| o.id != f.id && o.is_fighting())
            .map(|o| Pillar {
                x: o.position.x,
                y: o.position.y,
                radius: RADIUS,
            });
        let obstacles: Vec<Pillar> = arena.obstacles().into_iter().chain(others).collect();
        let direction = path_direction(&obstacles, f.position, door);
        arena.fighters[i].action = Action::Move(direction);
    }
}

/// Every fighter on the run that reached the door, or still running when time is up,
/// gives up: out of the fight, alive.
fn escape(arena: &mut Arena) {
    let Some(door) = arena.door else {
        return;
    };
    let last = arena.tick + 1 >= arena.max_ticks;
    for f in &mut arena.fighters {
        if f.fleeing && f.is_fighting() && (last || distance(f.position, door) <= 2.0 * RADIUS) {
            f.surrendered = true;
            f.action = Action::Wait;
            arena.log.push(Event {
                tick: arena.tick,
                kind: EventKind::Surrender { fighter: f.id },
            });
        }
    }
}

/// Moves every fighter standing that chose to, at its speed slowed by the ground, and
/// sped up by fear for one running for the door.
fn move_fighters(arena: &mut Arena) {
    let ground = f64::from(arena.speed_percent) / 100.0;
    let fear = f64::from(arena.flee_speed_percent) / 100.0;
    for f in arena.fighters.iter_mut().filter(|f| f.is_standing()) {
        if let Action::Move(direction) = f.action {
            let length = direction.length();
            if length > 0.0 {
                let ground = if f.fleeing { ground * fear } else { ground };
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
    use crate::fighter::{FighterId, Gauge, Stats, test_fighter as fighter, test_thrower};

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
        rise_percent_most_timid: 30,
        rise_percent_boldest: 90,
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
    fn the_bold_get_up_more_often_and_every_stun_makes_it_harder() {
        use crate::fighter::personality::TraitValue;

        let mut f = fighter(0, 0.0, 0.0);
        let rise = |f: &Fighter, boldness| {
            let mut f = f.clone();
            f.condition.personality.boldness = TraitValue::new(boldness);
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
            rise_percent_most_timid: rise_percent,
            rise_percent_boldest: rise_percent,
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
    };

    const SURE: CutRules = CutRules {
        limb_hundredths: 10_000,
        head_hundredths: 10_000,
        fatal_head_percent: 100,
    };

    /// Hits land only on `head`, `body`, `arm` or `leg`, by these weights.
    const fn landing(head: u32, body: u32, arm: u32, leg: u32) -> LocationWeights {
        LocationWeights {
            head,
            body,
            arm,
            leg,
        }
    }

    /// `duel(0.5)` where fighter 0 strikes with a cutting weapon, landing by `hits` and
    /// severing by `cut`.
    fn cutting_duel(cut: CutRules, hits: LocationWeights) -> Arena {
        let mut arena = duel(0.5);
        arena.fighters[0].spells[0].damage_kind = DamageKind::Cutting;
        arena.cut = cut;
        arena.hits = Some(hits);
        arena
    }

    #[test]
    fn a_fatal_cut_on_the_head_may_behead_and_a_lighter_one_sever_a_limb() {
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let mut target = fighter(1, 0.0, 0.0);
        let arm = Location::Arm(Side::Left);
        // Standing after the hit: the limb hit, never the body.
        assert!(severs(&mut rng, (10, false), &target, arm, &SURE));
        assert!(severs(
            &mut rng,
            (10, false),
            &target,
            Location::Leg(Side::Right),
            &SURE
        ));
        assert!(!severs(
            &mut rng,
            (10, false),
            &target,
            Location::Body,
            &SURE
        ));
        let no_head = CutRules {
            head_hundredths: 0,
            ..SURE
        };
        assert!(!severs(
            &mut rng,
            (10, false),
            &target,
            Location::Head,
            &no_head
        ));
        // Gave up: no limb is cut off a fighter out of the fight.
        target.surrendered = true;
        assert!(!severs(&mut rng, (10, false), &target, arm, &SURE));
        // Brought to 0 HP: the head, by fatal_head_percent.
        target.hp = 0;
        assert!(severs(
            &mut rng,
            (10, false),
            &target,
            Location::Head,
            &no_head
        ));
        let never = CutRules {
            fatal_head_percent: 0,
            ..no_head
        };
        assert!(!severs(
            &mut rng,
            (10, false),
            &target,
            Location::Head,
            &never
        ));
        let beheaded = (0..10_000)
            .filter(|_| severs(&mut rng, (10, false), &target, Location::Head, &CUT))
            .count();
        assert!(beheaded.abs_diff(3_000) < 150, "{beheaded}");
        // Standing, 10 % of the HP: 0.05 % for the head, 0.2 % for a limb.
        let mut target = fighter(1, 0.0, 0.0);
        target.hp = 90;
        let cut = |rng: &mut ChaCha8Rng, part| {
            (0..100_000)
                .filter(|_| severs(rng, (10, false), &target, part, &CUT))
                .count()
        };
        let head = cut(&mut rng, Location::Head);
        assert!(head.abs_diff(50) < 25, "{head}");
        let limb = cut(&mut rng, arm);
        assert!(limb.abs_diff(200) < 50, "{limb}");
    }

    #[test]
    fn a_limb_lost_before_is_a_stump_nothing_is_cut_off() {
        use crate::fighter::injury::severed_limb_wound;

        let mut target = fighter(1, 0.0, 0.0);
        target.condition.injuries = vec![severed_limb_wound(Location::Arm(Side::Left))];
        assert!(target.has_lost(Location::Arm(Side::Left)));
        assert!(!target.has_lost(Location::Arm(Side::Right)));
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        assert!(!severs(
            &mut rng,
            (10, false),
            &target,
            Location::Arm(Side::Left),
            &SURE
        ));
        assert!(severs(
            &mut rng,
            (10, false),
            &target,
            Location::Arm(Side::Right),
            &SURE
        ));
    }

    #[test]
    fn arms_and_legs_are_severed_by_their_weights() {
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        for _ in 0..100 {
            assert!(matches!(
                severed_limb(&mut rng, &landing(1, 1, 1, 0)),
                Location::Arm(_)
            ));
        }
        let left = (0..1000)
            .filter(|_| {
                matches!(
                    severed_limb(&mut rng, &landing(0, 0, 1, 1)),
                    Location::Arm(Side::Left) | Location::Leg(Side::Left)
                )
            })
            .count();
        assert!(left.abs_diff(500) < 60, "{left}");
    }

    #[test]
    fn hits_land_by_their_weights_and_the_log_tells_where() {
        let mut arena = duel(0.5);
        arena.hits = Some(landing(0, 1, 0, 0));
        play(&mut arena, &[HIT, Action::Wait], 30);
        assert!(arena.log.iter().all(|e| matches!(
            e.kind,
            EventKind::Cast {
                part: Some(Location::Body),
                ..
            }
        )));
        assert!(crate::log::log_text(&arena).contains("damage to the body"));
        // Without rules, no part is drawn.
        let mut arena = duel(0.5);
        play(&mut arena, &[HIT, Action::Wait], 30);
        assert!(
            arena
                .log
                .iter()
                .all(|e| matches!(e.kind, EventKind::Cast { part: None, .. }))
        );
    }

    #[test]
    fn a_clumsy_spell_keeps_its_share_of_the_hit_chance() {
        let (attacker, defender) = (fighter(0, 0.0, 0.0), fighter(1, 0.0, 0.0));
        let mut spell = attacker.spells[0].clone();
        let full = spell_hit_percent(&attacker, &defender.stats, &spell, &ACCURACY);
        assert_eq!(
            full,
            hit_percent(&attacker.stats, &defender.stats, &ACCURACY)
        );
        spell.accuracy_percent = 50;
        let half = spell_hit_percent(&attacker, &defender.stats, &spell, &ACCURACY);
        assert_eq!(half, full / 2);
        // It misses even in an arena that always hits.
        let mut arena = duel(0.5);
        arena.fighters[0].spells[0].accuracy_percent = 0;
        play(&mut arena, &[HIT, Action::Wait], 30);
        assert!(
            arena
                .log
                .iter()
                .all(|e| matches!(e.kind, EventKind::Cast { missed: true, .. }))
        );
    }

    #[test]
    fn a_sure_cut_spell_severs_from_its_second_hit_on_a_target() {
        let mut arena = cutting_duel(CutRules::NEVER, landing(0, 0, 1, 0));
        arena.fighters[0].spells[0].sure_cut_hits = Some(2);
        play(&mut arena, &[HIT, Action::Wait], 1);
        assert_eq!(arena.fighters[1].cleaving_hits, 1);
        assert_eq!(arena.fighters[1].severed, None);
        play(&mut arena, &[HIT, Action::Wait], 30);
        assert!(matches!(arena.fighters[1].severed, Some(Location::Arm(_))));
        // Never the body, however many hits.
        let mut arena = cutting_duel(CutRules::NEVER, landing(0, 1, 0, 0));
        arena.fighters[0].spells[0].sure_cut_hits = Some(2);
        play(&mut arena, &[HIT, Action::Wait], 61);
        assert_eq!(arena.fighters[1].cleaving_hits, 3);
        assert_eq!(arena.fighters[1].severed, None);
        // The head too, standing.
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let target = fighter(1, 0.0, 0.0);
        assert!(severs(
            &mut rng,
            (1, true),
            &target,
            Location::Head,
            &CutRules::NEVER
        ));
    }

    /// `duel(0.5)` where fighter 0's hits land on the body and make it bleed for 3 s,
    /// 2 % of the max HP a second.
    fn bleeding_duel() -> Arena {
        let mut arena = duel(0.5);
        arena.fighters[0].spells[0].bleed_percent = 100;
        arena.hits = Some(landing(0, 1, 0, 0));
        arena.bleed = BleedRules {
            hp_percent_per_second: 2,
            body_seconds: 3,
            ..BleedRules::NEVER
        };
        arena
    }

    fn bleeds(arena: &Arena) -> usize {
        let bled = |e: &&Event| {
            matches!(
                e.kind,
                EventKind::Bled {
                    fighter: FighterId(1),
                    ..
                }
            )
        };
        arena.log.iter().filter(bled).count()
    }

    #[test]
    fn a_bleeding_part_loses_hp_each_second_for_its_time_and_never_bleeds_twice() {
        let mut arena = bleeding_duel();
        play(&mut arena, &[HIT, Action::Wait], 1);
        assert_eq!(arena.fighters[1].hp, 100 - HIT_DAMAGE);
        assert!(arena.fighters[1].bleeds(Location::Body));
        play(&mut arena, &[Action::Wait, Action::Wait], 120);
        assert_eq!(arena.fighters[1].hp, 100 - HIT_DAMAGE - 3 * 2);
        assert_eq!(bleeds(&arena), 3);
        let text = crate::log::log_text(&arena);
        assert!(text.contains("fighter 1's body bleeds"));
        assert!(text.contains("fighter 1 bleeds 2 from the body"));
        assert_eq!(arena.fighters[1].effects, Vec::new());
        // The bleeding counts as damage its source dealt.
        assert_eq!(arena.fighters[0].round_damage, HIT_DAMAGE + 6);
        assert_eq!(
            crate::log::Report::new(&arena).fighters[0].damage_dealt,
            HIT_DAMAGE + 6
        );
        // A second hit on the part still bleeding adds no bleeding.
        let mut arena = bleeding_duel();
        play(&mut arena, &[HIT, Action::Wait], 31);
        let afflicted = |e: &&Event| matches!(e.kind, EventKind::Afflicted { .. });
        assert_eq!(arena.log.iter().filter(afflicted).count(), 1);
        // Without bleeding rules, nothing bleeds.
        let mut arena = bleeding_duel();
        arena.bleed = BleedRules::NEVER;
        play(&mut arena, &[HIT, Action::Wait], 1);
        assert_eq!(arena.fighters[1].effects, Vec::new());
    }

    #[test]
    fn a_bleeding_may_kill() {
        let mut arena = bleeding_duel();
        arena.fighters[1].hp = HIT_DAMAGE + 2;
        play(&mut arena, &[HIT, Action::Wait], 1);
        play(&mut arena, &[Action::Wait, Action::Wait], 30);
        assert!(!arena.fighters[1].is_alive());
        assert!(matches!(
            arena.log.last().map(|e| e.kind),
            Some(EventKind::Death {
                fighter: FighterId(1)
            })
        ));
        assert_eq!(bleeds(&arena), 1);
    }

    #[test]
    fn accuracy_drops_add_up_then_wear_off() {
        let mut arena = duel(0.5);
        arena.fighters[0].spells[0].accuracy_drop = Some(AccuracyDrop {
            percent: 30,
            ticks: 60,
        });
        let before = spell_hit_percent(
            &arena.fighters[1],
            &arena.fighters[0].stats,
            &arena.fighters[1].spells[0],
            &ACCURACY,
        );
        play(&mut arena, &[HIT, Action::Wait], 31);
        // Two hits: 60 % of 30 lost.
        assert_eq!(arena.fighters[1].accuracy(), 12);
        let after = spell_hit_percent(
            &arena.fighters[1],
            &arena.fighters[0].stats,
            &arena.fighters[1].spells[0],
            &ACCURACY,
        );
        assert!(after < before);
        play(&mut arena, &[Action::Wait, Action::Wait], 60);
        assert_eq!(arena.fighters[1].accuracy(), 30);
        assert_eq!(arena.fighters[1].effects, Vec::new());
    }

    #[test]
    fn a_fighter_that_lost_a_limb_is_out_alive_and_never_hit_again() {
        let mut arena = cutting_duel(SURE, landing(0, 0, 1, 1));
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
        // The limb the hit landed on.
        assert!(arena.log.iter().any(|e| matches!(
            e.kind,
            EventKind::Cast { target: FighterId(1), part, .. } if part == out.severed
        )));
        // Not a single stun: the cut took it out first.
        assert_eq!(out.stuns, 0);
        let report = crate::log::Report::new(&arena);
        assert_eq!(report.winner, Some(FighterId(0)));
        assert_eq!(report.fighters[1].severed, out.severed);
        assert!(crate::log::log_text(&arena).contains("fighter 1 loses its "));
    }

    #[test]
    fn a_beheading_kills_even_a_light_hit() {
        let mut arena = cutting_duel(SURE, landing(1, 0, 0, 0));
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
        // The test spell is blunt.
        let mut arena = duel(0.5);
        arena.cut = SURE;
        arena.hits = Some(landing(1, 0, 1, 1));
        play(&mut arena, &[HIT, Action::Wait], 150);
        assert_eq!(arena.fighters[1].severed, None);
        // Without cut rules, a cutting hit severs nothing either, nor one on the body.
        for (cut, hits) in [
            (CutRules::NEVER, landing(1, 0, 1, 1)),
            (SURE, landing(0, 1, 0, 0)),
        ] {
            let mut arena = cutting_duel(cut, hits);
            play(&mut arena, &[HIT, Action::Wait], 150);
            assert_eq!(arena.fighters[1].severed, None);
            assert_eq!(arena.fighters[1].hp, 100 - 5 * HIT_DAMAGE);
        }
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

    /// A 20 m floor of `element`, nothing on it.
    fn floor_of(element: Element) -> ArenaLayout {
        ArenaLayout {
            width: 20.0,
            height: 20.0,
            speed_percent: 100,
            element,
            pillars: Vec::new(),
            door: Vec2::new(10.0, 0.0),
        }
    }

    /// A thrower (`test_thrower`) 0.5 m from a test fighter, on a floor of `element`.
    fn throwing_duel(element: Option<Element>) -> Arena {
        let fighters = vec![test_thrower(0, 5.0, 5.0), fighter(1, 6.5, 5.0)];
        let arena = Arena::new(20.0, 20.0, fighters, 0);
        match element {
            Some(element) => arena.with_layout(&floor_of(element)),
            None => arena,
        }
    }

    const GATHER: Action = Action::Cast {
        spell: 1,
        target: FighterId(0),
    };
    const THROW: Action = Action::Cast {
        spell: 2,
        target: FighterId(1),
    };

    #[test]
    fn a_fighter_gathers_the_floor_s_element_up_to_its_max_and_throws_it() {
        let mut arena = throwing_duel(Some(Element::Stone));
        play(&mut arena, &[GATHER, Action::Wait], 31);
        // Twice 3, kept to 4; 5 endurance each time.
        assert_eq!(arena.fighters[0].elements.current, 4);
        assert_eq!(arena.fighters[0].endurance.current, 40);
        assert_eq!(arena.fighters[1].hp, 100);
        let text = crate::log::log_text(&arena);
        assert!(text.contains("fighter 0 casts Récolte: 3 elements gathered"));
        assert!(text.contains("fighter 0 casts Récolte: 1 elements gathered"));
        let throw = arena.fighters[0].spells[2].clone();
        assert_eq!((throw.power, throw.damage_kind), (200, DamageKind::Blunt));
        // Thrown at the next decision, tick 36.
        play(&mut arena, &[THROW, Action::Wait], 6);
        let damage = spell_damage(&arena.fighters[0].stats, &arena.fighters[1].stats, &throw);
        assert_eq!(arena.fighters[1].hp, 100 - damage);
        // Both costs paid.
        assert_eq!(arena.fighters[0].elements.current, 2);
        assert_eq!(arena.fighters[0].endurance.current, 35);
    }

    #[test]
    fn a_throw_needs_all_its_costs_and_a_gathering_a_floor_with_an_element() {
        // No elements gathered yet.
        let mut arena = throwing_duel(Some(Element::Stone));
        play(&mut arena, &[THROW, Action::Wait], 1);
        assert_eq!(arena.fighters[1].hp, 100);
        // Elements, but no endurance.
        let mut arena = throwing_duel(Some(Element::Stone));
        arena.fighters[0].elements.current = 2;
        arena.fighters[0].endurance.current = 4;
        play(&mut arena, &[THROW, Action::Wait], 1);
        assert_eq!(arena.fighters[1].hp, 100);
        // Nothing to gather, nor anyone to gather on but oneself.
        let mut arena = throwing_duel(None);
        play(&mut arena, &[GATHER, Action::Wait], 1);
        assert_eq!(arena.fighters[0].elements.current, 0);
        assert_eq!(arena.fighters[0].endurance.current, 50);
        let mut arena = throwing_duel(Some(Element::Stone));
        let aimed = Action::Cast {
            spell: 1,
            target: FighterId(1),
        };
        play(&mut arena, &[aimed, Action::Wait], 1);
        assert_eq!(arena.fighters[0].elements.current, 0);
        assert_eq!(arena.log, Vec::new());
    }

    #[test]
    fn thrown_sand_takes_accuracy_off() {
        let mut arena = throwing_duel(Some(Element::Sand));
        assert_eq!(arena.fighters[0].spells[2].power, 50);
        arena.fighters[0].elements.current = 2;
        play(&mut arena, &[THROW, Action::Wait], 1);
        assert!(arena.fighters[1].hp < 100);
        assert_eq!(arena.fighters[1].accuracy(), 24);
    }

    const MINES: MineRules = MineRules {
        arm_ticks: 30,
        life_ticks: 300,
        radius: 0.3,
        blast_radius: 1.5,
        max_per_fighter: 1,
    };
    const LAY: Action = Action::Cast {
        spell: 0,
        target: FighterId(1),
    };

    /// Fighter 0, its strike made a mine spell reaching 20 m, ready again after
    /// `cooldown` ticks, aims at fighter 1, 4 m away; fighter 2 stands just behind it.
    fn mining(cooldown: u32) -> Arena {
        let fighters = vec![
            fighter(0, 5.0, 5.0),
            fighter(1, 10.0, 5.0),
            fighter(2, 11.0, 5.0),
        ];
        let mut arena = Arena::new(20.0, 20.0, fighters, 0);
        arena.mine_rules = MINES;
        let spell = &mut arena.fighters[0].spells[0];
        (spell.mine, spell.range, spell.reach, spell.cooldown) =
            (true, Range::Ranged, 20.0, cooldown);
        arena
    }

    #[test]
    fn a_mine_falls_at_its_target_s_feet_and_blasts_on_everyone_around_once_armed() {
        let mut arena = mining(600);
        let actions = [LAY, Action::Wait, Action::Wait];
        play(&mut arena, &actions, 1);
        assert_eq!(arena.mines.len(), 1);
        assert_eq!(arena.mines[0].position, Vec2::new(10.0, 5.0));
        assert_eq!(arena.fighters[0].endurance.current, 40);
        play(&mut arena, &actions, 29);
        assert_eq!(arena.fighters[1].hp, 100, "not armed yet");
        play(&mut arena, &actions, 1);
        assert_eq!(arena.mines, Vec::new());
        // Ranged 10 against 10, power 100: 10 to both, nothing to the layer, 5 m off.
        let hp: Vec<u32> = arena.fighters.iter().map(|f| f.hp).collect();
        assert_eq!(hp, [100, 90, 90]);
        assert_eq!(crate::log::Report::new(&arena).fighters[0].damage_dealt, 20);
        let text = crate::log::log_text(&arena);
        assert!(text.contains("fighter 0 casts Coup: a mine is laid"));
        assert!(text.contains("fighter 0's Coup blasts: fighter 2 takes 10 damage"));
    }

    #[test]
    fn an_untouched_mine_is_gone_in_time_and_a_new_one_takes_the_oldest_s_place() {
        let mut arena = mining(60);
        let away = Action::Move(Vec2::new(1.0, 0.0));
        arena.fighters.pop();
        // Laid at tick 0, then at tick 60 where fighter 1 walked to.
        play(&mut arena, &[LAY, away], 61);
        assert_eq!(arena.mines.len(), 1);
        assert!(arena.mines[0].position.x > 15.0);
        play(&mut arena, &[Action::Wait, away], 299);
        assert_eq!(arena.mines.len(), 1);
        play(&mut arena, &[Action::Wait, away], 1);
        assert_eq!(arena.mines, Vec::new());
        assert!(
            !arena
                .log
                .iter()
                .any(|e| matches!(e.kind, EventKind::Blast { .. }))
        );
    }

    #[test]
    fn a_second_mine_falls_aside_out_of_the_first_s_blast_inside_the_walls() {
        for (target_y, second_y) in [(5.0, 6.8), (19.4, 17.6)] {
            let mut arena = mining(600);
            arena.mine_rules.max_per_fighter = 2;
            arena.fighters[0].spells[0].blows = 2;
            // Across a line along x: aside is along y.
            arena.fighters[0].position.y = target_y;
            arena.fighters[1].position.y = target_y;
            play(&mut arena, &[LAY, Action::Wait, Action::Wait], 1);
            let positions: Vec<Vec2> = arena.mines.iter().map(|m| m.position).collect();
            assert_eq!(positions[0], Vec2::new(10.0, target_y));
            assert_eq!(positions[1].x, 10.0);
            assert!((positions[1].y - second_y).abs() < 1e-9, "{positions:?}");
        }
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
            element: Element::Sand,
            pillars: Vec::new(),
            door: Vec2::new(10.0, 0.0),
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
