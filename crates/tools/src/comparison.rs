//! What `compare` does: plays the same fights between varied fighters as real duels and
//! through the statistical model, then sets their rates side by side.

use lanista_core::design_data::DesignData;
use lanista_core::fighter::career::{CareerFighter, Status};
use lanista_core::fighter::form::FormEvent;
use lanista_core::fighter::injury::Location;
use lanista_core::staff::Medicine;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::calibration::{share, varied_fighter};

/// How one fight ended for its two fighters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// Index of the winner, none at the time limit.
    pub winner: Option<usize>,
    pub died: [bool; 2],
    /// Whether each came out of the fight with a new injury.
    pub injured: [bool; 2],
    /// Whether that injury is to the head, as the wound a stun leaves.
    pub head_injured: [bool; 2],
    /// Whether that injury is permanent, as a limb a cutting hit severed.
    pub maimed: [bool; 2],
}

/// The same fight played both ways.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    /// Class index of each fighter.
    pub classes: [usize; 2],
    /// The fighter whose fighting stats add up to more, if one does.
    pub stronger: Option<usize>,
    pub real: Outcome,
    pub model: Outcome,
}

/// The fight of `seed` between two varied fighters, as a real duel and as the model
/// resolves it, each with what the fight leaves them (injuries included).
#[must_use]
pub fn play(seed: u64, data: &DesignData) -> Pair {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let fighters = [
        varied_fighter(&mut rng, data),
        varied_fighter(&mut rng, data),
    ];
    let [a, b] = &fighters;
    let rules = &data.fighter;
    let stronger = if a.stronger_than(b, rules) {
        Some(0)
    } else if b.stronger_than(a, rules) {
        Some(1)
    } else {
        None
    };
    let class = |f: &CareerFighter| usize::try_from(f.class.0).unwrap_or(usize::MAX);
    let mut model_rng = rng.clone();
    Pair {
        classes: [class(a), class(b)],
        stronger,
        real: outcome(fighters.clone(), false, &mut rng, data),
        model: outcome(fighters, true, &mut model_rng, data),
    }
}

/// How the fight between `fighters` ends, `modelled` or as a real duel.
fn outcome(
    fighters: [CareerFighter; 2],
    modelled: bool,
    rng: &mut ChaCha8Rng,
    data: &DesignData,
) -> Outcome {
    let before = fighters.clone();
    let [mut a, mut b] = fighters;
    if modelled {
        a.modelled_fight(
            &mut b,
            FormEvent::Fight,
            [Medicine::default(); 2],
            rng,
            data,
        );
    } else {
        a.dueled_fight(&mut b, FormEvent::Fight, rng, data);
    }
    let after = [a, b];
    let gained = |i: usize| after[i].record.wins > before[i].record.wins;
    let injured = |i: usize| after[i].injuries.len() > before[i].injuries.len();
    let head_injured = |i: usize| {
        injured(i)
            && after[i]
                .injuries
                .last()
                .is_some_and(|injury| injury.location == Location::Head)
    };
    let maimed = |i: usize| {
        injured(i)
            && after[i]
                .injuries
                .last()
                .is_some_and(lanista_core::fighter::injury::Injury::is_permanent)
    };
    Outcome {
        winner: (0..2).find(|&i| gained(i)),
        died: [0, 1].map(|i| after[i].status == Status::Dead),
        injured: [0, 1].map(injured),
        head_injured: [0, 1].map(head_injured),
        maimed: [0, 1].map(maimed),
    }
}

/// One rate, in percent, measured on the real duels and on the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Rate {
    pub name: String,
    pub real: f64,
    pub model: f64,
}

impl Rate {
    /// How far the model is from the real duels, in percentage points.
    #[must_use]
    pub fn gap(&self) -> f64 {
        (self.model - self.real).abs()
    }
}

/// The rates `compare` shows over `pairs`: fights at the time limit, wins of the first
/// fighter, of the stronger and of each class (named by `classes`), deaths, injuries,
/// head injuries and after-effects.
#[must_use]
pub fn rates(pairs: &[Pair], classes: &[String]) -> Vec<Rate> {
    let rate = |name: &str, count: &dyn Fn(&Pair, &Outcome) -> (u64, u64)| {
        let percent = |side: fn(&Pair) -> &Outcome| {
            let (part, whole) = pairs.iter().fold((0, 0), |(part, whole), pair| {
                let (p, w) = count(pair, side(pair));
                (part + p, whole + w)
            });
            share(part, whole, 0.0) * 100.0
        };
        Rate {
            name: name.to_owned(),
            real: percent(|pair| &pair.real),
            model: percent(|pair| &pair.model),
        }
    };
    let flags = |flags: [bool; 2]| (flags.iter().map(|&f| u64::from(f)).sum(), 2);
    let mut rates = vec![
        rate("unfinished fights", &|_, o| {
            (u64::from(o.winner.is_none()), 1)
        }),
        rate("first fighter wins", &|_, o| {
            (u64::from(o.winner == Some(0)), 1)
        }),
        rate("stronger wins, fights with a winner", &|p, o| {
            p.stronger
                .zip(o.winner)
                .map_or((0, 0), |(s, w)| (u64::from(s == w), 1))
        }),
    ];
    for (class, name) in classes.iter().enumerate() {
        rates.push(rate(&format!("{name} wins"), &|p, o| {
            (0..2)
                .filter(|&i| p.classes[i] == class)
                .fold((0, 0), |(won, fought), i| {
                    (won + u64::from(o.winner == Some(i)), fought + 1)
                })
        }));
    }
    rates.push(rate("fighters killed", &|_, o| flags(o.died)));
    rates.push(rate("fighters injured", &|_, o| flags(o.injured)));
    rates.push(rate("fighters injured at the head", &|_, o| {
        flags(o.head_injured)
    }));
    rates.push(rate("fighters with a new after-effect", &|_, o| {
        flags(o.maimed)
    }));
    rates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fight_is_played_the_same_way_twice() {
        let data = DesignData::load().unwrap();
        let pair = play(5, &data);
        assert_eq!(play(5, &data), pair);
        for outcome in [pair.real, pair.model] {
            if let Some(winner) = outcome.winner {
                assert!(!outcome.died[winner]);
            }
            // The dead take no injury.
            for i in 0..2 {
                assert!(!(outcome.died[i] && outcome.injured[i]));
            }
        }
    }

    #[test]
    fn the_model_stays_close_to_the_real_duels() {
        let data = DesignData::load().unwrap();
        let pairs: Vec<Pair> = (1_000_000..1_001_000)
            .map(|seed| play(seed, &data))
            .collect();
        let names: Vec<String> = data.classes.iter().map(|c| c.name.clone()).collect();
        let rates = rates(&pairs, &names);
        assert_eq!(rates.len(), 7 + names.len());
        // A thousand fights leave a few points of chance on each side.
        for rate in &rates {
            assert!(rate.gap() < 4.0, "{rate:?}");
        }
    }

    #[test]
    fn rates_count_what_happened() {
        let fight = |winner, died, injured| Outcome {
            winner,
            died,
            injured,
            head_injured: [false; 2],
            maimed: [false; 2],
        };
        let pair = Pair {
            classes: [0, 1],
            stronger: Some(1),
            real: fight(Some(1), [true, false], [false, true]),
            model: fight(None, [false; 2], [false; 2]),
        };
        let rates = rates(&[pair], &["A".into(), "B".into()]);
        let get = |name: &str| rates.iter().find(|r| r.name == name).unwrap();
        assert_eq!(
            (
                get("unfinished fights").real,
                get("unfinished fights").model
            ),
            (0.0, 100.0)
        );
        assert_eq!(get("stronger wins, fights with a winner").real, 100.0);
        assert_eq!(get("B wins").real, 100.0);
        assert_eq!(get("A wins").real, 0.0);
        assert_eq!(get("fighters killed").real, 50.0);
        assert_eq!(get("fighters injured").gap(), 50.0);
    }
}
