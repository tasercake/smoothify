use clap::{Parser, Subcommand};
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
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Commands::Local {
            files,
            cache_dir,
            hybrid_bottleneck_weight,
        } => {
            let mut config = Config::default();
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
                let result = smooth_audio_inputs(
                    &audio_inputs,
                    &Config::default(),
                    &FeatureCache::new(cache_dir.join("derived-dsp")),
                    |progress| eprintln!("{progress:?}"),
                )?;
                println!("Optimized bottleneck order:");
                for (index, track) in result.ordered_tracks.iter().enumerate() {
                    println!("{}. {}", index + 1, track.path.display());
                }
                eprintln!("worst transition: {:.4}", result.bottleneck_cost);
            }
        }
    }
    Ok(())
}
