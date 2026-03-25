# loob 🫧

Reorder playlists so tracks flow smoothly into each other.

Uses audio embeddings (MERT-330M) to compute per-frame representations of each track, then solves a directed TSP to minimize jarring transitions — weighting the *end* of one track against the *beginning* of the next.

## Architecture

```
loob-cli          → CLI frontend (clap). No logic, just glue.
loob-core         → Orchestration. Pulls everything together.
loob-optim        → Optimization algorithms (greedy NN, simulated annealing).
loob-yt           → YouTube interaction via yt-dlp.
```

## Usage

```bash
# Reorder a YouTube playlist
loob smooth "https://www.youtube.com/playlist?list=PLxxx"

# Just inspect metadata
loob inspect "https://www.youtube.com/playlist?list=PLxxx"
```

## Tunable knobs

- `--alpha` (0.6): transition vs global embedding weight
- `--beta` (0.3): bottleneck vs mean edge weight in SA objective  
- `--window` (10.0): seconds for head/tail embedding windows
- `--optimizer` (sa): `greedy` or `sa`

## Requirements

- [yt-dlp](https://github.com/yt-dlp/yt-dlp)
- Rust 2024 edition

## License

MIT
