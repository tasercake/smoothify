//! Simulated annealing solver for asymmetric TSP.

use crate::{
    bottleneck_cost, mean_cost, total_cost, validate_matrix, DistanceMatrix, OptimError, Optimizer,
    Ordering,
};
use rand::Rng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnnealingObjective {
    Bottleneck,
    Total,
    Hybrid { beta: f64 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct AnnealingProgress {
    pub iteration: usize,
    pub iterations: usize,
    pub temperature: f64,
    pub initial_temperature: f64,
    pub cooling_rate: f64,
    pub current_loss: f64,
    pub best_loss: f64,
    pub accepted_moves: usize,
    pub attempted_moves: usize,
}

impl AnnealingObjective {
    fn cost(&self, dist: &DistanceMatrix, order: &[usize]) -> f64 {
        match self {
            Self::Bottleneck => bottleneck_cost(dist, order),
            Self::Total => total_cost(dist, order),
            Self::Hybrid { beta } => {
                beta * bottleneck_cost(dist, order) + (1.0 - beta) * mean_cost(dist, order)
            }
        }
    }
}

pub struct SimulatedAnnealing {
    pub objective: AnnealingObjective,
    pub initial_temp: f64,
    pub cooling_rate: f64,
    pub iterations: usize,
    /// Optional seed for deterministic runs. None = random.
    pub seed: Option<u64>,
}

impl Default for SimulatedAnnealing {
    fn default() -> Self {
        Self {
            objective: AnnealingObjective::Bottleneck,
            initial_temp: 1.0,
            cooling_rate: 0.9995,
            iterations: 100_000,
            seed: Some(0),
        }
    }
}

impl Optimizer for SimulatedAnnealing {
    fn optimize(&self, dist: &DistanceMatrix) -> Result<Ordering, OptimError> {
        let n = validate_matrix(dist)?;
        self.optimize_from_validated(dist, (0..n).collect(), usize::MAX, |_| {})
    }
}

impl SimulatedAnnealing {
    /// Optimize from a caller-provided complete permutation, typically the
    /// deterministic all-start greedy solution.
    pub fn optimize_from(
        &self,
        dist: &DistanceMatrix,
        initial: Ordering,
    ) -> Result<Ordering, OptimError> {
        validate_initial_ordering(dist, &initial)?;
        self.optimize_from_validated(dist, initial, usize::MAX, |_| {})
    }

    /// Optimize while reporting the initial state, every `report_every`
    /// iterations, and the final state. Reporting does not change the random
    /// stream or annealing decisions.
    pub fn optimize_from_with_progress(
        &self,
        dist: &DistanceMatrix,
        initial: Ordering,
        report_every: usize,
        progress: impl FnMut(AnnealingProgress),
    ) -> Result<Ordering, OptimError> {
        validate_initial_ordering(dist, &initial)?;
        self.optimize_from_validated(dist, initial, report_every.max(1), progress)
    }

    fn optimize_from_validated(
        &self,
        dist: &DistanceMatrix,
        initial: Ordering,
        report_every: usize,
        mut progress: impl FnMut(AnnealingProgress),
    ) -> Result<Ordering, OptimError> {
        let n = dist.len();
        let mut rng: Box<dyn rand::RngCore> = match self.seed {
            Some(s) => Box::new(rand_chacha::ChaCha8Rng::seed_from_u64(s)),
            None => Box::new(rand::thread_rng()),
        };
        let mut current = initial;
        let mut current_cost = self.objective.cost(dist, &current);
        let mut best = current.clone();
        let mut best_cost = current_cost;
        let mut temp = self.initial_temp;
        let mut accepted_moves = 0;
        let mut attempted_moves = 0;
        progress(AnnealingProgress {
            iteration: 0,
            iterations: self.iterations,
            temperature: temp,
            initial_temperature: self.initial_temp,
            cooling_rate: self.cooling_rate,
            current_loss: current_cost,
            best_loss: best_cost,
            accepted_moves,
            attempted_moves,
        });
        for iteration in 1..=self.iterations {
            let i = rng.gen_range(0..n);
            let j = rng.gen_range(0..n);
            let (lo, hi) = if i < j { (i, j) } else { (j, i) };
            if lo != hi {
                attempted_moves += 1;
                current[lo..=hi].reverse();
                let new_cost = self.objective.cost(dist, &current);
                let delta = new_cost - current_cost;
                if delta < 0.0 || rng.gen::<f64>() < (-delta / temp).exp() {
                    accepted_moves += 1;
                    current_cost = new_cost;
                    if current_cost < best_cost {
                        best = current.clone();
                        best_cost = current_cost;
                    }
                } else {
                    current[lo..=hi].reverse();
                }
                temp *= self.cooling_rate;
            }
            if iteration % report_every == 0 || iteration == self.iterations {
                progress(AnnealingProgress {
                    iteration,
                    iterations: self.iterations,
                    temperature: temp,
                    initial_temperature: self.initial_temp,
                    cooling_rate: self.cooling_rate,
                    current_loss: current_cost,
                    best_loss: best_cost,
                    accepted_moves,
                    attempted_moves,
                });
            }
        }
        Ok(best)
    }
}

fn validate_initial_ordering(dist: &DistanceMatrix, initial: &Ordering) -> Result<(), OptimError> {
    let n = validate_matrix(dist)?;
    let mut sorted = initial.clone();
    sorted.sort_unstable();
    if initial.len() != n || sorted != (0..n).collect::<Vec<_>>() {
        return Err(OptimError::InvalidOrdering);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_reports_initial_periodic_and_final_state() {
        let matrix = vec![
            vec![0.0, 1.0, 4.0],
            vec![3.0, 0.0, 1.0],
            vec![1.0, 2.0, 0.0],
        ];
        let annealing = SimulatedAnnealing {
            iterations: 10,
            seed: Some(7),
            ..Default::default()
        };
        let mut updates = Vec::new();
        annealing
            .optimize_from_with_progress(&matrix, vec![0, 1, 2], 3, |update| updates.push(update))
            .unwrap();

        assert_eq!(
            updates
                .iter()
                .map(|update| update.iteration)
                .collect::<Vec<_>>(),
            vec![0, 3, 6, 9, 10]
        );
        assert!(updates.iter().all(|update| {
            update.temperature.is_finite()
                && update.current_loss.is_finite()
                && update.best_loss.is_finite()
                && update.best_loss <= update.current_loss
        }));
        assert!(updates
            .windows(2)
            .all(|pair| pair[1].best_loss <= pair[0].best_loss));
    }
}
