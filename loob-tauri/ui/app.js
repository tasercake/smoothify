const { invoke, Channel } = window.__TAURI__.core;
const choose = document.querySelector('#choose');
const run = document.querySelector('#run');
const selection = document.querySelector('#selection');
const progress = document.querySelector('#progress');
const status = document.querySelector('#status');
const progressSummary = document.querySelector('#progress-summary');
const activeWork = document.querySelector('#active-work');
const activeTasks = document.querySelector('#active-tasks');
const annealingTelemetry = document.querySelector('#annealing-telemetry');
const annealingLog = document.querySelector('#annealing-log');
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
const visualizationSection = document.querySelector('#visualization-section');
const visualizationCanvasWrap = document.querySelector('#visualization-canvas-wrap');
const visualizationCanvas = document.querySelector('#visualization-canvas');
const visualizationTooltip = document.querySelector('#visualization-tooltip');
const visualizationLegend = document.querySelector('#visualization-legend');
const projectionDescription = document.querySelector('#projection-description');
const colorByTrack = document.querySelector('#color-by-track');
const colorBySequence = document.querySelector('#color-by-sequence');
const pointWindowBar = document.querySelector('#point-window-bar');
const pointWindowSelection = document.querySelector('#point-window-selection');
const pointWindowStart = document.querySelector('#point-window-start');
const pointWindowFinish = document.querySelector('#point-window-finish');
const pointWindowStartValue = document.querySelector('#point-window-start-value');
const pointWindowFinishValue = document.querySelector('#point-window-finish-value');
const pointWindowCount = document.querySelector('#point-window-count');

const PREVIOUS_RESTART_SECONDS = 3;
const POINT_WINDOW_STEP = 0.1;
const TRACK_HUE_STEP = 137.508;
let selectionCount = 0;
let playableTracks = [];
let activeIndex = -1;
let activeOperation = 0;
let playlistVisualization = null;
let visualizationColorMode = 'track';
let visualizationScreenPoints = [];
let hoveredVisualizationPoint = null;
let lastVisualizedActiveIndex = -2;
let pendingVisualizationSeek = null;
let visualizationWindowStart = 0;
let visualizationWindowFinish = 100;
let pointWindowDrag = null;
let annealingHeaderWritten = false;

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

function resetAnnealingTelemetry() {
  annealingHeaderWritten = false;
  annealingLog.textContent = '';
  annealingTelemetry.hidden = true;
}

function prepareAnnealingTelemetry(total) {
  annealingHeaderWritten = false;
  annealingTelemetry.hidden = false;
  annealingLog.textContent = `status=preparing greedy seed and annealing for ${total} track${total === 1 ? '' : 's'}…\n`;
}

function formatTelemetryNumber(value) {
  return Number.isFinite(value) ? value.toExponential(8) : String(value);
}

function renderAnnealingTelemetry(payload) {
  annealingTelemetry.hidden = false;
  if (!annealingHeaderWritten) {
    annealingLog.textContent = [
      `objective=${payload.objective} seed=${payload.seed} iterations=${payload.iterations} report_every=${payload.report_every}`,
      `initial_temperature=${formatTelemetryNumber(payload.initial_temperature)} cooling_rate=${payload.cooling_rate.toFixed(8)}`,
      ''
    ].join('\n');
    annealingHeaderWritten = true;
  }
  const acceptance = payload.attempted_moves === 0
    ? 0
    : payload.accepted_moves / payload.attempted_moves * 100;
  const width = String(payload.iterations).length;
  annealingLog.textContent += [
    `iter=${String(payload.iteration).padStart(width)} / ${payload.iterations}`,
    `temp=${formatTelemetryNumber(payload.temperature)}`,
    `current_loss=${formatTelemetryNumber(payload.current_loss)}`,
    `best_loss=${formatTelemetryNumber(payload.best_loss)}`,
    `accepted=${payload.accepted_moves}/${payload.attempted_moves} (${acceptance.toFixed(2)}%)`
  ].join('  ') + '\n';
  if (payload.iteration === payload.iterations) {
    annealingLog.textContent += `status=complete best_loss=${formatTelemetryNumber(payload.best_loss)}\n`;
  }
  annealingLog.scrollTop = annealingLog.scrollHeight;
}

function renderAnnealingSkipped(payload) {
  annealingHeaderWritten = true;
  annealingTelemetry.hidden = false;
  annealingLog.textContent = `status=skipped tracks=${payload.total} reason=${payload.reason}\n`;
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
    prepareAnnealingTelemetry(payload.total);
  } else if (payload.phase === 'annealing') {
    clearActiveWork();
    setPhaseProgress(payload.iteration, payload.iterations);
    status.textContent = `Annealing playlist order — iteration ${payload.iteration} of ${payload.iterations}.`;
    renderAnnealingTelemetry(payload);
  } else if (payload.phase === 'annealing_skipped') {
    clearActiveWork();
    setPhaseProgress(1, 1);
    status.textContent = 'Annealing was not needed for this playlist.';
    renderAnnealingSkipped(payload);
  } else if (payload.phase === 'projecting_features') {
    clearActiveWork();
    setPhaseProgress(0, 1);
    status.textContent = `Projecting ${payload.chunks} DSP chunks from ${payload.tracks} tracks…`;
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
  resetAnnealingTelemetry();
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

function optimizedTrackHue(sequenceIndex) {
  return (Number(sequenceIndex) * TRACK_HUE_STEP + 18) % 360;
}

function stableTrackColor(track) {
  return `hsl(${optimizedTrackHue(track.sequence_index).toFixed(1)} 82% 68%)`;
}

function sequenceColor(index, count) {
  const anchors = [
    [68, 1, 84],
    [59, 82, 139],
    [33, 145, 140],
    [94, 201, 98],
    [253, 231, 37]
  ];
  const fraction = count <= 1 ? 0.5 : index / (count - 1);
  const scaled = Math.min(Math.max(fraction, 0), 1) * (anchors.length - 1);
  const lower = Math.floor(scaled);
  const upper = Math.min(lower + 1, anchors.length - 1);
  const mix = scaled - lower;
  const rgb = anchors[lower].map((value, channel) =>
    Math.round(value + (anchors[upper][channel] - value) * mix)
  );
  return `rgb(${rgb.join(', ')})`;
}

function visualizationPointColor(track, point, visiblePointCount) {
  return visualizationColorMode === 'sequence'
    ? sequenceColor(point.visible_sequence_index, visiblePointCount)
    : stableTrackColor(track);
}

function visualizationPointCount() {
  return playlistVisualization
    ? playlistVisualization.tracks.reduce((total, track) => total + track.points.length, 0)
    : 0;
}

function minimumPointWindowSpan() {
  const total = visualizationPointCount();
  return total > 0 ? Math.min(100, Math.max(POINT_WINDOW_STEP, 100 / total)) : POINT_WINDOW_STEP;
}

function pointWindowIndexes(total = visualizationPointCount()) {
  if (total === 0) return [0, 0];
  const start = Math.min(total - 1, Math.floor(total * visualizationWindowStart / 100));
  const finish = Math.min(total, Math.max(start + 1, Math.ceil(total * visualizationWindowFinish / 100)));
  return [start, finish];
}

function formatPointWindowPercent(value) {
  const rounded = Math.round(value * 10) / 10;
  return `${Number.isInteger(rounded) ? rounded.toFixed(0) : rounded.toFixed(1)}%`;
}

function updatePointWindow(start, finish, redraw = true) {
  const minimumSpan = minimumPointWindowSpan();
  visualizationWindowStart = Math.min(Math.max(start, 0), 100 - minimumSpan);
  visualizationWindowFinish = Math.max(Math.min(finish, 100), visualizationWindowStart + minimumSpan);
  visualizationWindowStart = Math.round(visualizationWindowStart / POINT_WINDOW_STEP) * POINT_WINDOW_STEP;
  visualizationWindowFinish = Math.round(visualizationWindowFinish / POINT_WINDOW_STEP) * POINT_WINDOW_STEP;

  pointWindowStart.value = String(visualizationWindowStart);
  pointWindowFinish.value = String(visualizationWindowFinish);
  pointWindowStartValue.value = formatPointWindowPercent(visualizationWindowStart);
  pointWindowFinishValue.value = formatPointWindowPercent(visualizationWindowFinish);
  pointWindowSelection.style.left = `${visualizationWindowStart}%`;
  pointWindowSelection.style.width = `${visualizationWindowFinish - visualizationWindowStart}%`;

  const total = visualizationPointCount();
  const [startIndex, finishIndex] = pointWindowIndexes(total);
  const visible = finishIndex - startIndex;
  pointWindowCount.textContent = total > 0
    ? `${(startIndex + 1).toLocaleString()}–${finishIndex.toLocaleString()} of ${total.toLocaleString()}`
    : '0 points';
  pointWindowStart.setAttribute('aria-valuetext', `Start ${formatPointWindowPercent(visualizationWindowStart)}, point ${total > 0 ? startIndex + 1 : 0}`);
  pointWindowFinish.setAttribute('aria-valuetext', `Finish ${formatPointWindowPercent(visualizationWindowFinish)}, point ${finishIndex}`);
  pointWindowSelection.setAttribute('aria-label', `Move visible points ${total > 0 ? startIndex + 1 : 0} through ${finishIndex} together`);

  hoveredVisualizationPoint = null;
  visualizationTooltip.hidden = true;
  if (redraw) drawVisualization();
}

function visibleVisualizationTracks() {
  const [startIndex, finishIndex] = pointWindowIndexes();
  let globalIndex = 0;
  let visibleSequenceIndex = 0;
  return playlistVisualization.tracks
    .map((track) => {
      const points = track.points.flatMap((point, trackPointIndex) => {
        const visible = globalIndex >= startIndex && globalIndex < finishIndex;
        globalIndex += 1;
        if (!visible) return [];
        const visiblePoint = {
          ...point,
          visible_sequence_index: visibleSequenceIndex,
          track_point_index: trackPointIndex,
          track_point_count: track.points.length,
          is_track_endpoint: trackPointIndex === 0 || trackPointIndex === track.points.length - 1
        };
        visibleSequenceIndex += 1;
        return [visiblePoint];
      });
      return { ...track, points };
    })
    .filter((track) => track.points.length > 0);
}

function clearVisualization() {
  playlistVisualization = null;
  visualizationScreenPoints = [];
  hoveredVisualizationPoint = null;
  lastVisualizedActiveIndex = -2;
  visualizationTooltip.hidden = true;
  visualizationLegend.replaceChildren();
  updatePointWindow(0, 100, false);
  visualizationSection.hidden = true;
}

function renderVisualizationLegend() {
  visualizationLegend.replaceChildren();
  if (!playlistVisualization) return;
  if (visualizationColorMode === 'sequence') {
    const first = document.createElement('span');
    first.textContent = 'Visible first';
    const gradient = document.createElement('span');
    gradient.className = 'sequence-gradient';
    gradient.setAttribute('aria-hidden', 'true');
    const last = document.createElement('span');
    last.textContent = 'Visible last';
    visualizationLegend.append(first, gradient, last);
  } else {
    visualizationLegend.textContent = 'Each track keeps one color; neighboring optimized tracks are deliberately separated in hue.';
  }
}

function renderVisualization(data) {
  playlistVisualization = data;
  hoveredVisualizationPoint = null;
  lastVisualizedActiveIndex = -2;
  const pointCount = data.tracks.reduce((total, track) => total + track.points.length, 0);
  projectionDescription.textContent = `${pointCount} timestamped chunks projected with metric MDS from the same normalized DSP feature geometry used by optimization. Axes are display dimensions without physical units.`;
  visualizationSection.hidden = false;
  updatePointWindow(0, 100, false);
  renderVisualizationLegend();
  requestAnimationFrame(resizeVisualizationCanvas);
}

function paddedDomain(values) {
  let minimum = Infinity;
  let maximum = -Infinity;
  for (const value of values) {
    minimum = Math.min(minimum, value);
    maximum = Math.max(maximum, value);
  }
  if (!Number.isFinite(minimum) || !Number.isFinite(maximum)) return [-1, 1];
  if (Math.abs(maximum - minimum) < 1e-12) {
    const padding = Math.max(Math.abs(minimum) * 0.1, 0.5);
    return [minimum - padding, maximum + padding];
  }
  const padding = (maximum - minimum) * 0.08;
  return [minimum - padding, maximum + padding];
}

function formatAxisValue(value, tickStep) {
  const absoluteStep = Math.abs(tickStep);
  const decimals = Number.isFinite(absoluteStep) && absoluteStep > 0
    ? Math.min(12, Math.max(0, Math.ceil(-Math.log10(absoluteStep)) + 1))
    : 2;
  const rounded = Number(value.toFixed(decimals));
  const normalized = Object.is(rounded, -0) ? 0 : rounded;
  return normalized.toFixed(decimals);
}

function drawSegmentArrowhead(context, from, to, color, position, size) {
  const dx = to.screenX - from.screenX;
  const dy = to.screenY - from.screenY;
  const distance = Math.hypot(dx, dy);
  if (distance < size * 1.5) return;
  const unitX = dx / distance;
  const unitY = dy / distance;
  const tipX = from.screenX + dx * position;
  const tipY = from.screenY + dy * position;
  const baseX = tipX - unitX * size;
  const baseY = tipY - unitY * size;
  const halfWidth = size * 0.52;
  context.beginPath();
  context.moveTo(tipX, tipY);
  context.lineTo(baseX - unitY * halfWidth, baseY + unitX * halfWidth);
  context.lineTo(baseX + unitY * halfWidth, baseY - unitX * halfWidth);
  context.closePath();
  context.fillStyle = color;
  context.fill();
}

function drawDirectionalSegment(context, from, to, color, lineWidth, alpha) {
  context.save();
  context.globalAlpha = alpha;
  context.strokeStyle = color;
  context.lineWidth = lineWidth;
  context.setLineDash([5, 5]);
  context.beginPath();
  context.moveTo(from.screenX, from.screenY);
  context.lineTo(to.screenX, to.screenY);
  context.stroke();
  context.setLineDash([]);
  drawSegmentArrowhead(context, from, to, color, 0.62, Math.max(6.75, lineWidth * 4.5));
  context.restore();
}

function drawTrackTransition(context, from, to, color) {
  const distance = Math.hypot(to.screenX - from.screenX, to.screenY - from.screenY);
  if (distance < 1) return;
  context.save();
  context.globalAlpha = 1;
  context.strokeStyle = color;
  context.fillStyle = color;
  context.lineWidth = 1.2;
  context.beginPath();
  context.moveTo(from.screenX, from.screenY);
  context.lineTo(to.screenX, to.screenY);
  context.stroke();
  drawSegmentArrowhead(context, from, to, color, 0.86, 9);
  context.restore();
}

function drawVisualization() {
  if (!playlistVisualization || visualizationSection.hidden) return;
  const context = visualizationCanvas.getContext('2d');
  const width = visualizationCanvas.clientWidth;
  const height = visualizationCanvas.clientHeight;
  if (!context || width <= 0 || height <= 0) return;
  context.clearRect(0, 0, width, height);

  const visibleTracks = visibleVisualizationTracks();
  const points = visibleTracks.flatMap((track) => track.points);
  if (points.length === 0) return;
  const [minimumX, maximumX] = paddedDomain(points.map((point) => point.x));
  const [minimumY, maximumY] = paddedDomain(points.map((point) => point.y));
  const margins = { left: 54, right: 16, top: 18, bottom: 48 };
  const plotWidth = Math.max(width - margins.left - margins.right, 1);
  const plotHeight = Math.max(height - margins.top - margins.bottom, 1);
  const mapX = (value) => margins.left + ((value - minimumX) / (maximumX - minimumX)) * plotWidth;
  const mapY = (value) => margins.top + (1 - (value - minimumY) / (maximumY - minimumY)) * plotHeight;
  const foreground = '#b8bbb7';
  const border = '#454b4c';
  const grid = '#252a2c';

  context.save();
  context.font = '11px Inter, system-ui, sans-serif';
  context.fillStyle = foreground;
  context.strokeStyle = grid;
  context.lineWidth = 1;
  const ticks = width < 430 ? 4 : 5;
  const xTickStep = (maximumX - minimumX) / (ticks - 1);
  const yTickStep = (maximumY - minimumY) / (ticks - 1);
  for (let index = 0; index < ticks; index += 1) {
    const fraction = index / (ticks - 1);
    const x = margins.left + fraction * plotWidth;
    const y = margins.top + (1 - fraction) * plotHeight;
    context.beginPath();
    context.moveTo(x, margins.top);
    context.lineTo(x, margins.top + plotHeight);
    context.moveTo(margins.left, y);
    context.lineTo(margins.left + plotWidth, y);
    context.stroke();
    const xValue = minimumX + fraction * (maximumX - minimumX);
    const yValue = minimumY + fraction * (maximumY - minimumY);
    context.textAlign = 'center';
    context.textBaseline = 'top';
    context.fillText(formatAxisValue(xValue, xTickStep), x, margins.top + plotHeight + 7);
    context.textAlign = 'right';
    context.textBaseline = 'middle';
    context.fillText(formatAxisValue(yValue, yTickStep), margins.left - 7, y);
  }
  context.strokeStyle = border;
  context.strokeRect(margins.left, margins.top, plotWidth, plotHeight);
  context.textAlign = 'center';
  context.textBaseline = 'bottom';
  context.fillText(playlistVisualization.x_axis_label, margins.left + plotWidth / 2, height - 2);
  context.save();
  context.translate(12, margins.top + plotHeight / 2);
  context.rotate(-Math.PI / 2);
  context.fillText(playlistVisualization.y_axis_label, 0, 0);
  context.restore();

  const screenTracks = visibleTracks.map((track) => ({
    track,
    screenPoints: track.points.map((point) => ({
      track,
      point,
      screenX: mapX(point.x),
      screenY: mapY(point.y)
    }))
  }));
  visualizationScreenPoints = screenTracks.flatMap(({ screenPoints }) => screenPoints);

  const sequenceOrderedTracks = [...screenTracks].sort((a, b) => a.track.sequence_index - b.track.sequence_index);
  for (let index = 0; index < sequenceOrderedTracks.length - 1; index += 1) {
    const current = sequenceOrderedTracks[index];
    const nextTrack = sequenceOrderedTracks[index + 1];
    if (nextTrack.track.sequence_index !== current.track.sequence_index + 1) continue;
    const lastPoint = current.screenPoints.find(({ point }) => point.track_point_index === point.track_point_count - 1);
    const firstPoint = nextTrack.screenPoints.find(({ point }) => point.track_point_index === 0);
    if (lastPoint && firstPoint) {
      let transitionColor = '#8be0bd';
      if (visualizationColorMode === 'track') {
        transitionColor = context.createLinearGradient(lastPoint.screenX, lastPoint.screenY, firstPoint.screenX, firstPoint.screenY);
        transitionColor.addColorStop(0, stableTrackColor(current.track));
        transitionColor.addColorStop(1, stableTrackColor(nextTrack.track));
      }
      drawTrackTransition(context, lastPoint, firstPoint, transitionColor);
    }
  }

  const orderedTracks = [...screenTracks].sort((a, b) => {
    if (a.track.sequence_index === activeIndex) return 1;
    if (b.track.sequence_index === activeIndex) return -1;
    return a.track.sequence_index - b.track.sequence_index;
  });
  for (const { track, screenPoints } of orderedTracks) {
    const active = track.sequence_index === activeIndex;
    if (screenPoints.length > 1) {
      const lineWidth = active ? 2.4 : 1.35;
      const lineAlpha = (active ? 0.95 : 0.45) * 0.5;
      for (let index = 1; index < screenPoints.length; index += 1) {
        const previousPoint = screenPoints[index - 1];
        const currentPoint = screenPoints[index];
        if (currentPoint.point.track_point_index !== previousPoint.point.track_point_index + 1) continue;
        const color = visualizationColorMode === 'sequence'
          ? sequenceColor((previousPoint.point.visible_sequence_index + currentPoint.point.visible_sequence_index) / 2, points.length)
          : stableTrackColor(track);
        drawDirectionalSegment(context, previousPoint, currentPoint, color, lineWidth, lineAlpha);
      }
    }
    const endpointAlpha = active ? 1 : 0.78;
    for (const screenPoint of screenPoints) {
      context.globalAlpha = screenPoint.point.is_track_endpoint ? endpointAlpha : endpointAlpha * 0.32;
      context.fillStyle = visualizationPointColor(track, screenPoint.point, points.length);
      context.beginPath();
      context.arc(screenPoint.screenX, screenPoint.screenY, active ? 3.8 : 2.7, 0, Math.PI * 2);
      context.fill();
    }
  }
  context.globalAlpha = 1;
  if (hoveredVisualizationPoint) {
    const hovered = visualizationScreenPoints.find(({ track, point }) =>
      track.sequence_index === hoveredVisualizationPoint.track.sequence_index &&
      point.chunk_index === hoveredVisualizationPoint.point.chunk_index
    );
    if (hovered) {
      context.beginPath();
      context.arc(hovered.screenX, hovered.screenY, 7, 0, Math.PI * 2);
      context.strokeStyle = '#f6f3ea';
      context.lineWidth = 1.5;
      context.stroke();
    }
  }
  context.restore();
}

function resizeVisualizationCanvas() {
  if (!playlistVisualization || visualizationSection.hidden) return;
  const width = Math.max(visualizationCanvasWrap.clientWidth, 1);
  const height = Math.min(Math.max(width * 0.68, 300), 800);
  const ratio = Math.min(window.devicePixelRatio || 1, 3);
  visualizationCanvas.style.height = `${height}px`;
  visualizationCanvas.width = Math.round(width * ratio);
  visualizationCanvas.height = Math.round(height * ratio);
  const context = visualizationCanvas.getContext('2d');
  context.setTransform(ratio, 0, 0, ratio, 0, 0);
  drawVisualization();
}

function nearestVisualizationPoint(x, y) {
  let nearest = null;
  let nearestSquared = 12 * 12;
  for (const candidate of visualizationScreenPoints) {
    const dx = candidate.screenX - x;
    const dy = candidate.screenY - y;
    const squared = dx * dx + dy * dy;
    if (squared <= nearestSquared) {
      nearest = candidate;
      nearestSquared = squared;
    }
  }
  return nearest;
}

function showVisualizationTooltip(candidate) {
  const midpoint = (candidate.point.start_seconds + candidate.point.end_seconds) / 2;
  visualizationTooltip.replaceChildren();
  const title = document.createElement('strong');
  title.textContent = `${candidate.track.sequence_index + 1}. ${candidate.track.title}`;
  const windowText = document.createElement('span');
  windowText.textContent = `${formatTime(candidate.point.start_seconds)}–${formatTime(candidate.point.end_seconds)} · click to seek ${formatTime(midpoint)}`;
  visualizationTooltip.append(title, windowText);
  visualizationTooltip.hidden = false;
  const tooltipWidth = visualizationTooltip.offsetWidth;
  const tooltipHeight = visualizationTooltip.offsetHeight;
  const left = Math.min(Math.max(candidate.screenX + 12, 6), visualizationCanvas.clientWidth - tooltipWidth - 6);
  const top = Math.min(Math.max(candidate.screenY - tooltipHeight - 10, 6), visualizationCanvas.clientHeight - tooltipHeight - 6);
  visualizationTooltip.style.left = `${left}px`;
  visualizationTooltip.style.top = `${top}px`;
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
  if (lastVisualizedActiveIndex !== activeIndex) {
    lastVisualizedActiveIndex = activeIndex;
    drawVisualization();
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
    pendingVisualizationSeek = null;
    playbackStatus.textContent = 'Optimize a playlist to enable playback.';
  }
  seek.value = '0';
  seek.max = '0';
  updatePlayerControls();
}

function selectTrack(index, autoplay) {
  if (index < 0 || index >= playableTracks.length) return;
  pendingVisualizationSeek = null;
  audio.pause();
  activeIndex = index;
  audio.src = playableTracks[index].media_url;
  audio.load();
  playbackStatus.textContent = autoplay ? 'Starting playback…' : 'Ready to play.';
  updatePlayerControls();
  if (autoplay) playCurrent();
}

function selectVisualizationPoint(candidate) {
  const index = candidate.track.sequence_index;
  const midpoint = (candidate.point.start_seconds + candidate.point.end_seconds) / 2;
  if (index === activeIndex && audio.readyState >= 1) {
    audio.currentTime = Math.min(midpoint, Number.isFinite(audio.duration) ? audio.duration : midpoint);
    playbackStatus.textContent = `Ready at ${formatTime(audio.currentTime)}.`;
    updatePlayerControls();
    return;
  }
  selectTrack(index, false);
  pendingVisualizationSeek = { index, seconds: midpoint };
  playbackStatus.textContent = `Loading ${candidate.track.title} at ${formatTime(midpoint)}…`;
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
  renderVisualization(result.visualization);
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
  clearVisualization();
  resetAnnealingTelemetry();
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
  clearVisualization();
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
  clearVisualization();
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

function setVisualizationColorMode(mode) {
  visualizationColorMode = mode;
  colorByTrack.setAttribute('aria-pressed', mode === 'track' ? 'true' : 'false');
  colorBySequence.setAttribute('aria-pressed', mode === 'sequence' ? 'true' : 'false');
  renderVisualizationLegend();
  drawVisualization();
}

colorByTrack.addEventListener('click', () => setVisualizationColorMode('track'));
colorBySequence.addEventListener('click', () => setVisualizationColorMode('sequence'));

pointWindowStart.addEventListener('input', () => {
  const maximumStart = visualizationWindowFinish - minimumPointWindowSpan();
  updatePointWindow(Math.min(Number(pointWindowStart.value), maximumStart), visualizationWindowFinish);
});

pointWindowFinish.addEventListener('input', () => {
  const minimumFinish = visualizationWindowStart + minimumPointWindowSpan();
  updatePointWindow(visualizationWindowStart, Math.max(Number(pointWindowFinish.value), minimumFinish));
});

function movePointWindowTo(start) {
  const span = visualizationWindowFinish - visualizationWindowStart;
  const nextStart = Math.min(Math.max(start, 0), 100 - span);
  updatePointWindow(nextStart, nextStart + span);
}

pointWindowSelection.addEventListener('pointerdown', (event) => {
  if (event.button !== 0) return;
  pointWindowDrag = {
    pointerId: event.pointerId,
    clientX: event.clientX,
    start: visualizationWindowStart
  };
  pointWindowSelection.classList.add('dragging');
  pointWindowSelection.setPointerCapture(event.pointerId);
  event.preventDefault();
});

pointWindowSelection.addEventListener('pointermove', (event) => {
  if (!pointWindowDrag || pointWindowDrag.pointerId !== event.pointerId) return;
  const width = pointWindowBar.getBoundingClientRect().width;
  if (width <= 0) return;
  const delta = (event.clientX - pointWindowDrag.clientX) / width * 100;
  movePointWindowTo(pointWindowDrag.start + delta);
});

function finishPointWindowDrag(event) {
  if (!pointWindowDrag || pointWindowDrag.pointerId !== event.pointerId) return;
  pointWindowDrag = null;
  pointWindowSelection.classList.remove('dragging');
  if (pointWindowSelection.hasPointerCapture(event.pointerId)) {
    pointWindowSelection.releasePointerCapture(event.pointerId);
  }
}

pointWindowSelection.addEventListener('pointerup', finishPointWindowDrag);
pointWindowSelection.addEventListener('pointercancel', finishPointWindowDrag);
pointWindowSelection.addEventListener('lostpointercapture', () => {
  pointWindowDrag = null;
  pointWindowSelection.classList.remove('dragging');
});

pointWindowSelection.addEventListener('keydown', (event) => {
  const span = visualizationWindowFinish - visualizationWindowStart;
  let nextStart = visualizationWindowStart;
  if (event.key === 'ArrowLeft') nextStart -= event.shiftKey ? 5 : 1;
  else if (event.key === 'ArrowRight') nextStart += event.shiftKey ? 5 : 1;
  else if (event.key === 'Home') nextStart = 0;
  else if (event.key === 'End') nextStart = 100 - span;
  else return;
  event.preventDefault();
  movePointWindowTo(nextStart);
});

visualizationCanvas.addEventListener('pointermove', (event) => {
  const bounds = visualizationCanvas.getBoundingClientRect();
  const candidate = nearestVisualizationPoint(event.clientX - bounds.left, event.clientY - bounds.top);
  const previousKey = hoveredVisualizationPoint
    ? `${hoveredVisualizationPoint.track.sequence_index}:${hoveredVisualizationPoint.point.chunk_index}`
    : '';
  const nextKey = candidate ? `${candidate.track.sequence_index}:${candidate.point.chunk_index}` : '';
  hoveredVisualizationPoint = candidate;
  if (candidate) showVisualizationTooltip(candidate);
  else visualizationTooltip.hidden = true;
  if (previousKey !== nextKey) drawVisualization();
});

visualizationCanvas.addEventListener('pointerleave', () => {
  hoveredVisualizationPoint = null;
  visualizationTooltip.hidden = true;
  drawVisualization();
});

visualizationCanvas.addEventListener('click', (event) => {
  const bounds = visualizationCanvas.getBoundingClientRect();
  const candidate = nearestVisualizationPoint(event.clientX - bounds.left, event.clientY - bounds.top);
  if (candidate) selectVisualizationPoint(candidate);
});

if (typeof ResizeObserver === 'function') {
  new ResizeObserver(() => resizeVisualizationCanvas()).observe(visualizationCanvasWrap);
} else {
  window.addEventListener('resize', resizeVisualizationCanvas);
}

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
audio.addEventListener('loadedmetadata', () => {
  if (pendingVisualizationSeek && pendingVisualizationSeek.index === activeIndex) {
    audio.currentTime = Math.min(pendingVisualizationSeek.seconds, audio.duration || pendingVisualizationSeek.seconds);
    playbackStatus.textContent = `Ready at ${formatTime(audio.currentTime)}.`;
    pendingVisualizationSeek = null;
  }
  updatePlayerControls();
});
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
clearVisualization();
