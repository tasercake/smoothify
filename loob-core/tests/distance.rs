
// Test the distance module directly
// We can't import private modules, so test via the public API

#[test]
fn asymmetric_distance_zero_when_identical() {
    let v = vec![1.0, 2.0, 3.0];
    let d = loob_core::asymmetric_distance(&v, &v, &v, &v, 0.5);
    assert_eq!(d, 0.0);
}

#[test]
fn asymmetric_distance_alpha_zero_ignores_transition() {
    let tail_a = vec![100.0, 0.0]; // very different from head_b
    let head_b = vec![0.0, 100.0];
    let global_a = vec![1.0, 1.0]; // identical globals
    let global_b = vec![1.0, 1.0];

    let d = loob_core::asymmetric_distance(&tail_a, &head_b, &global_a, &global_b, 0.0);
    assert_eq!(d, 0.0, "alpha=0 should only use global distance");
}

#[test]
fn asymmetric_distance_alpha_one_ignores_global() {
    let tail_a = vec![1.0, 0.0];
    let head_b = vec![1.0, 0.0]; // identical to tail
    let global_a = vec![100.0, 0.0]; // very different globals
    let global_b = vec![0.0, 100.0];

    let d = loob_core::asymmetric_distance(&tail_a, &head_b, &global_a, &global_b, 1.0);
    assert_eq!(d, 0.0, "alpha=1 should only use transition distance");
}

#[test]
fn asymmetric_distance_is_weighted_sum() {
    let tail_a = vec![1.0, 0.0];
    let head_b = vec![0.0, 1.0];
    let global_a = vec![3.0, 0.0];
    let global_b = vec![0.0, 4.0];

    let alpha = 0.6;
    let transition = ((1.0f64).powi(2) + (1.0f64).powi(2)).sqrt(); // sqrt(2)
    let global = ((3.0f64).powi(2) + (4.0f64).powi(2)).sqrt(); // 5.0

    let expected = alpha * transition + (1.0 - alpha) * global;
    let actual = loob_core::asymmetric_distance(&tail_a, &head_b, &global_a, &global_b, alpha);
    assert!((actual - expected).abs() < 1e-10);
}
