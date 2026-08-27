# loob 🫧

loob is a local-first playlist path optimizer. Choose audio files in the small Tauri desktop app and it orders every selected track exactly once while minimizing the **worst adjacent transition**.

The MVP is deliberately DSP-only: no neural models, embeddings, inference runtimes, uploads, or model downloads. Audio stays on the computer.

## How it works

1. Symphonia decodes WAV, MP3, FLAC, M4A/AAC, and Ogg audio in Rust and downmixes it to mono.
2. A deterministic FFT analysis summarizes each track's intro, outro, and whole-track character.
3. Directional `outro(A) → intro(B)` costs combine loudness/RMS, spectral centroid, rolloff, flatness, flux, zero-crossing rate, onset density, and chroma discontinuities. A lower-weight whole-track term discourages implausible global jumps.
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

Each optimization creates a typed, per-command IPC progress channel before Rust work begins. The UI distinguishes validation, cached/fetched manifests, cached/downloaded/skipped audio, cached/computed DSP features, optimization, player preparation, and completion. Playlist-source and usable-DSP track counts are separate progress phases, and superseded runs cannot overwrite the active run's display. A channel delivery failure becomes a visible command error instead of silently leaving the initial status on screen.

After either workflow finishes, the optimized order is available in the built-in local player. Choose a row to play it, or use play/pause, previous, next, and the seek slider. Playback advances through the optimized order and stops after the last track without wrapping. Previous restarts the current track after three seconds; earlier than that it selects the prior track. Starting a new optimization or changing the local selection stops playback and invalidates the previous media handles.

Submitting a YouTube URL is the explicit network action: the app canonicalizes and checks the manifest cache, downloads only missing validated WAV entries through `yt-dlp`, then feeds those cached paths into the same local DSP optimizer. Repeated submissions reuse manifest, audio, and DSP caches without avoidable YouTube requests. The desktop UI intentionally has no automatic or manual refresh action; use the CLI's explicit `--refresh` only when a refresh is actually desired. YouTube input requires `yt-dlp` and `ffmpeg` on the application path.

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

# Explicit replacement of the manifest and requested bounded audio entries:
cargo run -p loob-cli -- youtube-cache "$URL" --refresh --audio-limit 1
```

### Cache guarantees

The YouTube adapter keeps independent layers:

- `manifests/<canonical-url-sha256>.json`: playlist metadata, canonical playlist URL, and the original populating request. YouTube watch/playlist URL variants with the same validated `list` ID share one cache entry; unrelated single-video URLs remain distinct.
- `audio/objects/<content-sha256>.wav`: immutable content-addressed WAV objects. Refresh never overwrites an existing audio object.
- `audio/refs/<video-id>.json`: atomically replaced, validated per-video state. An available entry points to its content-addressed object and records its hashes and provenance; a confirmed unavailable/private/removed entry records a safe reason category instead.
- `archives/<video-id>.txt`: per-video yt-dlp archives, avoiding concurrent writes from downloads protected by different video locks.
- `features/<audio-sha256>-<analysis-fingerprint>.json`: derived DSP only, keyed by audio bytes and the complete analysis/window fingerprint.

Each per-video audio reference is either positive provenance for a content-addressed WAV or a versioned negative record for a confirmed private, removed, or unavailable video. Both forms are validated and atomically replaced under the same per-video inter-process lock. Populate and Offline reuse negative records without contacting YouTube; Refresh explicitly retries them and replaces the reference if availability changes. Confirmed unavailable entries are skipped while usable playlist tracks continue. Network, authorization, extraction, malformed-output, tool, and other systemic failures still abort instead of being silently skipped.

Each cache key has an inter-process lock. JSON writers use cross-platform atomic replacement and `fsync`; audio objects use same-filesystem atomic rename and are never overwritten when valid. Readers validate JSON, IDs, versions, canonical/source URLs, referenced object names, nonempty audio, and content hashes. Partial files and orphaned audio without a committed reference are misses; populate/refresh can reuse a valid orphaned content object and commit its pointer. Concurrent duplicate requests collapse behind the lock. `yt-dlp` uses continuation, no-overwrite behavior, and per-video download archives, but the validated application cache is authoritative. A successful archive skip that produces no staging WAV is retried once without the archive; other failures are not retried automatically.

For the CLI, `--optimize` requires a positive `--audio-limit`, so YouTube-to-DSP integration is always bounded. CLI default is offline/cache-only; `--populate` explicitly permits missing entries to download, and `--refresh` explicitly replaces requested entries. In Tauri, choosing **Cache & optimize playlist** is the explicit populate action: it downloads only missing entries, reuses cached manifest, audio, negative availability records, and DSP data, and never refreshes automatically. The result keeps optimizing when individual videos are confirmed unavailable and displays their titles, IDs, and safe reason categories in a separate skipped list. A playlist with no usable tracks returns a clear error.

## Verification

```bash
cargo fmt --all --check
cargo test --workspace
cargo check -p loob-tauri
node --check loob-tauri/ui/app.js
```

Tests use generated local WAV fixtures and fake `yt-dlp` backends. They cover empty/single/two/multi-track inputs, complete deterministic permutations, directional costs, non-finite configuration, chroma silence, playlist URL canonicalization, distinct single-video identities and archives, feature and manifest/audio cache behavior, orphan recovery, explicit refresh, cached-WAV optimization, concurrent duplicate requests without network access, and real loopback HTTP requests for media authorization, Host enforcement, stale-generation invalidation, traversal rejection, content types, methods, HEAD, and bounded/open/suffix/invalid byte ranges.

## Current limits

- Quality weights are principled starting values, not yet calibrated by listening tests.
- DSP analysis currently loads one decoded track into memory; feature extraction is bounded to a maximum number of whole-track FFT frames.
- YouTube extraction depends on the installed `yt-dlp` and `ffmpeg`, and users remain responsible for source terms and rights.
- Keep the cache on a local filesystem so file locks, atomic object renames, and durability guarantees retain their normal semantics.

MIT
