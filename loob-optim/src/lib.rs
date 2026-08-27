mod greedy_nn;
mod sa;

pub use greedy_nn::GreedyNn;
pub use sa::{AnnealingObjective, SimulatedAnnealing};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OptimError {
    #[error("distance matrix is empty")]
    EmptyMatrix,
    #[error("distance matrix is not square: {rows}x{cols}")]
    NotSquare { rows: usize, cols: usize },
    #[error("distance matrix contains a non-finite value at row {row}, column {col}")]
    NonFinite { row: usize, col: usize },
    #[error("initial ordering is not a complete permutation")]
    InvalidOrdering,
}

pub type DistanceMatrix = Vec<Vec<f64>>;
pub type Ordering = Vec<usize>;

pub trait Optimizer {
    fn optimize(&self, dist: &DistanceMatrix) -> Result<Ordering, OptimError>;
}

pub fn validate_matrix(dist: &DistanceMatrix) -> Result<usize, OptimError> {
    let n = dist.len();
    if n == 0 {
        return Err(OptimError::EmptyMatrix);
    }
    for (i, row) in dist.iter().enumerate() {
        if row.len() != n {
            return Err(OptimError::NotSquare {
                rows: n,
                cols: row.len(),
            });
        }
        for (j, value) in row.iter().enumerate() {
            if !value.is_finite() {
                return Err(OptimError::NonFinite { row: i, col: j });
            }
        }
    }
    Ok(n)
}

pub fn bottleneck_cost(dist: &DistanceMatrix, order: &[usize]) -> f64 {
    order
        .windows(2)
        .map(|w| dist[w[0]][w[1]])
        .fold(0.0, f64::max)
}

pub fn total_cost(dist: &DistanceMatrix, order: &[usize]) -> f64 {
    order.windows(2).map(|w| dist[w[0]][w[1]]).sum()
}

pub fn mean_cost(dist: &DistanceMatrix, order: &[usize]) -> f64 {
    let n = order.len();
    if n < 2 {
        return 0.0;
    }
    total_cost(dist, order) / (n - 1) as f64
}
