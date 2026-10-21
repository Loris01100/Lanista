//! The learning AI's environment (phase 7): a standard duel played one decision at a
//! time by actions chosen from outside, each fighter seeing an observation and getting
//! a reward. `lanista_py` exposes it to Python for training; the learned AI will read
//! the same observations.

use crate::ai::{Action, CombatAi};
use crate::combat::{Arena, TICKS_PER_DECISION, Vec2, edge_distance, tick};
use crate::design_data::DesignData;
use crate::duel::random_duel;
use crate::fighter::{Fighter, FighterId, Gauge};
use crate::log::EventKind;

// ponytail: reward weights in code; move them to `data/` once training tunes them.
/// Reward of the winner, and minus that of the loser.
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
/// Values of a fighter's own state, then of its opponent's, in an observation.
const OWN_VALUES: usize = 6;
const OPPONENT_VALUES: usize = 5;
/// Values of each spell slot in an observation: castable, in reach.
const SPELL_VALUES: usize = 2;

/// A standard duel ([`random_duel`]) whose two fighters are played from outside.
#[derive(Debug, Clone)]
pub struct Environment {
    data: DesignData,
    arena: Arena,
    /// Spells of the class with the most: the spell slots of actions and observations.
    max_spells: usize,
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
        let max_spells = data
            .classes
            .iter()
            .map(|c| c.spells.len())
            .max()
            .unwrap_or(0);
        let arena = random_duel(&data, 0);
        Self {
            data,
            arena,
            max_spells,
        }
    }

    /// Starts the duel of `seed` and gives each fighter's observation.
    pub fn reset(&mut self, seed: u64) -> Vec<Vec<f64>> {
        self.arena = random_duel(&self.data, seed);
        self.observations()
    }

    /// Plays `actions[i]` (below [`Environment::action_count`]) for fighter `i` until the
    /// next decision: 0 waits, 1 to 8 move (see `MOVES`), then each spell slot casts on
    /// the opponent. A fighter down or out ignores its action.
    ///
    /// # Panics
    /// If there is not one action per fighter.
    pub fn step(&mut self, actions: &[usize]) -> Step {
        let mut ais: Vec<Box<dyn CombatAi>> = (0..self.arena.fighters.len())
            .map(|i| Box::new(Chosen(self.action(i, actions[i]))) as Box<dyn CombatAi>)
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
                let result = match (over, me.is_fighting()) {
                    (false, _) => 0.0,
                    (true, true) => WIN_REWARD,
                    (true, false) => -WIN_REWARD,
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
        1 + MOVES.len() + self.max_spells
    }

    /// Number of values of each fighter's observation.
    #[must_use]
    pub fn observation_size(&self) -> usize {
        OWN_VALUES + SPELL_VALUES * self.max_spells + OPPONENT_VALUES
    }

    #[must_use]
    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    fn is_done(&self) -> bool {
        self.arena.is_over() || self.arena.tick >= self.arena.max_ticks
    }

    /// The duel's other fighter.
    fn opponent(&self, i: usize) -> &Fighter {
        &self.arena.fighters[1 - i]
    }

    fn action(&self, i: usize, index: usize) -> Action {
        match index {
            0 => Action::Wait,
            m if m <= MOVES.len() => Action::Move(MOVES[m - 1]),
            s => Action::Cast {
                spell: s - 1 - MOVES.len(),
                target: self.opponent(i).id,
            },
        }
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

    /// Each fighter's view of the fight, every value between -1 and 1: its hit points,
    /// mana, endurance, position and whether it stands; for each spell slot, whether it
    /// can cast and whether the opponent is in reach; the opponent's hit points, offset,
    /// distance and whether it stands.
    fn observations(&self) -> Vec<Vec<f64>> {
        let (width, height) = (self.arena.width, self.arena.height);
        let diagonal = Vec2::new(width, height).length();
        (0..self.arena.fighters.len())
            .map(|i| {
                let me = &self.arena.fighters[i];
                let other = self.opponent(i);
                let distance = edge_distance(me, other);
                let mut values = vec![
                    ratio(me.hp, me.stats.hp),
                    gauge(me.mana),
                    gauge(me.endurance),
                    me.position.x / width,
                    me.position.y / height,
                    flag(me.is_standing()),
                ];
                for spell in 0..self.max_spells {
                    let reach = me.spells.get(spell).map(|s| s.reach);
                    values.push(flag(me.can_cast(spell)));
                    values.push(flag(reach.is_some_and(|r| distance <= r)));
                }
                values.extend([
                    ratio(other.hp, other.stats.hp),
                    (other.position.x - me.position.x) / width,
                    (other.position.y - me.position.y) / height,
                    distance / diagonal,
                    flag(other.is_standing()),
                ]);
                values
            })
            .collect()
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
            let actions: Vec<usize> = observations
                .iter()
                .map(|o| {
                    let spells = &o[OWN_VALUES..OWN_VALUES + SPELL_VALUES * env.max_spells];
                    let cast = spells.chunks(SPELL_VALUES).position(|s| s == [1.0, 1.0]);
                    let offset = &o[OWN_VALUES + spells.len() + 1..];
                    let towards = |k: &usize| {
                        let m = MOVES[*k];
                        (m.x * offset[0] + m.y * offset[1]) / m.length()
                    };
                    let closest = (0..MOVES.len()).max_by(|a, b| towards(a).total_cmp(&towards(b)));
                    cast.map_or_else(|| 1 + closest.unwrap(), |s| 1 + MOVES.len() + s)
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
    fn same_seed_same_fight() {
        let mut env = Environment::new(DesignData::load().unwrap());
        let first = play(&mut env, 5);
        assert_eq!(first, play(&mut env, 5));
    }
}
