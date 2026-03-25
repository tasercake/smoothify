//! Core library that orchestrates downloading, embedding, and optimization.

pub mod embed;
pub mod pipeline;
pub mod types;

pub use pipeline::LoobPipeline;
pub use types::{LoobConfig, Track, TrackEmbedding};
