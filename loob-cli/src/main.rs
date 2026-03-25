use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "loob", about = "Reorder playlists for smooth transitions using audio embeddings")]
struct Cli {
    /// YouTube playlist URL
    #[arg(short, long)]
    playlist: String,

    /// Transition vs global weight (0.0-1.0)
    #[arg(long, default_value = "0.6")]
    alpha: f64,

    /// Bottleneck vs mean weight (0.0-1.0)
    #[arg(long, default_value = "0.3")]
    beta: f64,

    /// Head/tail window in seconds
    #[arg(long, default_value = "8.0")]
    window: f64,

    /// SA iterations
    #[arg(long, default_value = "500000")]
    iterations: usize,

    /// Download directory
    #[arg(short, long, default_value = "/tmp/loob-audio")]
    output_dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let config = loob_core::LoobConfig {
        alpha: cli.alpha,
        beta: cli.beta,
        window_secs: cli.window,
        sa_iterations: cli.iterations,
    };

    let _pipeline = loob_core::LoobPipeline::new(config);

    // TODO: instantiate embedder (Python sidecar) and run pipeline
    eprintln!("loob: embedder not yet configured — scaffold only");
    eprintln!("playlist: {}", cli.playlist);

    Ok(())
}
