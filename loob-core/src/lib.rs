mod analysis;
mod cache;
mod config;
mod distance;
mod pipeline;
mod track;

pub use analysis::{
    analysis_fingerprint, analyze_audio, analyze_audio_with_hash, hash_file,
    ANALYSIS_PIPELINE_VERSION, CHUNK_OVERLAP_SECONDS, CHUNK_SECONDS,
};
pub use cache::{CacheStatus, FeatureCache};
pub use config::{Config, Objective};
pub use distance::{directed_transition_cost, distance_matrix};
pub use pipeline::{smooth_audio_inputs, smooth_local_files, AudioInput, Progress};
pub use track::{DspChunk, DspSummary, SmoothResult, Track, TrackAnalysis};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoobError {
    #[error("no audio files were selected")]
    EmptySelection,
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("audio analysis failed for {path}: {message}")]
    Analysis { path: String, message: String },
    #[error("cache error: {0}")]
    Cache(String),
    #[error("optimization: {0}")]
    Optim(#[from] loob_optim::OptimError),
}
