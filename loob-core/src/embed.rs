//! Audio embedding trait — abstraction over MERT or other models.

use anyhow::Result;
use ndarray::Array2;
use std::path::Path;

/// Trait for audio embedding backends.
#[async_trait::async_trait]
pub trait AudioEmbedder: Send + Sync {
    /// Embed an audio file, returning per-frame embeddings.
    /// Shape: (num_frames, embed_dim)
    async fn embed(&self, audio_path: &Path) -> Result<Array2<f32>>;

    /// Embedding dimensionality.
    fn embed_dim(&self) -> usize;

    /// Approximate frames per second of audio.
    fn fps(&self) -> f64;
}
