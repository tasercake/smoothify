use loob_optim::*;

fn identity_matrix(n: usize) -> DistanceMatrix {
    let mut m = vec![vec![0.0; n]; n];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            if i != j {
                *value = 1.0;
            }
        }
    }
    m
}

/// A simple asymmetric chain: 0→1→2→3 is cheap, everything else is expensive.
fn chain_matrix() -> DistanceMatrix {
    let n = 4;
    let mut m = vec![vec![10.0; n]; n];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 0.0;
    }
    // Cheap forward edges: 0→1, 1→2, 2→3
    m[0][1] = 1.0;
    m[1][2] = 1.0;
    m[2][3] = 1.0;
    // Reverse edges are expensive (asymmetric)
    m[1][0] = 8.0;
    m[2][1] = 8.0;
    m[3][2] = 8.0;
    m
}

/// Asymmetric ring: 0→1→2→3→0 is the optimal tour.
fn ring_matrix() -> DistanceMatrix {
    let n = 4;
    let mut m = vec![vec![100.0; n]; n];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 0.0;
    }
    m[0][1] = 1.0;
    m[1][2] = 2.0;
    m[2][3] = 1.0;
    m[3][0] = 2.0;
    // Reverse direction is much worse
    m[1][0] = 50.0;
    m[2][1] = 50.0;
    m[3][2] = 50.0;
    m[0][3] = 50.0;
    m
}

// --- Validation ---

#[test]
fn empty_matrix_errors() {
    let m: DistanceMatrix = vec![];
    assert!(matches!(validate_matrix(&m), Err(OptimError::EmptyMatrix)));
}

#[test]
fn non_square_matrix_errors() {
    let m = vec![vec![0.0, 1.0], vec![1.0]];
    assert!(matches!(
        validate_matrix(&m),
        Err(OptimError::NotSquare { .. })
    ));
}

#[test]
fn valid_matrix_returns_size() {
    let m = identity_matrix(5);
    assert_eq!(validate_matrix(&m).unwrap(), 5);
}

// --- Cost functions ---

#[test]
fn bottleneck_cost_finds_max_edge() {
    let m = chain_matrix();
    // Path 0→1→2→3: edges are 1, 1, 1 → bottleneck = 1
    assert_eq!(bottleneck_cost(&m, &[0, 1, 2, 3]), 1.0);
    // Path 3→2→1→0: edges are 8, 8, 8 → bottleneck = 8
    assert_eq!(bottleneck_cost(&m, &[3, 2, 1, 0]), 8.0);
}

#[test]
fn total_cost_sums_edges() {
    let m = chain_matrix();
    assert_eq!(total_cost(&m, &[0, 1, 2, 3]), 3.0);
    assert_eq!(total_cost(&m, &[3, 2, 1, 0]), 24.0);
}

#[test]
fn mean_cost_averages() {
    let m = chain_matrix();
    assert_eq!(mean_cost(&m, &[0, 1, 2, 3]), 1.0); // 3.0 / 3
}

#[test]
fn single_node_costs_are_zero() {
    let m = identity_matrix(1);
    assert_eq!(bottleneck_cost(&m, &[0]), 0.0); // no edges
    assert_eq!(total_cost(&m, &[0]), 0.0);
    assert_eq!(mean_cost(&m, &[0]), 0.0);
}

// --- Greedy NN ---

#[test]
fn greedy_finds_forward_chain() {
    let m = chain_matrix();
    let result = GreedyNn.optimize(&m).unwrap();
    // Starting from 0, greedy should pick 0→1→2→3 (cheapest neighbors)
    assert_eq!(result, vec![0, 1, 2, 3]);
}

#[test]
fn greedy_respects_asymmetry() {
    let m = ring_matrix();
    let result = GreedyNn.optimize(&m).unwrap();
    // The forward ring 0→1→2→3 has bottleneck 2.0
    // The reverse would have bottleneck 50.0
    assert_eq!(bottleneck_cost(&m, &result), 2.0);
}

#[test]
fn greedy_single_node() {
    let m = vec![vec![0.0]];
    let result = GreedyNn.optimize(&m).unwrap();
    assert_eq!(result, vec![0]);
}

#[test]
fn greedy_two_nodes() {
    let m = vec![vec![0.0, 5.0], vec![3.0, 0.0]];
    let result = GreedyNn.optimize(&m).unwrap();
    // From 0: 0→1 cost 5. From 1: 1→0 cost 3. Bottleneck: min(5, 3) = 3, so starts from 1.
    assert_eq!(result, vec![1, 0]);
}

// --- Simulated Annealing ---

#[test]
fn sa_is_deterministic_with_seed() {
    let m = chain_matrix();
    let sa = SimulatedAnnealing {
        seed: Some(42),
        iterations: 10_000,
        ..Default::default()
    };
    let r1 = sa.optimize(&m).unwrap();
    let r2 = sa.optimize(&m).unwrap();
    assert_eq!(r1, r2, "same seed must produce identical results");
}

#[test]
fn sa_different_seeds_may_differ() {
    // With a larger matrix, different seeds should explore differently.
    // Use a 6-node random-ish matrix.
    let m = vec![
        vec![0.0, 3.0, 7.0, 2.0, 9.0, 1.0],
        vec![8.0, 0.0, 4.0, 6.0, 1.0, 5.0],
        vec![2.0, 9.0, 0.0, 3.0, 7.0, 4.0],
        vec![5.0, 1.0, 8.0, 0.0, 4.0, 6.0],
        vec![3.0, 7.0, 2.0, 9.0, 0.0, 8.0],
        vec![6.0, 4.0, 1.0, 5.0, 3.0, 0.0],
    ];
    let r1 = SimulatedAnnealing {
        seed: Some(1),
        iterations: 50_000,
        ..Default::default()
    }
    .optimize(&m)
    .unwrap();
    let r2 = SimulatedAnnealing {
        seed: Some(999),
        iterations: 50_000,
        ..Default::default()
    }
    .optimize(&m)
    .unwrap();
    // Not guaranteed to differ, but with different seeds on a complex matrix they usually will.
    // At minimum, both must be valid permutations.
    let mut s1 = r1.clone();
    s1.sort();
    let mut s2 = r2.clone();
    s2.sort();
    assert_eq!(s1, vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(s2, vec![0, 1, 2, 3, 4, 5]);
}

#[test]
fn sa_finds_optimal_chain() {
    let m = chain_matrix();
    let result = SimulatedAnnealing {
        seed: Some(42),
        iterations: 50_000,
        objective: AnnealingObjective::Bottleneck,
        ..Default::default()
    }
    .optimize(&m)
    .unwrap();
    // Optimal path is 0→1→2→3 with bottleneck 1.0
    assert_eq!(bottleneck_cost(&m, &result), 1.0);
    assert_eq!(result, vec![0, 1, 2, 3]);
}

#[test]
fn sa_respects_asymmetry() {
    let m = ring_matrix();
    let result = SimulatedAnnealing {
        seed: Some(7),
        iterations: 50_000,
        objective: AnnealingObjective::Bottleneck,
        ..Default::default()
    }
    .optimize(&m)
    .unwrap();
    // Forward ring has bottleneck 2.0, reverse has 50.0
    assert!(bottleneck_cost(&m, &result) <= 2.0);
}

#[test]
fn sa_single_node() {
    let m = vec![vec![0.0]];
    let result = SimulatedAnnealing {
        seed: Some(0),
        iterations: 100,
        ..Default::default()
    }
    .optimize(&m)
    .unwrap();
    assert_eq!(result, vec![0]);
}

#[test]
fn sa_two_nodes() {
    let m = vec![vec![0.0, 5.0], vec![3.0, 0.0]];
    let result = SimulatedAnnealing {
        seed: Some(0),
        iterations: 1000,
        ..Default::default()
    }
    .optimize(&m)
    .unwrap();
    // Only two possible orderings, both valid
    assert!(result == vec![0, 1] || result == vec![1, 0]);
}

// --- loob-core distance ---

#[test]
fn asymmetric_distance_is_directional() {
    // Tail of A, head of B vs tail of B, head of A should differ
    let tail_a = vec![1.0, 0.0, 0.0];
    let head_a = vec![0.0, 0.0, 1.0];
    let tail_b = vec![0.0, 1.0, 0.0];
    let head_b = vec![1.0, 0.0, 0.0]; // close to tail_a

    let global_a = vec![0.5, 0.0, 0.5];
    let global_b = vec![0.5, 0.5, 0.0];

    let alpha = 0.6;

    // d(A→B): alpha * dist(tail_a, head_b) + (1-alpha) * dist(global_a, global_b)
    let d_ab =
        alpha * euclidean(&tail_a, &head_b) + (1.0 - alpha) * euclidean(&global_a, &global_b);
    // d(B→A): alpha * dist(tail_b, head_a) + (1-alpha) * dist(global_b, global_a)
    let d_ba =
        alpha * euclidean(&tail_b, &head_a) + (1.0 - alpha) * euclidean(&global_b, &global_a);

    // tail_a == head_b, so transition A→B is 0. But tail_b ≠ head_a.
    assert!(
        d_ab < d_ba,
        "A→B should be cheaper since tail_a matches head_b"
    );
    assert!((d_ab - (1.0 - alpha) * euclidean(&global_a, &global_b)).abs() < 1e-10);
}

fn euclidean(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f64>()
        .sqrt()
}
