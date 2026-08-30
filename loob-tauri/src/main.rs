mod media;
mod progress;

use loob_core::{
    project_summaries, smooth_audio_inputs, smooth_local_files, AudioInput, Config, FeatureCache,
    Objective, Progress, SmoothResult,
};
use loob_yt::{CachePolicy, RealYtDlp, SkippedTrack, YoutubeCache, YoutubeProgress, YtError};
use media::{MediaRegistry, MediaServer};
use progress::{AppProgress, ProgressEvent, ProgressReporter, ProgressSource};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{ipc::Channel, Manager};

#[derive(Clone)]
struct DesktopState {
    media: MediaRegistry,
    selected_files: Arc<Mutex<Vec<PathBuf>>>,
}

#[derive(Debug, Serialize)]
struct LocalSelectionResponse {
    titles: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PlayableTrack {
    title: String,
    media_url: String,
}

#[derive(Debug, Serialize, PartialEq)]
struct VisualizationPoint {
    chunk_index: usize,
    start_seconds: f64,
    end_seconds: f64,
    x: f64,
    y: f64,
}

#[derive(Debug, Serialize, PartialEq)]
struct VisualizationTrack {
    track_id: usize,
    sequence_index: usize,
    title: String,
    points: Vec<VisualizationPoint>,
}

#[derive(Debug, Serialize, PartialEq)]
struct PlaylistVisualization {
    algorithm: String,
    x_axis_label: String,
    y_axis_label: String,
    tracks: Vec<VisualizationTrack>,
}

#[derive(Debug, Serialize)]
struct PlayableResult {
    ordered_tracks: Vec<PlayableTrack>,
    bottleneck_cost: f64,
    mean_cost: f64,
    visualization: PlaylistVisualization,
}

#[tauri::command]
async fn choose_audio_files(
    state: tauri::State<'_, DesktopState>,
) -> Result<LocalSelectionResponse, String> {
    let state = state.inner().clone();
    state.media.invalidate();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "mp3", "flac", "m4a", "aac", "ogg"])
            .pick_files()
            .unwrap_or_default();
        let titles = paths.iter().map(|path| display_title(path)).collect();
        *state
            .selected_files
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = paths;
        LocalSelectionResponse { titles }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn order_audio_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    hybrid_bottleneck_weight: Option<f64>,
    on_progress: Channel<AppProgress>,
) -> Result<PlayableResult, String> {
    let state = state.inner().clone();
    state.media.invalidate();
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("dsp-cache");
    let file_paths = state
        .selected_files
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let reporter = ProgressReporter::new(on_progress);
    reporter.send(ProgressEvent::Validating {
        source: ProgressSource::LocalFiles,
        total: file_paths.len(),
    })?;
    let mut config = Config::default();
    if let Some(weight) = hybrid_bottleneck_weight {
        config.objective = Objective::Hybrid {
            bottleneck_weight: weight,
        };
    }
    tauri::async_runtime::spawn_blocking(move || {
        let cache = FeatureCache::new(cache_root);
        let core_reporter = reporter.clone();
        let result = smooth_local_files(&file_paths, &config, &cache, |progress: Progress| {
            core_reporter.core(progress);
        });
        if result.is_err() {
            reporter.fail_active();
        }
        let result = result.map_err(|e| e.to_string())?;
        reporter.ensure_delivery()?;
        reporter.send(ProgressEvent::ProjectingFeatures {
            tracks: result.ordered_tracks.len(),
            chunks: result
                .ordered_tracks
                .iter()
                .map(|track| track.analysis.chunks.len())
                .sum(),
        })?;
        let visualization = build_visualization(&result)?;
        reporter.send(ProgressEvent::PreparingPlayer {
            total: result.ordered_tracks.len(),
        })?;
        let result = register_result(&state.media, result, visualization)?;
        reporter.send(ProgressEvent::Completed {
            total: result.ordered_tracks.len(),
        })?;
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Deserialize)]
struct YoutubePlaylistRequest {
    url: String,
}

#[derive(Debug, Serialize)]
struct YoutubeOrderResponse {
    playlist_title: String,
    result: PlayableResult,
    skipped_tracks: Vec<SkippedTrack>,
}

#[tauri::command]
async fn order_youtube_playlist(
    app: tauri::AppHandle,
    state: tauri::State<'_, DesktopState>,
    request: YoutubePlaylistRequest,
    on_progress: Channel<AppProgress>,
) -> Result<YoutubeOrderResponse, String> {
    let url = request.url.trim().to_string();
    if url.is_empty() {
        return Err("Enter a YouTube playlist URL.".into());
    }
    let state = state.inner().clone();
    state.media.invalidate();
    let reporter = ProgressReporter::new(on_progress);
    reporter.send(ProgressEvent::Validating {
        source: ProgressSource::YoutubePlaylist,
        total: 0,
    })?;
    let app_cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let youtube = YoutubeCache::new(app_cache.join("youtube-cache"), RealYtDlp);
        let youtube_reporter = reporter.clone();
        let prepared = youtube.prepare_playlist_audio(
            &url,
            CachePolicy::Populate,
            |progress: YoutubeProgress| {
                youtube_reporter.youtube(progress);
            },
        );
        if prepared.is_err() {
            reporter.fail_active();
        }
        let prepared = prepared.map_err(user_youtube_error)?;
        reporter.ensure_delivery()?;
        if prepared.tracks.is_empty() {
            return Err(if prepared.skipped.is_empty() {
                "That playlist does not contain any tracks.".into()
            } else {
                format!(
                    "None of the {} playlist tracks are currently available.",
                    prepared.skipped.len()
                )
            });
        }
        let inputs = prepared
            .tracks
            .iter()
            .enumerate()
            .map(|(index, track)| AudioInput {
                task_id: format!("{}-{index}", track.video_id),
                title: track.title.clone(),
                path: track.path.clone(),
                content_sha256: Some(track.content_sha256.clone()),
            })
            .collect::<Vec<_>>();
        let result = smooth_audio_inputs(
            &inputs,
            &Config::default(),
            &FeatureCache::new(app_cache.join("dsp-cache")),
            |progress: Progress| {
                reporter.core(progress);
            },
        );
        if result.is_err() {
            reporter.fail_active();
        }
        let result = result.map_err(|error| format!("Could not analyze this playlist: {error}"))?;
        reporter.ensure_delivery()?;
        reporter.send(ProgressEvent::ProjectingFeatures {
            tracks: result.ordered_tracks.len(),
            chunks: result
                .ordered_tracks
                .iter()
                .map(|track| track.analysis.chunks.len())
                .sum(),
        })?;
        let visualization = build_visualization(&result)?;
        reporter.send(ProgressEvent::PreparingPlayer {
            total: result.ordered_tracks.len(),
        })?;
        let result = register_result(&state.media, result, visualization)?;
        reporter.send(ProgressEvent::Completed {
            total: result.ordered_tracks.len(),
        })?;
        Ok(YoutubeOrderResponse {
            playlist_title: prepared.title,
            result,
            skipped_tracks: prepared.skipped,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn register_result(
    registry: &MediaRegistry,
    result: SmoothResult,
    visualization: PlaylistVisualization,
) -> Result<PlayableResult, String> {
    let paths = result
        .ordered_tracks
        .iter()
        .map(|track| track.path.clone())
        .collect::<Vec<_>>();
    let media_urls = registry.replace(&paths)?;
    let ordered_tracks = result
        .ordered_tracks
        .into_iter()
        .zip(media_urls)
        .map(|(track, media_url)| PlayableTrack {
            title: track.title,
            media_url,
        })
        .collect();
    Ok(PlayableResult {
        ordered_tracks,
        bottleneck_cost: result.bottleneck_cost,
        mean_cost: result.mean_cost,
        visualization,
    })
}

fn build_visualization(result: &SmoothResult) -> Result<PlaylistVisualization, String> {
    let summaries = result
        .ordered_tracks
        .iter()
        .flat_map(|track| track.analysis.chunks.iter().map(|chunk| &chunk.summary))
        .collect::<Vec<_>>();
    let projection = project_summaries(&summaries)
        .map_err(|error| format!("Could not project DSP features: {error}"))?;
    let mut coordinates = projection.coordinates.into_iter();
    let tracks = result
        .ordered_tracks
        .iter()
        .enumerate()
        .map(|(sequence_index, track)| VisualizationTrack {
            track_id: track.selection_index,
            sequence_index,
            title: track.title.clone(),
            points: track
                .analysis
                .chunks
                .iter()
                .enumerate()
                .map(|(chunk_index, chunk)| {
                    let [x, y] = coordinates
                        .next()
                        .expect("projection must preserve the requested point count");
                    VisualizationPoint {
                        chunk_index,
                        start_seconds: chunk.start_seconds,
                        end_seconds: chunk.end_seconds,
                        x,
                        y,
                    }
                })
                .collect(),
        })
        .collect();
    debug_assert!(coordinates.next().is_none());
    Ok(PlaylistVisualization {
        algorithm: projection.algorithm.to_string(),
        x_axis_label: "MDS 1".into(),
        y_axis_label: "MDS 2".into(),
        tracks,
    })
}

fn display_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled track")
        .to_string()
}

fn user_youtube_error(error: YtError) -> String {
    match error {
        YtError::YtDlpNotFound => {
            "yt-dlp is not installed or is unavailable on the application path.".into()
        }
        YtError::InvalidData(message) => format!("Invalid playlist URL or data: {message}"),
        YtError::YtDlpFailed(message) => {
            let concise = message.lines().next().unwrap_or("unknown YouTube error");
            format!("YouTube could not provide this playlist: {concise}")
        }
        YtError::ArchiveSkippedMissing(video_id) => {
            format!("Cached download state for track {video_id} could not be recovered.")
        }
        YtError::VideoUnavailable { .. } => {
            "A playlist track is unavailable and could not be prepared.".into()
        }
        YtError::OfflineMiss(_) => "Required playlist audio is not available in the cache.".into(),
        YtError::Io(error) => format!("Could not store playlist audio locally: {error}"),
        YtError::Json(_) => "YouTube returned playlist data the app could not read.".into(),
    }
}

fn main() {
    let media_server = MediaServer::start().expect("failed to start loopback audio server");
    let state = DesktopState {
        media: media_server.registry(),
        selected_files: Arc::new(Mutex::new(Vec::new())),
    };
    tauri::Builder::default()
        .manage(media_server)
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            choose_audio_files,
            order_audio_files,
            order_youtube_playlist
        ])
        .run(tauri::generate_context!())
        .expect("failed to run loob");
}

#[cfg(test)]
mod tests {
    use super::*;
    use loob_core::{DspChunk, DspSummary, Track, TrackAnalysis, FEATURE_PROJECTION_ALGORITHM};

    fn summary(value: f64) -> DspSummary {
        let mut chroma = [0.0; 12];
        chroma[value as usize % 12] = 1.0;
        DspSummary {
            rms_db: -40.0 + value,
            spectral_centroid: value / 10.0,
            spectral_rolloff: value / 10.0,
            spectral_flatness: value / 10.0,
            spectral_flux: value / 10.0,
            zero_crossing_rate: value / 10.0,
            onset_density: value,
            chroma,
        }
    }

    fn track(selection_index: usize, title: &str, offset: f64) -> Track {
        Track {
            selection_index,
            title: title.into(),
            path: PathBuf::from(format!("{title}.wav")),
            analysis: TrackAnalysis {
                pipeline_version: "test".into(),
                analysis_fingerprint: "test".into(),
                content_sha256: format!("hash-{selection_index}"),
                sample_rate: 48_000,
                duration_seconds: 19.0,
                chunks: vec![
                    DspChunk {
                        start_seconds: 0.0,
                        end_seconds: 10.0,
                        summary: summary(offset),
                    },
                    DspChunk {
                        start_seconds: 9.0,
                        end_seconds: 19.0,
                        summary: summary(offset + 1.0),
                    },
                ],
                head_chunks: vec![DspChunk {
                    start_seconds: 0.0,
                    end_seconds: 2.0,
                    summary: summary(offset),
                }],
                tail_chunks: vec![DspChunk {
                    start_seconds: 17.0,
                    end_seconds: 19.0,
                    summary: summary(offset + 1.0),
                }],
                whole: summary(offset + 0.5),
            },
        }
    }

    #[test]
    fn visualization_preserves_optimized_track_and_chunk_identity() {
        let result = SmoothResult {
            ordered_tracks: vec![track(7, "Second", 3.0), track(2, "First", 6.0)],
            bottleneck_cost: 0.2,
            mean_cost: 0.1,
            distance_matrix: vec![vec![0.0, 0.2], vec![0.3, 0.0]],
        };
        let visualization = build_visualization(&result).unwrap();

        assert_eq!(visualization.algorithm, FEATURE_PROJECTION_ALGORITHM);
        assert_eq!(visualization.tracks.len(), 2);
        assert_eq!(visualization.tracks[0].track_id, 7);
        assert_eq!(visualization.tracks[0].sequence_index, 0);
        assert_eq!(visualization.tracks[1].track_id, 2);
        assert_eq!(visualization.tracks[1].sequence_index, 1);
        assert_eq!(visualization.tracks[0].points[1].chunk_index, 1);
        assert_eq!(visualization.tracks[0].points[1].start_seconds, 9.0);
        assert_eq!(visualization.tracks[0].points[1].end_seconds, 19.0);
        assert!(visualization
            .tracks
            .iter()
            .flat_map(|track| &track.points)
            .flat_map(|point| [point.x, point.y])
            .all(f64::is_finite));
    }
}
