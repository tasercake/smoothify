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

    fn download_wav(
        &self,
        video: &VideoInfo,
        staging_dir: &Path,
        archive: Option<&Path>,
    ) -> Result<PathBuf, YtError> {
        let download_number = self.downloads.fetch_add(1, Ordering::SeqCst) + 1;
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
        let path = staging_dir.join(format!("{}.wav", video.id));
        write_fake_wav(
            &path,
            video,
            if self.vary_downloads {
                download_number
            } else {
                0
            },
        )?;
        Ok(path)
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
        playlist_ids: Arc::new(Mutex::new(vec!["usable".into(), "fails".into()])),
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
    assert_eq!(downloads.load(Ordering::SeqCst), 1);
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
    assert_ne!(first, second);
    assert!(first.exists());
    assert!(second.exists());
    assert_eq!(
        cache.audio(&item, CachePolicy::Offline).unwrap().value,
        second
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
        .map(|video| cache.audio(video, CachePolicy::Populate).unwrap().value)
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
