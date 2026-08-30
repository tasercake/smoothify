use crate::{
    distance_matrix, CacheStatus, Config, FeatureCache, LoobError, Objective, SmoothResult, Track,
};
use loob_optim::{
    bottleneck_cost, characteristic_edge_cost, mean_cost, AnnealingObjective, AnnealingProgress,
    GreedyNn, Optimizer, SimulatedAnnealing,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
    thread,
};

#[derive(Debug, Clone)]
pub struct AudioInput {
    pub task_id: String,
    pub title: String,
    pub path: PathBuf,
    /// Trusted hash supplied by a validated content-addressed source cache.
    /// Arbitrary local files leave this empty and are hashed normally.
    pub content_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Progress {
    Analyzing {
        task_id: String,
        source_index: usize,
        total: usize,
        title: String,
    },
    CacheHit {
        task_id: String,
        source_index: usize,
        total: usize,
        title: String,
    },
    Analyzed {
        task_id: String,
        source_index: usize,
        total: usize,
        title: String,
    },
    Optimizing {
        total: usize,
    },
    Annealing {
        objective: String,
        seed: u64,
        report_every: usize,
        update: AnnealingProgress,
    },
    AnnealingSkipped {
        total: usize,
        reason: String,
    },
}

pub fn smooth_local_files(
    paths: &[PathBuf],
    config: &Config,
    cache: &FeatureCache,
    progress: impl Fn(Progress) + Sync,
) -> Result<SmoothResult, LoobError> {
    let inputs = paths
        .iter()
        .enumerate()
        .map(|(index, path)| AudioInput {
            task_id: format!("local-{index}"),
            title: display_title(path),
            path: path.clone(),
            content_sha256: None,
        })
        .collect::<Vec<_>>();
    smooth_audio_inputs(&inputs, config, cache, progress)
}

pub fn smooth_audio_inputs(
    inputs: &[AudioInput],
    config: &Config,
    cache: &FeatureCache,
    progress: impl Fn(Progress) + Sync,
) -> Result<SmoothResult, LoobError> {
    if inputs.is_empty() {
        return Err(LoobError::EmptySelection);
    }
    config.validate().map_err(LoobError::InvalidConfig)?;
    let total = inputs.len();
    let results = Mutex::new(
        (0..total)
            .map(|_| None)
            .collect::<Vec<Option<Result<(crate::TrackAnalysis, CacheStatus), LoobError>>>>(),
    );
    let next = AtomicUsize::new(0);
    let cancelled = AtomicBool::new(false);
    let available = thread::available_parallelism().map_or(1, usize::from);
    let workers = total.min(available).min(4);
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
                let Some(input) = inputs.get(index) else {
                    break;
                };
                progress(Progress::Analyzing {
                    task_id: input.task_id.clone(),
                    source_index: index,
                    total,
                    title: input.title.clone(),
                });
                let analyzed = match &input.content_sha256 {
                    Some(hash) => cache.load_or_analyze_known_hash(&input.path, hash),
                    None => cache.load_or_analyze(&input.path),
                }
                .map_err(|message| LoobError::Analysis {
                    path: input.path.display().to_string(),
                    message,
                });
                if let Ok((_, status)) = &analyzed {
                    let event = match status {
                        CacheStatus::Hit => Progress::CacheHit {
                            task_id: input.task_id.clone(),
                            source_index: index,
                            total,
                            title: input.title.clone(),
                        },
                        CacheStatus::Miss => Progress::Analyzed {
                            task_id: input.task_id.clone(),
                            source_index: index,
                            total,
                            title: input.title.clone(),
                        },
                    };
                    progress(event);
                } else {
                    cancelled.store(true, Ordering::Release);
                }
                results.lock().unwrap_or_else(|error| error.into_inner())[index] = Some(analyzed);
            });
        }
    });
    let analyses = results
        .into_inner()
        .unwrap_or_else(|error| error.into_inner())
        .into_iter()
        .flatten()
        .collect::<Result<Vec<_>, _>>()?;
    let tracks = inputs
        .iter()
        .zip(analyses)
        .enumerate()
        .map(|(index, (input, (analysis, _)))| Track {
            selection_index: index,
            title: input.title.clone(),
            path: input.path.clone(),
            analysis,
        })
        .collect::<Vec<_>>();
    progress(Progress::Optimizing { total });
    let analyses = tracks
        .iter()
        .map(|t| t.analysis.clone())
        .collect::<Vec<_>>();
    let matrix = distance_matrix(&analyses, config).map_err(LoobError::Metric)?;
    let order = if tracks.len() == 1 {
        progress(Progress::AnnealingSkipped {
            total,
            reason: "one usable track requires no path search".into(),
        });
        vec![0]
    } else {
        let greedy = GreedyNn.optimize(&matrix)?;
        let (objective, objective_label) = match &config.objective {
            Objective::Bottleneck => (AnnealingObjective::Bottleneck, "bottleneck".to_string()),
            Objective::Hybrid { bottleneck_weight } => (
                AnnealingObjective::Hybrid {
                    beta: *bottleneck_weight,
                },
                format!("hybrid(beta={bottleneck_weight:.6})"),
            ),
        };
        let initial_temperature = characteristic_edge_cost(&matrix)?;
        let report_every = (config.iterations / 200).max(1);
        SimulatedAnnealing {
            objective,
            iterations: config.iterations,
            seed: Some(config.seed),
            initial_temp: initial_temperature,
            ..Default::default()
        }
        .optimize_from_with_progress(&matrix, greedy, report_every, |update| {
            progress(Progress::Annealing {
                objective: objective_label.clone(),
                seed: config.seed,
                report_every,
                update,
            });
        })?
    };
    let ordered_tracks = order.iter().map(|index| tracks[*index].clone()).collect();
    Ok(SmoothResult {
        bottleneck_cost: bottleneck_cost(&matrix, &order),
        mean_cost: mean_cost(&matrix, &order),
        ordered_tracks,
        distance_matrix: matrix,
    })
}

fn display_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("Untitled track")
        .to_string()
}
