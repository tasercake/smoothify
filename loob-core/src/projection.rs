use crate::{distance::feature_distance, DspSummary};

/// The projection operates exclusively on pairwise Euclidean distances between
/// the exact normalized, weighted DSP vectors used by the optimizer. Metric MDS
/// consumes that shared geometry without introducing a second preprocessing
/// pipeline.
pub const FEATURE_PROJECTION_ALGORITHM: &str = "metric_mds_v1";

const EPSILON: f64 = 1e-12;
const ORTHOGONAL_ITERATIONS: usize = 96;
const SMACOF_ITERATIONS: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureProjection {
    pub algorithm: &'static str,
    pub coordinates: Vec<[f64; 2]>,
}

/// Project DSP summaries into two display dimensions using their exact shared
/// normalized feature geometry. Coordinates preserve point order.
pub fn project_summaries(summaries: &[&DspSummary]) -> Result<FeatureProjection, String> {
    let distances = pairwise_distances(summaries)?;
    Ok(FeatureProjection {
        algorithm: FEATURE_PROJECTION_ALGORITHM,
        coordinates: project_distances(&distances),
    })
}

fn pairwise_distances(summaries: &[&DspSummary]) -> Result<Vec<Vec<f64>>, String> {
    let mut distances = vec![vec![0.0; summaries.len()]; summaries.len()];
    for i in 0..summaries.len() {
        for j in i + 1..summaries.len() {
            let distance = feature_distance(summaries[i], summaries[j])?;
            distances[i][j] = distance;
            distances[j][i] = distance;
        }
    }
    Ok(distances)
}

fn project_distances(distances: &[Vec<f64>]) -> Vec<[f64; 2]> {
    let count = distances.len();
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![[0.0, 0.0]];
    }
    if count == 2 {
        let half_distance = distances[0][1] * 0.5;
        return vec![[-half_distance, 0.0], [half_distance, 0.0]];
    }
    if distances
        .iter()
        .flatten()
        .all(|distance| distance.abs() <= EPSILON)
    {
        return vec![[0.0, 0.0]; count];
    }

    let gram = double_centered_gram(distances);
    let mut coordinates = classical_mds_coordinates(&gram);
    refine_metric_stress(distances, &mut coordinates);
    canonicalize_orientation(&mut coordinates);
    coordinates
}

fn double_centered_gram(distances: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let count = distances.len();
    let mut squared = vec![vec![0.0; count]; count];
    let mut row_means = vec![0.0; count];
    for i in 0..count {
        for j in 0..count {
            let value = distances[i][j] * distances[i][j];
            squared[i][j] = value;
            row_means[i] += value;
        }
        row_means[i] /= count as f64;
    }
    let total_mean = row_means.iter().sum::<f64>() / count as f64;
    let mut gram = vec![vec![0.0; count]; count];
    for i in 0..count {
        for j in 0..count {
            gram[i][j] = -0.5 * (squared[i][j] - row_means[i] - row_means[j] + total_mean);
        }
    }
    gram
}

fn classical_mds_coordinates(gram: &[Vec<f64>]) -> Vec<[f64; 2]> {
    let count = gram.len();
    let shift = gram
        .iter()
        .enumerate()
        .map(|(i, row)| {
            row.iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, value)| value.abs())
                .sum::<f64>()
                - row[i]
        })
        .fold(0.0_f64, f64::max)
        .max(0.0)
        + EPSILON;

    let mut first = deterministic_vector(count, 0x9e37_79b9_7f4a_7c15);
    let mut second = deterministic_vector(count, 0xd1b5_4a32_d192_ed03);
    center(&mut first);
    normalize_or_basis(&mut first, 0);
    remove_component(&mut second, &first);
    center(&mut second);
    normalize_or_zero(&mut second);

    for _ in 0..ORTHOGONAL_ITERATIONS {
        let mut next_first = shifted_multiply(gram, &first, shift);
        let mut next_second = shifted_multiply(gram, &second, shift);
        center(&mut next_first);
        normalize_or_basis(&mut next_first, 0);
        remove_component(&mut next_second, &next_first);
        center(&mut next_second);
        normalize_or_zero(&mut next_second);
        first = next_first;
        second = next_second;
    }

    let gram_first = multiply(gram, &first);
    let gram_second = multiply(gram, &second);
    let a = dot(&first, &gram_first);
    let b = dot(&first, &gram_second);
    let c = dot(&second, &gram_second);
    let midpoint = (a + c) * 0.5;
    let radius = (((a - c) * 0.5).powi(2) + b * b).sqrt();
    let eigenvalues = [midpoint + radius, midpoint - radius];
    let high_rotation = eigenvector_2x2(a, b, c, eigenvalues[0]);
    let low_rotation = [-high_rotation[1], high_rotation[0]];
    let rotations = [high_rotation, low_rotation];

    (0..count)
        .map(|index| {
            let mut point = [0.0; 2];
            for axis in 0..2 {
                let component =
                    first[index] * rotations[axis][0] + second[index] * rotations[axis][1];
                point[axis] = component * eigenvalues[axis].max(0.0).sqrt();
            }
            point
        })
        .collect()
}

fn refine_metric_stress(distances: &[Vec<f64>], coordinates: &mut Vec<[f64; 2]>) {
    let count = coordinates.len();
    if count <= 1 {
        return;
    }

    for _ in 0..SMACOF_ITERATIONS {
        let mut next = vec![[0.0; 2]; count];
        for i in 0..count {
            for j in i + 1..count {
                let dx = coordinates[i][0] - coordinates[j][0];
                let dy = coordinates[i][1] - coordinates[j][1];
                let projected_distance = (dx * dx + dy * dy).sqrt();
                if projected_distance <= EPSILON || distances[i][j] <= EPSILON {
                    continue;
                }
                let factor = distances[i][j] / projected_distance;
                next[i][0] += factor * dx;
                next[i][1] += factor * dy;
                next[j][0] -= factor * dx;
                next[j][1] -= factor * dy;
            }
        }
        let divisor = count as f64;
        for point in &mut next {
            point[0] /= divisor;
            point[1] /= divisor;
        }
        if next
            .iter()
            .flatten()
            .all(|coordinate| coordinate.is_finite())
        {
            *coordinates = next;
        } else {
            break;
        }
    }
}

fn canonicalize_orientation(coordinates: &mut [[f64; 2]]) {
    if coordinates.is_empty() {
        return;
    }
    for axis in 0..2 {
        let anchor = coordinates
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a[axis].abs().total_cmp(&b[axis].abs()))
            .map(|(_, point)| point[axis])
            .unwrap_or(0.0);
        if anchor < 0.0 {
            for point in coordinates.iter_mut() {
                point[axis] = -point[axis];
            }
        }
    }
}

fn deterministic_vector(count: usize, mut state: u64) -> Vec<f64> {
    (0..count)
        .map(|_| {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let bits = state.wrapping_mul(0x2545_f491_4f6c_dd1d);
            bits as f64 / u64::MAX as f64 - 0.5
        })
        .collect()
}

fn multiply(matrix: &[Vec<f64>], vector: &[f64]) -> Vec<f64> {
    matrix
        .iter()
        .map(|row| row.iter().zip(vector).map(|(a, b)| a * b).sum())
        .collect()
}

fn shifted_multiply(matrix: &[Vec<f64>], vector: &[f64], shift: f64) -> Vec<f64> {
    multiply(matrix, vector)
        .into_iter()
        .zip(vector)
        .map(|(value, original)| value + shift * original)
        .collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn center(vector: &mut [f64]) {
    let mean = vector.iter().sum::<f64>() / vector.len() as f64;
    for value in vector {
        *value -= mean;
    }
}

fn remove_component(vector: &mut [f64], basis: &[f64]) {
    let component = dot(vector, basis);
    for (value, basis_value) in vector.iter_mut().zip(basis) {
        *value -= component * basis_value;
    }
}

fn normalize_or_basis(vector: &mut [f64], basis_index: usize) {
    let norm = dot(vector, vector).sqrt();
    if norm > EPSILON {
        for value in vector {
            *value /= norm;
        }
    } else {
        vector.fill(0.0);
        vector[basis_index] = 1.0;
        center(vector);
        let fallback_norm = dot(vector, vector).sqrt();
        if fallback_norm > EPSILON {
            for value in vector {
                *value /= fallback_norm;
            }
        }
    }
}

fn normalize_or_zero(vector: &mut [f64]) {
    let norm = dot(vector, vector).sqrt();
    if norm > EPSILON {
        for value in vector {
            *value /= norm;
        }
    } else {
        vector.fill(0.0);
    }
}

fn eigenvector_2x2(a: f64, b: f64, c: f64, eigenvalue: f64) -> [f64; 2] {
    let candidate = if b.abs() > EPSILON {
        [b, eigenvalue - a]
    } else if a >= c {
        [1.0, 0.0]
    } else {
        [0.0, 1.0]
    };
    let norm = (candidate[0] * candidate[0] + candidate[1] * candidate[1]).sqrt();
    [candidate[0] / norm, candidate[1] / norm]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(value: f64) -> DspSummary {
        let mut chroma = [0.0; 12];
        chroma[(value.round() as usize) % 12] = 1.0;
        DspSummary {
            rms_db: -60.0 + value,
            spectral_centroid: value / 12.0,
            spectral_rolloff: value / 12.0,
            spectral_flatness: value / 12.0,
            spectral_flux: value / 12.0,
            zero_crossing_rate: value / 12.0,
            onset_density: value / 2.0,
            chroma,
        }
    }

    #[test]
    fn handles_empty_single_and_constant_inputs() {
        assert!(project_summaries(&[]).unwrap().coordinates.is_empty());
        let one = summary(1.0);
        assert_eq!(
            project_summaries(&[&one]).unwrap().coordinates,
            vec![[0.0, 0.0]]
        );
        assert_eq!(
            project_summaries(&[&one, &one, &one]).unwrap().coordinates,
            vec![[0.0, 0.0]; 3]
        );
    }

    #[test]
    fn two_points_retain_their_exact_shared_metric_distance() {
        let a = summary(1.0);
        let b = summary(7.0);
        let projection = project_summaries(&[&a, &b]).unwrap();
        let projected_distance =
            (projection.coordinates[1][0] - projection.coordinates[0][0]).abs();
        assert_eq!(projected_distance, feature_distance(&a, &b).unwrap());
        assert_eq!(projection.coordinates[0][1], 0.0);
        assert_eq!(projection.coordinates[1][1], 0.0);
    }

    #[test]
    fn projection_is_deterministic_finite_and_preserves_point_order() {
        let values = (0..8)
            .map(|value| summary(value as f64))
            .collect::<Vec<_>>();
        let references = values.iter().collect::<Vec<_>>();
        let first = project_summaries(&references).unwrap();
        let second = project_summaries(&references).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.coordinates.len(), values.len());
        assert!(first
            .coordinates
            .iter()
            .flatten()
            .all(|coordinate| coordinate.is_finite()));
        assert_ne!(first.coordinates[0], first.coordinates[7]);
    }

    #[test]
    fn pairwise_geometry_comes_from_the_optimizer_summary_metric() {
        let a = summary(1.0);
        let b = summary(4.0);
        let c = summary(9.0);
        let summaries = [&a, &b, &c];
        let distances = pairwise_distances(&summaries).unwrap();
        for i in 0..summaries.len() {
            for j in 0..summaries.len() {
                assert_eq!(
                    distances[i][j],
                    feature_distance(summaries[i], summaries[j]).unwrap()
                );
            }
        }
    }
}
