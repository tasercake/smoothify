use crate::track::Track;
use std::path::Path;
use rand::Rng;

pub trait EmbeddingProvider {
    fn embed(&self, audio_path: &Path, head_seconds: f64, tail_seconds: f64) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>), String>;
}

pub struct RandomEmbeddingProvider { pub dims: usize }

impl Default for RandomEmbeddingProvider {
    fn default() -> Self { Self { dims: 768 } }
}

impl EmbeddingProvider for RandomEmbeddingProvider {
    fn embed(&self, _audio_path: &Path, _head_seconds: f64, _tail_seconds: f64) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>), String> {
        let mut rng = rand::thread_rng();
        let mut make_vec = || (0..self.dims).map(|_| rng.gen::<f64>()).collect::<Vec<_>>();
        Ok((make_vec(), make_vec(), make_vec()))
    }
}

pub fn embed_tracks(provider: &dyn EmbeddingProvider, videos: &[loob_yt::VideoInfo], audio_dir: &Path, audio_ext: &str, head_seconds: f64, tail_seconds: f64) -> Result<Vec<Track>, String> {
    videos.iter().map(|v| {
        let path = audio_dir.join(format!("{}.{}", v.id, audio_ext));
        let (global, head, tail) = provider.embed(&path, head_seconds, tail_seconds)?;
        Ok(Track { id: v.id.clone(), title: v.title.clone(), global_embedding: global, head_embedding: head, tail_embedding: tail })
    }).collect()
}
