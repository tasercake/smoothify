use crate::{
    distance_matrix, CacheStatus, Config, FeatureCache, LoobError, Objective, SmoothResult, Track,
};
use loob_optim::{
    bottleneck_cost, mean_cost, AnnealingObjective, GreedyNn, Optimizer, SimulatedAnnealing,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Progress {
    Analyzing {
        current: usize,
        total: usize,
        title: String,
    },
    CacheHit {
        current: usize,
        total: usize,
        title: String,
    },
    Analyzed {
        current: usize,
        total: usize,
        title: String,
    },
    Optimizing {
        total: usize,
    },
}

pub fn smooth_local_files(
    paths: &[PathBuf],
    config: &Config,
    cache: &FeatureCache,
    mut progress: impl FnMut(Progress),
) -> Result<SmoothResult, LoobError> {
    if paths.is_empty() {
        return Err(LoobError::EmptySelection);
    }
    config.validate().map_err(LoobError::InvalidConfig)?;
    let mut tracks = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        let title = display_title(path);
        progress(Progress::Analyzing {
            current: index + 1,
            total: paths.len(),
            title: title.clone(),
        });
        let (analysis, status) = cache
            .load_or_analyze(path, config.intro_seconds, config.outro_seconds)
            .map_err(|message| LoobError::Analysis {
                path: path.display().to_string(),
                message,
            })?;
        if status == CacheStatus::Hit {
            progress(Progress::CacheHit {
                current: index + 1,
                total: paths.len(),
                title: title.clone(),
            });
        } else {
            progress(Progress::Analyzed {
                current: index + 1,
                total: paths.len(),
                title: title.clone(),
            });
        }
        tracks.push(Track {
            selection_index: index,
            title,
            path: path.clone(),
            analysis,
        });
    }
    progress(Progress::Optimizing {
        total: tracks.len(),
    });
    let analyses = tracks
        .iter()
        .map(|t| t.analysis.clone())
        .collect::<Vec<_>>();
    let matrix = distance_matrix(&analyses, config.whole_track_weight);
    let order = if tracks.len() == 1 {
        vec![0]
    } else {
        let greedy = GreedyNn.optimize(&matrix)?;
        let objective = match config.objective {
            Objective::Bottleneck => AnnealingObjective::Bottleneck,
            Objective::Hybrid { bottleneck_weight } => AnnealingObjective::Hybrid {
                beta: bottleneck_weight,
            },
        };
        SimulatedAnnealing {
            objective,
            iterations: config.iterations,
            seed: Some(config.seed),
            ..Default::default()
        }
        .optimize_from(&matrix, greedy)?
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
