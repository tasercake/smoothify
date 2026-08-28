# loob 🫧

loob is a local-first playlist path optimizer. Choose audio files in the small Tauri desktop app and it orders every selected track exactly once while minimizing the **worst adjacent transition**.

The MVP is deliberately DSP-only: no neural models, embeddings, inference runtimes, uploads, or model downloads. Audio stays on the computer.

## How it works

1. Symphonia decodes WAV, MP3, FLAC, M4A/AAC, and Ogg audio in Rust and downmixes it to mono.
2. A deterministic FFT analysis extracts timestamped, overlapping 10-second feature chunks across each track, using a 9-second hop (one second of overlap), and retains a separate whole-track summary.
3. Directional transition costs compare the source track's last chunk with the destination track's first chunk using loudness/RMS, spectral centroid, rolloff, flatness, flux, zero-crossing rate, onset density, and chroma discontinuities. A lower-weight whole-track term discourages implausible global jumps.
4. An all-start greedy path seeds deterministic simulated annealing. Pure bottleneck cost is the default, preserving Smoothify's original minimax objective. Hybrid bottleneck/mean cost is explicit opt-in only.

The initial DSP deliberately does **not** claim a full tempo estimate or named musical key. Onset density and normalized chroma are reliable, inspectable primitives; tempo/key labeling can be added after evaluation against a transition-quality fixture set.

## Workspace

- `loob-core`: audio decoding, DSP analysis, feature cache, directed costs, and local-file orchestration.
- `loob-optim`: reusable deterministic greedy and simulated-annealing path optimizers.
- `loob-yt`: optional, explicitly networked YouTube manifest/audio cache around `yt-dlp`.
- `loob-cli`: local-file runner and YouTube cache inspection/population commands.
- `loob-tauri`: minimal native file picker, progress display, and ordered-results UI.

## Desktop app

Install the [Tauri v2 Linux prerequisites](https://v2.tauri.app/start/prerequisites/) for your distribution, then run:

```bash
cargo run -p loob-tauri
```

On Ubuntu, the important development packages include WebKitGTK 4.1, JavaScriptCoreGTK 4.1, and libsoup 3 development files. The core and CLI do not require those desktop libraries.

The Tauri app offers two source paths: use the OS file dialog for local audio, or paste a YouTube playlist URL and choose **Cache & optimize playlist**. Both paths store DSP features under the application cache directory, report per-track progress, preserve useful titles, and show the final order. They always use pure bottleneck mode in this MVP.

Each optimization creates a typed, per-command IPC progress channel before Rust work begins. Parallel audio and DSP workers feed a mutex-serialized per-run tracker: every update has a monotonic sequence, aggregate completed/total counters, outcome counts, and a bounded list of active tasks. The UI ignores stale sequences, so out-of-order worker completion cannot move a progress bar backwards or replace a newer snapshot. Fast cache hits are coalesced rather than flooding IPC. Playlist-source and usable-DSP track counts remain separate phases, superseded runs cannot overwrite the active run, and a channel delivery failure becomes a visible command error.

After either workflow finishes, the optimized order is available in the built-in local player. Choose a row to play it, or use play/pause, previous, next, and the seek slider. Playback advances through the optimized order and stops after the last track without wrapping. Previous restarts the current track after three seconds; earlier than that it selects the prior track. Starting a new optimization or changing the local selection stops playback and invalidates the previous media handles.

Submitting a YouTube URL is the explicit network action: the app canonicalizes and checks the manifest cache, downloads only missing validated audio through `yt-dlp`, then feeds those cached paths and their already-known content hashes into the same local DSP optimizer. New downloads prefer YouTube's native AAC/M4A and remain compact; if no supported native source is available, `yt-dlp`/FFmpeg converts the fallback to M4A rather than keeping decoded WAV. Repeated submissions reuse manifest, audio, and DSP caches without avoidable YouTube requests or full-audio hash passes. The desktop UI intentionally has no automatic or manual refresh action; use the CLI's explicit `--refresh` only when a refresh is actually desired. YouTube input requires `yt-dlp` and `ffmpeg` on the application path.

Playback does not enable Tauri's broad filesystem asset scope. Rust retains the selected and cached paths, canonicalizes only files from the latest successful optimized result, and gives the webview opaque generation-scoped media URLs. For WebKitGTK/GStreamer compatibility, a small Hyper HTTP/1 server listens only on IPv4 loopback at an OS-assigned port for the app lifetime. Each URL carries an OS-random per-process bearer secret and an exact current-result handle; the server validates its Host header, rejects traversal, unknown and stale generations, permits only GET/HEAD, and exposes no listing or metadata endpoints. Single byte ranges are supported and capped per response for scrubbing without loading a requested large range into memory. The CSP permits media only from IPv4 loopback, and audio is never uploaded or serialized through command JSON.

## CLI

```bash
cargo run -p loob-cli -- local song-a.flac song-b.mp3 song-c.wav

# Explicit hybrid objective, if desired:
cargo run -p loob-cli -- local --hybrid-bottleneck-weight 0.7 song-a.flac song-b.mp3
```

## YouTube cache testbed

YouTube access is separate from the local-file UI and is never implicit. The integration testbed is:

```text
https://www.youtube.com/watch?v=WfFnhlKTb3s&list=PLBQHjiaq5YpXxipRt4IZnB9D2z-nUU2Y4&pp=sAgC
```

Default invocation is offline/cache-only. An initial manifest fetch, bounded one-track audio smoke test, explicit refresh, and later offline reuse look like:

```bash
# Explicit network access; manifest only, no audio:
cargo run -p loob-cli -- youtube-cache "$URL" --populate

# Explicit network access; at most one audio file:
cargo run -p loob-cli -- youtube-cache "$URL" --populate --audio-limit 1

# Explicitly analyze and bottleneck-optimize that bounded subset:
cargo run -p loob-cli -- youtube-cache "$URL" --populate --audio-limit 5 --optimize

# Guaranteed cache-only inspection; errors rather than accessing the network:
cargo run -p loob-cli -- youtube-cache "$URL"

# Explicit bounded strong integrity scrub (never implicit in the desktop app):
cargo run -p loob-cli -- youtube-cache "$URL" --audio-limit 5 --verify

# Explicit replacement of the manifest and requested bounded audio entries:
cargo run -p loob-cli -- youtube-cache "$URL" --refresh --audio-limit 1
```

### Cache guarantees

The YouTube adapter keeps independent layers:

- `manifests/<canonical-url-sha256>.json`: playlist metadata, canonical playlist URL, and the original populating request. YouTube watch/playlist URL variants with the same validated `list` ID share one cache entry; unrelated single-video URLs remain distinct.
- `audio/objects/<content-sha256>.<actual-extension>`: immutable content-addressed audio objects. New entries are normally compact M4A; readable legacy WAV objects remain in place and are never automatically deleted. Refresh never overwrites a valid existing audio object.
- `audio/refs/<video-id>.json`: atomically replaced, validated per-video state. An available entry binds the canonical video ID to a confined content-addressed object and records the format, byte size, nanosecond modification timestamp, content hash, and provenance; a confirmed unavailable/private/removed entry records a safe reason category instead.
- `archives/<video-id>.txt`: per-video yt-dlp archives, avoiding concurrent writes from downloads protected by different video locks.
- `features/<audio-sha256>-<analysis-fingerprint>.json`: derived DSP only, keyed by audio bytes and the complete analysis/window fingerprint.

Each per-video audio reference is either positive provenance for a content-addressed audio object or a versioned negative record for a confirmed private, removed, or unavailable video. Both forms are validated and atomically replaced under the same per-video inter-process lock. Populate and Offline reuse negative records without contacting YouTube; Refresh explicitly retries them and replaces the reference if availability changes. Confirmed unavailable entries are skipped while usable playlist tracks continue. Network, authorization, extraction, malformed-output, tool, and other systemic failures still abort instead of being silently skipped.

Each cache key has an inter-process lock. JSON writers use cross-platform atomic replacement and `fsync`; audio objects use same-filesystem atomic rename and are never overwritten when valid. A normal reader validates the reference schema, canonical video ID, safe relative object path, expected extension/format, regular-file status, byte size, and stored modification timestamp. An unchanged fingerprint is a cache hit without reading the audio bytes. A changed fingerprint falls back to SHA-256; success atomically repairs metadata, while mismatch quarantines the reference and allows explicit Populate to recover it. `youtube-cache --verify --audio-limit N` performs an explicit bounded strong scrub. On their first updated access, legacy WAV references trust the established content-addressed binding, gather stat metadata, and atomically upgrade the small reference JSON without hashing, redownloading, or deleting the existing WAV. Partial files and orphaned audio without a committed reference are misses. Concurrent duplicate requests collapse behind the lock. `yt-dlp` uses continuation, no-overwrite behavior, and per-video download archives, but the validated application cache is authoritative. A successful archive skip that produces no staging audio is retried once without the archive; other failures are not retried automatically.

Playlist audio preparation uses at most three workers, keeping YouTube/FFmpeg concurrency conservative. DSP feature lookup and analysis uses up to four available CPU workers, retains source-index ordering when collecting results, and leaves seeded simulated annealing sequential and deterministic. A fatal worker result stops either pool from claiming more tracks while already-running work finishes, and deterministic result collection reports the earliest fatal source index that actually started. The YouTube cache passes the trusted object hash directly to the DSP cache; arbitrary local files calculate their hash once and reuse it during analysis.

For the CLI, `--optimize` requires a positive `--audio-limit`, so YouTube-to-DSP integration is always bounded. CLI default is offline/cache-only; `--populate` explicitly permits missing entries to download, and `--refresh` explicitly replaces requested entries. In Tauri, choosing **Cache & optimize playlist** is the explicit populate action: it downloads only missing entries, reuses cached manifest, audio, negative availability records, and DSP data, and never refreshes automatically. The result keeps optimizing when individual videos are confirmed unavailable and displays their titles, IDs, and safe reason categories in a separate skipped list. A playlist with no usable tracks returns a clear error.

## Verification

```bash
cargo fmt --all --check
cargo test --workspace
cargo check -p loob-tauri
node --check loob-tauri/ui/app.js
```

Tests use generated local fixtures and fake `yt-dlp` backends. They cover empty/single/two/multi-track inputs, complete deterministic permutations, directional costs, non-finite configuration, chroma silence, playlist URL canonicalization, compact-format commits, stat-only hits with zero full hashes, metadata fallback and repair, mismatch recovery, path confinement, hash-free legacy WAV migration, known-hash DSP hits, bounded parallelism with deterministic collection order, negative caching, distinct single-video identities and archives, explicit refresh, concurrent duplicate requests, and real loopback HTTP requests for media authorization, Host enforcement, stale-generation invalidation, traversal rejection, content types, methods, HEAD, and byte ranges.

## Current limits

- Quality weights are principled starting values, not yet calibrated by listening tests.
- Each DSP worker currently loads one decoded track into memory; concurrency is capped at four and whole-track FFT frame extraction remains bounded.
- YouTube extraction depends on the installed `yt-dlp` and `ffmpeg`, and users remain responsible for source terms and rights.
- Keep the cache on a local filesystem so file locks, atomic object renames, and durability guarantees retain their normal semantics.

MIT
