//! The statistical model: resolves a fight between rival fighters without playing it.
//! A logistic regression gives the winner, probability tables how the fight ends
//! (time limit, knock-out, lost limb, beheading, death or surrender of the loser,
//! stuns, spectacular hits).
//! The `calibrate` tool fits it on thousands of real duels and writes
//! `data/stat_model.ron`.
//!
//! Injuries need no table: in both modes they are drawn after the fight from the
//! fighter's form, or follow a fatal blow, a lost limb or a stun
//! (`CareerFighter::after_fight`).

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::ai::fear;
use crate::combat::{Arena, CutRules, severed_limb};
use crate::duel::STATS;
use crate::fighter::fame::{FameRules, FightOutcome, FightResult};
use crate::fighter::injury::Location;
use crate::fighter::personality::Personality;
use crate::fighter::{Fighter, FighterId};

/// Features of a fighter besides its class: the seven stats, fear, surrender
/// threshold, aggression and showmanship.
pub const BASE_FEATURES: usize = 11;

/// Coefficients fitted by `calibrate`, read from `data/stat_model.ron`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatModel {
    /// Real duels the model was fitted on.
    pub fights: u64,
    pub winner: Winner,
    /// Chance a fight reaches the time limit without a winner, by the classes of the
    /// two fighters (`unfinished[a][b]`).
    pub unfinished: Vec<Vec<f64>>,
    /// Chance the loser of a finished fight was knocked out, by the winner's class.
    pub knocked_out: Vec<f64>,
    /// Chance the loser of a finished fight lost a limb, by the winner's class.
    pub lost_limb: Vec<f64>,
    /// Chance the loser of a finished fight was beheaded, by the winner's class.
    pub beheaded: Vec<f64>,
    /// Chance the loser of a finished fight neither knocked out nor cut (limb or head)
    /// falls to 0 HP rather than gives up, by band of determination: the first band
    /// starts at 0, the last ends at 100.
    pub death: Vec<f64>,
    /// Chance a fighter not knocked out was stunned in the fight, by its opponent's class.
    pub stunned: Vec<f64>,
    pub spectacular_hits: SpectacularHits,
}

/// Weights of the logistic regression on the winner: the chance that `a` beats `b`
/// is `1 / (1 + e^-z)`, `z` adding each weight times `a`'s feature minus `b`'s.
/// A fighter's stats count as a share of the standard duel's, its traits as a share
/// of their maximum, its surrender threshold as a share of its HP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Winner {
    pub hp: f64,
    pub melee_attack: f64,
    pub ranged_attack: f64,
    pub accuracy: f64,
    pub speed: f64,
    pub melee_defense: f64,
    pub ranged_defense: f64,
    pub fear: f64,
    pub surrender: f64,
    pub aggression: f64,
    pub showmanship: f64,
    /// One weight per class; a class missing here weighs 0.
    pub classes: Vec<f64>,
}

/// Chance of each number of spectacular hits, from 0, by result; the last entry counts
/// that many hits or more.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectacularHits {
    pub won: Vec<f64>,
    /// Killed or given up.
    pub lost: Vec<f64>,
    pub unfinished: Vec<f64>,
}

/// How a modelled fight ended for one fighter: what `FightOutcome` reads from a duel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ending {
    pub result: FightResult,
    pub died: bool,
    /// Stunned at least once, got up or not.
    pub stunned: bool,
    /// Lost by not getting up after a stun.
    pub knocked_out: bool,
    /// What a cutting hit severed: the head (`died` too) or a limb (lost, alive).
    pub severed: Option<Location>,
    pub spectacular_hits: u32,
}

impl Ending {
    /// How the finished fight in `arena` ended for fighter `id`.
    #[must_use]
    pub fn from_arena(arena: &Arena, id: FighterId, rules: &FameRules) -> Self {
        let outcome = FightOutcome::from_arena(arena, id, false, rules);
        let fighter = arena.fighter(id);
        Self {
            result: outcome.result,
            died: fighter.is_some_and(|f| !f.is_alive()),
            stunned: fighter.is_some_and(|f| f.stuns > 0),
            knocked_out: fighter.is_some_and(|f| f.knocked_out),
            severed: fighter.and_then(|f| f.severed),
            spectacular_hits: outcome.spectacular_hits,
        }
    }
}

/// The logistic function.
#[must_use]
pub fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + libm::exp(-z))
}

/// The features of `fighter` the winner's regression reads, in the order of
/// [`Winner::to_weights`]: [`BASE_FEATURES`] values, then one per class, 1 for its own.
#[must_use]
pub fn features(fighter: &Fighter, classes: usize) -> Vec<f64> {
    let share = |value: u32, of: u32| f64::from(value) / f64::from(of.max(1));
    let trait_share = |value: u32| share(value, Personality::MAX);
    let personality = &fighter.condition.personality;
    let mut features: Vec<f64> = fighter
        .stats
        .to_array()
        .into_iter()
        .zip(STATS.to_array())
        .map(|(value, standard)| share(value, standard))
        .collect();
    features.extend([
        fear(&fighter.condition),
        share(fighter.surrender_hp, fighter.stats.hp),
        trait_share(personality.aggression.value()),
        trait_share(personality.showmanship.value()),
    ]);
    let class = usize::try_from(fighter.class.0).unwrap_or(usize::MAX);
    features.extend((0..classes).map(|c| if c == class { 1.0 } else { 0.0 }));
    features
}

impl Winner {
    /// The weights in the order of [`features`], `classes` of them for the classes.
    #[must_use]
    pub fn to_weights(&self, classes: usize) -> Vec<f64> {
        let mut weights = vec![
            self.hp,
            self.melee_attack,
            self.ranged_attack,
            self.accuracy,
            self.speed,
            self.melee_defense,
            self.ranged_defense,
            self.fear,
            self.surrender,
            self.aggression,
            self.showmanship,
        ];
        weights.extend((0..classes).map(|c| self.classes.get(c).copied().unwrap_or(0.0)));
        weights
    }

    /// The inverse of [`Winner::to_weights`]; missing weights are 0.
    #[must_use]
    pub fn from_weights(weights: &[f64]) -> Self {
        let w = |i: usize| weights.get(i).copied().unwrap_or(0.0);
        Self {
            hp: w(0),
            melee_attack: w(1),
            ranged_attack: w(2),
            accuracy: w(3),
            speed: w(4),
            melee_defense: w(5),
            ranged_defense: w(6),
            fear: w(7),
            surrender: w(8),
            aggression: w(9),
            showmanship: w(10),
            classes: weights.get(BASE_FEATURES..).unwrap_or_default().to_vec(),
        }
    }

    /// Chance that `a` beats `b`, should the fight have a winner.
    #[must_use]
    pub fn chance(&self, a: &Fighter, b: &Fighter) -> f64 {
        let classes = self.classes.len();
        let z: f64 = self
            .to_weights(classes)
            .iter()
            .zip(features(a, classes).iter().zip(features(b, classes)))
            .map(|(w, (fa, fb))| w * (fa - fb))
            .sum();
        sigmoid(z)
    }
}

impl StatModel {
    /// A model where no fighter has an edge, fights always end, the loser always gives
    /// up, nobody is stunned and no hit is spectacular: what `calibrate` starts from
    /// without a model.
    #[must_use]
    pub fn neutral() -> Self {
        Self {
            fights: 0,
            winner: Winner::from_weights(&[]),
            unfinished: Vec::new(),
            knocked_out: Vec::new(),
            lost_limb: Vec::new(),
            beheaded: Vec::new(),
            death: vec![0.0],
            stunned: Vec::new(),
            spectacular_hits: SpectacularHits {
                won: vec![1.0],
                lost: vec![1.0],
                unfinished: vec![1.0],
            },
        }
    }

    /// Chance the fight between `a` and `b` reaches the time limit; 0 for classes the
    /// model does not know.
    #[must_use]
    pub fn unfinished_chance(&self, a: &Fighter, b: &Fighter) -> f64 {
        let class = |f: &Fighter| usize::try_from(f.class.0).unwrap_or(usize::MAX);
        self.unfinished
            .get(class(a))
            .and_then(|row| row.get(class(b)))
            .copied()
            .unwrap_or(0.0)
    }

    /// Chance that the loser of a finished fight won by `winner` was knocked out; 0 for
    /// a class the model does not know.
    #[must_use]
    pub fn knocked_out_chance(&self, winner: &Fighter) -> f64 {
        by_class(&self.knocked_out, winner)
    }

    /// Chance that the loser of a finished fight won by `winner` lost a limb; 0 for a
    /// class the model does not know.
    #[must_use]
    pub fn lost_limb_chance(&self, winner: &Fighter) -> f64 {
        by_class(&self.lost_limb, winner)
    }

    /// Chance that the loser of a finished fight won by `winner` was beheaded; 0 for a
    /// class the model does not know.
    #[must_use]
    pub fn beheaded_chance(&self, winner: &Fighter) -> f64 {
        by_class(&self.beheaded, winner)
    }

    /// Chance that a fighter not knocked out was stunned by `opponent`; 0 for a class
    /// the model does not know.
    #[must_use]
    pub fn stunned_chance(&self, opponent: &Fighter) -> f64 {
        by_class(&self.stunned, opponent)
    }

    /// Chance that `loser`, beaten in a finished fight, neither knocked out nor cut,
    /// falls to 0 HP rather than gives up.
    #[must_use]
    pub fn death_chance(&self, loser: &Fighter) -> f64 {
        let determination = loser.condition.personality.determination.value();
        let band = usize::try_from(determination).unwrap_or(usize::MAX) * self.death.len()
            / usize::try_from(Personality::MAX + 1).unwrap_or(1);
        self.death
            .get(band.min(self.death.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0.0)
    }

    /// The fight between `a` and `b`, as the model draws it: what each side gets. A
    /// limb the loser lost is drawn by `cut` (see [`severed_limb`]).
    pub fn resolve<R: Rng + ?Sized>(
        &self,
        a: &Fighter,
        b: &Fighter,
        cut: &CutRules,
        rng: &mut R,
    ) -> [Ending; 2] {
        let hits = &self.spectacular_hits;
        if rng.random::<f64>() < self.unfinished_chance(a, b) {
            let side = |rng: &mut R, opponent| Ending {
                result: FightResult::Unfinished,
                died: false,
                stunned: rng.random::<f64>() < self.stunned_chance(opponent),
                knocked_out: false,
                severed: None,
                spectacular_hits: draw(rng, &hits.unfinished),
            };
            return [side(rng, b), side(rng, a)];
        }
        let a_wins = rng.random::<f64>() < self.winner.chance(a, b);
        let (first, second) = if a_wins { (a, b) } else { (b, a) };
        // One draw for the loser's way out: knocked out, a limb lost, beheaded, or else.
        let out = rng.random::<f64>();
        let knocked_out_below = self.knocked_out_chance(first);
        let lost_limb_below = knocked_out_below + self.lost_limb_chance(first);
        let beheaded_below = lost_limb_below + self.beheaded_chance(first);
        let knocked_out = out < knocked_out_below;
        let severed = if knocked_out {
            None
        } else if out < lost_limb_below {
            Some(severed_limb(rng, cut))
        } else if out < beheaded_below {
            Some(Location::Head)
        } else {
            None
        };
        let died = severed == Some(Location::Head)
            || !knocked_out && severed.is_none() && rng.random::<f64>() < self.death_chance(second);
        let winner = Ending {
            result: FightResult::Won,
            died: false,
            stunned: rng.random::<f64>() < self.stunned_chance(second),
            knocked_out: false,
            severed: None,
            spectacular_hits: draw(rng, &hits.won),
        };
        let loser = Ending {
            result: if died || knocked_out || severed.is_some() {
                FightResult::Lost
            } else {
                FightResult::GaveUp
            },
            died,
            stunned: knocked_out || rng.random::<f64>() < self.stunned_chance(first),
            knocked_out,
            severed,
            spectacular_hits: draw(rng, &hits.lost),
        };
        if a_wins {
            [winner, loser]
        } else {
            [loser, winner]
        }
    }

    /// The model as `data/stat_model.ron` holds it.
    ///
    /// # Errors
    /// If RON cannot write it, which would be a bug.
    pub fn to_ron(&self) -> Result<String, ron::Error> {
        let ron = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
        Ok(format!(
            "// Written by `cargo run --release -p tools --bin calibrate`: rerun it after any\n\
             // balancing change rather than editing by hand.\n{ron}\n"
        ))
    }

    /// The name of the first value the model may not hold: a chance outside 0 to 1,
    /// a weight that is not a number, or an empty table.
    #[must_use]
    pub fn invalid_field(&self) -> Option<&'static str> {
        let chance = |c: &f64| (0.0..=1.0).contains(c);
        let table = |t: &[f64]| !t.is_empty() && t.iter().all(chance);
        let weights = self.winner.to_weights(self.winner.classes.len());
        if !weights.iter().all(|w| w.is_finite()) {
            Some("winner")
        } else if !self.unfinished.iter().flatten().all(chance) {
            Some("unfinished")
        } else if !self.knocked_out.iter().all(chance) {
            Some("knocked_out")
        } else if !self.lost_limb.iter().all(chance) {
            Some("lost_limb")
        } else if !self.beheaded.iter().all(chance) {
            Some("beheaded")
        } else if !table(&self.death) {
            Some("death")
        } else if !self.stunned.iter().all(chance) {
            Some("stunned")
        } else if !(table(&self.spectacular_hits.won)
            && table(&self.spectacular_hits.lost)
            && table(&self.spectacular_hits.unfinished))
        {
            Some("spectacular_hits")
        } else {
            None
        }
    }
}

/// The chance `table` gives `fighter`'s class, 0 for a class it does not hold.
fn by_class(table: &[f64], fighter: &Fighter) -> f64 {
    usize::try_from(fighter.class.0)
        .ok()
        .and_then(|class| table.get(class))
        .copied()
        .unwrap_or(0.0)
}

/// An index drawn among `chances`, the last one when they add up to less than the draw.
fn draw<R: Rng + ?Sized>(rng: &mut R, chances: &[f64]) -> u32 {
    let mut left = rng.random::<f64>();
    let last = chances.len().saturating_sub(1);
    let index = chances
        .iter()
        .position(|&chance| {
            left -= chance;
            left < 0.0
        })
        .unwrap_or(last);
    u32::try_from(index).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Vec2;
    use crate::design_data::DesignData;
    use crate::fighter::ClassId;
    use crate::fighter::personality::TraitValue;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn data() -> DesignData {
        DesignData::load().unwrap()
    }

    fn fighter(data: &DesignData, class: u32, stats: crate::fighter::Stats) -> Fighter {
        let class = ClassId(class);
        let class_data = data.class(class).unwrap();
        Fighter::new(FighterId(0), class, class_data, stats, Vec2::new(0.0, 0.0))
    }

    fn determined(mut f: Fighter, determination: u32) -> Fighter {
        f.condition.personality.determination = TraitValue::new(determination);
        f
    }

    #[test]
    fn the_shipped_model_is_calibrated() {
        let model = data().stat_model;
        assert!(model.fights >= 10_000, "rerun calibrate");
        assert_eq!(model.winner.classes.len(), data().classes.len());
        assert_eq!(model.invalid_field(), None);
    }

    #[test]
    fn the_stronger_wins_more_often_and_mirrors_are_even() {
        let data = data();
        let model = &data.stat_model.winner;
        let standard = fighter(&data, 0, STATS);
        let strong = fighter(&data, 0, STATS.map(|v| v * 3 / 2));
        assert!(model.chance(&strong, &standard) > 0.8);
        assert!((model.chance(&standard, &standard) - 0.5).abs() < 1e-12);
        let other = fighter(&data, 1, STATS);
        let both = model.chance(&strong, &other) + model.chance(&other, &strong);
        assert!((both - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_determined_fall_far_more_often_than_the_timid() {
        let model = data().stat_model;
        let f = fighter(&data(), 0, STATS);
        let (bold, timid) = (determined(f.clone(), 100), determined(f, 0));
        // The most determined still give up, at 10 % of their HP.
        assert!(model.death_chance(&bold) > 0.3);
        assert!(model.death_chance(&timid) < 0.05);
        assert!(model.death_chance(&bold) > 10.0 * model.death_chance(&timid));
    }

    #[test]
    fn a_neutral_model_ends_every_fight_by_a_surrender() {
        let model = StatModel::neutral();
        let data = data();
        let f = fighter(&data, 0, STATS);
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        for _ in 0..20 {
            let endings = model.resolve(&f, &f, &data.combat.cut, &mut rng);
            assert_eq!(
                endings
                    .iter()
                    .filter(|e| e.result == FightResult::Won)
                    .count(),
                1
            );
            assert!(endings.iter().any(|e| e.result == FightResult::GaveUp));
            assert!(
                endings.iter().all(|e| !e.died
                    && !e.stunned
                    && e.severed.is_none()
                    && e.spectacular_hits == 0)
            );
        }
    }

    #[test]
    fn draws_follow_the_table() {
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(draw(&mut rng, &[0.0, 1.0, 0.0]), 1);
        assert_eq!(draw(&mut rng, &[0.0, 0.0]), 1);
        assert_eq!(draw(&mut rng, &[1.0]), 0);
    }

    proptest! {
        /// One winner, never killed, knocked out nor cut, or a time limit for both; a
        /// knocked-out loser was stunned and lives; a beheaded one is dead, one that lost
        /// a limb lives; the same seed gives the same fight.
        #[test]
        fn a_modelled_fight_ends_consistently(
            seed: u64,
            a in 0_u32..2,
            b in 0_u32..2,
            hp in 50_u32..200,
            determination in 0_u32..=100,
        ) {
            let data = data();
            let model = &data.stat_model;
            let first = fighter(&data, a, STATS);
            let second = determined(fighter(&data, b, crate::fighter::Stats { hp, ..STATS }), determination);
            let cut = &data.combat.cut;
            let endings = model.resolve(&first, &second, cut, &mut ChaCha8Rng::seed_from_u64(seed));
            prop_assert_eq!(endings, model.resolve(&first, &second, cut, &mut ChaCha8Rng::seed_from_u64(seed)));
            let winners = endings.iter().filter(|e| e.result == FightResult::Won).count();
            if winners == 0 {
                prop_assert!(endings.iter().all(|e| e.result == FightResult::Unfinished && !e.died && !e.knocked_out && e.severed.is_none()));
            } else {
                prop_assert_eq!(winners, 1);
                let loser = endings.iter().find(|e| e.result != FightResult::Won).unwrap();
                prop_assert_eq!(loser.died || loser.knocked_out || loser.severed.is_some(), loser.result == FightResult::Lost);
                prop_assert!(!(loser.died && loser.knocked_out));
                prop_assert!(!loser.knocked_out || loser.stunned && loser.severed.is_none());
                prop_assert_eq!(loser.severed == Some(Location::Head), loser.died && loser.severed.is_some());
                prop_assert!(endings.iter().all(|e| e.result != FightResult::Won || !e.died && !e.knocked_out && e.severed.is_none()));
            }
        }
    }
}
