use crate::YtError;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum AudioFormat { Mp3, Wav }

impl AudioFormat {
    pub fn ext(&self) -> &str {
        match self { Self::Mp3 => "mp3", Self::Wav => "wav" }
    }
}

pub async fn download_audio(video_url: &str, video_id: &str, output_dir: &Path, format: AudioFormat) -> Result<PathBuf, YtError> {
    tokio::fs::create_dir_all(output_dir).await?;
    let output_template = output_dir.join(format!("{}.%(ext)s", video_id)).to_string_lossy().to_string();

    let status = tokio::process::Command::new("yt-dlp")
        .args(["-x", "--audio-format", format.ext(), "--audio-quality", "0", "-o", &output_template, "--no-warnings", video_url])
        .status()
        .await
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { YtError::YtDlpNotFound } else { YtError::Io(e) })?;

    if !status.success() {
        return Err(YtError::YtDlpFailed(format!("download failed for {}", video_id)));
    }
    Ok(output_dir.join(format!("{}.{}", video_id, format.ext())))
}
