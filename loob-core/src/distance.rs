use crate::{Config, DspChunk, DspSummary, JetConfig, TrackAnalysis};

const FEATURE_DIM: usize = 19;
const CONDITION_LIMIT: f64 = 1.0e10;
const SOLVE_EPSILON: f64 = 1.0e-12;

type FeatureVector = [f64; FEATURE_DIM];

#[derive(Debug, Clone)]
struct LocalJet {
    position: FeatureVector,
    velocity: FeatureVector,
    acceleration: FeatureVector,
    order: u8,
}

#[derive(Debug, Clone)]
struct PreparedTrack {
    head: LocalJet,
    tail: LocalJet,
    whole: FeatureVector,
}

#[derive(Debug, Clone)]
struct RegressionSample {
    time: f64,
    weight: f64,
    features: FeatureVector,
}

/// Directed transition cost based on fitted boundary position, velocity, and
/// acceleration. The source tail is projected to the configured time after
/// the seam before it is compared with the destination head.
pub fn directed_transition_cost(
    a: &TrackAnalysis,
    b: &TrackAnalysis,
    config: &Config,
) -> Result<f64, String> {
    config.validate()?;
    let a = prepare_track(a, &config.jet)?;
    let b = prepare_track(b, &config.jet)?;
    Ok(prepared_transition_cost(&a, &b, config))
}

/// Precompute the two boundary jets for every track and then build the full
/// directed edge matrix in O(track_count^2 * feature_count).
pub fn distance_matrix(tracks: &[TrackAnalysis], config: &Config) -> Result<Vec<Vec<f64>>, String> {
    config.validate()?;
    let prepared = tracks
        .iter()
        .map(|track| prepare_track(track, &config.jet))
        .collect::<Result<Vec<_>, _>>()?;
    let n = prepared.len();
    let mut matrix = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i != j {
                matrix[i][j] = prepared_transition_cost(&prepared[i], &prepared[j], config);
            }
        }
    }
    Ok(matrix)
}

fn prepare_track(track: &TrackAnalysis, config: &JetConfig) -> Result<PreparedTrack, String> {
    if !track.has_valid_chunk_timeline() {
        return Err("track analysis has an invalid or incomplete chunk timeline".into());
    }
    Ok(PreparedTrack {
        head: fit_boundary(
            &track.head_chunks,
            track.duration_seconds,
            Boundary::Head,
            config,
        )?,
        tail: fit_boundary(
            &track.tail_chunks,
            track.duration_seconds,
            Boundary::Tail,
            config,
        )?,
        whole: feature_vector(&track.whole)?,
    })
}

#[derive(Debug, Clone, Copy)]
enum Boundary {
    Head,
    Tail,
}

fn fit_boundary(
    chunks: &[DspChunk],
    duration_seconds: f64,
    boundary: Boundary,
    config: &JetConfig,
) -> Result<LocalJet, String> {
    let count = chunks.len().min(config.samples);
    let selected = match boundary {
        Boundary::Head => chunks.iter().take(count).collect::<Vec<_>>(),
        Boundary::Tail => chunks.iter().rev().take(count).collect::<Vec<_>>(),
    };
    let samples = selected
        .into_iter()
        .enumerate()
        .map(|(rank, chunk)| {
            let midpoint = (chunk.start_seconds + chunk.end_seconds) * 0.5;
            let time = match boundary {
                Boundary::Head => midpoint,
                Boundary::Tail => midpoint - duration_seconds,
            };
            Ok(RegressionSample {
                time,
                weight: config.seam_weight_decay.powi(rank as i32),
                features: feature_vector(&chunk.summary)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    fit_local_jet(&samples, config.max_order)
}

fn fit_local_jet(samples: &[RegressionSample], max_order: u8) -> Result<LocalJet, String> {
    if samples.is_empty() {
        return Err("cannot fit a boundary jet without samples".into());
    }
    for order in (0..=max_order.min(2)).rev() {
        if samples.len() < order as usize + 1 {
            continue;
        }
        if let Some(coefficients) = solve_weighted_fit(samples, order) {
            return Ok(LocalJet {
                position: coefficients[0],
                velocity: coefficients.get(1).copied().unwrap_or([0.0; FEATURE_DIM]),
                acceleration: coefficients.get(2).copied().unwrap_or([0.0; FEATURE_DIM]),
                order,
            });
        }
    }
    Err("boundary samples could not be fit even as a constant".into())
}

fn solve_weighted_fit(samples: &[RegressionSample], order: u8) -> Option<Vec<FeatureVector>> {
    let columns = order as usize + 1;
    let mut normal = vec![vec![0.0; columns]; columns];
    let mut rhs = vec![[0.0; FEATURE_DIM]; columns];
    for sample in samples {
        if !sample.time.is_finite()
            || !sample.weight.is_finite()
            || sample.weight <= 0.0
            || sample.features.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let basis = [1.0, sample.time, 0.5 * sample.time * sample.time];
        for row in 0..columns {
            for column in 0..columns {
                normal[row][column] += sample.weight * basis[row] * basis[column];
            }
            for (feature, value) in rhs[row].iter_mut().enumerate() {
                *value += sample.weight * basis[row] * sample.features[feature];
            }
        }
    }
    solve_normal_system(normal, rhs)
}

fn solve_normal_system(
    mut matrix: Vec<Vec<f64>>,
    mut rhs: Vec<FeatureVector>,
) -> Option<Vec<FeatureVector>> {
    let size = matrix.len();
    let matrix_scale = matrix
        .iter()
        .flat_map(|row| row.iter())
        .map(|value| value.abs())
        .fold(0.0, f64::max);
    if !matrix_scale.is_finite() || matrix_scale <= 0.0 {
        return None;
    }
    let mut largest_pivot: f64 = 0.0;
    let mut smallest_pivot = f64::INFINITY;
    for column in 0..size {
        let pivot_row = (column..size)
            .max_by(|&a, &b| matrix[a][column].abs().total_cmp(&matrix[b][column].abs()))?;
        let pivot = matrix[pivot_row][column].abs();
        if !pivot.is_finite() || pivot <= SOLVE_EPSILON * matrix_scale {
            return None;
        }
        matrix.swap(column, pivot_row);
        rhs.swap(column, pivot_row);
        largest_pivot = largest_pivot.max(pivot);
        smallest_pivot = smallest_pivot.min(pivot);

        let divisor = matrix[column][column];
        for value in &mut matrix[column][column..] {
            *value /= divisor;
        }
        for value in &mut rhs[column] {
            *value /= divisor;
        }
        let pivot_matrix_row = matrix[column].clone();
        let pivot_rhs = rhs[column];
        for row in 0..size {
            if row == column {
                continue;
            }
            let factor = matrix[row][column];
            for (target, pivot_value) in matrix[row][column..]
                .iter_mut()
                .zip(&pivot_matrix_row[column..])
            {
                *target -= factor * pivot_value;
            }
            for (target, pivot_value) in rhs[row].iter_mut().zip(pivot_rhs) {
                *target -= factor * pivot_value;
            }
        }
    }
    if largest_pivot / smallest_pivot > CONDITION_LIMIT
        || rhs
            .iter()
            .flat_map(|row| row.iter())
            .any(|value| !value.is_finite())
    {
        return None;
    }
    Some(rhs)
}

fn prepared_transition_cost(a: &PreparedTrack, b: &PreparedTrack, config: &Config) -> f64 {
    let delta = config.jet.delta_seconds;
    let projected_position = std::array::from_fn(|index| {
        a.tail.position[index]
            + a.tail.velocity[index] * delta
            + 0.5 * a.tail.acceleration[index] * delta * delta
    });
    let projected_velocity =
        std::array::from_fn(|index| a.tail.velocity[index] + a.tail.acceleration[index] * delta);

    let mut endpoint =
        config.jet.lambda_position * squared_distance(&projected_position, &b.head.position);
    let mut active_weight = config.jet.lambda_position;
    if a.tail.order >= 1 && b.head.order >= 1 {
        endpoint +=
            config.jet.lambda_velocity * squared_distance(&projected_velocity, &b.head.velocity);
        active_weight += config.jet.lambda_velocity;
    }
    if a.tail.order >= 2 && b.head.order >= 2 {
        endpoint += config.jet.lambda_acceleration
            * squared_distance(&a.tail.acceleration, &b.head.acceleration);
        active_weight += config.jet.lambda_acceleration;
    }
    endpoint /= active_weight;

    let whole = squared_distance(&a.whole, &b.whole);
    (1.0 - config.whole_track_weight) * endpoint + config.whole_track_weight * whole
}

fn squared_distance(a: &FeatureVector, b: &FeatureVector) -> f64 {
    a.iter()
        .zip(b)
        .map(|(left, right)| (left - right).powi(2))
        .sum()
}

/// Euclidean distance between two summaries after applying the exact feature
/// normalization and family weighting used by the transition optimizer.
/// Squaring this value yields the position/whole-track geometry used in the
/// optimizer's objective.
pub(crate) fn feature_distance(a: &DspSummary, b: &DspSummary) -> Result<f64, String> {
    Ok(squared_distance(&feature_vector(a)?, &feature_vector(b)?).sqrt())
}

/// Convert heterogeneous DSP values into a fixed, dimensionless linear
/// feature space. The square-root weights make squared Euclidean distance
/// preserve the previous feature-family weighting; normalized chroma makes
/// its contribution equivalent to weighted cosine distance.
fn feature_vector(summary: &DspSummary) -> Result<FeatureVector, String> {
    let raw_scalars = [
        summary.rms_db,
        summary.spectral_centroid,
        summary.spectral_rolloff,
        summary.spectral_flatness,
        summary.spectral_flux,
        summary.zero_crossing_rate,
        summary.onset_density,
    ];
    if raw_scalars.iter().any(|value| !value.is_finite())
        || summary.chroma.iter().any(|value| !value.is_finite())
    {
        return Err("DSP summary contains a non-finite feature".into());
    }
    let scalar_values = [
        ((summary.rms_db + 80.0) / 80.0).clamp(0.0, 1.0),
        summary.spectral_centroid.clamp(0.0, 1.0),
        summary.spectral_rolloff.clamp(0.0, 1.0),
        summary.spectral_flatness.clamp(0.0, 1.0),
        summary.spectral_flux.clamp(0.0, 1.0),
        summary.zero_crossing_rate.clamp(0.0, 1.0),
        (summary.onset_density / 8.0).clamp(0.0, 1.0),
    ];
    let scalar_weights = [0.24_f64, 0.16, 0.10, 0.08, 0.12, 0.05, 0.10];
    let mut vector = [0.0; FEATURE_DIM];
    for index in 0..scalar_values.len() {
        vector[index] = scalar_values[index] * scalar_weights[index].sqrt();
    }

    let chroma_norm = summary
        .chroma
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if chroma_norm > 0.0 {
        let chroma_scale = (0.15_f64 / 2.0).sqrt();
        for index in 0..12 {
            vector[7 + index] = summary.chroma[index] / chroma_norm * chroma_scale;
        }
    }
    if vector.iter().any(|value| !value.is_finite()) {
        return Err("DSP summary contains a non-finite feature".into());
    }
    Ok(vector)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_quadratic_fit_recovers_the_boundary_jet() {
        let position = 0.25;
        let velocity = -0.4;
        let acceleration = 0.12;
        let samples = [-4.0_f64, -3.0, -2.0, -1.0]
            .into_iter()
            .enumerate()
            .map(|(rank, time)| {
                let mut features = [0.0; FEATURE_DIM];
                features[0] = position + velocity * time + 0.5 * acceleration * time * time;
                RegressionSample {
                    time,
                    weight: 0.85_f64.powi(rank as i32),
                    features,
                }
            })
            .collect::<Vec<_>>();

        let jet = fit_local_jet(&samples, 2).unwrap();
        assert_eq!(jet.order, 2);
        assert!((jet.position[0] - position).abs() < 1e-10);
        assert!((jet.velocity[0] - velocity).abs() < 1e-10);
        assert!((jet.acceleration[0] - acceleration).abs() < 1e-10);
    }

    #[test]
    fn duplicate_timestamps_fall_back_to_position_only() {
        let samples = [0.2, 0.4, 0.8]
            .into_iter()
            .map(|value| {
                let mut features = [0.0; FEATURE_DIM];
                features[0] = value;
                RegressionSample {
                    time: 1.0,
                    weight: 1.0,
                    features,
                }
            })
            .collect::<Vec<_>>();

        let jet = fit_local_jet(&samples, 2).unwrap();
        assert_eq!(jet.order, 0);
        assert!((jet.position[0] - (1.4 / 3.0)).abs() < 1e-12);
    }

    #[test]
    fn non_finite_chroma_is_rejected_before_normalization() {
        let mut summary = DspSummary {
            rms_db: -40.0,
            spectral_centroid: 0.2,
            spectral_rolloff: 0.3,
            spectral_flatness: 0.1,
            spectral_flux: 0.05,
            zero_crossing_rate: 0.02,
            onset_density: 1.0,
            chroma: [0.0; 12],
        };
        summary.chroma[3] = f64::NAN;
        assert!(feature_vector(&summary).is_err());
    }
}
