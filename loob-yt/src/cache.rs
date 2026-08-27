use crate::{
    process::now, AudioProvenance, PlaylistInfo, PreparedAudioTrack, PreparedPlaylist,
    SkippedTrack, UnavailabilityReason, UnavailableAudioProvenance, VideoInfo, YoutubeProgress,
    YtDlpBackend, YtError,
};
use atomicwrites::{AllowOverwrite, AtomicFile};
use fs2::FileExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use url::Url;

const AUDIO_PIPELINE_VERSION: &str = "yt-dlp-wav-v1";
const UNAVAILABLE_PIPELINE_VERSION: &str = "yt-dlp-unavailability-v1";
const MANIFEST_PIPELINE_VERSION: &str = "yt-dlp-flat-playlist-v1";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    Offline,
    Populate,
    Refresh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheOutcome<T> {
    pub value: T,
    pub was_cached: bool,
}

pub struct YoutubeCache<B> {
    root: PathBuf,
    backend: B,
}

impl<B: YtDlpBackend> YoutubeCache<B> {
    pub fn new(root: impl Into<PathBuf>, backend: B) -> Self {
        Self {
            root: root.into(),
            backend,
        }
    }

    pub fn playlist(
        &self,
        url: &str,
        policy: CachePolicy,
    ) -> Result<CacheOutcome<PlaylistInfo>, YtError> {
        let identity = PlaylistIdentity::from_request(url)?;
        let key = sha256_bytes(identity.canonical_url.as_bytes());
        let target = self.root.join("manifests").join(format!("{key}.json"));
        let lock = self.lock(&format!("manifest-{key}"))?;
        if policy != CachePolicy::Refresh {
            if let Some(value) = read_manifest(&target, &identity.canonical_url) {
                let _ = lock.unlock();
                return Ok(CacheOutcome {
                    value,
                    was_cached: true,
                });
            }
        }
        if policy == CachePolicy::Offline {
            let _ = lock.unlock();
            return Err(YtError::OfflineMiss(format!("playlist manifest {url}")));
        }
        let mut value = self.backend.fetch_playlist(&identity.fetch_url)?;
        value.canonical_url = identity.canonical_url;
        value.requested_url = url.to_string();
        validate_manifest(&value, &value.canonical_url)?;
        atomic_write_json(&target, &value)?;
        let _ = lock.unlock();
        Ok(CacheOutcome {
            value,
            was_cached: false,
        })
    }

    /// Resolve a playlist manifest and all of its WAVs through the validated
    /// layered cache. `Populate` permits only missing entries to access the
    /// backend; `Offline` guarantees cache-only behavior.
    pub fn prepare_playlist_audio(
        &self,
        url: &str,
        policy: CachePolicy,
        mut progress: impl FnMut(YoutubeProgress),
    ) -> Result<PreparedPlaylist, YtError> {
        canonical_playlist_url(url)?;
        progress(YoutubeProgress::FetchingManifest);
        let manifest = self.playlist(url, policy)?;
        let total = manifest.value.videos.len();
        progress(YoutubeProgress::ManifestReady {
            title: manifest.value.title.clone(),
            total,
            was_cached: manifest.was_cached,
        });
        let mut tracks = Vec::with_capacity(total);
        let mut skipped = Vec::new();
        for (index, video) in manifest.value.videos.iter().enumerate() {
            progress(YoutubeProgress::ResolvingAudio {
                current: index + 1,
                total,
                title: video.title.clone(),
            });
            match self.audio(video, policy) {
                Ok(audio) => {
                    progress(YoutubeProgress::AudioReady {
                        current: index + 1,
                        total,
                        title: video.title.clone(),
                        was_cached: audio.was_cached,
                    });
                    tracks.push(PreparedAudioTrack {
                        video_id: video.id.clone(),
                        title: video.title.clone(),
                        path: audio.value,
                        was_cached: audio.was_cached,
                    });
                }
                Err(YtError::VideoUnavailable {
                    video_id,
                    reason,
                    was_cached,
                }) => {
                    if video_id != video.id {
                        return Err(YtError::InvalidData(
                            "downloader reported unavailability for the wrong video".into(),
                        ));
                    }
                    progress(YoutubeProgress::AudioSkipped {
                        current: index + 1,
                        total,
                        title: video.title.clone(),
                        reason,
                        was_cached,
                    });
                    skipped.push(SkippedTrack {
                        video_id,
                        title: video.title.clone(),
                        reason,
                    });
                }
                Err(error) => return Err(error),
            }
        }
        Ok(PreparedPlaylist {
            title: manifest.value.title,
            manifest_was_cached: manifest.was_cached,
            tracks,
            skipped,
        })
    }

    pub fn audio(
        &self,
        video: &VideoInfo,
        policy: CachePolicy,
    ) -> Result<CacheOutcome<PathBuf>, YtError> {
        validate_video_id(&video.id)?;
        let audio_root = self.root.join("audio");
        let provenance_path = audio_root.join("refs").join(format!("{}.json", video.id));
        let lock = self.lock(&format!("audio-{}", video.id))?;
        if policy != CachePolicy::Refresh {
            match resolve_audio_reference(&audio_root, &provenance_path, video) {
                Some(CachedAudioReference::Available(path)) => {
                    let _ = lock.unlock();
                    return Ok(CacheOutcome {
                        value: path,
                        was_cached: true,
                    });
                }
                Some(CachedAudioReference::Unavailable(reason)) => {
                    let _ = lock.unlock();
                    return Err(YtError::VideoUnavailable {
                        video_id: video.id.clone(),
                        reason,
                        was_cached: true,
                    });
                }
                None => {}
            }
        }
        if policy == CachePolicy::Offline {
            let _ = lock.unlock();
            return Err(YtError::OfflineMiss(format!("audio for {}", video.id)));
        }

        let staging = self.root.join("staging").join(format!(
            "{}-{}-{}",
            video.id,
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&staging)?;
        let result = (|| {
            // Different video IDs use different locks, so their yt-dlp archive
            // files must also be independent.
            let archive = self.root.join("archives").join(format!("{}.txt", video.id));
            if let Some(parent) = archive.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut downloaded = self.backend.download_wav(video, &staging, Some(&archive));
            if downloaded
                .as_ref()
                .is_err_and(|err| matches!(err, YtError::ArchiveSkippedMissing(_)))
            {
                // A stale yt-dlp archive can skip an invalid/missing local entry. The
                // cache remains authoritative, so retry once without the archive.
                downloaded = self.backend.download_wav(video, &staging, None);
            }
            let downloaded = match downloaded {
                Ok(path) => path,
                Err(YtError::VideoUnavailable {
                    video_id, reason, ..
                }) => {
                    if video_id != video.id {
                        return Err(YtError::InvalidData(
                            "downloader reported unavailability for the wrong video".into(),
                        ));
                    }
                    let unavailable = UnavailableAudioProvenance {
                        pipeline_version: UNAVAILABLE_PIPELINE_VERSION.into(),
                        status: "unavailable".into(),
                        video_id: video.id.clone(),
                        source_url: video.url.clone(),
                        title: video.title.clone(),
                        reason,
                        detected_unix_seconds: now(),
                    };
                    atomic_write_json(&provenance_path, &unavailable)?;
                    return Err(YtError::VideoUnavailable {
                        video_id,
                        reason,
                        was_cached: false,
                    });
                }
                Err(error) => return Err(error),
            };
            let metadata = fs::metadata(&downloaded)?;
            if metadata.len() == 0 {
                return Err(YtError::InvalidData("downloaded audio is empty".into()));
            }
            let content_sha256 = sha256_file(&downloaded)?;
            let object_file = format!("objects/{content_sha256}.wav");
            let object_path = audio_root.join(&object_file);
            let object_lock = self.lock(&format!("object-{content_sha256}"))?;
            if sha256_file(&object_path).is_ok_and(|hash| hash == content_sha256) {
                fs::remove_file(&downloaded)?;
            } else {
                if object_path.exists() {
                    fs::remove_file(&object_path)?;
                }
                if let Some(parent) = object_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&downloaded, &object_path)?;
            }
            let _ = object_lock.unlock();
            let provenance = AudioProvenance {
                pipeline_version: AUDIO_PIPELINE_VERSION.into(),
                video_id: video.id.clone(),
                source_url: video.url.clone(),
                title: video.title.clone(),
                content_sha256,
                object_file,
                downloaded_unix_seconds: now(),
            };
            atomic_write_json(&provenance_path, &provenance)?;
            Ok(CacheOutcome {
                value: object_path,
                was_cached: false,
            })
        })();
        let _ = fs::remove_dir_all(&staging);
        let _ = lock.unlock();
        result
    }

    fn lock(&self, name: &str) -> Result<File, YtError> {
        let dir = self.root.join("locks");
        fs::create_dir_all(&dir)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(format!("{name}.lock")))?;
        lock.lock_exclusive()?;
        Ok(lock)
    }
}

struct PlaylistIdentity {
    canonical_url: String,
    fetch_url: String,
    is_playlist: bool,
}

impl PlaylistIdentity {
    fn from_request(request: &str) -> Result<Self, YtError> {
        let parsed = Url::parse(request)
            .map_err(|error| YtError::InvalidData(format!("invalid YouTube URL: {error}")))?;
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        let is_youtube = host == "youtube.com"
            || host.ends_with(".youtube.com")
            || host == "youtu.be"
            || host.ends_with(".youtu.be");
        let playlist_id = is_youtube
            .then(|| parsed.query_pairs().find(|(key, _)| key == "list"))
            .flatten()
            .map(|(_, value)| value.into_owned());
        if let Some(playlist_id) = playlist_id {
            validate_playlist_id(&playlist_id)?;
            let canonical_url = format!("https://www.youtube.com/playlist?list={playlist_id}");
            Ok(Self {
                fetch_url: canonical_url.clone(),
                canonical_url,
                is_playlist: true,
            })
        } else {
            // Exact identity for non-playlist requests avoids coalescing unrelated
            // single videos. Tracking cleanup is intentionally playlist-only.
            Ok(Self {
                canonical_url: request.to_string(),
                fetch_url: request.to_string(),
                is_playlist: false,
            })
        }
    }
}

pub fn canonical_playlist_url(request: &str) -> Result<String, YtError> {
    let identity = PlaylistIdentity::from_request(request)?;
    if identity.is_playlist {
        Ok(identity.canonical_url)
    } else {
        Err(YtError::InvalidData(
            "enter a YouTube playlist URL containing a valid list parameter".into(),
        ))
    }
}

fn validate_playlist_id(id: &str) -> Result<(), YtError> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Err(YtError::InvalidData(format!("unsafe playlist id: {id}")))
    } else {
        Ok(())
    }
}

fn validate_manifest(value: &PlaylistInfo, canonical_url: &str) -> Result<(), YtError> {
    if value.pipeline_version != MANIFEST_PIPELINE_VERSION {
        return Err(YtError::InvalidData(
            "manifest pipeline version mismatch".into(),
        ));
    }
    if value.canonical_url != canonical_url {
        return Err(YtError::InvalidData("manifest source URL mismatch".into()));
    }
    for video in &value.videos {
        validate_video_id(&video.id)?;
    }
    Ok(())
}

fn validate_video_id(id: &str) -> Result<(), YtError> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Err(YtError::InvalidData(format!("unsafe video id: {id}")))
    } else {
        Ok(())
    }
}

fn read_manifest(path: &Path, canonical_url: &str) -> Option<PlaylistInfo> {
    let value = serde_json::from_slice::<PlaylistInfo>(&fs::read(path).ok()?).ok()?;
    validate_manifest(&value, canonical_url).ok()?;
    Some(value)
}

enum CachedAudioReference {
    Available(PathBuf),
    Unavailable(UnavailabilityReason),
}

fn resolve_audio_reference(
    audio_root: &Path,
    provenance_path: &Path,
    video: &VideoInfo,
) -> Option<CachedAudioReference> {
    let bytes = fs::read(provenance_path).ok()?;
    if let Ok(provenance) = serde_json::from_slice::<AudioProvenance>(&bytes) {
        return resolve_available_audio(audio_root, video, provenance)
            .map(CachedAudioReference::Available);
    }
    let unavailable = serde_json::from_slice::<UnavailableAudioProvenance>(&bytes).ok()?;
    (unavailable.pipeline_version == UNAVAILABLE_PIPELINE_VERSION
        && unavailable.status == "unavailable"
        && unavailable.video_id == video.id
        && unavailable.source_url == video.url)
        .then_some(CachedAudioReference::Unavailable(unavailable.reason))
}

fn resolve_available_audio(
    audio_root: &Path,
    video: &VideoInfo,
    provenance: AudioProvenance,
) -> Option<PathBuf> {
    let expected_file = format!("objects/{}.wav", provenance.content_sha256);
    if provenance.object_file != expected_file
        || !provenance
            .content_sha256
            .chars()
            .all(|c| c.is_ascii_hexdigit())
        || provenance.content_sha256.len() != 64
    {
        return None;
    }
    let audio = audio_root.join(expected_file);
    let metadata = fs::metadata(&audio).ok()?;
    if metadata.len() == 0 {
        return None;
    }
    (provenance.pipeline_version == AUDIO_PIPELINE_VERSION
        && provenance.video_id == video.id
        && provenance.source_url == video.url
        && sha256_file(&audio).is_ok_and(|hash| hash == provenance.content_sha256))
    .then_some(audio)
}

fn sha256_file(path: &Path) -> Result<String, YtError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), YtError> {
    let parent = path
        .parent()
        .ok_or_else(|| YtError::InvalidData("cache path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| {
            file.write_all(&bytes)?;
            file.sync_all()
        })
        .map_err(|error| YtError::InvalidData(format!("atomic cache write failed: {error}")))?;
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}
