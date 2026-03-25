//! Greedy nearest-neighbor with all-starts (baseline solver).

use crate::CostFn;

/// Result of a greedy solve: ordered node indices + the bottleneck (max edge).
pub struct GreedyResult {
    pub path: Vec<usize>,
    pub max_edge: f64,
    pub total_cost: f64,
}

/// Try every node as a starting point, return the path with the smallest bottleneck edge.
pub fn solve_all_starts(cost: &impl CostFn) -> GreedyResult {
    let n = cost.len();
    let mut best: Option<GreedyResult> = None;

    for start in 0..n {
        let result = solve_from(cost, start);
        if best.as_ref().is_none_or(|b| result.max_edge < b.max_edge) {
            best = Some(result);
        }
    }

    best.expect("empty graph")
}

fn solve_from(cost: &impl CostFn, start: usize) -> GreedyResult {
    let n = cost.len();
    let mut visited = vec![false; n];
    let mut path = Vec::with_capacity(n);
    let mut max_edge: f64 = 0.0;
    let mut total_cost: f64 = 0.0;

    let mut current = start;
    visited[current] = true;
    path.push(current);

    for _ in 1..n {
        let mut best_next = usize::MAX;
        let mut best_cost = f64::INFINITY;

        for candidate in 0..n {
            if !visited[candidate] {
                let c = cost.cost(current, candidate);
                if c < best_cost {
                    best_cost = c;
                    best_next = candidate;
                }
            }
        }

        visited[best_next] = true;
        path.push(best_next);
        max_edge = max_edge.max(best_cost);
        total_cost += best_cost;
        current = best_next;
    }

    GreedyResult { path, max_edge, total_cost }
}
