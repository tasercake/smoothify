//! YouTube playlist interaction and audio downloading via yt-dlp.

pub mod download;
pub mod playlist;

pub use download::Downloader;
pub use playlist::PlaylistInfo;
