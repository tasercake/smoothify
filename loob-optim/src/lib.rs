mod greedy_nn;
mod sa;

pub use greedy_nn::GreedyNn;
pub use sa::{AnnealingObjective, AnnealingProgress, SimulatedAnnealing};

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

/// Use the median positive off-diagonal transition as a scale-aware starting
/// temperature. This keeps annealing behavior stable when the metric's units
/// or weights change.
pub fn characteristic_edge_cost(dist: &DistanceMatrix) -> Result<f64, OptimError> {
    let n = validate_matrix(dist)?;
    let mut edges = Vec::with_capacity(n.saturating_mul(n.saturating_sub(1)));
    for (row, values) in dist.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            if row != column && *value > 0.0 {
                edges.push(*value);
            }
        }
    }
    if edges.is_empty() {
        return Ok(1.0);
    }
    edges.sort_by(f64::total_cmp);
    let middle = edges.len() / 2;
    let median = if edges.len() % 2 == 0 {
        (edges[middle - 1] + edges[middle]) * 0.5
    } else {
        edges[middle]
    };
    Ok(median.max(1.0e-9))
}

#[cfg(test)]
mod scale_tests {
    use super::*;

    #[test]
    fn characteristic_cost_scales_with_the_matrix() {
        let matrix = vec![
            vec![0.0, 1.0, 3.0],
            vec![2.0, 0.0, 4.0],
            vec![5.0, 6.0, 0.0],
        ];
        let scaled = matrix
            .iter()
            .map(|row| row.iter().map(|value| value * 10.0).collect())
            .collect::<Vec<Vec<f64>>>();
        assert_eq!(
            characteristic_edge_cost(&scaled).unwrap(),
            10.0 * characteristic_edge_cost(&matrix).unwrap()
        );
    }

    #[test]
    fn all_zero_matrix_has_a_safe_temperature() {
        assert_eq!(characteristic_edge_cost(&vec![vec![0.0]]).unwrap(), 1.0);
    }
}
