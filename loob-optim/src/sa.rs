//! Simulated annealing solver for asymmetric TSP.

use crate::CostFn;
use rand::Rng;

pub struct SaConfig {
    /// Starting temperature.
    pub t_start: f64,
    /// Final temperature.
    pub t_end: f64,
    /// Number of iterations.
    pub iterations: usize,
    /// Weight on bottleneck (max edge) vs total cost.
    /// objective = beta * max_edge + (1 - beta) * mean_edge
    pub beta: f64,
}

impl Default for SaConfig {
    fn default() -> Self {
        Self {
            t_start: 1.0,
            t_end: 1e-4,
            iterations: 500_000,
            beta: 0.3,
        }
    }
}

pub struct SaResult {
    pub path: Vec<usize>,
    pub max_edge: f64,
    pub total_cost: f64,
}

pub fn solve(cost: &impl CostFn, initial_path: Vec<usize>, config: &SaConfig) -> SaResult {
    let n = initial_path.len();
    if n < 3 {
        return SaResult {
            path: initial_path,
            max_edge: 0.0,
            total_cost: 0.0,
        };
    }

    let mut rng = rand::rng();
    let mut path = initial_path;
    let mut current_obj = objective(cost, &path, config.beta);

    let mut best_path = path.clone();
    let mut best_obj = current_obj;

    let cooling = (config.t_end / config.t_start).ln() / config.iterations as f64;

    for i in 0..config.iterations {
        let t = config.t_start * (cooling * i as f64).exp();

        // 2-opt reversal
        let a = rng.random_range(0..n);
        let b = rng.random_range(0..n);
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        if lo == hi { continue; }

        path[lo..=hi].reverse();
        let new_obj = objective(cost, &path, config.beta);
        let delta = new_obj - current_obj;

        if delta < 0.0 || rng.random::<f64>() < (-delta / t).exp() {
            current_obj = new_obj;
            if current_obj < best_obj {
                best_obj = current_obj;
                best_path = path.clone();
            }
        } else {
            path[lo..=hi].reverse(); // revert
        }
    }

    let (max_edge, total_cost) = path_stats(cost, &best_path);
    SaResult { path: best_path, max_edge, total_cost }
}

fn objective(cost: &impl CostFn, path: &[usize], beta: f64) -> f64 {
    let (max_edge, total_cost) = path_stats(cost, path);
    let mean_edge = total_cost / (path.len().saturating_sub(1).max(1)) as f64;
    beta * max_edge + (1.0 - beta) * mean_edge
}

fn path_stats(cost: &impl CostFn, path: &[usize]) -> (f64, f64) {
    let mut max_edge: f64 = 0.0;
    let mut total: f64 = 0.0;
    for w in path.windows(2) {
        let c = cost.cost(w[0], w[1]);
        max_edge = max_edge.max(c);
        total += c;
    }
    (max_edge, total)
}
