//! Download audio from YouTube videos via yt-dlp.

use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::info;

pub struct Downloader {
    output_dir: PathBuf,
}

impl Downloader {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self { output_dir: output_dir.into() }
    }

    /// Download audio for a video ID, returning the path to the wav file.
    pub async fn download_audio(&self, video_id: &str) -> Result<PathBuf> {
        let output_template = self.output_dir.join(format!("{video_id}.%(ext)s"));
        let output_path = self.output_dir.join(format!("{video_id}.wav"));

        if output_path.exists() {
            info!(%video_id, "audio already downloaded, skipping");
            return Ok(output_path);
        }

        let url = format!("https://www.youtube.com/watch?v={video_id}");

        Command::new("yt-dlp")
            .args([
                "-x",
                "--audio-format", "wav",
                "--audio-quality", "0",
                "--no-playlist",
                "-o", &output_template.to_string_lossy(),
                &url,
            ])
            .output()
            .await?;

        anyhow::ensure!(output_path.exists(), "download failed for {video_id}");
        Ok(output_path)
    }
}
