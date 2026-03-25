fn euclidean(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt()
}

pub fn asymmetric_distance(tail_a: &[f64], head_b: &[f64], global_a: &[f64], global_b: &[f64], alpha: f64) -> f64 {
    alpha * euclidean(tail_a, head_b) + (1.0 - alpha) * euclidean(global_a, global_b)
}
