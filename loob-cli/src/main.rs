use clap::{Parser, Subcommand};
use loob_core::{Config, OptimizerChoice};
use loob_core::embedding::RandomEmbeddingProvider;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "loob", about = "Reorder playlists so tracks flow smoothly into each other")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Smooth {
        url: String,
        #[arg(long, default_value = "0.6")]
        alpha: f64,
        #[arg(long, default_value = "0.3")]
        beta: f64,
        #[arg(long, default_value = "10.0")]
        window: f64,
        #[arg(long, default_value = "sa")]
        optimizer: String,
        #[arg(long, default_value = "downloads")]
        download_dir: PathBuf,
    },
    Inspect {
        url: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Smooth { url, alpha, beta, window, optimizer, download_dir } => {
            let config = Config {
                alpha, beta, head_seconds: window, tail_seconds: window,
                optimizer: match optimizer.as_str() { "greedy" => OptimizerChoice::GreedyNn, _ => OptimizerChoice::SimulatedAnnealing },
            };
            let provider = RandomEmbeddingProvider::default();
            let ordered = loob_core::smooth(&url, &config, &provider, &download_dir).await?;
            println!("Smoothed order:");
            for (i, t) in ordered.iter().enumerate() { println!("  {}. {}", i + 1, t.title); }
        }
        Commands::Inspect { url } => {
            let playlist = loob_yt::fetch_playlist(&url).await?;
            println!("{} - {} tracks", playlist.title, playlist.videos.len());
            for (i, v) in playlist.videos.iter().enumerate() { println!("  {}. {} ({:.0}s)", i + 1, v.title, v.duration); }
        }
    }
    Ok(())
}
