use crate::config::{Config, OptimizerChoice};
use crate::distance::asymmetric_distance;
use crate::embedding::{EmbeddingProvider, embed_tracks};
use crate::track::Track;
use crate::LoobError;
use loob_optim::{AnnealingObjective, GreedyNn, Optimizer, SimulatedAnnealing};
use loob_yt::AudioFormat;
use std::path::Path;

pub async fn smooth(playlist_url: &str, config: &Config, provider: &dyn EmbeddingProvider, download_dir: &Path) -> Result<Vec<Track>, LoobError> {
    let playlist = loob_yt::fetch_playlist(playlist_url).await?;
    if playlist.videos.is_empty() { return Err(LoobError::EmptyPlaylist); }
    eprintln!("Fetched [{}] - {} tracks", playlist.title, playlist.videos.len());
    for video in &playlist.videos {
        eprintln!("Downloading: {}", video.title);
        loob_yt::download_audio(&video.url, &video.id, download_dir, AudioFormat::Wav).await?;
    }
    let tracks = embed_tracks(provider, &playlist.videos, download_dir, "wav", config.head_seconds, config.tail_seconds).map_err(LoobError::Embedding)?;
    let n = tracks.len();
    let mut dist = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i != j {
                dist[i][j] = asymmetric_distance(&tracks[i].tail_embedding, &tracks[j].head_embedding, &tracks[i].global_embedding, &tracks[j].global_embedding, config.alpha);
            }
        }
    }
    let ordering = match config.optimizer {
        OptimizerChoice::GreedyNn => GreedyNn.optimize(&dist)?,
        OptimizerChoice::SimulatedAnnealing => SimulatedAnnealing { objective: AnnealingObjective::Hybrid { beta: config.beta }, ..Default::default() }.optimize(&dist)?,
    };
    Ok(ordering.into_iter().map(|i| tracks[i].clone()).collect())
}
