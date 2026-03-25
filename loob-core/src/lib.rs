mod config;
mod distance;
pub mod embedding;
mod smooth;
mod track;

pub use config::{Config, OptimizerChoice};
pub use distance::asymmetric_distance;
pub use embedding::EmbeddingProvider;
pub use smooth::smooth;
pub use track::Track;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoobError {
    #[error("youtube: {0}")]
    Yt(#[from] loob_yt::YtError),
    #[error("optimization: {0}")]
    Optim(#[from] loob_optim::OptimError),
    #[error("embedding: {0}")]
    Embedding(String),
    #[error("empty playlist")]
    EmptyPlaylist,
}
