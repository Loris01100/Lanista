//! Traits: 0 to 2 per fighter, drawn for life beside its temperament, each tied to a
//! decision of its club: the workaholic and the nonchalant to training and rest, the
//! vain to the colosseums it fights in, the spiteful to the challenges, the
//! superstitious to the arenas and loadouts, the pack leader to the club's roster.

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::fighter::pick;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Trait {
    /// Gains more from training, sulks when made to rest.
    Workaholic,
    /// Likes resting, and training wears it out.
    Nonchalant,
    /// Glad in a great colosseum or a title fight, sulking in a small one.
    Vain,
    /// Fights harder against whoever beat it, until it wins one back.
    Spiteful,
    /// Fights better on the floor of its first win, sulks when its loadout changes.
    Superstitious,
    /// Lifts the morale of the club's other fighters every week.
    PackLeader,
}

impl Trait {
    pub const ALL: [Self; 6] = [
        Self::Workaholic,
        Self::Nonchalant,
        Self::Vain,
        Self::Spiteful,
        Self::Superstitious,
        Self::PackLeader,
    ];

    /// Whether a fighter with `self` may not also have `other`.
    #[must_use]
    pub fn excludes(self, other: Self) -> bool {
        self == other
            || matches!(
                (self, other),
                (Self::Workaholic, Self::Nonchalant) | (Self::Nonchalant, Self::Workaholic)
            )
    }
}

/// A new fighter's traits: their count drawn by `rules.count_weights`, each one drawn
/// evenly among those its earlier ones don't exclude.
pub fn random_traits<R: Rng + ?Sized>(rng: &mut R, rules: &TraitRules) -> Vec<Trait> {
    let counts: Vec<(u32, usize)> = (0..)
        .zip(rules.count_weights)
        .map(|(n, w)| (w, n))
        .collect();
    let count = pick(rng, &counts);
    let mut traits = Vec::new();
    for _ in 0..count {
        let open: Vec<Trait> = Trait::ALL
            .into_iter()
            .filter(|t| !traits.iter().any(|&had: &Trait| had.excludes(*t)))
            .collect();
        if open.is_empty() {
            break;
        }
        traits.push(open[rng.random_range(0..open.len())]);
    }
    traits
}

/// Read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraitRules {
    /// Chances of a new fighter having 0, 1 and 2 traits, against each other.
    pub count_weights: [u32; 3],
    pub workaholic: WorkaholicRules,
    pub nonchalant: NonchalantRules,
    pub vain: VainRules,
    pub spiteful: SpitefulRules,
    pub superstitious: SuperstitiousRules,
    pub pack_leader: PackLeaderRules,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkaholicRules {
    /// Added to a training session's coaching percentage.
    pub training_percent: u32,
    /// Morale lost in a week of rest.
    pub rest_morale: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NonchalantRules {
    /// Morale gained in a week of rest.
    pub rest_morale: u32,
    /// Morale lost in a training session.
    pub training_morale: u32,
    /// Of the form a training session costs.
    pub training_form_percent: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VainRules {
    /// A duel in a colosseum of at least this prestige, or for a title, is a great stage.
    pub great_prestige: u32,
    /// A duel in a colosseum of at most this prestige, but for a title, is a small stage.
    pub small_prestige: u32,
    /// Morale gained after a duel on a great stage, whatever its result.
    pub great_morale: u32,
    /// Morale lost after a duel on a small stage.
    pub small_morale: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpitefulRules {
    /// Of every stat in a duel against a fighter it bears a grudge against.
    pub grudge_percent: u32,
    /// Duels lost to the same fighter that make a fighter spiteful.
    pub earned_defeats: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperstitiousRules {
    /// Of every stat in a duel on the floor of its first win.
    pub lucky_percent: u32,
    /// Morale lost in a week its loadout changes.
    pub loadout_morale: u32,
    /// Duels won in a row on the same floor that make a fighter superstitious.
    pub earned_wins: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackLeaderRules {
    /// Morale gained every week by the other active fighters of its club.
    pub weekly_morale: u32,
    /// Wins in its career that make a fighter a pack leader.
    pub earned_wins: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design_data::DesignData;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    #[test]
    fn traits_number_zero_to_two_and_never_clash() {
        let rules = DesignData::load().unwrap().fighter.traits;
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let drawn: Vec<Vec<Trait>> = (0..500).map(|_| random_traits(&mut rng, &rules)).collect();
        for traits in &drawn {
            assert!(traits.len() <= 2);
            if let [a, b] = traits[..] {
                assert!(!a.excludes(b), "{traits:?}");
            }
        }
        for n in 0..=2 {
            assert!(drawn.iter().any(|t| t.len() == n), "{n}");
        }
        for t in Trait::ALL {
            assert!(drawn.iter().any(|d| d.contains(&t)), "{t:?}");
        }
    }
}
