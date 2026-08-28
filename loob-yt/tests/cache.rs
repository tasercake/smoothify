use loob_yt::*;
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

#[derive(Clone, Default)]
struct FakeBackend {
    fetches: Arc<AtomicUsize>,
    downloads: Arc<AtomicUsize>,
    archive_paths: Arc<Mutex<Vec<(String, PathBuf)>>>,
    archive_miss: bool,
    generic_failure: bool,
    vary_downloads: bool,
    playlist_ids: Arc<Mutex<Vec<String>>>,
    unavailable_ids: Arc<Mutex<HashSet<String>>>,
    m4a: bool,
    active_downloads: Arc<AtomicUsize>,
    max_active_downloads: Arc<AtomicUsize>,
}

impl YtDlpBackend for FakeBackend {
    fn fetch_playlist(&self, url: &str) -> Result<PlaylistInfo, YtError> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        Ok(PlaylistInfo {
            pipeline_version: "yt-dlp-flat-playlist-v1".into(),
            canonical_url: url.into(),
            requested_url: url.into(),
            title: "Fixture playlist".into(),
            fetched_unix_seconds: 1,
            videos: {
                let ids = self.playlist_ids.lock().unwrap();
                if ids.is_empty() {
                    vec![video("video_one")]
                } else {
                    ids.iter().map(|id| video(id)).collect()
                }
            },
        })
    }

    fn download_audio(
        &self,
        video: &VideoInfo,
        staging_dir: &Path,
        archive: Option<&Path>,
    ) -> Result<DownloadedAudio, YtError> {
        let download_number = self.downloads.fetch_add(1, Ordering::SeqCst) + 1;
        let active = self.active_downloads.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active_downloads
            .fetch_max(active, Ordering::SeqCst);
        struct ActiveGuard<'a>(&'a AtomicUsize);
        impl Drop for ActiveGuard<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let _active = ActiveGuard(&self.active_downloads);
        if let Some(archive) = archive {
            self.archive_paths
                .lock()
                .unwrap()
                .push((video.id.clone(), archive.to_path_buf()));
        }
        thread::sleep(Duration::from_millis(20));
        if self.generic_failure {
            return Err(YtError::YtDlpFailed("ordinary backend failure".into()));
        }
        if self.unavailable_ids.lock().unwrap().contains(&video.id) {
            return Err(YtError::VideoUnavailable {
                video_id: video.id.clone(),
                reason: UnavailabilityReason::Unavailable,
                was_cached: false,
            });
        }
        if self.archive_miss && archive.is_some() {
            return Err(YtError::ArchiveSkippedMissing(video.id.clone()));
        }
        fs::create_dir_all(staging_dir)?;
        let format = if self.m4a {
            AudioFormat::M4a
        } else {
            AudioFormat::Wav
        };
        let path = staging_dir.join(format!("{}.{}", video.id, format.extension()));
        write_fake_wav(
            &path,
            video,
            if self.vary_downloads {
                download_number
            } else {
                0
            },
        )?;
        Ok(DownloadedAudio { path, format })
    }
}

fn video(id: &str) -> VideoInfo {
    VideoInfo {
        id: id.into(),
        title: "Fixture".into(),
        duration: 1.0,
        url: format!("https://www.youtube.com/watch?v={id}"),
    }
}

fn playlist_backend(ids: &[&str], unavailable: &[&str]) -> FakeBackend {
    FakeBackend {
        playlist_ids: Arc::new(Mutex::new(ids.iter().map(|id| (*id).to_string()).collect())),
        unavailable_ids: Arc::new(Mutex::new(
            unavailable.iter().map(|id| (*id).to_string()).collect(),
        )),
        ..Default::default()
    }
}

#[test]
fn manifest_cache_hit_makes_zero_additional_backend_calls() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let fetches = Arc::clone(&backend.fetches);
    let cache = YoutubeCache::new(dir.path(), backend);
    assert!(
        !cache
            .playlist("https://fixture/playlist", CachePolicy::Populate)
            .unwrap()
            .was_cached
    );
    assert!(
        cache
            .playlist("https://fixture/playlist", CachePolicy::Offline)
            .unwrap()
            .was_cached
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
}

#[test]
fn equivalent_youtube_playlist_urls_share_one_manifest_entry() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let fetches = Arc::clone(&backend.fetches);
    let cache = YoutubeCache::new(dir.path(), backend);
    let watch_variant = "https://www.youtube.com/watch?v=first&list=PL_same-123&pp=tracking";
    let playlist_variant = "https://youtube.com/playlist?list=PL_same-123&utm_source=other";
    let populated = cache
        .playlist(watch_variant, CachePolicy::Populate)
        .unwrap();
    let cached = cache
        .playlist(playlist_variant, CachePolicy::Offline)
        .unwrap();
    assert!(!populated.was_cached);
    assert!(cached.was_cached);
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
    assert_eq!(
        cached.value.canonical_url,
        "https://www.youtube.com/playlist?list=PL_same-123"
    );
    assert_eq!(cached.value.requested_url, watch_variant);
}

#[test]
fn playlist_audio_orchestration_reuses_manifest_and_audio_cache() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let fetches = Arc::clone(&backend.fetches);
    let downloads = Arc::clone(&backend.downloads);
    let cache = YoutubeCache::new(dir.path(), backend);
    let url = "https://www.youtube.com/watch?v=first&list=PL_orchestration";
    let first = cache
        .prepare_playlist_audio(url, CachePolicy::Populate, |_| {})
        .unwrap();
    let second = cache
        .prepare_playlist_audio(url, CachePolicy::Populate, |_| {})
        .unwrap();
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
    assert!(!first.manifest_was_cached);
    assert!(second.manifest_was_cached);
    assert!(!first.tracks[0].was_cached);
    assert!(second.tracks[0].was_cached);
    assert_eq!(first.tracks[0].path, second.tracks[0].path);
}

#[test]
fn unavailable_track_is_skipped_and_negative_cache_is_reused() {
    let dir = tempfile::tempdir().unwrap();
    let backend = playlist_backend(&["usable", "gone"], &["gone"]);
    let downloads = Arc::clone(&backend.downloads);
    let cache = YoutubeCache::new(dir.path(), backend);
    let url = "https://www.youtube.com/playlist?list=PL_skip_one";

    let first = cache
        .prepare_playlist_audio(url, CachePolicy::Populate, |_| {})
        .unwrap();
    assert_eq!(first.tracks.len(), 1);
    assert_eq!(first.tracks[0].video_id, "usable");
    assert_eq!(first.skipped.len(), 1);
    assert_eq!(first.skipped[0].video_id, "gone");
    assert_eq!(downloads.load(Ordering::SeqCst), 2);

    let second = cache
        .prepare_playlist_audio(url, CachePolicy::Populate, |_| {})
        .unwrap();
    assert_eq!(second.tracks.len(), 1);
    assert_eq!(second.skipped.len(), 1);
    assert!(second.tracks[0].was_cached);
    assert_eq!(downloads.load(Ordering::SeqCst), 2);
}

#[test]
fn refresh_retries_a_negative_audio_reference() {
    let dir = tempfile::tempdir().unwrap();
    let backend = playlist_backend(&["gone"], &["gone"]);
    let downloads = Arc::clone(&backend.downloads);
    let unavailable_ids = Arc::clone(&backend.unavailable_ids);
    let cache = YoutubeCache::new(dir.path(), backend);
    let item = video("gone");

    assert!(matches!(
        cache.audio(&item, CachePolicy::Populate),
        Err(YtError::VideoUnavailable {
            was_cached: false,
            ..
        })
    ));
    assert!(matches!(
        cache.audio(&item, CachePolicy::Populate),
        Err(YtError::VideoUnavailable {
            was_cached: true,
            ..
        })
    ));
    assert!(matches!(
        cache.verify_audio(&item),
        Err(YtError::VideoUnavailable {
            was_cached: true,
            ..
        })
    ));
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
    unavailable_ids.lock().unwrap().remove("gone");
    let refreshed = cache.audio(&item, CachePolicy::Refresh).unwrap();
    assert!(!refreshed.was_cached);
    assert_eq!(downloads.load(Ordering::SeqCst), 2);
    assert!(cache.audio(&item, CachePolicy::Offline).unwrap().was_cached);
}

#[test]
fn generic_downloader_failure_still_aborts_playlist_orchestration() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend {
        playlist_ids: Arc::new(Mutex::new(
            (0..20).map(|index| format!("fails_{index}")).collect(),
        )),
        generic_failure: true,
        ..Default::default()
    };
    let downloads = Arc::clone(&backend.downloads);
    let cache = YoutubeCache::new(dir.path(), backend);
    assert!(matches!(
        cache.prepare_playlist_audio(
            "https://www.youtube.com/playlist?list=PL_systemic",
            CachePolicy::Populate,
            |_| {}
        ),
        Err(YtError::YtDlpFailed(_))
    ));
    assert!((1..=3).contains(&downloads.load(Ordering::SeqCst)));
}

#[test]
fn all_unavailable_playlist_returns_an_explicit_empty_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let backend = playlist_backend(&["gone_a", "gone_b"], &["gone_a", "gone_b"]);
    let cache = YoutubeCache::new(dir.path(), backend);
    let prepared = cache
        .prepare_playlist_audio(
            "https://www.youtube.com/playlist?list=PL_all_gone",
            CachePolicy::Populate,
            |_| {},
        )
        .unwrap();
    assert!(prepared.tracks.is_empty());
    assert_eq!(prepared.skipped.len(), 2);
}

#[test]
fn invalid_negative_cache_record_is_an_offline_miss() {
    let dir = tempfile::tempdir().unwrap();
    let backend = playlist_backend(&["gone"], &["gone"]);
    let cache = YoutubeCache::new(dir.path(), backend);
    let item = video("gone");
    assert!(matches!(
        cache.audio(&item, CachePolicy::Populate),
        Err(YtError::VideoUnavailable { .. })
    ));
    let reference = dir.path().join("audio/refs/gone.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&reference).unwrap()).unwrap();
    value["pipeline_version"] = "obsolete".into();
    fs::write(&reference, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(
        cache.audio(&item, CachePolicy::Offline),
        Err(YtError::OfflineMiss(_))
    ));
}

#[test]
fn playlist_audio_orchestration_rejects_non_playlist_input_without_backend_calls() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let fetches = Arc::clone(&backend.fetches);
    let cache = YoutubeCache::new(dir.path(), backend);
    assert!(matches!(
        cache.prepare_playlist_audio(
            "https://www.youtube.com/watch?v=single",
            CachePolicy::Populate,
            |_| {}
        ),
        Err(YtError::InvalidData(_))
    ));
    assert_eq!(fetches.load(Ordering::SeqCst), 0);
}

#[test]
fn unrelated_single_video_urls_do_not_share_manifest_entries() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let fetches = Arc::clone(&backend.fetches);
    let cache = YoutubeCache::new(dir.path(), backend);
    cache
        .playlist(
            "https://www.youtube.com/watch?v=video_a",
            CachePolicy::Populate,
        )
        .unwrap();
    assert!(matches!(
        cache.playlist(
            "https://www.youtube.com/watch?v=video_b",
            CachePolicy::Offline
        ),
        Err(YtError::OfflineMiss(_))
    ));
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
}

#[test]
fn offline_miss_never_calls_backend() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let fetches = Arc::clone(&backend.fetches);
    let cache = YoutubeCache::new(dir.path(), backend);
    assert!(matches!(
        cache.playlist("https://fixture/missing", CachePolicy::Offline),
        Err(YtError::OfflineMiss(_))
    ));
    assert_eq!(fetches.load(Ordering::SeqCst), 0);
}

#[test]
fn audio_cache_hit_and_concurrent_duplicates_download_once() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let downloads = Arc::clone(&backend.downloads);
    let cache = Arc::new(YoutubeCache::new(dir.path(), backend));
    let item = video("same_id");
    let handles = (0..6)
        .map(|_| {
            let cache = Arc::clone(&cache);
            let item = item.clone();
            thread::spawn(move || cache.audio(&item, CachePolicy::Populate).unwrap())
        })
        .collect::<Vec<_>>();
    let outcomes = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
    assert_eq!(outcomes.iter().filter(|v| !v.was_cached).count(), 1);
    assert!(cache.audio(&item, CachePolicy::Offline).unwrap().was_cached);
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
}

#[test]
fn incomplete_audio_without_provenance_is_not_a_cache_hit() {
    let dir = tempfile::tempdir().unwrap();
    let item = video("orphan");
    let orphan_source = dir.path().join("orphan-source.wav");
    write_fake_wav(&orphan_source, &item, 0).unwrap();
    let orphan_hash = loob_core::hash_file(&orphan_source).unwrap();
    let orphan_object = dir
        .path()
        .join("audio/objects")
        .join(format!("{orphan_hash}.wav"));
    fs::create_dir_all(orphan_object.parent().unwrap()).unwrap();
    fs::rename(&orphan_source, &orphan_object).unwrap();
    let backend = FakeBackend::default();
    let downloads = Arc::clone(&backend.downloads);
    let cache = YoutubeCache::new(dir.path(), backend);
    assert!(matches!(
        cache.audio(&item, CachePolicy::Offline),
        Err(YtError::OfflineMiss(_))
    ));
    assert!(
        !cache
            .audio(&item, CachePolicy::Populate)
            .unwrap()
            .was_cached
    );
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
    assert!(orphan_object.exists());
}

#[test]
fn explicit_audio_refresh_commits_a_new_content_object_and_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend {
        vary_downloads: true,
        ..Default::default()
    };
    let cache = YoutubeCache::new(dir.path(), backend);
    let item = video("refresh_id");
    let first = cache.audio(&item, CachePolicy::Populate).unwrap().value;
    let second = cache.audio(&item, CachePolicy::Refresh).unwrap().value;
    assert_ne!(first.path, second.path);
    assert!(first.path.exists());
    assert!(second.path.exists());
    assert_eq!(
        cache.audio(&item, CachePolicy::Offline).unwrap().value.path,
        second.path
    );
}

#[test]
fn different_video_ids_use_distinct_archive_files() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let archives = Arc::clone(&backend.archive_paths);
    let cache = Arc::new(YoutubeCache::new(dir.path(), backend));
    let handles = [video("archive_a"), video("archive_b")]
        .into_iter()
        .map(|item| {
            let cache = Arc::clone(&cache);
            thread::spawn(move || cache.audio(&item, CachePolicy::Populate).unwrap())
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    let archive_paths = archives.lock().unwrap();
    assert_eq!(archive_paths.len(), 2);
    assert_ne!(archive_paths[0].1, archive_paths[1].1);
    for (video_id, path) in archive_paths.iter() {
        assert!(path.ends_with(format!("archives/{video_id}.txt")));
    }
}

#[test]
fn stale_archive_is_retried_without_archive() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend {
        archive_miss: true,
        ..Default::default()
    };
    let downloads = Arc::clone(&backend.downloads);
    let cache = YoutubeCache::new(dir.path(), backend);
    assert!(
        !cache
            .audio(&video("retry_id"), CachePolicy::Populate)
            .unwrap()
            .was_cached
    );
    assert_eq!(downloads.load(Ordering::SeqCst), 2);
}

#[test]
fn generic_yt_dlp_failure_is_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend {
        generic_failure: true,
        ..Default::default()
    };
    let downloads = Arc::clone(&backend.downloads);
    let cache = YoutubeCache::new(dir.path(), backend);
    assert!(matches!(
        cache.audio(&video("fail_once"), CachePolicy::Populate),
        Err(YtError::YtDlpFailed(_))
    ));
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
}

#[test]
fn refresh_is_explicit_and_replaces_cached_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let fetches = Arc::clone(&backend.fetches);
    let cache = YoutubeCache::new(dir.path(), backend);
    cache
        .playlist("https://fixture/playlist", CachePolicy::Populate)
        .unwrap();
    cache
        .playlist("https://fixture/playlist", CachePolicy::Refresh)
        .unwrap();
    assert_eq!(fetches.load(Ordering::SeqCst), 2);
}

#[test]
fn cached_youtube_wavs_feed_the_dsp_optimizer() {
    let dir = tempfile::tempdir().unwrap();
    let cache = YoutubeCache::new(dir.path().join("youtube"), FakeBackend::default());
    let paths = [video("tone_a"), video("tone_b")]
        .iter()
        .map(|video| {
            cache
                .audio(video, CachePolicy::Populate)
                .unwrap()
                .value
                .path
        })
        .collect::<Vec<_>>();
    let result = loob_core::smooth_local_files(
        &paths,
        &loob_core::Config::default(),
        &loob_core::FeatureCache::new(dir.path().join("dsp")),
        |_| {},
    )
    .unwrap();
    assert_eq!(result.ordered_tracks.len(), 2);
    assert_ne!(
        result.ordered_tracks[0].selection_index,
        result.ordered_tracks[1].selection_index
    );
}

#[test]
fn unchanged_reference_hit_performs_zero_additional_full_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let cache = YoutubeCache::new(dir.path(), FakeBackend::default());
    let item = video("fast_hit");
    cache.audio(&item, CachePolicy::Populate).unwrap();
    let hashes_after_commit = cache.full_hash_count();
    let hit = cache.audio(&item, CachePolicy::Offline).unwrap();
    assert!(hit.was_cached);
    assert_eq!(cache.full_hash_count(), hashes_after_commit);
}

#[test]
fn explicit_verify_performs_one_strong_hash() {
    let dir = tempfile::tempdir().unwrap();
    let cache = YoutubeCache::new(dir.path(), FakeBackend::default());
    let item = video("strong_verify");
    cache.audio(&item, CachePolicy::Populate).unwrap();
    let before = cache.full_hash_count();
    cache.verify_audio(&item).unwrap();
    assert_eq!(cache.full_hash_count(), before + 1);
}

#[test]
fn metadata_change_hashes_once_and_repairs_the_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    let cache = YoutubeCache::new(dir.path(), FakeBackend::default());
    let item = video("metadata_change");
    let audio = cache.audio(&item, CachePolicy::Populate).unwrap().value;
    let original = fs::read(&audio.path).unwrap();
    thread::sleep(Duration::from_millis(5));
    fs::write(&audio.path, &original).unwrap();
    let before = cache.full_hash_count();
    assert!(cache.audio(&item, CachePolicy::Offline).unwrap().was_cached);
    assert_eq!(cache.full_hash_count(), before + 1);
    assert!(cache.audio(&item, CachePolicy::Offline).unwrap().was_cached);
    assert_eq!(cache.full_hash_count(), before + 1);
}

#[test]
fn changed_bytes_invalidate_offline_and_populate_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let downloads = Arc::clone(&backend.downloads);
    let cache = YoutubeCache::new(dir.path(), backend);
    let item = video("corrupt");
    let audio = cache.audio(&item, CachePolicy::Populate).unwrap().value;
    let mut corrupted = fs::read(&audio.path).unwrap();
    corrupted[64] ^= 0xff;
    thread::sleep(Duration::from_millis(5));
    fs::write(&audio.path, corrupted).unwrap();
    assert!(matches!(
        cache.audio(&item, CachePolicy::Offline),
        Err(YtError::OfflineMiss(_))
    ));
    let recovered = cache.audio(&item, CachePolicy::Populate).unwrap();
    assert!(!recovered.was_cached);
    assert_eq!(downloads.load(Ordering::SeqCst), 2);
    assert!(dir.path().join("audio/quarantine").is_dir());
}

#[test]
fn legacy_wav_reference_is_upgraded_without_a_migration_hash() {
    let dir = tempfile::tempdir().unwrap();
    let cache = YoutubeCache::new(dir.path(), FakeBackend::default());
    let item = video("legacy");
    cache.audio(&item, CachePolicy::Populate).unwrap();
    let reference = dir.path().join("audio/refs/legacy.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&reference).unwrap()).unwrap();
    value["pipeline_version"] = "yt-dlp-wav-v1".into();
    value.as_object_mut().unwrap().remove("format");
    value.as_object_mut().unwrap().remove("content_type");
    value.as_object_mut().unwrap().remove("fingerprint");
    fs::write(&reference, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let before = cache.full_hash_count();
    let cached = cache.audio(&item, CachePolicy::Offline).unwrap();
    assert!(cached.was_cached);
    assert_eq!(cached.value.format, AudioFormat::Wav);
    assert_eq!(cache.full_hash_count(), before);
    let upgraded: serde_json::Value =
        serde_json::from_slice(&fs::read(reference).unwrap()).unwrap();
    assert_eq!(upgraded["pipeline_version"], "yt-dlp-audio-reference-v2");
    assert_eq!(upgraded["format"], "wav");
    assert_eq!(upgraded["content_type"], "audio/wav");
    assert!(upgraded["fingerprint"].is_object());
}

#[test]
fn unsafe_object_path_is_never_followed() {
    let dir = tempfile::tempdir().unwrap();
    let cache = YoutubeCache::new(dir.path(), FakeBackend::default());
    let item = video("confined");
    cache.audio(&item, CachePolicy::Populate).unwrap();
    let reference = dir.path().join("audio/refs/confined.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&reference).unwrap()).unwrap();
    value["object_file"] = "../outside.wav".into();
    fs::write(reference, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    assert!(matches!(
        cache.audio(&item, CachePolicy::Offline),
        Err(YtError::OfflineMiss(_))
    ));
}

#[cfg(unix)]
#[test]
fn symlinked_object_is_not_a_cache_hit() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let cache = YoutubeCache::new(dir.path(), FakeBackend::default());
    let item = video("no_symlinks");
    let audio = cache.audio(&item, CachePolicy::Populate).unwrap().value;
    let outside = dir.path().join("outside.wav");
    fs::copy(&audio.path, &outside).unwrap();
    fs::remove_file(&audio.path).unwrap();
    symlink(&outside, &audio.path).unwrap();
    assert!(matches!(
        cache.audio(&item, CachePolicy::Offline),
        Err(YtError::OfflineMiss(_))
    ));
}

#[test]
fn compact_format_is_explicit_and_uses_its_real_extension() {
    let dir = tempfile::tempdir().unwrap();
    let cache = YoutubeCache::new(
        dir.path(),
        FakeBackend {
            m4a: true,
            ..Default::default()
        },
    );
    let item = video("compact");
    let first = cache.audio(&item, CachePolicy::Populate).unwrap().value;
    assert_eq!(first.format, AudioFormat::M4a);
    assert_eq!(first.path.extension().unwrap(), "m4a");
    let reference: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("audio/refs/compact.json")).unwrap())
            .unwrap();
    assert_eq!(reference["content_type"], "audio/mp4");
    let second = cache.audio(&item, CachePolicy::Offline).unwrap().value;
    assert_eq!(second.path, first.path);
    assert_eq!(second.content_sha256, first.content_sha256);
}

#[test]
fn video_id_binding_reuses_reference_when_request_url_changes() {
    let dir = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let downloads = Arc::clone(&backend.downloads);
    let cache = YoutubeCache::new(dir.path(), backend);
    let mut original = video("stable_id");
    original.url = "https://music.youtube.com/watch?v=stable_id&feature=tracking".into();
    cache.audio(&original, CachePolicy::Populate).unwrap();
    let variant = video("stable_id");
    assert!(
        cache
            .audio(&variant, CachePolicy::Offline)
            .unwrap()
            .was_cached
    );
    let reference: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("audio/refs/stable_id.json")).unwrap())
            .unwrap();
    assert_eq!(
        reference["source_url"],
        "https://www.youtube.com/watch?v=stable_id"
    );
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
}

#[test]
fn v2_reference_requires_the_canonical_source_url() {
    let dir = tempfile::tempdir().unwrap();
    let cache = YoutubeCache::new(dir.path(), FakeBackend::default());
    let item = video("canonical_source");
    cache.audio(&item, CachePolicy::Populate).unwrap();
    let reference = dir.path().join("audio/refs/canonical_source.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&reference).unwrap()).unwrap();
    value["source_url"] = "https://music.youtube.com/watch?v=canonical_source".into();
    fs::write(reference, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    assert!(matches!(
        cache.audio(&item, CachePolicy::Offline),
        Err(YtError::OfflineMiss(_))
    ));
}

#[test]
fn playlist_preparation_uses_bounded_parallel_downloads_and_preserves_order() {
    let dir = tempfile::tempdir().unwrap();
    let backend = playlist_backend(&["one", "two", "three", "four", "five"], &[]);
    let max_active = Arc::clone(&backend.max_active_downloads);
    let cache = YoutubeCache::new(dir.path(), backend);
    let prepared = cache
        .prepare_playlist_audio(
            "https://www.youtube.com/playlist?list=PL_parallel",
            CachePolicy::Populate,
            |_| {},
        )
        .unwrap();
    assert!((2..=3).contains(&max_active.load(Ordering::SeqCst)));
    assert_eq!(
        prepared
            .tracks
            .iter()
            .map(|track| track.video_id.as_str())
            .collect::<Vec<_>>(),
        ["one", "two", "three", "four", "five"]
    );
}

fn write_fake_wav(path: &Path, video: &VideoInfo, salt: usize) -> Result<(), std::io::Error> {
    let sample_rate = 8_000_u32;
    let samples = sample_rate as usize;
    let frequency = 180.0
        + video.id.bytes().map(|byte| byte as usize).sum::<usize>() as f32 % 400.0
        + salt as f32 * 7.0;
    let mut file = fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + samples as u32 * 2).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * 2).to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&(samples as u32 * 2).to_le_bytes())?;
    for index in 0..samples {
        let value = ((std::f32::consts::TAU * frequency * index as f32 / sample_rate as f32).sin()
            * i16::MAX as f32
            * 0.4) as i16;
        file.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}
