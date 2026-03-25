//! The main pipeline: playlist → download → embed → optimize → reorder.

use anyhow::Result;
use loob_optim::{AsymmetricCostMatrix, greedy, sa};
use loob_yt::download::{self, AudioFormat};
use loob_yt::playlist;

use crate::embed::AudioEmbedder;
use crate::types::{LoobConfig, Track, TrackEmbedding};

pub struct LoobPipeline {
    config: LoobConfig,
}

impl LoobPipeline {
    pub fn new(config: LoobConfig) -> Self {
        Self { config }
    }

    /// Run the full pipeline on a YouTube playlist URL.
    /// Returns the reordered track list.
    pub async fn run(
        &self,
        playlist_url: &str,
        embedder: &dyn AudioEmbedder,
        download_dir: &std::path::Path,
    ) -> Result<Vec<Track>> {
        // 1. Fetch playlist
        let playlist = playlist::fetch_playlist(playlist_url).await?;
        let tracks: Vec<Track> = playlist.videos.iter().map(|v| Track {
            id: v.id.clone(),
            title: v.title.clone(),
            source_url: v.url.clone(),
            duration_secs: Some(v.duration),
        }).collect();

        // 2. Download audio
        let mut audio_paths = Vec::new();
        for track in &tracks {
            let path = download::download_audio(
                &track.source_url,
                &track.id,
                download_dir,
                AudioFormat::Wav,
            ).await?;
            audio_paths.push(path);
        }

        // 3. Embed
        let mut embeddings = Vec::new();
        for path in &audio_paths {
            let frames = embedder.embed(path).await?;
            let embedding = self.compute_track_embedding(frames, embedder.fps());
            embeddings.push(embedding);
        }

        // 4. Build cost matrix
        let n = tracks.len();
        let cost = AsymmetricCostMatrix::from_fn(n, |i, j| {
            self.transition_cost(&embeddings[i], &embeddings[j])
        });

        // 5. Optimize
        let greedy_result = greedy::solve_all_starts(&cost);
        let sa_config = sa::SaConfig {
            iterations: self.config.sa_iterations,
            beta: self.config.beta,
            ..Default::default()
        };
        let result = sa::solve(&cost, greedy_result.path, &sa_config);

        // 6. Reorder
        let reordered = result.path.into_iter().map(|i| tracks[i].clone()).collect();
        Ok(reordered)
    }

    fn compute_track_embedding(&self, frames: ndarray::Array2<f32>, fps: f64) -> TrackEmbedding {
        let n_frames = frames.nrows();
        let window_frames = (self.config.window_secs * fps).round() as usize;
        let head_end = window_frames.min(n_frames);
        let tail_start = n_frames.saturating_sub(window_frames);

        let head = mean_rows(&frames, 0, head_end);
        let tail = mean_rows(&frames, tail_start, n_frames);
        let global = mean_rows(&frames, 0, n_frames);

        TrackEmbedding { frames, head, tail, global }
    }

    fn transition_cost(&self, from: &TrackEmbedding, to: &TrackEmbedding) -> f64 {
        let transition_dist = cosine_distance(&from.tail, &to.head);
        let global_dist = cosine_distance(&from.global, &to.global);
        self.config.alpha * transition_dist + (1.0 - self.config.alpha) * global_dist
    }
}

fn mean_rows(arr: &ndarray::Array2<f32>, start: usize, end: usize) -> Vec<f32> {
    let slice = arr.slice(ndarray::s![start..end, ..]);
    let mean = slice.mean_axis(ndarray::Axis(0)).unwrap();
    mean.to_vec()
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    let sim = if na * nb > 0.0 { dot / (na * nb) } else { 0.0 };
    (1.0 - sim) as f64
}
