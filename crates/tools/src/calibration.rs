//! What `calibrate` does: plays real duels between varied fighters, then fits the
//! statistical model of `lanista_core::stat_model` on their results.

use lanista_core::combat::ArenaKind;
use lanista_core::design_data::DesignData;
use lanista_core::fighter::career::CareerFighter;
use lanista_core::fighter::equipment::Refused;
use lanista_core::fighter::fame::FightResult;
use lanista_core::fighter::form::Form;
use lanista_core::fighter::illness::roll_illness;
use lanista_core::fighter::injury::{Location, roll_injury};
use lanista_core::fighter::morale::Morale;
use lanista_core::fighter::{Fighter, FighterId};
use lanista_core::stat_model::{Ending, SpectacularHits, StatModel, Winner, features, sigmoid};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Bands of boldness in the death table.
pub const DEATH_BANDS: usize = 10;
/// Entries of the spectacular hits tables: 0 to 3 hits, then 4 or more.
pub const HIT_COUNTS: usize = 5;
/// Pull of every regression weight towards 0. Small next to thousands of fights, it
/// settles the class weights, which only count by their differences.
const RIDGE: f64 = 1.0;

/// What `calibrate` keeps of one real duel.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    /// The fighters as they started, fighter 0 first.
    pub fighters: [Fighter; 2],
    /// How the duel ended for each.
    pub sides: [Ending; 2],
}

/// A generated fighter in a random state: form and morale anywhere from 0 to 100,
/// with the injuries of two fights and the illness of a week drawn at that form.
pub fn varied_fighter(rng: &mut ChaCha8Rng, data: &DesignData) -> CareerFighter {
    let rules = &data.fighter;
    let mut fighter = CareerFighter::random(rng, data);
    fighter.form = Form::new(rng.random_range(0..=Form::MAX));
    fighter.morale = Morale::new(rng.random_range(0..=Morale::MAX));
    for _ in 0..2 {
        fighter.injuries.extend(roll_injury(
            rng,
            (fighter.form, 100),
            &rules.injury,
            &rules.form,
        ));
    }
    fighter.illness = roll_illness(rng, fighter.form, &rules.illness, &rules.form);
    fighter
}

/// The real duel of `seed` between two varied fighters, in an arena of `kind`.
#[must_use]
pub fn record(seed: u64, kind: ArenaKind, data: &DesignData) -> Sample {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let a = varied_fighter(&mut rng, data);
    let b = varied_fighter(&mut rng, data);
    let (_, start, arena) = a.duel(&b, (kind, Refused::NONE), &mut rng, data);
    let side = |i| Ending::from_arena(&arena, FighterId(i), &data.fighter.fame);
    Sample {
        fighters: [start.fighters[0].clone(), start.fighters[1].clone()],
        sides: [side(0), side(1)],
    }
}

/// The model fitted on `samples`, for `classes` classes: a logistic regression on the
/// fights with a winner, then the share of each ending (knock-out, lost limb, beheading,
/// death by boldness of the others), of the stunned and of the bled.
#[must_use]
pub fn fit(samples: &[Sample], classes: usize) -> StatModel {
    let class = |f: &Fighter| usize::try_from(f.class.0).unwrap_or(0).min(classes - 1);
    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut unfinished = vec![vec![(0_u64, 0_u64); classes]; classes];
    // By the winner's class, then by the opponent's class.
    let mut knocked_out = vec![(0_u64, 0_u64); classes];
    let mut lost_limb = vec![(0_u64, 0_u64); classes];
    let mut beheaded = vec![(0_u64, 0_u64); classes];
    let mut stunned = vec![(0_u64, 0_u64); classes];
    let mut bled = vec![(0_u64, 0_u64); classes];
    let mut death = [(0_u64, 0_u64); DEATH_BANDS];
    let mut hits = [[0_u64; HIT_COUNTS]; 3];
    for s in samples {
        let [a, b] = &s.fighters;
        let timed_out = s.sides[0].result == FightResult::Unfinished;
        // Both orders, so the table does not depend on who started on the left.
        for (first, second) in [(a, b), (b, a)] {
            let cell = &mut unfinished[class(first)][class(second)];
            *cell = (cell.0 + u64::from(timed_out), cell.1 + 1);
        }
        for (i, side) in s.sides.iter().enumerate() {
            let opponent = class(&s.fighters[1 - i]);
            if !side.knocked_out {
                let cell = &mut stunned[opponent];
                *cell = (cell.0 + u64::from(side.stunned), cell.1 + 1);
            }
            let cell = &mut bled[opponent];
            *cell = (cell.0 + u64::from(side.bled.is_some()), cell.1 + 1);
            let table = match side.result {
                FightResult::Won => 0,
                FightResult::Lost | FightResult::GaveUp => 1,
                FightResult::Unfinished => 2,
            };
            let count = usize::try_from(side.spectacular_hits).unwrap_or(usize::MAX);
            hits[table][count.min(HIT_COUNTS - 1)] += 1;
        }
        let Some(winner) = s
            .sides
            .iter()
            .position(|side| side.result == FightResult::Won)
        else {
            continue;
        };
        let (fa, fb) = (features(a, classes), features(b, classes));
        x.push(fa.iter().zip(&fb).map(|(a, b)| a - b).collect());
        y.push(winner == 0);
        let loser = &s.sides[1 - winner];
        let by_winner = class(&s.fighters[winner]);
        let head = loser.severed == Some(Location::Head);
        let limb = loser.severed.is_some() && !head;
        for (table, counted) in [
            (&mut knocked_out, loser.knocked_out),
            (&mut lost_limb, limb),
            (&mut beheaded, head),
        ] {
            let cell = &mut table[by_winner];
            *cell = (cell.0 + u64::from(counted), cell.1 + 1);
        }
        if loser.knocked_out || loser.severed.is_some() {
            continue;
        }
        let boldness = s.fighters[1 - winner]
            .condition
            .personality
            .boldness
            .value();
        let band = (usize::try_from(boldness).unwrap_or(usize::MAX) * DEATH_BANDS / 101)
            .min(DEATH_BANDS - 1);
        death[band] = (death[band].0 + u64::from(loser.died), death[band].1 + 1);
    }
    let all_deaths = death
        .iter()
        .fold((0, 0), |(d, n), &(dead, losers)| (d + dead, n + losers));
    let fights = u64::try_from(samples.len()).unwrap_or(u64::MAX);
    StatModel {
        fights,
        winner: Winner::from_weights(&logistic(&x, &y, RIDGE)),
        unfinished: unfinished
            .iter()
            .map(|row| row.iter().map(|&(u, n)| share(u, n, 0.0)).collect())
            .collect(),
        knocked_out: knocked_out.iter().map(|&(k, n)| share(k, n, 0.0)).collect(),
        lost_limb: lost_limb.iter().map(|&(l, n)| share(l, n, 0.0)).collect(),
        beheaded: beheaded.iter().map(|&(b, n)| share(b, n, 0.0)).collect(),
        death: death
            .iter()
            .map(|&(dead, losers)| share(dead, losers, share(all_deaths.0, all_deaths.1, 0.0)))
            .collect(),
        stunned: stunned.iter().map(|&(s, n)| share(s, n, 0.0)).collect(),
        bled: bled.iter().map(|&(b, n)| share(b, n, 0.0)).collect(),
        spectacular_hits: SpectacularHits {
            won: shares(&hits[0]),
            lost: shares(&hits[1]),
            unfinished: shares(&hits[2]),
        },
    }
}

/// `part / whole`, or `otherwise` for nothing.
pub(crate) fn share(part: u64, whole: u64, otherwise: f64) -> f64 {
    if whole == 0 {
        return otherwise;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "counts of fights, far below 2^52"
    )]
    let share = part as f64 / whole as f64;
    share
}

/// Each count's share of their total; no fight at all counts as surely none.
fn shares(counts: &[u64]) -> Vec<f64> {
    let total = counts.iter().sum();
    if total == 0 {
        let mut none = vec![0.0; counts.len()];
        none[0] = 1.0;
        return none;
    }
    counts.iter().map(|&c| share(c, total, 0.0)).collect()
}

/// Weights of the logistic regression of `wins` on `features` without intercept, by
/// Newton's method, each weight pulled towards 0 by `ridge`.
#[must_use]
pub fn logistic(features: &[Vec<f64>], wins: &[bool], ridge: f64) -> Vec<f64> {
    let count = features.first().map_or(0, Vec::len);
    let mut weights = vec![0.0; count];
    for _ in 0..50 {
        let mut gradient: Vec<f64> = weights.iter().map(|w| -ridge * w).collect();
        let mut hessian = vec![vec![0.0; count]; count];
        for (i, row) in hessian.iter_mut().enumerate() {
            row[i] = ridge;
        }
        for (x, &won) in features.iter().zip(wins) {
            let chance = sigmoid(x.iter().zip(&weights).map(|(x, w)| x * w).sum());
            let error = f64::from(u8::from(won)) - chance;
            let spread = chance * (1.0 - chance);
            for ((row, slope), &xj) in hessian.iter_mut().zip(&mut gradient).zip(x) {
                *slope += error * xj;
                for (cell, &xk) in row.iter_mut().zip(x) {
                    *cell += spread * xj * xk;
                }
            }
        }
        let step = solve(hessian, gradient);
        for (w, s) in weights.iter_mut().zip(&step) {
            *w += s;
        }
        if step.iter().all(|s| s.abs() < 1e-10) {
            break;
        }
    }
    weights
}

/// The solution of `matrix * x = rhs`, by Gaussian elimination with partial pivoting;
/// `matrix` must be invertible, which the ridge makes sure of.
fn solve(mut matrix: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Vec<f64> {
    let size = rhs.len();
    for col in 0..size {
        let pivot = (col..size)
            .max_by(|&i, &j| matrix[i][col].abs().total_cmp(&matrix[j][col].abs()))
            .unwrap_or(col);
        matrix.swap(col, pivot);
        rhs.swap(col, pivot);
        let (above, below) = matrix.split_at_mut(col + 1);
        let pivot_row = &above[col];
        for (offset, row) in below.iter_mut().enumerate() {
            let factor = row[col] / pivot_row[col];
            for (cell, &p) in row.iter_mut().zip(pivot_row).skip(col) {
                *cell -= factor * p;
            }
            rhs[col + 1 + offset] -= factor * rhs[col];
        }
    }
    let mut solution = vec![0.0; size];
    for row in (0..size).rev() {
        let known: f64 = (row + 1..size).map(|k| matrix[row][k] * solution[k]).sum();
        solution[row] = (rhs[row] - known) / matrix[row][row];
    }
    solution
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_a_linear_system() {
        let x = solve(vec![vec![2.0, 1.0], vec![1.0, 3.0]], vec![5.0, 10.0]);
        assert!((x[0] - 1.0).abs() < 1e-12 && (x[1] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn the_regression_finds_the_weights_behind_the_results() {
        // y drawn with the chance sigmoid(2 x0 - x1).
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let x: Vec<Vec<f64>> = (0..20_000)
            .map(|_| vec![rng.random_range(-1.0..1.0), rng.random_range(-1.0..1.0)])
            .collect();
        let y: Vec<bool> = x
            .iter()
            .map(|xi| rng.random::<f64>() < sigmoid(2.0 * xi[0] - xi[1]))
            .collect();
        let w = logistic(&x, &y, RIDGE);
        assert!((w[0] - 2.0).abs() < 0.15, "{w:?}");
        assert!((w[1] + 1.0).abs() < 0.15, "{w:?}");
    }

    #[test]
    fn a_fitted_model_is_valid_and_favors_the_stronger() {
        let data = DesignData::load().unwrap();
        let samples: Vec<Sample> = (0..400)
            .map(|seed| record(seed, ArenaKind::Sand, &data))
            .collect();
        assert_eq!(record(3, ArenaKind::Sand, &data), samples[3]);
        assert_ne!(record(3, ArenaKind::Narrow, &data), samples[3]);
        let model = fit(&samples, data.classes.len());
        assert_eq!(model.fights, 400);
        assert_eq!(model.invalid_field(), None);
        assert_eq!(model.death.len(), DEATH_BANDS);
        let total: f64 = model.spectacular_hits.lost.iter().sum();
        assert!((total - 1.0).abs() < 1e-9);
        // More HP and attack win more often.
        assert!(model.winner.hp > 0.0, "{:?}", model.winner);
        assert!(model.winner.melee_attack + model.winner.ranged_attack > 0.0);
    }
}
