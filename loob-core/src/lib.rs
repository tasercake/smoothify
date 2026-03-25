//! Core library that orchestrates downloading, embedding, and optimization.
//!
//! Generic enough to be driven by a CLI, web server, or WASM frontend.

pub mod embed;
pub mod pipeline;
pub mod types;

pub use pipeline::LoobPipeline;
pub use types::{LoobConfig, Track, TrackEmbedding};
