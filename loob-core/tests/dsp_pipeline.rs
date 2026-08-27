use loob_core::*;
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
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
        analysis_fingerprint: analysis_fingerprint(10.0, 10.0),
        content_sha256: hash.into(),
        sample_rate: 8_000,
        duration_seconds: 1.0,
        intro: summary(intro, pitch),
        outro: summary(outro, pitch),
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
    a.outro.chroma = [0.0; 12];
    b.intro.chroma = [0.0; 12];
    assert_eq!(directed_transition_cost(&a, &b, 0.0), 0.0);
    b.intro.chroma[0] = 1.0;
    assert!(directed_transition_cost(&a, &b, 0.0) > 0.0);
}

#[test]
fn non_finite_analysis_windows_are_rejected() {
    for (intro_seconds, outro_seconds) in [
        (f64::NAN, 10.0),
        (f64::INFINITY, 10.0),
        (10.0, f64::NAN),
        (10.0, f64::NEG_INFINITY),
    ] {
        let config = Config {
            intro_seconds,
            outro_seconds,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }
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
fn feature_cache_hits_and_window_changes_miss() {
    let dir = tempfile::tempdir().unwrap();
    let audio = dir.path().join("tone.wav");
    write_wav(&audio, 440.0, 0.5);
    let cache = FeatureCache::new(dir.path().join("cache"));
    assert_eq!(
        cache.load_or_analyze(&audio, 0.2, 0.2).unwrap().1,
        CacheStatus::Miss
    );
    assert_eq!(
        cache.load_or_analyze(&audio, 0.2, 0.2).unwrap().1,
        CacheStatus::Hit
    );
    assert_eq!(
        cache.load_or_analyze(&audio, 0.3, 0.2).unwrap().1,
        CacheStatus::Miss
    );
}

#[test]
fn interrupted_temp_file_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let feature_dir = dir.path().join("features");
    fs::create_dir_all(&feature_dir).unwrap();
    fs::write(feature_dir.join(".tmp-interrupted"), b"partial json").unwrap();
    let fingerprint = analysis_fingerprint(10.0, 10.0);
    let value = analysis("abc", 0.1, 0.2, 0);
    let outcome = FeatureCache::new(dir.path())
        .get_or_compute("abc", &fingerprint, || Ok(value))
        .unwrap();
    assert_eq!(outcome.1, CacheStatus::Miss);
}

#[test]
fn concurrent_duplicate_feature_requests_compute_once() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Arc::new(FeatureCache::new(dir.path()));
    let count = Arc::new(AtomicUsize::new(0));
    let fingerprint = analysis_fingerprint(10.0, 10.0);
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

fn write_wav(path: &Path, frequency: f32, amplitude: f32) {
    let sample_rate = 8_000_u32;
    let samples = sample_rate as usize;
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
