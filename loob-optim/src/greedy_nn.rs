use crate::{DistanceMatrix, OptimError, Optimizer, Ordering, bottleneck_cost, validate_matrix};

pub struct GreedyNn;

impl Optimizer for GreedyNn {
    fn optimize(&self, dist: &DistanceMatrix) -> Result<Ordering, OptimError> {
        let n = validate_matrix(dist)?;
        let mut best_order: Option<Ordering> = None;
        let mut best_cost = f64::INFINITY;
        for start in 0..n {
            let order = greedy_from(dist, n, start);
            let cost = bottleneck_cost(dist, &order);
            if cost < best_cost { best_cost = cost; best_order = Some(order); }
        }
        Ok(best_order.unwrap())
    }
}

fn greedy_from(dist: &DistanceMatrix, n: usize, start: usize) -> Ordering {
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    let mut current = start;
    visited[current] = true;
    order.push(current);
    for _ in 1..n {
        let mut best_next = 0;
        let mut best_dist = f64::INFINITY;
        for j in 0..n {
            if !visited[j] && dist[current][j] < best_dist { best_dist = dist[current][j]; best_next = j; }
        }
        visited[best_next] = true;
        order.push(best_next);
        current = best_next;
    }
    order
}
