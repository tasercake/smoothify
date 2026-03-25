//! Fetch playlist metadata from YouTube via yt-dlp --flat-playlist.

use anyhow::Result;
use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug, Clone, Deserialize)]
pub struct PlaylistEntry {
    pub id: String,
    pub title: Option<String>,
    pub url: String,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct PlaylistInfo {
    pub title: String,
    pub entries: Vec<PlaylistEntry>,
}

pub async fn fetch_playlist(url: &str) -> Result<PlaylistInfo> {
    let output = Command::new("yt-dlp")
        .args(["--flat-playlist", "--dump-json", "--no-warnings", url])
        .output()
        .await?;

    let stdout = String::from_utf8(output.stdout)?;
    let mut entries = Vec::new();

    for line in stdout.lines() {
        if let Ok(entry) = serde_json::from_str::<PlaylistEntry>(line) {
            entries.push(entry);
        }
    }

    Ok(PlaylistInfo {
        title: "YouTube Playlist".into(), // TODO: extract from yt-dlp
        entries,
    })
}
