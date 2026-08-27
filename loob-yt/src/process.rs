use crate::{PlaylistInfo, UnavailabilityReason, VideoInfo, YtError};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub trait YtDlpBackend: Send + Sync {
    fn fetch_playlist(&self, url: &str) -> Result<PlaylistInfo, YtError>;
    fn download_wav(
        &self,
        video: &VideoInfo,
        staging_dir: &Path,
        archive: Option<&Path>,
    ) -> Result<PathBuf, YtError>;
}

#[derive(Debug, Clone, Default)]
pub struct RealYtDlp;

impl YtDlpBackend for RealYtDlp {
    fn fetch_playlist(&self, url: &str) -> Result<PlaylistInfo, YtError> {
        let output = Command::new("yt-dlp")
            .args([
                "--flat-playlist",
                "--dump-single-json",
                "--no-warnings",
                url,
            ])
            .output()
            .map_err(map_spawn)?;
        if !output.status.success() {
            return Err(YtError::YtDlpFailed(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        let value: Value = serde_json::from_slice(&output.stdout)?;
        let entries = value["entries"]
            .as_array()
            .ok_or_else(|| YtError::InvalidData("playlist has no entries".into()))?;
        let videos = entries
            .iter()
            .map(|entry| {
                let id = entry["id"].as_str().unwrap_or_default();
                if id.is_empty() {
                    return Err(YtError::InvalidData(
                        "playlist entry has no video id".into(),
                    ));
                }
                Ok(VideoInfo {
                    id: id.to_string(),
                    title: entry["title"].as_str().unwrap_or("Unknown").to_string(),
                    duration: entry["duration"].as_f64().unwrap_or(0.0),
                    url: format!("https://www.youtube.com/watch?v={id}"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PlaylistInfo {
            pipeline_version: "yt-dlp-flat-playlist-v1".into(),
            canonical_url: url.to_string(),
            requested_url: url.to_string(),
            title: value["title"]
                .as_str()
                .unwrap_or("Untitled playlist")
                .to_string(),
            fetched_unix_seconds: now(),
            videos,
        })
    }

    fn download_wav(
        &self,
        video: &VideoInfo,
        staging_dir: &Path,
        archive: Option<&Path>,
    ) -> Result<PathBuf, YtError> {
        std::fs::create_dir_all(staging_dir)?;
        let template = staging_dir.join(format!("{}.%(ext)s", video.id));
        let mut command = Command::new("yt-dlp");
        command
            .args([
                "-x",
                "--audio-format",
                "wav",
                "--audio-quality",
                "0",
                "--continue",
                "--no-overwrites",
                "--no-warnings",
                "-o",
            ])
            .arg(&template);
        if let Some(archive) = archive {
            command.arg("--download-archive").arg(archive);
        }
        let output = command.arg(&video.url).output().map_err(map_spawn)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if let Some(reason) = classify_video_unavailability(&stderr, &video.id) {
                return Err(YtError::VideoUnavailable {
                    video_id: video.id.clone(),
                    reason,
                    was_cached: false,
                });
            }
            return Err(YtError::YtDlpFailed(stderr));
        }
        let expected = staging_dir.join(format!("{}.wav", video.id));
        if !expected.is_file() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            if archive.is_some()
                && stdout.contains("archive")
                && (stdout.contains("already") || stdout.contains("recorded"))
            {
                return Err(YtError::ArchiveSkippedMissing(video.id.clone()));
            }
            return Err(YtError::YtDlpFailed(format!(
                "yt-dlp exited successfully but produced no WAV for {}: {}",
                video.id,
                String::from_utf8_lossy(&output.stdout).trim()
            )));
        }
        Ok(expected)
    }
}

fn classify_video_unavailability(stderr: &str, video_id: &str) -> Option<UnavailabilityReason> {
    let marker = format!("error: [youtube] {}:", video_id.to_ascii_lowercase());
    stderr.lines().find_map(|line| {
        let line = line.trim().to_ascii_lowercase();
        let message = line.strip_prefix(&marker)?.trim();
        if message.contains("private video") || message.contains("video is private") {
            Some(UnavailabilityReason::Private)
        } else if message.contains("video") && message.contains("removed") {
            Some(UnavailabilityReason::Removed)
        } else if message == "video unavailable"
            || message.starts_with("this video is unavailable")
            || message.starts_with("this video is not available")
            || (message.starts_with("video unavailable")
                && (message.contains("this video is unavailable")
                    || message.contains("this video is not available")))
        {
            Some(UnavailabilityReason::Unavailable)
        } else {
            None
        }
    })
}

fn map_spawn(error: std::io::Error) -> YtError {
    if error.kind() == std::io::ErrorKind::NotFound {
        YtError::YtDlpNotFound
    } else {
        YtError::Io(error)
    }
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::classify_video_unavailability;
    use crate::UnavailabilityReason;

    #[test]
    fn classifies_conservative_per_video_unavailability_messages() {
        let cases = [
            (
                "ERROR: [youtube] GANhRMjf7OY: Video unavailable. This video is not available",
                UnavailabilityReason::Unavailable,
            ),
            (
                "ERROR: [youtube] GANhRMjf7OY: Private video. Sign in if you've been granted access",
                UnavailabilityReason::Private,
            ),
            (
                "ERROR: [youtube] GANhRMjf7OY: Video unavailable. This video has been removed by the uploader",
                UnavailabilityReason::Removed,
            ),
        ];
        for (message, expected) in cases {
            assert_eq!(
                classify_video_unavailability(message, "GANhRMjf7OY"),
                Some(expected)
            );
        }
    }

    #[test]
    fn does_not_misclassify_systemic_or_unrelated_errors() {
        let cases = [
            "ERROR: [youtube] GANhRMjf7OY: Unable to download API page: connection timed out",
            "ERROR: [youtube] GANhRMjf7OY: Sign in to confirm you're not a bot",
            "ERROR: [youtube] GANhRMjf7OY: Video unavailable. Sign in to confirm your age",
            "ERROR: [youtube] OTHER: Video unavailable. This video is not available",
            "malformed output mentioning video unavailable",
        ];
        for message in cases {
            assert_eq!(classify_video_unavailability(message, "GANhRMjf7OY"), None);
        }
    }
}
