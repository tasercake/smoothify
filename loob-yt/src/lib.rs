mod download;
mod playlist;

pub use download::{AudioFormat, download_audio};
pub use playlist::{PlaylistInfo, VideoInfo, fetch_playlist};

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum YtError {
    #[error("yt-dlp not found")]
    YtDlpNotFound,
    #[error("yt-dlp failed: {0}")]
    YtDlpFailed(String),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub fn default_download_dir() -> PathBuf { PathBuf::from("downloads") }
