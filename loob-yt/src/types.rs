use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub duration: f64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlaylistInfo {
    pub pipeline_version: String,
    /// Stable identity used for cache lookup and fetching.
    pub canonical_url: String,
    /// The request that originally populated this manifest.
    pub requested_url: String,
    pub title: String,
    pub fetched_unix_seconds: u64,
    pub videos: Vec<VideoInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioProvenance {
    pub pipeline_version: String,
    pub video_id: String,
    pub source_url: String,
    pub title: String,
    pub content_sha256: String,
    /// Relative content-addressed path below the cache's audio directory.
    pub object_file: String,
    pub downloaded_unix_seconds: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnavailabilityReason {
    Private,
    Removed,
    Unavailable,
}

impl UnavailabilityReason {
    pub fn user_message(self) -> &'static str {
        match self {
            Self::Private => "Private video",
            Self::Removed => "Removed video",
            Self::Unavailable => "Unavailable video",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnavailableAudioProvenance {
    pub pipeline_version: String,
    pub status: String,
    pub video_id: String,
    pub source_url: String,
    pub title: String,
    pub reason: UnavailabilityReason,
    pub detected_unix_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedAudioTrack {
    pub video_id: String,
    pub title: String,
    pub path: PathBuf,
    pub was_cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedPlaylist {
    pub title: String,
    pub manifest_was_cached: bool,
    pub tracks: Vec<PreparedAudioTrack>,
    pub skipped: Vec<SkippedTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedTrack {
    pub video_id: String,
    pub title: String,
    pub reason: UnavailabilityReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum YoutubeProgress {
    FetchingManifest,
    ManifestReady {
        title: String,
        total: usize,
        was_cached: bool,
    },
    ResolvingAudio {
        current: usize,
        total: usize,
        title: String,
    },
    AudioReady {
        current: usize,
        total: usize,
        title: String,
        was_cached: bool,
    },
    AudioSkipped {
        current: usize,
        total: usize,
        title: String,
        reason: UnavailabilityReason,
        was_cached: bool,
    },
}
