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
pub struct DspChunk {
    /// Inclusive beginning of this chunk in the decoded track timeline.
    pub start_seconds: f64,
    /// Exclusive end of this chunk in the decoded track timeline.
    pub end_seconds: f64,
    pub summary: DspSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackAnalysis {
    pub pipeline_version: String,
    pub analysis_fingerprint: String,
    pub content_sha256: String,
    pub sample_rate: u32,
    pub duration_seconds: f64,
    /// Chronological, overlapping feature windows spanning the entire track.
    /// A valid analysis always contains at least one chunk.
    pub chunks: Vec<DspChunk>,
    /// Retained independently to preserve the existing whole-track transition term.
    pub whole: DspSummary,
}

impl TrackAnalysis {
    pub(crate) fn has_valid_chunk_timeline(&self) -> bool {
        if !self.duration_seconds.is_finite()
            || self.duration_seconds <= 0.0
            || self.chunks.is_empty()
        {
            return false;
        }

        let tolerance = 1e-6 * self.duration_seconds.max(1.0);
        if self.chunks[0].start_seconds.abs() > tolerance
            || (self.chunks.last().unwrap().end_seconds - self.duration_seconds).abs() > tolerance
        {
            return false;
        }

        self.chunks.iter().enumerate().all(|(index, chunk)| {
            let bounds_are_valid = chunk.start_seconds.is_finite()
                && chunk.end_seconds.is_finite()
                && chunk.start_seconds >= 0.0
                && chunk.end_seconds > chunk.start_seconds
                && chunk.end_seconds <= self.duration_seconds + tolerance;
            let follows_previous = index == 0
                || (chunk.start_seconds > self.chunks[index - 1].start_seconds
                    && chunk.end_seconds > self.chunks[index - 1].end_seconds);
            bounds_are_valid && follows_previous
        })
    }
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
