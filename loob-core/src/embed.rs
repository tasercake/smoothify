//! Audio embedding trait — abstraction over MERT or other models.
//!
//! The actual model inference is expected to happen out-of-process
//! (e.g. a Python sidecar running MERT via transformers) to keep
//! the Rust binary lean. This module defines the interface.

use anyhow::Result;
use ndarray::Array2;
use std::path::Path;

/// Trait for audio embedding backends.
#[trait_variant::make(Send)]
pub trait AudioEmbedder {
    /// Embed an audio file, returning per-frame embeddings.
    /// Shape: (num_frames, embed_dim)
    async fn embed(&self, audio_path: &Path) -> Result<Array2<f32>>;

    /// Embedding dimensionality.
    fn embed_dim(&self) -> usize;

    /// Approximate frames per second of audio.
    fn fps(&self) -> f64;
}

// TODO: Implement a concrete embedder that calls a Python sidecar
// running MERT-330M via stdin/stdout JSON protocol.
