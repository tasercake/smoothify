use clap::{Args, Parser, Subcommand};
use loob_core::{
    smooth_audio_inputs, smooth_local_files, AudioInput, Config, FeatureCache, Objective,
};
use loob_yt::{CachePolicy, RealYtDlp, YoutubeCache, YtError};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "loob", about = "Local DSP playlist transition optimizer")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args, Debug, Clone)]
struct MetricArgs {
    /// Highest fitted boundary-jet order: 0=position, 1=velocity, 2=acceleration.
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(0..=2))]
    jet_order: u8,
    /// Maximum dense boundary windows used by each fit.
    #[arg(long, default_value_t = 8)]
    jet_samples: usize,
    /// Multiplicative weight per window away from the seam.
    #[arg(long, default_value_t = 0.85)]
    jet_seam_weight_decay: f64,
    /// Seconds to extrapolate the source jet beyond the seam; gapless is zero.
    #[arg(long, default_value_t = 0.0)]
    jet_delta_seconds: f64,
    #[arg(long, default_value_t = 1.0)]
    jet_position_weight: f64,
    #[arg(long, default_value_t = 0.5)]
    jet_velocity_weight: f64,
    #[arg(long, default_value_t = 0.25)]
    jet_acceleration_weight: f64,
    /// Blend weight of whole-track similarity versus the boundary jet.
    #[arg(long, default_value_t = 0.2)]
    whole_track_weight: f64,
}

impl MetricArgs {
    fn apply(&self, config: &mut Config) {
        config.whole_track_weight = self.whole_track_weight;
        config.jet.max_order = self.jet_order;
        config.jet.samples = self.jet_samples;
        config.jet.seam_weight_decay = self.jet_seam_weight_decay;
        config.jet.delta_seconds = self.jet_delta_seconds;
        config.jet.lambda_position = self.jet_position_weight;
        config.jet.lambda_velocity = self.jet_velocity_weight;
        config.jet.lambda_acceleration = self.jet_acceleration_weight;
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze local audio and print a deterministic smooth ordering.
    Local {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long, default_value = ".loob-cache")]
        cache_dir: PathBuf,
        /// Explicitly opt into a hybrid objective; 1.0 is pure bottleneck.
        #[arg(long)]
        hybrid_bottleneck_weight: Option<f64>,
        #[command(flatten)]
        metric: MetricArgs,
    },
    /// Inspect or explicitly populate the layered YouTube cache.
    YoutubeCache {
        url: String,
        #[arg(long, default_value = ".loob-cache/youtube")]
        cache_dir: PathBuf,
        /// Permit network access for missing cache entries.
        #[arg(long, conflicts_with = "refresh")]
        populate: bool,
        /// Explicitly refresh the manifest and requested audio entries.
        #[arg(long)]
        refresh: bool,
        /// Download at most this many tracks. Defaults to manifest only.
        #[arg(long, default_value_t = 0)]
        audio_limit: usize,
        /// Analyze and bottleneck-optimize the bounded cached/populated subset.
        #[arg(long, requires = "audio_limit")]
        optimize: bool,
        /// Strongly hash and scrub each requested cached audio object.
        #[arg(long, requires = "audio_limit", conflicts_with_all = ["populate", "refresh"])]
        verify: bool,
        #[command(flatten)]
        metric: MetricArgs,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Commands::Local {
            files,
            cache_dir,
            hybrid_bottleneck_weight,
            metric,
        } => {
            let mut config = Config::default();
            metric.apply(&mut config);
            if let Some(weight) = hybrid_bottleneck_weight {
                config.objective = Objective::Hybrid {
                    bottleneck_weight: weight,
                };
            }
            let result =
                smooth_local_files(&files, &config, &FeatureCache::new(cache_dir), |progress| {
                    eprintln!("{progress:?}");
                })?;
            for (index, track) in result.ordered_tracks.iter().enumerate() {
                println!("{}. {}", index + 1, track.path.display());
            }
            eprintln!(
                "worst transition: {:.4}; mean: {:.4}",
                result.bottleneck_cost, result.mean_cost
            );
        }
        Commands::YoutubeCache {
            url,
            cache_dir,
            populate,
            refresh,
            audio_limit,
            optimize,
            verify,
            metric,
        } => {
            if optimize && audio_limit == 0 {
                anyhow::bail!("--optimize requires a positive --audio-limit");
            }
            if verify && audio_limit == 0 {
                anyhow::bail!("--verify requires a positive --audio-limit");
            }
            let policy = if refresh {
                CachePolicy::Refresh
            } else if populate {
                CachePolicy::Populate
            } else {
                CachePolicy::Offline
            };
            let cache = YoutubeCache::new(&cache_dir, RealYtDlp);
            let manifest = cache.playlist(&url, policy)?;
            println!(
                "{} — {} tracks ({})",
                manifest.value.title,
                manifest.value.videos.len(),
                if manifest.was_cached {
                    "cache hit"
                } else {
                    "fetched"
                }
            );
            let mut audio_inputs = Vec::new();
            for (index, video) in manifest.value.videos.iter().take(audio_limit).enumerate() {
                let outcome = if verify {
                    cache
                        .verify_audio(video)
                        .map(|value| loob_yt::CacheOutcome {
                            value,
                            was_cached: true,
                        })
                } else {
                    cache.audio(video, policy)
                };
                match outcome {
                    Ok(audio) => {
                        println!(
                            "{}: {} ({})",
                            video.id,
                            audio.value.path.display(),
                            if audio.was_cached {
                                "cache hit"
                            } else {
                                "downloaded"
                            }
                        );
                        audio_inputs.push(AudioInput {
                            task_id: format!("{}-{index}", video.id),
                            title: video.title.clone(),
                            path: audio.value.path,
                            content_sha256: Some(audio.value.content_sha256),
                        });
                    }
                    Err(YtError::VideoUnavailable { reason, .. }) => {
                        eprintln!("skipped {}: {}", video.title, reason.user_message());
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            if optimize {
                if audio_inputs.is_empty() {
                    anyhow::bail!("none of the requested playlist tracks are available");
                }
                let mut config = Config::default();
                metric.apply(&mut config);
                let result = smooth_audio_inputs(
                    &audio_inputs,
                    &config,
                    &FeatureCache::new(cache_dir.join("derived-dsp")),
                    |progress| eprintln!("{progress:?}"),
                )?;
                println!("Optimized bottleneck order:");
                for (index, track) in result.ordered_tracks.iter().enumerate() {
                    println!("{}. {}", index + 1, track.path.display());
                }
                eprintln!(
                    "worst transition: {:.4}; mean: {:.4}",
                    result.bottleneck_cost, result.mean_cost
                );
            }
        }
    }
    Ok(())
}
