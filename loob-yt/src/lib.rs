mod cache;
mod process;
mod types;

pub use cache::{canonical_playlist_url, CacheOutcome, CachePolicy, YoutubeCache};
pub use process::{RealYtDlp, YtDlpBackend};
pub use types::{
    AudioProvenance, PlaylistInfo, PreparedAudioTrack, PreparedPlaylist, SkippedTrack,
    UnavailabilityReason, UnavailableAudioProvenance, VideoInfo, YoutubeProgress,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum YtError {
    #[error("yt-dlp was not found")]
    YtDlpNotFound,
    #[error("yt-dlp failed: {0}")]
    YtDlpFailed(String),
    #[error("yt-dlp archive skipped {0}, but no cached WAV exists")]
    ArchiveSkippedMissing(String),
    #[error("video {video_id} is unavailable: {reason:?}")]
    VideoUnavailable {
        video_id: String,
        reason: UnavailabilityReason,
        was_cached: bool,
    },
    #[error("invalid playlist or cache data: {0}")]
    InvalidData(String),
    #[error("offline cache miss for {0}; explicitly populate or refresh the cache first")]
    OfflineMiss(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
