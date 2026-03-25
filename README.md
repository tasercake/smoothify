# loob 🫧

Reorder playlists for buttery-smooth transitions using real audio embeddings.

Feed it a YouTube playlist. It downloads the audio, runs each track through
[MERT-330M](https://huggingface.co/m-a-p/MERT-v1-330M) for per-frame embeddings,
then solves an asymmetric bottleneck TSP to find the smoothest listening order —
matching the *end* of each track to the *beginning* of the next.

## Workspace

| Crate | Purpose |
|-------|---------|
| `loob-cli` | CLI frontend (`loob` binary) |
| `loob-core` | Orchestration library (download → embed → optimize) |
| `loob-optim` | Generic optimization algorithms (greedy NN, simulated annealing) |
| `loob-yt` | YouTube playlist fetching & audio download via yt-dlp |

## Usage

```bash
loob --playlist "https://www.youtube.com/playlist?list=PLxxxxx"
```

## Tuning knobs

- `--alpha` (0.6): weight on tail→head transition vs global track similarity
- `--beta` (0.3): weight on worst-edge vs mean-edge in SA objective
- `--window` (8.0): seconds for head/tail embedding windows
- `--iterations` (500000): simulated annealing steps

## Requirements

- [yt-dlp](https://github.com/yt-dlp/yt-dlp)
- Python + MERT-330M sidecar (TBD)
