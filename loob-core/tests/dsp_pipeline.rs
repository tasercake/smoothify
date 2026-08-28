use loob_core::*;
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

fn summary(value: f64, pitch: usize) -> DspSummary {
    let mut chroma = [0.0; 12];
    chroma[pitch] = 1.0;
    DspSummary {
        rms_db: -40.0 + value * 20.0,
        spectral_centroid: value,
        spectral_rolloff: value,
        spectral_flatness: value,
        spectral_flux: value,
        zero_crossing_rate: value,
        onset_density: value * 4.0,
        chroma,
    }
}

fn analysis(hash: &str, intro: f64, outro: f64, pitch: usize) -> TrackAnalysis {
    TrackAnalysis {
        pipeline_version: ANALYSIS_PIPELINE_VERSION.into(),
        analysis_fingerprint: analysis_fingerprint(),
        content_sha256: hash.into(),
        sample_rate: 8_000,
        duration_seconds: 1.0,
        chunks: vec![
            DspChunk {
                start_seconds: 0.0,
                end_seconds: 0.5,
                summary: summary(intro, pitch),
            },
            DspChunk {
                start_seconds: 0.5,
                end_seconds: 1.0,
                summary: summary(outro, pitch),
            },
        ],
        whole: summary((intro + outro) / 2.0, pitch),
    }
}

#[test]
fn transition_cost_is_directed() {
    let a = analysis("a", 0.0, 0.9, 0);
    let b = analysis("b", 0.9, 0.1, 0);
    assert!(directed_transition_cost(&a, &b, 0.0) < directed_transition_cost(&b, &a, 0.0));
}

#[test]
fn zero_chroma_vectors_match_each_other_but_not_nonzero_chroma() {
    let mut a = analysis("a", 0.2, 0.2, 0);
    let mut b = analysis("b", 0.2, 0.2, 0);
    a.chunks.last_mut().unwrap().summary.chroma = [0.0; 12];
    b.chunks.first_mut().unwrap().summary.chroma = [0.0; 12];
    assert_eq!(directed_transition_cost(&a, &b, 0.0), 0.0);
    b.chunks.first_mut().unwrap().summary.chroma[0] = 1.0;
    assert!(directed_transition_cost(&a, &b, 0.0) > 0.0);
}

#[test]
fn endpoint_cost_uses_only_last_source_and_first_destination_chunks() {
    let mut a = analysis("a", 0.0, 0.25, 0);
    let mut b = analysis("b", 0.25, 0.9, 0);
    let baseline = directed_transition_cost(&a, &b, 0.0);

    a.chunks.insert(
        1,
        DspChunk {
            start_seconds: 0.25,
            end_seconds: 0.5,
            summary: summary(1.0, 6),
        },
    );
    b.chunks.insert(
        1,
        DspChunk {
            start_seconds: 0.25,
            end_seconds: 0.5,
            summary: summary(1.0, 6),
        },
    );
    assert_eq!(directed_transition_cost(&a, &b, 0.0), baseline);

    a.chunks.last_mut().unwrap().summary = summary(0.95, 6);
    assert_ne!(directed_transition_cost(&a, &b, 0.0), baseline);
}

#[test]
#[should_panic(expected = "source track has no DSP chunks")]
fn endpoint_cost_rejects_an_analysis_without_chunks() {
    let mut a = analysis("a", 0.0, 0.25, 0);
    let b = analysis("b", 0.25, 0.9, 0);
    a.chunks.clear();
    let _ = directed_transition_cost(&a, &b, 0.0);
}

#[test]
fn analyzed_chunks_store_exact_timeline_windows() {
    let dir = tempfile::tempdir().unwrap();
    let audio = dir.path().join("twenty-five-seconds.wav");
    write_wav_for_seconds(&audio, 440.0, 0.5, 25);
    let result = analyze_audio(&audio).unwrap();
    let windows = result
        .chunks
        .iter()
        .map(|chunk| (chunk.start_seconds, chunk.end_seconds))
        .collect::<Vec<_>>();
    assert_eq!(windows, vec![(0.0, 10.0), (9.0, 19.0), (15.0, 25.0)]);
    assert_eq!(result.duration_seconds, 25.0);
}

#[test]
fn empty_selection_errors() {
    let dir = tempfile::tempdir().unwrap();
    let err = smooth_local_files(
        &[],
        &Config::default(),
        &FeatureCache::new(dir.path()),
        |_| {},
    )
    .unwrap_err();
    assert!(matches!(err, LoobError::EmptySelection));
}

#[test]
fn single_two_and_three_track_orderings_are_complete_and_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let files = [220.0, 330.0, 440.0]
        .into_iter()
        .enumerate()
        .map(|(i, frequency)| {
            let path = dir.path().join(format!("tone-{i}.wav"));
            write_wav(&path, frequency, 0.3 + i as f32 * 0.1);
            path
        })
        .collect::<Vec<_>>();
    let cache = FeatureCache::new(dir.path().join("cache"));
    for n in 1..=3 {
        let first = smooth_local_files(&files[..n], &Config::default(), &cache, |_| {}).unwrap();
        let second = smooth_local_files(&files[..n], &Config::default(), &cache, |_| {}).unwrap();
        let first_ids = first
            .ordered_tracks
            .iter()
            .map(|t| t.selection_index)
            .collect::<Vec<_>>();
        let second_ids = second
            .ordered_tracks
            .iter()
            .map(|t| t.selection_index)
            .collect::<Vec<_>>();
        assert_eq!(first_ids, second_ids);
        assert_eq!(
            first_ids.iter().copied().collect::<BTreeSet<_>>(),
            (0..n).collect()
        );
        assert_eq!(first_ids.len(), n);
    }
}

#[test]
fn feature_cache_hits_for_unchanged_audio() {
    let dir = tempfile::tempdir().unwrap();
    let audio = dir.path().join("tone.wav");
    write_wav(&audio, 440.0, 0.5);
    let cache = FeatureCache::new(dir.path().join("cache"));
    assert_eq!(cache.load_or_analyze(&audio).unwrap().1, CacheStatus::Miss);
    assert_eq!(cache.load_or_analyze(&audio).unwrap().1, CacheStatus::Hit);
}

#[test]
fn interrupted_temp_file_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let feature_dir = dir.path().join("features");
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(feature_dir.join(".tmp-interrupted"), b"partial json").unwrap();
    let fingerprint = analysis_fingerprint();
    let value = analysis("abc", 0.1, 0.2, 0);
    let outcome = FeatureCache::new(dir.path())
        .get_or_compute("abc", &fingerprint, || Ok(value))
        .unwrap();
    assert_eq!(outcome.1, CacheStatus::Miss);
}

#[test]
fn invalid_cached_chunk_timeline_is_recomputed() {
    let dir = tempfile::tempdir().unwrap();
    let cache = FeatureCache::new(dir.path());
    let fingerprint = analysis_fingerprint();
    let value = analysis("invalid-timeline", 0.1, 0.2, 0);
    cache
        .get_or_compute("invalid-timeline", &fingerprint, || Ok(value.clone()))
        .unwrap();

    let feature_path = fs::read_dir(dir.path().join("features"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&feature_path).unwrap()).unwrap();
    json["chunks"] = serde_json::json!([]);
    fs::write(&feature_path, serde_json::to_vec(&json).unwrap()).unwrap();

    let (recomputed, status) = cache
        .get_or_compute("invalid-timeline", &fingerprint, || Ok(value))
        .unwrap();
    assert_eq!(status, CacheStatus::Miss);
    assert!(!recomputed.chunks.is_empty());
}

#[test]
fn invalid_computed_chunk_timeline_is_not_cached() {
    let dir = tempfile::tempdir().unwrap();
    let cache = FeatureCache::new(dir.path());
    let fingerprint = analysis_fingerprint();
    let mut invalid = analysis("invalid-computed", 0.1, 0.2, 0);
    invalid.chunks[0].start_seconds = 0.25;

    let error = cache
        .get_or_compute("invalid-computed", &fingerprint, || Ok(invalid))
        .unwrap_err();
    assert!(error.contains("chunk timeline"));
    assert!(fs::read_dir(dir.path().join("features"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn concurrent_duplicate_feature_requests_compute_once() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(FeatureCache::new(dir.path()));
    let count = Arc::new(AtomicUsize::new(0));
    let fingerprint = analysis_fingerprint();
    let mut handles = Vec::new();
    for _ in 0..6 {
        let cache = Arc::clone(&cache);
        let count = Arc::clone(&count);
        let fingerprint = fingerprint.clone();
        handles.push(thread::spawn(move || {
            cache
                .get_or_compute("same", &fingerprint, || {
                    count.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(20));
                    Ok(analysis("same", 0.1, 0.2, 0))
                })
                .unwrap()
        }));
    }
    let outcomes = handles
        .into_iter()
        .map(|h| h.join().unwrap().1)
        .collect::<Vec<_>>();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|status| **status == CacheStatus::Miss)
            .count(),
        1
    );
}

#[test]
fn trusted_hash_can_hit_feature_cache_without_reading_the_audio_file() {
    let dir = tempfile::tempdir().unwrap();
    let cache = FeatureCache::new(dir.path());
    let hash = "a".repeat(64);
    let fingerprint = analysis_fingerprint();
    let mut cached_analysis = analysis(&hash, 0.1, 0.2, 0);
    cached_analysis
        .analysis_fingerprint
        .clone_from(&fingerprint);
    cache
        .get_or_compute(&hash, &fingerprint, || Ok(cached_analysis))
        .unwrap();
    let missing = dir.path().join("audio-does-not-exist.m4a");
    let (_, status) = cache.load_or_analyze_known_hash(&missing, &hash).unwrap();
    assert_eq!(status, CacheStatus::Hit);
}

#[test]
fn parallel_analysis_events_keep_stable_source_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let files = [220.0, 330.0, 440.0, 550.0]
        .into_iter()
        .enumerate()
        .map(|(index, frequency)| {
            let path = dir.path().join(format!("parallel-{index}.wav"));
            write_wav(&path, frequency, 0.4);
            path
        })
        .collect::<Vec<_>>();
    let started = Mutex::new(Vec::new());
    let result = smooth_local_files(
        &files,
        &Config::default(),
        &FeatureCache::new(dir.path().join("parallel-cache")),
        |event| {
            if let Progress::Analyzing { source_index, .. } = event {
                started.lock().unwrap().push(source_index);
            }
        },
    )
    .unwrap();
    let mut indexes = started.into_inner().unwrap();
    indexes.sort_unstable();
    assert_eq!(indexes, vec![0, 1, 2, 3]);
    let mut selections = result
        .ordered_tracks
        .iter()
        .map(|track| track.selection_index)
        .collect::<Vec<_>>();
    selections.sort_unstable();
    assert_eq!(selections, vec![0, 1, 2, 3]);
}

#[test]
fn fatal_parallel_dsp_failure_stops_claiming_the_whole_input() {
    let dir = tempfile::tempdir().unwrap();
    let files = (0..20)
        .map(|index| dir.path().join(format!("missing-{index}.wav")))
        .collect::<Vec<_>>();
    let started = AtomicUsize::new(0);
    assert!(smooth_local_files(
        &files,
        &Config::default(),
        &FeatureCache::new(dir.path().join("cancel-cache")),
        |event| {
            if matches!(event, Progress::Analyzing { .. }) {
                started.fetch_add(1, Ordering::SeqCst);
            }
        },
    )
    .is_err());
    assert!((1..=4).contains(&started.load(Ordering::SeqCst)));
}

fn write_wav(path: &Path, frequency: f32, amplitude: f32) {
    write_wav_for_seconds(path, frequency, amplitude, 1);
}

fn write_wav_for_seconds(path: &Path, frequency: f32, amplitude: f32, seconds: usize) {
    let sample_rate = 8_000_u32;
    let samples = sample_rate as usize * seconds;
    let data_len = samples * 2;
    let mut file = fs::File::create(path).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + data_len as u32).to_le_bytes())
        .unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16_u32.to_le_bytes()).unwrap();
    file.write_all(&1_u16.to_le_bytes()).unwrap();
    file.write_all(&1_u16.to_le_bytes()).unwrap();
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&(sample_rate * 2).to_le_bytes()).unwrap();
    file.write_all(&2_u16.to_le_bytes()).unwrap();
    file.write_all(&16_u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&(data_len as u32).to_le_bytes()).unwrap();
    for i in 0..samples {
        let value = (amplitude
            * (std::f32::consts::TAU * frequency * i as f32 / sample_rate as f32).sin()
            * i16::MAX as f32) as i16;
        file.write_all(&value.to_le_bytes()).unwrap();
    }
}
