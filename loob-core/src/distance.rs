use crate::{DspSummary, TrackAnalysis};

fn chroma_distance(a: &[f64; 12], b: &[f64; 12]) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if na == 0.0 && nb == 0.0 {
        0.0
    } else if na == 0.0 || nb == 0.0 {
        1.0
    } else {
        (1.0 - dot / (na * nb)).clamp(0.0, 2.0)
    }
}

fn summary_distance(a: &DspSummary, b: &DspSummary) -> f64 {
    let loudness = ((a.rms_db - b.rms_db).abs() / 60.0).min(1.0);
    let centroid = (a.spectral_centroid - b.spectral_centroid).abs();
    let rolloff = (a.spectral_rolloff - b.spectral_rolloff).abs();
    let flatness = (a.spectral_flatness - b.spectral_flatness).abs();
    let flux = (a.spectral_flux - b.spectral_flux).abs().min(1.0);
    let zcr = (a.zero_crossing_rate - b.zero_crossing_rate).abs();
    let onset = ((a.onset_density - b.onset_density).abs() / 8.0).min(1.0);
    let chroma = chroma_distance(&a.chroma, &b.chroma);

    0.24 * loudness
        + 0.16 * centroid
        + 0.10 * rolloff
        + 0.08 * flatness
        + 0.12 * flux
        + 0.05 * zcr
        + 0.10 * onset
        + 0.15 * chroma
}

/// Directional cost from the outro of A to the intro of B, plus a lower-weight
/// whole-track similarity term.
pub fn directed_transition_cost(
    a: &TrackAnalysis,
    b: &TrackAnalysis,
    whole_track_weight: f64,
) -> f64 {
    let endpoint = summary_distance(&a.outro, &b.intro);
    let whole = summary_distance(&a.whole, &b.whole);
    (1.0 - whole_track_weight) * endpoint + whole_track_weight * whole
}

pub fn distance_matrix(tracks: &[TrackAnalysis], whole_track_weight: f64) -> Vec<Vec<f64>> {
    let n = tracks.len();
    let mut matrix = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i != j {
                matrix[i][j] = directed_transition_cost(&tracks[i], &tracks[j], whole_track_weight);
            }
        }
    }
    matrix
}
