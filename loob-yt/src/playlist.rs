use crate::YtError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub duration: f64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistInfo {
    pub title: String,
    pub videos: Vec<VideoInfo>,
}

pub async fn fetch_playlist(url: &str) -> Result<PlaylistInfo, YtError> {
    let output = tokio::process::Command::new("yt-dlp")
        .args(["--flat-playlist", "--dump-json", "--no-warnings", url])
        .output()
        .await
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { YtError::YtDlpNotFound } else { YtError::Io(e) })?;

    if !output.status.success() {
        return Err(YtError::YtDlpFailed(String::from_utf8_lossy(&output.stderr).to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut videos = Vec::new();
    let mut playlist_title = String::from("Untitled Playlist");

    for (i, line) in stdout.lines().enumerate() {
        let v: serde_json::Value = serde_json::from_str(line).map_err(|e| YtError::ParseError(e.to_string()))?;
        if i == 0 {
            if let Some(t) = v["playlist_title"].as_str() { playlist_title = t.to_string(); }
        }
        videos.push(VideoInfo {
            id: v["id"].as_str().unwrap_or_default().to_string(),
            title: v["title"].as_str().unwrap_or("Unknown").to_string(),
            duration: v["duration"].as_f64().unwrap_or(0.0),
            url: format!("https://www.youtube.com/watch?v={}", v["id"].as_str().unwrap_or_default()),
        });
    }

    Ok(PlaylistInfo { title: playlist_title, videos })
}
