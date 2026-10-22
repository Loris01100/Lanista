//! The learning AI's environment (phase 7): a duel or a melee of generated fighters played one
//! decision at a time by actions chosen from outside or by the utility AI, each fighter
//! seeing an observation and getting a reward. `lanista_py` exposes it to Python for training; the learned AI (`learned`) reads
//! the same observations.

use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::ai::{Action, CombatAi, UtilityAi};
use crate::combat::{Arena, ArenaKind, Pillar, TICKS_PER_DECISION, Vec2, edge_distance, tick};
use crate::design_data::DesignData;
use crate::duel::{STATS, random_duel};
use crate::fighter::career::CareerFighter;
use crate::fighter::equipment::Refused;
use crate::fighter::form::Form;
use crate::fighter::illness::IllnessLevel;
use crate::fighter::injury::{Location, Severity, Side};
use crate::fighter::morale::Morale;
use crate::fighter::personality::Personality;
use crate::fighter::{Fighter, FighterId, Gauge, STAT_COUNT};
use crate::log::EventKind;
use crate::melee::melee_arena;

// ponytail: reward weights in code; move them to `data/` once training tunes them.
/// Reward of the winner, and minus that of each fighter out of the fight.
const WIN_REWARD: f64 = 1.0;
/// Taken off each fighter at each decision, so stalling does not pay.
const TIME_PENALTY: f64 = 0.001;

/// Directions of the move actions, clockwise from the right; their length is ignored.
const MOVES: [Vec2; 8] = [
    Vec2::new(1.0, 0.0),
    Vec2::new(1.0, 1.0),
    Vec2::new(0.0, 1.0),
    Vec2::new(-1.0, 1.0),
    Vec2::new(-1.0, 0.0),
    Vec2::new(-1.0, -1.0),
    Vec2::new(0.0, -1.0),
    Vec2::new(1.0, -1.0),
];
/// Values of a fighter's own state in an observation, its class aside: hit points,
/// mana, endurance, position, standing, surrender share, stats, form, morale,
/// personality, injury by location, illness, refused equipment.
const OWN_VALUES: usize = 6 + 1 + STAT_COUNT + 2 + 4 + INJURY_LOCATIONS + 1 + 2;
/// Head, body, then left and right arm and leg.
const INJURY_LOCATIONS: usize = 6;
/// Values of the arena: size, speed, time left, nearest pillar's offset and radius.
const ARENA_VALUES: usize = 7;
/// Values of each spell slot: castable, nearest opponent in reach, cooldown left.
const SPELL_VALUES: usize = 3;
/// Opponents seen, then allies seen, nearest first.
const OTHERS_SEEN: usize = 4;
/// Values of each other fighter seen, its class aside: there, hit points, offset,
/// distance, standing.
const OTHER_VALUES: usize = 6;
// ponytail: fixed scales, larger arenas reading as 1; move to `data/` if melees outgrow it.
/// Meters an arena's side is measured against.
const ARENA_SCALE: f64 = 50.0;

/// A duel ([`Environment::reset`]) or a melee ([`Environment::reset_melee`]) whose
/// fighters are played from outside.
#[derive(Debug, Clone)]
pub struct Environment {
    data: DesignData,
    arena: Arena,
    senses: Senses,
}

/// What a fighter sees of a fight and can do in it, shaped by the design data: the
/// spell slots of actions and observations, and the classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Senses {
    /// Spells of the class with the most.
    max_spells: usize,
    class_count: usize,
}

/// What a [`Environment::step`] gives back, one entry per fighter.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub observations: Vec<Vec<f64>>,
    pub rewards: Vec<f64>,
    /// The fight is over, or reached its time limit.
    pub done: bool,
}

/// Plays the action chosen from outside.
struct Chosen(Action);

impl CombatAi for Chosen {
    fn decide(&mut self, _: &Arena, _: FighterId) -> Action {
        self.0
    }
}

impl Environment {
    /// Ready to play the duel of seed 0.
    #[must_use]
    pub fn new(data: DesignData) -> Self {
        let senses = Senses::of(&data);
        let arena = random_duel(&data, 0);
        Self {
            data,
            arena,
            senses,
        }
    }

    /// Starts the duel of `seed`: two generated fighters on a drawn arena kind's floor,
    /// as a career duel; gives each fighter's observation.
    pub fn reset(&mut self, seed: u64) -> Vec<Vec<f64>> {
        let (kind, fighters) = self.draw(seed, 2);
        let arena = fighters[0].duel_arena(&fighters[1], (kind, Refused::NONE), seed, &self.data);
        self.arena = arena;
        self.observations()
    }

    /// Starts the melee of `seed`: `count` generated fighters on a drawn arena kind's
    /// floor, as the season-opening melee; gives each fighter's observation.
    pub fn reset_melee(&mut self, seed: u64, count: u32) -> Vec<Vec<f64>> {
        let (kind, fighters) = self.draw(seed, count);
        let fighters = (0..count)
            .zip(fighters)
            .map(|(id, fighter)| {
                let at = Vec2::new(0.0, 0.0);
                fighter.combat_fighter(FighterId(id), Refused::NONE, &self.data, at)
            })
            .collect();
        self.arena = melee_arena(fighters, kind, seed, &self.data.combat);
        self.observations()
    }

    /// The arena kind and the `count` generated fighters of the fight of `seed`.
    fn draw(&self, seed: u64, count: u32) -> (ArenaKind, Vec<CareerFighter>) {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let kind = ArenaKind::ALL[rng.random_range(0..ArenaKind::ALL.len())];
        let fighters = (0..count)
            .map(|_| CareerFighter::random(&mut rng, &self.data))
            .collect();
        (kind, fighters)
    }

    /// Plays `actions[i]` (below [`Environment::action_count`]) for fighter `i` until the
    /// next decision: 0 waits, 1 to 8 move (see `MOVES`), then each spell slot casts on
    /// the nearest opponent, waiting without one; `None` lets the utility AI decide. A
    /// fighter down or out ignores its action.
    ///
    /// # Panics
    /// If there is not one action per fighter.
    pub fn step(&mut self, actions: &[Option<usize>]) -> Step {
        let mut ais: Vec<Box<dyn CombatAi>> = (0..self.arena.fighters.len())
            .map(|i| match actions[i] {
                Some(a) => {
                    Box::new(Chosen(self.senses.action(&self.arena, i, a))) as Box<dyn CombatAi>
                }
                None => Box::new(UtilityAi),
            })
            .collect();
        let was_fighting: Vec<bool> = self
            .arena
            .fighters
            .iter()
            .map(Fighter::is_fighting)
            .collect();
        let log_start = self.arena.log.len();
        loop {
            tick(&mut self.arena, &mut ais);
            if self.is_done() || self.arena.tick.is_multiple_of(TICKS_PER_DECISION) {
                break;
            }
        }
        let over = self.arena.is_over();
        let rewards = (0..self.arena.fighters.len())
            .map(|i| {
                let me = &self.arena.fighters[i];
                let result = match (was_fighting[i], me.is_fighting()) {
                    (true, false) => -WIN_REWARD,
                    (_, true) if over => WIN_REWARD,
                    _ => 0.0,
                };
                self.damage_dealt(me.id, log_start) + result - TIME_PENALTY
            })
            .collect();
        Step {
            observations: self.observations(),
            rewards,
            done: self.is_done(),
        }
    }

    /// Number of actions of each fighter: wait, the moves, then a cast per spell slot.
    #[must_use]
    pub fn action_count(&self) -> usize {
        self.senses.action_count()
    }

    /// Number of values of each fighter's observation.
    #[must_use]
    pub fn observation_size(&self) -> usize {
        self.senses.observation_size()
    }

    #[must_use]
    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    fn is_done(&self) -> bool {
        self.arena.is_over() || self.arena.tick >= self.arena.max_ticks
    }

    /// Hit points `caster` took off since log entry `log_start`, each hit as a share of
    /// its target's maximum.
    fn damage_dealt(&self, caster: FighterId, log_start: usize) -> f64 {
        self.arena.log[log_start..]
            .iter()
            .filter_map(|event| match event.kind {
                EventKind::Cast {
                    caster: c,
                    target,
                    damage,
                    ..
                } if c == caster => {
                    let max = self.arena.fighter(target)?.stats.hp;
                    Some(ratio(damage, max))
                }
                _ => None,
            })
            .sum()
    }

    /// Each fighter's view of the fight (see [`Senses::observation`]).
    fn observations(&self) -> Vec<Vec<f64>> {
        (0..self.arena.fighters.len())
            .map(|i| self.senses.observation(&self.arena, i))
            .collect()
    }
}

impl Senses {
    /// The senses of a fighter of any class of `data`.
    #[must_use]
    pub fn of(data: &DesignData) -> Self {
        Self {
            max_spells: data
                .classes
                .iter()
                .map(|c| c.spells_in(0).len())
                .max()
                .unwrap_or(0),
            class_count: data.classes.len(),
        }
    }

    /// Number of actions of each fighter: wait, the moves, then a cast per spell slot.
    #[must_use]
    pub fn action_count(self) -> usize {
        1 + MOVES.len() + self.max_spells
    }

    /// Number of values of each fighter's observation.
    #[must_use]
    pub fn observation_size(self) -> usize {
        self.spells_start()
            + SPELL_VALUES * self.max_spells
            + 2 * OTHERS_SEEN * (OTHER_VALUES + self.class_count)
    }

    /// Where the spell slots start in an observation.
    fn spells_start(self) -> usize {
        OWN_VALUES + self.class_count + ARENA_VALUES
    }

    /// The other fighters still in the fight, nearest first: opponents, then allies.
    fn others(arena: &Arena, i: usize) -> (Vec<&Fighter>, Vec<&Fighter>) {
        let me = &arena.fighters[i];
        let mut others: Vec<&Fighter> = arena
            .fighters
            .iter()
            .filter(|f| f.id != me.id && f.is_fighting())
            .collect();
        others.sort_by(|a, b| edge_distance(me, a).total_cmp(&edge_distance(me, b)));
        others.into_iter().partition(|f| !arena.allied(me.id, f.id))
    }

    /// The action of `index` (see [`Environment::step`]) for fighter `i` of `arena`.
    #[must_use]
    pub fn action(self, arena: &Arena, i: usize, index: usize) -> Action {
        match index {
            0 => Action::Wait,
            m if m <= MOVES.len() => Action::Move(MOVES[m - 1]),
            s => Self::others(arena, i)
                .0
                .first()
                .map_or(Action::Wait, |target| Action::Cast {
                    spell: s - 1 - MOVES.len(),
                    target: target.id,
                }),
        }
    }

    /// Fighter `i`'s view of the fight, every value between -1 and 1 (see
    /// [`Senses::observation_size`]): its own state and class, the arena, for each
    /// spell slot whether it can cast, whether the nearest opponent is in reach and the
    /// cooldown left, then its [`OTHERS_SEEN`] nearest opponents and allies, an empty
    /// slot all 0.
    #[must_use]
    pub fn observation(self, arena: &Arena, i: usize) -> Vec<f64> {
        let me = &arena.fighters[i];
        let mut values = self.own_values(arena, me);
        let (opponents, allies) = Self::others(arena, i);
        let distance = opponents.first().map(|o| edge_distance(me, o));
        for slot in 0..self.max_spells {
            let spell = me.spells.get(slot);
            values.extend([
                flag(me.can_cast(slot)),
                flag(spell.zip(distance).is_some_and(|(s, d)| d <= s.reach)),
                spell.map_or(0.0, |s| ratio(me.cooldowns[slot], s.cooldown).min(1.0)),
            ]);
        }
        for seen in [opponents, allies] {
            for slot in 0..OTHERS_SEEN {
                match seen.get(slot) {
                    Some(other) => values.extend(self.other_values(arena, me, other)),
                    None => values.extend(vec![0.0; OTHER_VALUES + self.class_count]),
                }
            }
        }
        values
    }

    /// The fighter's own state and class, then the arena.
    fn own_values(self, arena: &Arena, me: &Fighter) -> Vec<f64> {
        let condition = &me.condition;
        let traits = condition.personality;
        let mut values = vec![
            ratio(me.hp, me.stats.hp),
            gauge(me.mana),
            gauge(me.endurance),
            me.position.x / arena.width,
            me.position.y / arena.height,
            flag(me.is_standing()),
            ratio(me.surrender_hp, me.stats.hp),
        ];
        // A standard duel's stats read as 0.5.
        values.extend(
            me.stats
                .to_array()
                .into_iter()
                .zip(STATS.to_array())
                .map(|(stat, standard)| ratio(stat, 2 * standard).min(1.0)),
        );
        values.extend([
            ratio(condition.form.value(), Form::MAX),
            ratio(condition.morale.value(), Morale::MAX),
        ]);
        values.extend(
            [
                traits.aggression,
                traits.caution,
                traits.showmanship,
                traits.determination,
            ]
            .map(|t| ratio(t.value(), Personality::MAX)),
        );
        let mut injuries = [0.0; INJURY_LOCATIONS];
        for injury in &condition.injuries {
            let slot = &mut injuries[location_index(injury.location)];
            *slot = f64::max(*slot, severity_value(injury.severity));
        }
        values.extend(injuries);
        values.push(
            condition
                .illness
                .map_or(0.0, |illness| match illness.level {
                    IllnessLevel::Mild => 1.0 / 3.0,
                    IllnessLevel::Moderate => 2.0 / 3.0,
                    IllnessLevel::Severe => 1.0,
                }),
        );
        // ponytail: the refused weapon's slot, always 0, kept for the trained models
        // until the observation is redone (docs/Classes.md, step 11).
        values.extend([flag(me.refused.armour), 0.0]);
        values.extend(self.class_values(me));
        values.extend([
            (arena.width / ARENA_SCALE).min(1.0),
            (arena.height / ARENA_SCALE).min(1.0),
            ratio(arena.speed_percent, 100).min(1.0),
            1.0 - ratio(arena.tick, arena.max_ticks).min(1.0),
        ]);
        let offset = |p: &Pillar| Vec2::new(p.x - me.position.x, p.y - me.position.y);
        let pillar = arena
            .pillars
            .iter()
            .min_by(|a, b| offset(a).length().total_cmp(&offset(b).length()));
        values.extend(pillar.map_or([0.0; 3], |p| {
            [
                offset(p).x / arena.width,
                offset(p).y / arena.height,
                (p.radius / arena.width).min(1.0),
            ]
        }));
        values
    }

    /// What `me` sees of `other`: there, hit points, offset, distance, standing, class.
    fn other_values(self, arena: &Arena, me: &Fighter, other: &Fighter) -> Vec<f64> {
        let (width, height) = (arena.width, arena.height);
        let diagonal = Vec2::new(width, height).length();
        let mut values = vec![
            1.0,
            ratio(other.hp, other.stats.hp),
            (other.position.x - me.position.x) / width,
            (other.position.y - me.position.y) / height,
            (edge_distance(me, other) / diagonal).min(1.0),
            flag(other.is_standing()),
        ];
        values.extend(self.class_values(other));
        values
    }

    /// One flag per class, set for the fighter's.
    fn class_values(self, fighter: &Fighter) -> Vec<f64> {
        (0..self.class_count)
            .map(|c| flag(usize::try_from(fighter.class.0) == Ok(c)))
            .collect()
    }
}

fn location_index(location: Location) -> usize {
    match location {
        Location::Head => 0,
        Location::Body => 1,
        Location::Arm(Side::Left) => 2,
        Location::Arm(Side::Right) => 3,
        Location::Leg(Side::Left) => 4,
        Location::Leg(Side::Right) => 5,
    }
}

/// From a light wound up to 1 for a lost limb; 0 is unhurt.
fn severity_value(severity: Severity) -> f64 {
    match severity {
        Severity::Light => 0.2,
        Severity::Medium => 0.4,
        Severity::Severe => 0.6,
        Severity::Permanent => 0.8,
        Severity::Lost => 1.0,
    }
}

fn ratio(value: u32, max: u32) -> f64 {
    if max == 0 {
        0.0
    } else {
        f64::from(value) / f64::from(max)
    }
}

fn gauge(gauge: Gauge) -> f64 {
    ratio(gauge.current, gauge.max)
}

fn flag(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each fighter casts a spell it can in reach, else moves towards the other.
    fn play(env: &mut Environment, seed: u64) -> (Vec<Step>, Arena) {
        let mut observations = env.reset(seed);
        let mut steps = Vec::new();
        loop {
            let actions: Vec<Option<usize>> = observations
                .iter()
                .map(|o| {
                    let start = env.senses.spells_start();
                    let spells = &o[start..start + SPELL_VALUES * env.senses.max_spells];
                    let cast = spells
                        .chunks(SPELL_VALUES)
                        .position(|s| s[..2] == [1.0, 1.0]);
                    // The nearest opponent's offset, after "there" and its hit points.
                    let offset = &o[start + spells.len() + 2..];
                    let towards = |k: &usize| {
                        let m = MOVES[*k];
                        (m.x * offset[0] + m.y * offset[1]) / m.length()
                    };
                    let closest = (0..MOVES.len()).max_by(|a, b| towards(a).total_cmp(&towards(b)));
                    Some(cast.map_or_else(|| 1 + closest.unwrap(), |s| 1 + MOVES.len() + s))
                })
                .collect();
            let step = env.step(&actions);
            observations.clone_from(&step.observations);
            let done = step.done;
            steps.push(step);
            if done {
                return (steps, env.arena().clone());
            }
        }
    }

    #[test]
    fn a_fight_ends_with_a_winner_rewarded() {
        let mut env = Environment::new(DesignData::load().unwrap());
        let (steps, arena) = play(&mut env, 3);
        let last = steps.last().unwrap();
        assert!(arena.is_over(), "the fighters closed in and fought");
        let winner = arena
            .fighters
            .iter()
            .position(Fighter::is_fighting)
            .unwrap();
        assert!(last.rewards[winner] > 0.5);
        assert!(last.rewards[1 - winner] < -0.5);
        for step in &steps {
            assert_eq!(step.observations.len(), 2);
            for o in &step.observations {
                assert_eq!(o.len(), env.observation_size());
                assert!(o.iter().all(|v| (-1.0..=1.0).contains(v)), "{o:?}");
            }
        }
    }

    #[test]
    fn waiting_out_the_clock_costs_both_fighters() {
        let mut env = Environment::new(DesignData::load().unwrap());
        env.reset(3);
        loop {
            let step = env.step(&[Some(0), Some(0)]);
            assert_eq!(step.rewards, [-TIME_PENALTY, -TIME_PENALTY]);
            if step.done {
                break;
            }
        }
        assert!(!env.arena().is_over(), "a draw at the time limit");
    }

    #[test]
    fn a_melee_against_the_utility_ai_ends_and_punishes_each_fighter_out_once() {
        let mut env = Environment::new(DesignData::load().unwrap());
        let observations = env.reset_melee(4, 6);
        assert_eq!(observations.len(), 6);
        let mut actions = vec![None; 6];
        actions[0] = Some(0);
        let mut punished = [0; 6];
        loop {
            let step = env.step(&actions);
            for (i, reward) in step.rewards.iter().enumerate() {
                punished[i] += usize::from(*reward < -0.5);
            }
            if step.done {
                break;
            }
        }
        let arena = env.arena();
        assert!(arena.is_over(), "the utility AIs fought it out");
        for (fighter, punished) in arena.fighters.iter().zip(punished) {
            assert_eq!(punished, usize::from(!fighter.is_fighting()));
        }
    }

    #[test]
    fn same_seed_same_fight() {
        let mut env = Environment::new(DesignData::load().unwrap());
        let first = play(&mut env, 5);
        assert_eq!(first, play(&mut env, 5));
    }

    #[test]
    fn a_fighter_sees_its_wounds_its_equipment_and_its_ally() {
        let mut env = Environment::new(DesignData::load().unwrap());
        env.reset(1);
        let mut ally = env.arena.fighters[1].clone();
        ally.id = FighterId(2);
        env.arena.fighters.push(ally);
        env.arena.pacts.push((FighterId(0), FighterId(2)));
        let me = &mut env.arena.fighters[0];
        me.refused.armour = true;
        me.condition.injuries.push(crate::fighter::injury::Injury {
            location: Location::Arm(Side::Left),
            severity: Severity::Lost,
            weeks_left: 0,
        });
        let o = &env.observations()[0];
        assert_eq!(o.len(), env.observation_size());
        let injuries = 6 + 1 + STAT_COUNT + 2 + 4;
        assert_eq!(
            o[injuries..injuries + INJURY_LOCATIONS],
            [0.0, 0.0, 1.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(o[OWN_VALUES - 2..OWN_VALUES], [1.0, 0.0]);
        let seen = OTHER_VALUES + env.senses.class_count;
        let opponents = env.senses.spells_start() + SPELL_VALUES * env.senses.max_spells;
        let allies = opponents + OTHERS_SEEN * seen;
        let there = |slot: usize| o[slot] > 0.5;
        assert!(there(opponents) && !there(opponents + seen), "one opponent");
        assert!(there(allies) && !there(allies + seen), "one ally");
    }
}
