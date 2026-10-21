//! The learned combat AI (phase 7): each class's networks, written by `python/export.py`
//! from its ONNX models as `assets/ai/<class>-<level>.json`, played here with `libm` so
//! that a fight replays the same on every processor. A fighter whose model is missing
//! fights with the utility AI.

use std::sync::Arc;

use serde::Deserialize;

use crate::ai::{Action, CombatAi};
use crate::combat::Arena;
use crate::design_data::DesignData;
use crate::fighter::FighterId;
use crate::learning::Senses;

/// Where the networks are read, next to the ONNX models.
pub const MODELS_DIR: &str = "assets/ai";

/// A learned policy: dense layers, each one maybe followed by tanh, the last one
/// giving one logit per action.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    /// One row of input weights per output.
    pub weights: Vec<Vec<f64>>,
    pub bias: Vec<f64>,
    pub tanh: bool,
}

impl Network {
    /// The logits of `input`, summed in order so that every processor gives the same.
    #[must_use]
    pub fn logits(&self, input: &[f64]) -> Vec<f64> {
        self.layers.iter().fold(input.to_vec(), |x, layer| {
            layer
                .weights
                .iter()
                .zip(&layer.bias)
                .map(|(row, &bias)| {
                    let y = row.iter().zip(&x).fold(bias, |sum, (w, v)| sum + w * v);
                    if layer.tanh { libm::tanh(y) } else { y }
                })
                .collect()
        })
    }

    /// Takes `inputs` values and gives `outputs`, every layer fitting the next.
    fn fits(&self, inputs: usize, outputs: usize) -> bool {
        let mut size = inputs;
        for layer in &self.layers {
            if layer.bias.len() != layer.weights.len()
                || layer.weights.iter().any(|row| row.len() != size)
            {
                return false;
            }
            size = layer.bias.len();
        }
        !self.layers.is_empty() && size == outputs
    }
}

/// A class's learned AI at one level: its network and what it sees.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub network: Network,
    pub senses: Senses,
}

impl Model {
    /// `None` unless `network` takes the observation of `senses` and gives one logit
    /// per action.
    #[must_use]
    pub fn new(network: Network, senses: Senses) -> Option<Self> {
        network
            .fits(senses.observation_size(), senses.action_count())
            .then_some(Self { network, senses })
    }
}

/// A class's models: a lightly trained one for recruits, the most advanced for veterans.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClassModels {
    pub recruit: Option<Arc<Model>>,
    pub veteran: Option<Arc<Model>>,
}

impl ClassModels {
    /// Each class's models of `dir` (under the source tree, see [`DesignData::path`]), in
    /// the order of `data.classes`; a file missing, unreadable or not fitting the
    /// observation leaves its fighters to the utility AI.
    #[must_use]
    pub fn load(dir: &str, data: &DesignData) -> Vec<Self> {
        let senses = Senses::of(data);
        let read = |key: &str, level: &str| {
            let text =
                std::fs::read_to_string(DesignData::path(&format!("{dir}/{key}-{level}.json")))
                    .ok()?;
            Model::new(serde_json::from_str(&text).ok()?, senses).map(Arc::new)
        };
        data.classes
            .iter()
            .map(|class| Self {
                recruit: read(&class.key, "recruit"),
                veteran: read(&class.key, "veteran"),
            })
            .collect()
    }
}

/// Plays its model's likeliest action, the first one on a tie.
pub struct LearnedAi(pub Arc<Model>);

impl CombatAi for LearnedAi {
    fn decide(&mut self, arena: &Arena, me: FighterId) -> Action {
        let i = usize::try_from(me.0).expect("a fighter index");
        let Model { network, senses } = &*self.0;
        let logits = network.logits(&senses.observation(arena, i));
        let best =
            (0..logits.len()).fold(0, |best, a| if logits[a] > logits[best] { a } else { best });
        senses.action(arena, i, best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use crate::ai::UtilityAi;
    use crate::combat::{Vec2, tick};
    use crate::duel::{ais, random_duel};
    use crate::fighter::career::CareerFighter;
    use crate::fighter::equipment::Refused;

    /// Every weight 0, so the bias alone picks `action`.
    fn always(action: usize, data: &DesignData) -> Arc<Model> {
        let senses = Senses::of(data);
        let layer = |inputs, outputs, bias: Vec<f64>| Layer {
            weights: vec![vec![0.0; inputs]; outputs],
            bias,
            tanh: false,
        };
        let mut bias = vec![0.0; senses.action_count()];
        bias[action] = 1.0;
        let network = Network {
            layers: vec![
                layer(senses.observation_size(), 3, vec![0.0; 3]),
                layer(3, senses.action_count(), bias),
            ],
        };
        Arc::new(Model::new(network, senses).unwrap())
    }

    #[test]
    fn a_fighter_with_a_model_plays_it_and_one_without_the_utility_ai() {
        let data = DesignData::load().unwrap();
        let mut arena = random_duel(&data, 1);
        // Action 1 moves along the first direction.
        let model = always(1, &data);
        arena.fighters[0].model = Some(Arc::clone(&model));
        let learned = Senses::of(&data).action(&arena, 0, 1);
        assert!(matches!(learned, Action::Move(_)));
        let mut expected = arena.clone();
        let mut ais = ais(&arena);
        let mut chosen: Vec<Box<dyn CombatAi>> =
            vec![Box::new(LearnedAi(model)), Box::new(UtilityAi)];
        for _ in 0..60 {
            tick(&mut arena, &mut ais);
            tick(&mut expected, &mut chosen);
        }
        assert_eq!(arena, expected);
        assert_eq!(arena.fighters[0].action, learned);
    }

    #[test]
    fn a_recruit_fights_with_the_recruits_model_and_a_veteran_with_the_veterans() {
        let mut data = DesignData::load().unwrap();
        let (recruit, veteran) = (always(1, &data), always(2, &data));
        data.models = vec![
            ClassModels {
                recruit: Some(Arc::clone(&recruit)),
                veteran: Some(Arc::clone(&veteran)),
            };
            data.classes.len()
        ];
        let mut fighter = CareerFighter::random(&mut ChaCha8Rng::seed_from_u64(1), &data);
        let model = |f: &CareerFighter| {
            f.combat_fighter(FighterId(0), Refused::NONE, &data, Vec2::new(0.0, 0.0))
                .model
        };
        assert_eq!(model(&fighter), Some(recruit));
        fighter.record.wins = data.fighter.career.veteran_fights;
        assert_eq!(model(&fighter), Some(veteran));
    }

    #[test]
    fn a_missing_or_misfit_network_is_no_model() {
        let data = DesignData::load().unwrap();
        let models = ClassModels::load("assets/no-such-dir", &data);
        assert_eq!(models, vec![ClassModels::default(); data.classes.len()]);
        let wrong = Network {
            layers: vec![Layer {
                weights: vec![vec![0.0; 3]],
                bias: vec![0.0],
                tanh: true,
            }],
        };
        assert_eq!(Model::new(wrong, Senses::of(&data)), None);
    }
}
