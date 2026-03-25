//! Shared types.

use ndarray::Array2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub source_url: String,
    pub duration_secs: Option<f64>,
}

/// Per-frame embeddings for a track, plus precomputed head/tail/global summaries.
pub struct TrackEmbedding {
    /// Full frame-level embeddings: (num_frames, embed_dim)
    pub frames: Array2<f32>,
    /// Mean of first N seconds of frames.
    pub head: Vec<f32>,
    /// Mean of last N seconds of frames.
    pub tail: Vec<f32>,
    /// Mean of all frames.
    pub global: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoobConfig {
    /// Weight on transition (tail→head) vs global similarity. 0.0 = all global, 1.0 = all transition.
    #[serde(default = "default_alpha")]
    pub alpha: f64,
    /// Weight on bottleneck vs mean edge in SA objective.
    #[serde(default = "default_beta")]
    pub beta: f64,
    /// Seconds to use for head/tail windows.
    #[serde(default = "default_window_secs")]
    pub window_secs: f64,
    /// Simulated annealing iterations.
    #[serde(default = "default_sa_iters")]
    pub sa_iterations: usize,
}

fn default_alpha() -> f64 { 0.6 }
fn default_beta() -> f64 { 0.3 }
fn default_window_secs() -> f64 { 8.0 }
fn default_sa_iters() -> usize { 500_000 }

impl Default for LoobConfig {
    fn default() -> Self {
        Self {
            alpha: default_alpha(),
            beta: default_beta(),
            window_secs: default_window_secs(),
            sa_iterations: default_sa_iters(),
        }
    }
}
