//! Traits: none at first, each earned in a fighter's career for life beside its
//! temperament, and tied to a decision of its club: the workaholic and the nonchalant to training and rest, the
//! vain to the colosseums it fights in, the spiteful to the challenges, the
//! superstitious to the arenas and loadouts, the pack leader to the club's roster.

use serde::{Deserialize, Serialize};

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

/// Read from `data/fighter.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraitRules {
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
    /// A rival manager rests it only below this percent of its profile's `rest_form`.
    pub rest_form_percent: u32,
    /// Weeks of training in a row that make a fighter a workaholic.
    pub earned_trainings: u32,
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
    /// Weeks of rest in a row that make a fighter nonchalant.
    pub earned_rests: u32,
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
    /// Fights on a great stage in its career that make a fighter vain.
    pub earned_stages: u32,
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
