const { invoke, Channel } = window.__TAURI__.core;
const choose = document.querySelector('#choose');
const run = document.querySelector('#run');
const selection = document.querySelector('#selection');
const progress = document.querySelector('#progress');
const status = document.querySelector('#status');
const progressSummary = document.querySelector('#progress-summary');
const activeWork = document.querySelector('#active-work');
const activeTasks = document.querySelector('#active-tasks');
const results = document.querySelector('#results');
const skippedSection = document.querySelector('#skipped-section');
const skipped = document.querySelector('#skipped');
const youtubeForm = document.querySelector('#youtube-form');
const youtubeUrl = document.querySelector('#youtube-url');
const youtubeRun = document.querySelector('#youtube-run');
const audio = document.querySelector('#audio');
const nowPlaying = document.querySelector('#now-playing');
const playPause = document.querySelector('#play-pause');
const previous = document.querySelector('#previous');
const next = document.querySelector('#next');
const seek = document.querySelector('#seek');
const elapsed = document.querySelector('#elapsed');
const duration = document.querySelector('#duration');
const playbackStatus = document.querySelector('#playback-status');

const PREVIOUS_RESTART_SECONDS = 3;
let selectionCount = 0;
let playableTracks = [];
let activeIndex = -1;
let activeOperation = 0;

function setPhaseProgress(current, total) {
  progress.max = Math.max(total, 1);
  progress.value = Math.min(Math.max(current, 0), progress.max);
}

function clearActiveWork(clearSummary = false) {
  activeTasks.replaceChildren();
  activeWork.hidden = true;
  if (clearSummary) progressSummary.textContent = '';
}

function renderWorkSnapshot(payload) {
  setPhaseProgress(payload.completed, payload.total);
  const audio = payload.work_phase === 'audio';
  status.textContent = audio
    ? `Preparing playlist audio — ${payload.completed} of ${payload.total} complete.`
    : `Preparing DSP features — ${payload.completed} of ${payload.total} complete.`;
  const counters = payload.counters;
  const parts = audio
    ? [
        ['Reused', counters.cached],
        ['Downloaded', counters.downloaded],
        ['Skipped cached', counters.skipped_cached],
        ['Skipped', counters.skipped],
        ['Failed', counters.failed]
      ]
    : [
        ['Cached DSP', counters.cached],
        ['Computed', counters.computed],
        ['Failed', counters.failed]
      ];
  progressSummary.textContent = parts
    .filter(([, count]) => count > 0)
    .map(([label, count]) => `${label} ${count}`)
    .join(' · ');
  activeTasks.replaceChildren();
  for (const task of payload.active) {
    const item = document.createElement('li');
    const title = document.createElement('span');
    title.textContent = task.title;
    const state = document.createElement('span');
    state.className = 'task-state';
    state.textContent = task.state === 'analyzing' ? 'decoding / analyzing' : 'checking / downloading';
    item.append(title, state);
    activeTasks.appendChild(item);
  }
  activeWork.hidden = payload.active.length === 0;
}

function renderProgress(payload) {
  if (payload.phase === 'validating') {
    clearActiveWork(true);
    setPhaseProgress(0, payload.total);
    status.textContent = payload.source === 'youtube_playlist'
      ? 'Validating the YouTube playlist…'
      : `Validating ${payload.total} selected track${payload.total === 1 ? '' : 's'}…`;
  } else if (payload.phase === 'checking_manifest') {
    clearActiveWork(true);
    setPhaseProgress(0, 1);
    status.textContent = 'Checking the playlist manifest cache…';
  } else if (payload.phase === 'manifest_ready') {
    clearActiveWork(true);
    setPhaseProgress(1, 1);
    status.textContent = `${payload.was_cached ? 'Reused cached' : 'Fetched'} manifest for ${payload.title} — ${payload.total} source tracks.`;
  } else if (payload.phase === 'work_snapshot') {
    renderWorkSnapshot(payload);
  } else if (payload.phase === 'optimizing') {
    clearActiveWork();
    setPhaseProgress(0, 1);
    status.textContent = `Optimizing ${payload.total} usable tracks…`;
  } else if (payload.phase === 'preparing_player') {
    clearActiveWork();
    setPhaseProgress(0, 1);
    status.textContent = `Preparing ${payload.total} optimized tracks for local playback…`;
  } else if (payload.phase === 'completed') {
    clearActiveWork();
    setPhaseProgress(1, 1);
    status.textContent = `Completed ${payload.total} tracks.`;
  }
}

function beginOperation(initialStatus) {
  const id = ++activeOperation;
  setPhaseProgress(0, 1);
  status.textContent = initialStatus;
  if (typeof Channel !== 'function') {
    throw new Error('Progress updates are unavailable in this application build.');
  }
  const channel = new Channel();
  let lastSequence = 0;
  channel.onmessage = (payload) => {
    if (id !== activeOperation || payload.seq <= lastSequence) return;
    lastSequence = payload.seq;
    renderProgress(payload);
  };
  return { id, channel };
}

function setBusy(busy) {
  choose.disabled = busy;
  run.disabled = busy || selectionCount === 0;
  youtubeUrl.disabled = busy;
  youtubeRun.disabled = busy;
}

function formatTime(value) {
  if (!Number.isFinite(value) || value < 0) return '0:00';
  const totalSeconds = Math.floor(value);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = String(totalSeconds % 60).padStart(2, '0');
  return `${minutes}:${seconds}`;
}

function mediaErrorDetail() {
  const code = audio.error?.code;
  const categories = {
    1: 'playback aborted',
    2: 'media network error',
    3: 'audio decode error',
    4: 'audio source not supported'
  };
  return code ? `${categories[code] || 'unknown media error'} (code ${code})` : 'player request rejected';
}

function updateActiveTrack() {
  for (const [index, item] of [...results.children].entries()) {
    const active = index === activeIndex;
    item.classList.toggle('active', active);
    item.querySelector('button')?.setAttribute('aria-current', active ? 'true' : 'false');
  }
}

function updatePlayerControls() {
  const empty = playableTracks.length === 0 || activeIndex < 0;
  playPause.disabled = empty;
  previous.disabled = empty;
  next.disabled = empty || activeIndex >= playableTracks.length - 1;
  playPause.textContent = audio.paused ? 'Play' : 'Pause';
  const mediaDuration = Number.isFinite(audio.duration) ? audio.duration : 0;
  seek.disabled = empty || mediaDuration <= 0;
  seek.max = String(mediaDuration);
  seek.value = String(Math.min(audio.currentTime || 0, mediaDuration));
  elapsed.textContent = formatTime(audio.currentTime);
  duration.textContent = formatTime(mediaDuration);
  nowPlaying.textContent = empty ? 'No optimized playlist yet' : playableTracks[activeIndex].title;
  updateActiveTrack();
}

function resetPlayback(clearPlaylist = true) {
  audio.pause();
  audio.removeAttribute('src');
  audio.load();
  if (clearPlaylist) {
    playableTracks = [];
    activeIndex = -1;
    playbackStatus.textContent = 'Optimize a playlist to enable playback.';
  }
  seek.value = '0';
  seek.max = '0';
  updatePlayerControls();
}

function selectTrack(index, autoplay) {
  if (index < 0 || index >= playableTracks.length) return;
  audio.pause();
  activeIndex = index;
  audio.src = playableTracks[index].media_url;
  audio.load();
  playbackStatus.textContent = autoplay ? 'Starting playback…' : 'Ready to play.';
  updatePlayerControls();
  if (autoplay) playCurrent();
}

async function playCurrent() {
  if (playableTracks.length === 0) return;
  if (activeIndex < 0) selectTrack(0, false);
  try {
    await audio.play();
    playbackStatus.textContent = 'Playing.';
  } catch (_error) {
    audio.pause();
    playbackStatus.textContent = `Playback could not start: ${mediaErrorDetail()}.`;
  }
  updatePlayerControls();
}

function renderResult(result) {
  results.replaceChildren();
  playableTracks = result.ordered_tracks;
  for (const [index, track] of playableTracks.entries()) {
    const item = document.createElement('li');
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'track-button';
    button.textContent = track.title;
    button.addEventListener('click', () => selectTrack(index, true));
    item.appendChild(button);
    results.appendChild(item);
  }
  if (playableTracks.length > 0) selectTrack(0, false);
}

function clearSkipped() {
  skipped.replaceChildren();
  skippedSection.hidden = true;
}

function renderSkipped(tracks) {
  clearSkipped();
  for (const track of tracks) {
    const item = document.createElement('li');
    const reason = track.reason.replaceAll('_', ' ');
    item.textContent = `${track.title} (${track.video_id}) — ${reason}`;
    skipped.appendChild(item);
  }
  skippedSection.hidden = tracks.length === 0;
}

choose.addEventListener('click', async () => {
  activeOperation += 1;
  resetPlayback();
  results.replaceChildren();
  clearSkipped();
  setBusy(true);
  try {
    const response = await invoke('choose_audio_files');
    selectionCount = response.titles.length;
    selection.textContent = selectionCount ? `${selectionCount} tracks selected.` : 'No files selected.';
    status.textContent = 'Ready.';
  } catch (error) {
    selectionCount = 0;
    selection.textContent = 'No files selected.';
    status.textContent = `Could not choose audio files: ${error}`;
  } finally {
    setBusy(false);
  }
});

run.addEventListener('click', async () => {
  resetPlayback();
  setBusy(true);
  results.replaceChildren();
  clearSkipped();
  progress.max = selectionCount;
  progress.value = 0;
  let operation;
  try {
    operation = beginOperation('Starting local analysis…');
    const result = await invoke('order_audio_files', {
      hybridBottleneckWeight: null,
      onProgress: operation.channel
    });
    if (operation.id !== activeOperation) return;
    renderResult(result);
    status.textContent = `Done. Worst transition cost: ${result.bottleneck_cost.toFixed(3)}.`;
  } catch (error) {
    if (!operation || operation.id === activeOperation) {
      status.textContent = `Could not order these tracks: ${error}`;
    }
  } finally {
    if (!operation || operation.id === activeOperation) setBusy(false);
  }
});

youtubeForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  const url = youtubeUrl.value.trim();
  if (!url) {
    status.textContent = 'Enter a YouTube playlist URL.';
    youtubeUrl.focus();
    return;
  }
  resetPlayback();
  setBusy(true);
  results.replaceChildren();
  clearSkipped();
  progress.max = 1;
  progress.value = 0;
  status.textContent = 'Checking the playlist…';
  let operation;
  try {
    operation = beginOperation('Starting playlist preparation…');
    const response = await invoke('order_youtube_playlist', {
      request: { url },
      onProgress: operation.channel
    });
    if (operation.id !== activeOperation) return;
    renderResult(response.result);
    renderSkipped(response.skipped_tracks);
    const skippedSummary = response.skipped_tracks.length
      ? ` Skipped ${response.skipped_tracks.length} unavailable track${response.skipped_tracks.length === 1 ? '' : 's'}.`
      : '';
    status.textContent = `Done: ${response.playlist_title}. Worst transition cost: ${response.result.bottleneck_cost.toFixed(3)}.${skippedSummary}`;
  } catch (error) {
    if (!operation || operation.id === activeOperation) {
      status.textContent = `Could not optimize this playlist: ${error}`;
    }
  } finally {
    if (!operation || operation.id === activeOperation) setBusy(false);
  }
});

playPause.addEventListener('click', () => {
  if (audio.paused) playCurrent();
  else audio.pause();
});

previous.addEventListener('click', () => {
  if (audio.currentTime > PREVIOUS_RESTART_SECONDS || activeIndex === 0) {
    audio.currentTime = 0;
    if (audio.paused) playCurrent();
  } else {
    selectTrack(activeIndex - 1, true);
  }
});

next.addEventListener('click', () => {
  if (activeIndex < playableTracks.length - 1) selectTrack(activeIndex + 1, true);
});

seek.addEventListener('input', () => {
  if (Number.isFinite(audio.duration)) audio.currentTime = Number(seek.value);
  elapsed.textContent = formatTime(Number(seek.value));
});

audio.addEventListener('play', updatePlayerControls);
audio.addEventListener('pause', updatePlayerControls);
audio.addEventListener('loadedmetadata', updatePlayerControls);
audio.addEventListener('durationchange', updatePlayerControls);
audio.addEventListener('timeupdate', updatePlayerControls);
audio.addEventListener('ended', () => {
  if (activeIndex < playableTracks.length - 1) {
    selectTrack(activeIndex + 1, true);
  } else {
    playbackStatus.textContent = 'Reached the end of the optimized playlist.';
    updatePlayerControls();
  }
});
audio.addEventListener('error', () => {
  audio.pause();
  playbackStatus.textContent = `This track could not be played: ${mediaErrorDetail()}. The optimized order is still available.`;
  updatePlayerControls();
});

resetPlayback();
