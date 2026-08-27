use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DspSummary {
    pub rms_db: f64,
    pub spectral_centroid: f64,
    pub spectral_rolloff: f64,
    pub spectral_flatness: f64,
    pub spectral_flux: f64,
    pub zero_crossing_rate: f64,
    pub onset_density: f64,
    pub chroma: [f64; 12],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackAnalysis {
    pub pipeline_version: String,
    pub analysis_fingerprint: String,
    pub content_sha256: String,
    pub sample_rate: u32,
    pub duration_seconds: f64,
    pub intro: DspSummary,
    pub outro: DspSummary,
    pub whole: DspSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Unique selection identity. Equal-content files selected twice remain distinct.
    pub selection_index: usize,
    pub title: String,
    pub path: PathBuf,
    pub analysis: TrackAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmoothResult {
    pub ordered_tracks: Vec<Track>,
    pub bottleneck_cost: f64,
    pub mean_cost: f64,
    pub distance_matrix: Vec<Vec<f64>>,
}
