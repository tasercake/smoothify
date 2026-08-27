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
        self.optimize_from_validated(dist, (0..n).collect())
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
        let n = validate_matrix(dist)?;
        let mut sorted = initial.clone();
        sorted.sort_unstable();
        if initial.len() != n || sorted != (0..n).collect::<Vec<_>>() {
            return Err(OptimError::InvalidOrdering);
        }
        self.optimize_from_validated(dist, initial)
    }

    fn optimize_from_validated(
        &self,
        dist: &DistanceMatrix,
        initial: Ordering,
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
        for _ in 0..self.iterations {
            let i = rng.gen_range(0..n);
            let j = rng.gen_range(0..n);
            let (lo, hi) = if i < j { (i, j) } else { (j, i) };
            if lo == hi {
                continue;
            }
            current[lo..=hi].reverse();
            let new_cost = self.objective.cost(dist, &current);
            let delta = new_cost - current_cost;
            if delta < 0.0 || rng.gen::<f64>() < (-delta / temp).exp() {
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
        Ok(best)
    }
}
