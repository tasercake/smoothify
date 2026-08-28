use crate::{
    process::now, AudioFormat, AudioProvenance, FileFingerprint, PlaylistInfo, PreparedAudio,
    PreparedAudioTrack, PreparedPlaylist, SkippedTrack, UnavailabilityReason,
    UnavailableAudioProvenance, VideoInfo, YoutubeProgress, YtDlpBackend, YtError,
};
use atomicwrites::{AllowOverwrite, AtomicFile};
use fs2::FileExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::UNIX_EPOCH,
};
use url::Url;

const AUDIO_PIPELINE_VERSION: &str = "yt-dlp-audio-reference-v2";
const LEGACY_AUDIO_PIPELINE_VERSION: &str = "yt-dlp-wav-v1";
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
    full_hashes: Arc<AtomicUsize>,
}

impl<B: YtDlpBackend> YoutubeCache<B> {
    pub fn new(root: impl Into<PathBuf>, backend: B) -> Self {
        Self {
            root: root.into(),
            backend,
            full_hashes: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Number of strong whole-file validations performed by this cache instance.
    /// Normal stat-fingerprint hits do not increment this counter.
    pub fn full_hash_count(&self) -> usize {
        self.full_hashes.load(Ordering::Relaxed)
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

    /// Resolve a playlist manifest and all of its audio through the validated
    /// layered cache. `Populate` permits only missing entries to access the
    /// backend; `Offline` guarantees cache-only behavior.
    pub fn prepare_playlist_audio(
        &self,
        url: &str,
        policy: CachePolicy,
        progress: impl Fn(YoutubeProgress) + Sync,
    ) -> Result<PreparedPlaylist, YtError>
    where
        B: Sync,
    {
        canonical_playlist_url(url)?;
        progress(YoutubeProgress::FetchingManifest);
        let manifest = self.playlist(url, policy)?;
        let total = manifest.value.videos.len();
        progress(YoutubeProgress::ManifestReady {
            title: manifest.value.title.clone(),
            total,
            was_cached: manifest.was_cached,
        });
        let results = Mutex::new(
            (0..total)
                .map(|_| None)
                .collect::<Vec<Option<Result<CacheOutcome<PreparedAudio>, YtError>>>>(),
        );
        let next = AtomicUsize::new(0);
        let cancelled = AtomicBool::new(false);
        let workers = total.min(3);
        thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| loop {
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let Some(video) = manifest.value.videos.get(index) else {
                        break;
                    };
                    let task_id = format!("{}-{index}", video.id);
                    progress(YoutubeProgress::ResolvingAudio {
                        task_id: task_id.clone(),
                        source_index: index,
                        total,
                        title: video.title.clone(),
                    });
                    let result = self.audio(video, policy);
                    match &result {
                        Ok(audio) => progress(YoutubeProgress::AudioReady {
                            task_id: task_id.clone(),
                            source_index: index,
                            total,
                            title: video.title.clone(),
                            was_cached: audio.was_cached,
                        }),
                        Err(YtError::VideoUnavailable {
                            reason, was_cached, ..
                        }) => progress(YoutubeProgress::AudioSkipped {
                            task_id,
                            source_index: index,
                            total,
                            title: video.title.clone(),
                            reason: *reason,
                            was_cached: *was_cached,
                        }),
                        Err(_) => {}
                    }
                    if result
                        .as_ref()
                        .is_err_and(|error| !matches!(error, YtError::VideoUnavailable { .. }))
                    {
                        cancelled.store(true, Ordering::Release);
                    }
                    results.lock().unwrap_or_else(|error| error.into_inner())[index] = Some(result);
                });
            }
        });

        let mut results = results
            .into_inner()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(index) = results.iter().position(|result| {
            matches!(result, Some(Err(error)) if !matches!(error, YtError::VideoUnavailable { .. }))
        }) {
            return Err(results[index]
                .take()
                .expect("fatal result exists")
                .expect_err("fatal result is an error"));
        }
        let mut tracks = Vec::with_capacity(total);
        let mut skipped = Vec::new();
        for (index, result) in results.into_iter().enumerate() {
            let Some(result) = result else {
                continue;
            };
            let video = &manifest.value.videos[index];
            match result {
                Ok(audio) => tracks.push(PreparedAudioTrack {
                    video_id: video.id.clone(),
                    title: video.title.clone(),
                    path: audio.value.path,
                    was_cached: audio.was_cached,
                    content_sha256: audio.value.content_sha256,
                    format: audio.value.format,
                    byte_size: audio.value.byte_size,
                }),
                Err(YtError::VideoUnavailable {
                    video_id, reason, ..
                }) => {
                    if video_id != video.id {
                        return Err(YtError::InvalidData(
                            "downloader reported unavailability for the wrong video".into(),
                        ));
                    }
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
    ) -> Result<CacheOutcome<PreparedAudio>, YtError> {
        validate_video_id(&video.id)?;
        let audio_root = self.root.join("audio");
        let provenance_path = audio_root.join("refs").join(format!("{}.json", video.id));
        let lock = self.lock(&format!("audio-{}", video.id))?;
        if policy != CachePolicy::Refresh {
            match self.resolve_audio_reference(&audio_root, &provenance_path, video)? {
                Some(CachedAudioReference::Available(audio)) => {
                    let _ = lock.unlock();
                    return Ok(CacheOutcome {
                        value: audio,
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
            let mut downloaded = self.backend.download_audio(video, &staging, Some(&archive));
            if downloaded
                .as_ref()
                .is_err_and(|err| matches!(err, YtError::ArchiveSkippedMissing(_)))
            {
                // A stale yt-dlp archive can skip an invalid/missing local entry. The
                // cache remains authoritative, so retry once without the archive.
                downloaded = self.backend.download_audio(video, &staging, None);
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
                        source_url: canonical_video_url(&video.id),
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
            let metadata = fs::symlink_metadata(&downloaded.path)?;
            if !metadata.file_type().is_file() || metadata.len() == 0 {
                return Err(YtError::InvalidData(
                    "downloaded audio is not a nonempty regular file".into(),
                ));
            }
            let canonical_staging = fs::canonicalize(&staging)?;
            let canonical_downloaded = fs::canonicalize(&downloaded.path)?;
            if !canonical_downloaded.starts_with(&canonical_staging)
                || downloaded
                    .path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_none_or(|value| !value.eq_ignore_ascii_case(downloaded.format.extension()))
            {
                return Err(YtError::InvalidData(
                    "downloader returned audio outside staging or with the wrong extension".into(),
                ));
            }
            let content_sha256 = self.sha256_file(&downloaded.path)?;
            let extension = downloaded.format.extension();
            let object_file = format!("objects/{content_sha256}.{extension}");
            let object_path = audio_root.join(&object_file);
            let object_lock = self.lock(&format!("object-{content_sha256}"))?;
            if fs::symlink_metadata(&object_path).is_ok_and(|existing| {
                existing.file_type().is_file()
                    && existing.len() == metadata.len()
                    && self
                        .sha256_file(&object_path)
                        .is_ok_and(|hash| hash == content_sha256)
            }) {
                fs::remove_file(&downloaded.path)?;
            } else {
                if fs::symlink_metadata(&object_path).is_ok() {
                    quarantine_object(&audio_root, &object_path, &content_sha256)?;
                }
                if let Some(parent) = object_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&downloaded.path, &object_path)?;
            }
            let _ = object_lock.unlock();
            let fingerprint = file_fingerprint(&fs::symlink_metadata(&object_path)?)?;
            let provenance = AudioProvenance {
                pipeline_version: AUDIO_PIPELINE_VERSION.into(),
                video_id: video.id.clone(),
                source_url: canonical_video_url(&video.id),
                title: video.title.clone(),
                content_sha256: content_sha256.clone(),
                object_file,
                downloaded_unix_seconds: now(),
                format: Some(downloaded.format),
                content_type: Some(downloaded.format.content_type().into()),
                fingerprint: Some(fingerprint),
            };
            atomic_write_json(&provenance_path, &provenance)?;
            Ok(CacheOutcome {
                value: PreparedAudio {
                    path: object_path,
                    content_sha256,
                    format: downloaded.format,
                    byte_size: fingerprint.byte_size,
                },
                was_cached: false,
            })
        })();
        let _ = fs::remove_dir_all(&staging);
        let _ = lock.unlock();
        result
    }

    /// Explicit strong validation. This is intentionally not part of a normal hit.
    pub fn verify_audio(&self, video: &VideoInfo) -> Result<PreparedAudio, YtError> {
        validate_video_id(&video.id)?;
        let audio_root = self.root.join("audio");
        let reference = audio_root.join("refs").join(format!("{}.json", video.id));
        let lock = self.lock(&format!("audio-{}", video.id))?;
        let result = fs::read(&reference)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<UnavailableAudioProvenance>(&bytes).ok())
            .filter(|unavailable| {
                unavailable.pipeline_version == UNAVAILABLE_PIPELINE_VERSION
                    && unavailable.status == "unavailable"
                    && unavailable.video_id == video.id
            })
            .map_or_else(
                || self.strongly_verify_reference(&audio_root, &reference, video),
                |unavailable| {
                    Err(YtError::VideoUnavailable {
                        video_id: video.id.clone(),
                        reason: unavailable.reason,
                        was_cached: true,
                    })
                },
            );
        let _ = lock.unlock();
        result
    }

    fn resolve_audio_reference(
        &self,
        audio_root: &Path,
        provenance_path: &Path,
        video: &VideoInfo,
    ) -> Result<Option<CachedAudioReference>, YtError> {
        let bytes = match fs::read(provenance_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if let Ok(provenance) = serde_json::from_slice::<AudioProvenance>(&bytes) {
            return self
                .resolve_available_audio(audio_root, provenance_path, video, provenance)
                .map(|value| value.map(CachedAudioReference::Available));
        }
        let unavailable = match serde_json::from_slice::<UnavailableAudioProvenance>(&bytes) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        Ok(
            (unavailable.pipeline_version == UNAVAILABLE_PIPELINE_VERSION
                && unavailable.status == "unavailable"
                && unavailable.video_id == video.id)
                .then_some(CachedAudioReference::Unavailable(unavailable.reason)),
        )
    }

    fn resolve_available_audio(
        &self,
        audio_root: &Path,
        provenance_path: &Path,
        video: &VideoInfo,
        mut provenance: AudioProvenance,
    ) -> Result<Option<PreparedAudio>, YtError> {
        if provenance.video_id != video.id || !valid_sha256(&provenance.content_sha256) {
            return Ok(None);
        }
        let legacy = provenance.pipeline_version == LEGACY_AUDIO_PIPELINE_VERSION
            && provenance.format.is_none()
            && provenance.content_type.is_none()
            && provenance.fingerprint.is_none();
        let format = if legacy {
            AudioFormat::Wav
        } else if provenance.pipeline_version == AUDIO_PIPELINE_VERSION
            && provenance.source_url == canonical_video_url(&video.id)
        {
            match provenance.format {
                Some(format)
                    if provenance.content_type.as_deref() == Some(format.content_type()) =>
                {
                    format
                }
                None => return Ok(None),
                Some(_) => return Ok(None),
            }
        } else {
            return Ok(None);
        };
        let expected_file = format!(
            "objects/{}.{}",
            provenance.content_sha256,
            format.extension()
        );
        if provenance.object_file != expected_file
            || !safe_object_reference(&provenance.object_file)
        {
            return Ok(None);
        }
        let audio = audio_root.join(&expected_file);
        let metadata = match fs::symlink_metadata(&audio) {
            Ok(value) if value.file_type().is_file() && value.len() > 0 => value,
            _ => return Ok(None),
        };
        let current = file_fingerprint(&metadata)?;

        if legacy {
            // V1 references were content-addressed and atomically committed. Trust
            // that established binding once, attach the cheap stat fingerprint,
            // and avoid a multi-gigabyte migration hash pass.
            provenance.pipeline_version = AUDIO_PIPELINE_VERSION.into();
            provenance.source_url = canonical_video_url(&video.id);
            provenance.format = Some(format);
            provenance.content_type = Some(format.content_type().into());
            provenance.fingerprint = Some(current);
            atomic_write_json(provenance_path, &provenance)?;
        } else if provenance.fingerprint != Some(current) {
            if self.sha256_file(&audio)? != provenance.content_sha256 {
                quarantine_reference(provenance_path)?;
                return Ok(None);
            }
            provenance.source_url = canonical_video_url(&video.id);
            provenance.fingerprint = Some(current);
            atomic_write_json(provenance_path, &provenance)?;
        }
        Ok(Some(PreparedAudio {
            path: audio,
            content_sha256: provenance.content_sha256,
            format,
            byte_size: current.byte_size,
        }))
    }

    fn strongly_verify_reference(
        &self,
        audio_root: &Path,
        provenance_path: &Path,
        video: &VideoInfo,
    ) -> Result<PreparedAudio, YtError> {
        let bytes = fs::read(provenance_path)?;
        let mut provenance: AudioProvenance = serde_json::from_slice(&bytes)?;
        if provenance.video_id != video.id || !valid_sha256(&provenance.content_sha256) {
            return Err(YtError::InvalidData(
                "audio reference identity mismatch".into(),
            ));
        }
        let legacy = provenance.pipeline_version == LEGACY_AUDIO_PIPELINE_VERSION
            && provenance.format.is_none()
            && provenance.content_type.is_none()
            && provenance.fingerprint.is_none();
        let format = if legacy {
            AudioFormat::Wav
        } else if provenance.pipeline_version == AUDIO_PIPELINE_VERSION
            && provenance.source_url == canonical_video_url(&video.id)
        {
            match provenance.format {
                Some(format)
                    if provenance.content_type.as_deref() == Some(format.content_type()) =>
                {
                    format
                }
                _ => {
                    return Err(YtError::InvalidData(
                        "audio reference format or content type mismatch".into(),
                    ))
                }
            }
        } else {
            return Err(YtError::InvalidData(
                "audio reference pipeline or canonical source mismatch".into(),
            ));
        };
        let expected_file = format!(
            "objects/{}.{}",
            provenance.content_sha256,
            format.extension()
        );
        if provenance.object_file != expected_file
            || !safe_object_reference(&provenance.object_file)
        {
            return Err(YtError::InvalidData("unsafe audio object reference".into()));
        }
        let audio = audio_root.join(&expected_file);
        let metadata = fs::symlink_metadata(&audio)?;
        if !metadata.file_type().is_file() || metadata.len() == 0 {
            return Err(YtError::InvalidData(
                "audio object is not a regular file".into(),
            ));
        }
        if self.sha256_file(&audio)? != provenance.content_sha256 {
            quarantine_reference(provenance_path)?;
            return Err(YtError::InvalidData(
                "audio object failed strong integrity validation".into(),
            ));
        }
        let fingerprint = file_fingerprint(&metadata)?;
        provenance.pipeline_version = AUDIO_PIPELINE_VERSION.into();
        provenance.source_url = canonical_video_url(&video.id);
        provenance.format = Some(format);
        provenance.content_type = Some(format.content_type().into());
        provenance.fingerprint = Some(fingerprint);
        atomic_write_json(provenance_path, &provenance)?;
        Ok(PreparedAudio {
            path: audio,
            content_sha256: provenance.content_sha256,
            format,
            byte_size: fingerprint.byte_size,
        })
    }

    fn sha256_file(&self, path: &Path) -> Result<String, YtError> {
        self.full_hashes.fetch_add(1, Ordering::Relaxed);
        sha256_file(path)
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
    Available(PreparedAudio),
    Unavailable(UnavailabilityReason),
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_object_reference(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(std::path::Component::Normal(value)) if value == "objects")
        && matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

fn canonical_video_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

fn file_fingerprint(metadata: &fs::Metadata) -> Result<FileFingerprint, YtError> {
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            YtError::InvalidData("audio object modification time predates Unix epoch".into())
        })?;
    Ok(FileFingerprint {
        byte_size: metadata.len(),
        modified_unix_nanos: modified.as_nanos().min(u64::MAX as u128) as u64,
    })
}

fn quarantine_object(audio_root: &Path, path: &Path, hash: &str) -> Result<(), YtError> {
    let quarantine = audio_root.join("quarantine");
    fs::create_dir_all(&quarantine)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bin");
    let target = quarantine.join(format!(
        "{hash}-{}-{}.{}",
        now(),
        std::process::id(),
        extension
    ));
    fs::rename(path, target)?;
    Ok(())
}

fn quarantine_reference(path: &Path) -> Result<(), YtError> {
    let Some(parent) = path.parent() else {
        return Err(YtError::InvalidData("reference path has no parent".into()));
    };
    let quarantine = parent.join("quarantine");
    fs::create_dir_all(&quarantine)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("invalid.json");
    fs::rename(
        path,
        quarantine.join(format!("{name}-{}-{}", now(), std::process::id())),
    )?;
    Ok(())
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
