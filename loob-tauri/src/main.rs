mod media;
mod progress;

use loob_core::{
    smooth_audio_inputs, smooth_local_files, AudioInput, Config, FeatureCache, Objective, Progress,
    SmoothResult,
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

#[derive(Debug, Serialize)]
struct PlayableResult {
    ordered_tracks: Vec<PlayableTrack>,
    bottleneck_cost: f64,
    mean_cost: f64,
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
        reporter.send(ProgressEvent::PreparingPlayer {
            total: result.ordered_tracks.len(),
        })?;
        let result = register_result(&state.media, result)?;
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
        reporter.send(ProgressEvent::PreparingPlayer {
            total: result.ordered_tracks.len(),
        })?;
        let result = register_result(&state.media, result)?;
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
