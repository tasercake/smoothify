use loob_core::Progress;
use loob_yt::YoutubeProgress;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::ipc::Channel;

const MAX_ACTIVE_TASKS: usize = 8;
const FAST_UPDATE_INTERVAL: Duration = Duration::from_millis(75);

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressSource {
    LocalFiles,
    YoutubePlaylist,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkPhase {
    Audio,
    Dsp,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Checking,
    Analyzing,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActiveTask {
    task_id: String,
    source_index: usize,
    title: String,
    state: TaskState,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct OutcomeCounters {
    cached: usize,
    downloaded: usize,
    skipped_cached: usize,
    skipped: usize,
    computed: usize,
    failed: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ProgressEvent {
    Validating {
        source: ProgressSource,
        total: usize,
    },
    CheckingManifest,
    ManifestReady {
        title: String,
        total: usize,
        was_cached: bool,
    },
    WorkSnapshot {
        work_phase: WorkPhase,
        completed: usize,
        total: usize,
        counters: OutcomeCounters,
        active: Vec<ActiveTask>,
    },
    Optimizing {
        total: usize,
    },
    PreparingPlayer {
        total: usize,
    },
    Completed {
        total: usize,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AppProgress {
    pub seq: u64,
    #[serde(flatten)]
    pub event: ProgressEvent,
}

#[derive(Debug)]
struct WorkState {
    phase: WorkPhase,
    completed: usize,
    total: usize,
    counters: OutcomeCounters,
    active: BTreeMap<String, ActiveTask>,
}

impl WorkState {
    fn new(phase: WorkPhase, total: usize) -> Self {
        Self {
            phase,
            completed: 0,
            total,
            counters: OutcomeCounters::default(),
            active: BTreeMap::new(),
        }
    }

    fn snapshot(&self) -> ProgressEvent {
        ProgressEvent::WorkSnapshot {
            work_phase: self.phase,
            completed: self.completed,
            total: self.total,
            counters: self.counters.clone(),
            active: self
                .active
                .values()
                .take(MAX_ACTIVE_TASKS)
                .cloned()
                .collect(),
        }
    }
}

struct ReporterState {
    channel: Channel<AppProgress>,
    delivery_error: bool,
    seq: u64,
    work: Option<WorkState>,
    last_work_send: Option<Instant>,
    work_dirty: bool,
}

#[derive(Clone)]
pub struct ProgressReporter {
    inner: Arc<Mutex<ReporterState>>,
}

impl ProgressReporter {
    pub fn new(channel: Channel<AppProgress>) -> Self {
        let inner = Arc::new(Mutex::new(ReporterState {
            channel,
            delivery_error: false,
            seq: 0,
            work: None,
            last_work_send: None,
            work_dirty: false,
        }));
        let weak = Arc::downgrade(&inner);
        std::thread::spawn(move || loop {
            std::thread::sleep(FAST_UPDATE_INTERVAL);
            let Some(inner) = weak.upgrade() else {
                break;
            };
            let mut state = inner.lock().unwrap_or_else(|error| error.into_inner());
            if state.work_dirty {
                maybe_send_work(&mut state, false);
            }
        });
        Self { inner }
    }

    pub fn send(&self, event: ProgressEvent) -> Result<(), String> {
        let mut state = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        state.work = None;
        state.work_dirty = false;
        send_locked(&mut state, event)
    }

    pub fn youtube(&self, progress: YoutubeProgress) {
        let mut state = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        match progress {
            YoutubeProgress::FetchingManifest => {
                state.work = None;
                state.work_dirty = false;
                let _ = send_locked(&mut state, ProgressEvent::CheckingManifest);
            }
            YoutubeProgress::ManifestReady {
                title,
                total,
                was_cached,
            } => {
                state.work = None;
                state.work_dirty = false;
                let _ = send_locked(
                    &mut state,
                    ProgressEvent::ManifestReady {
                        title,
                        total,
                        was_cached,
                    },
                );
            }
            YoutubeProgress::ResolvingAudio {
                task_id,
                source_index,
                total,
                title,
            } => {
                ensure_work(&mut state, WorkPhase::Audio, total);
                state.work.as_mut().unwrap().active.insert(
                    task_id.clone(),
                    ActiveTask {
                        task_id,
                        source_index,
                        title,
                        state: TaskState::Checking,
                    },
                );
                maybe_send_work(&mut state, false);
            }
            YoutubeProgress::AudioReady {
                task_id,
                total,
                was_cached,
                ..
            } => {
                ensure_work(&mut state, WorkPhase::Audio, total);
                let work = state.work.as_mut().unwrap();
                work.active.remove(&task_id);
                work.completed += 1;
                if was_cached {
                    work.counters.cached += 1;
                } else {
                    work.counters.downloaded += 1;
                }
                maybe_send_work(&mut state, false);
            }
            YoutubeProgress::AudioSkipped {
                task_id,
                total,
                was_cached,
                ..
            } => {
                ensure_work(&mut state, WorkPhase::Audio, total);
                let work = state.work.as_mut().unwrap();
                work.active.remove(&task_id);
                work.completed += 1;
                if was_cached {
                    work.counters.skipped_cached += 1;
                } else {
                    work.counters.skipped += 1;
                }
                maybe_send_work(&mut state, false);
            }
        }
    }

    pub fn core(&self, progress: Progress) {
        let mut state = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        match progress {
            Progress::Analyzing {
                task_id,
                source_index,
                total,
                title,
            } => {
                ensure_work(&mut state, WorkPhase::Dsp, total);
                state.work.as_mut().unwrap().active.insert(
                    task_id.clone(),
                    ActiveTask {
                        task_id,
                        source_index,
                        title,
                        state: TaskState::Analyzing,
                    },
                );
                maybe_send_work(&mut state, false);
            }
            Progress::CacheHit { task_id, total, .. } => {
                finish_dsp(&mut state, task_id, total, true);
            }
            Progress::Analyzed { task_id, total, .. } => {
                finish_dsp(&mut state, task_id, total, false);
            }
            Progress::Optimizing { total } => {
                maybe_send_work(&mut state, true);
                state.work = None;
                state.work_dirty = false;
                let _ = send_locked(&mut state, ProgressEvent::Optimizing { total });
            }
        }
    }

    pub fn ensure_delivery(&self) -> Result<(), String> {
        let mut state = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        maybe_send_work(&mut state, true);
        if state.delivery_error {
            Err("Progress updates could not be delivered to the window.".into())
        } else {
            Ok(())
        }
    }

    pub fn fail_active(&self) {
        let mut state = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(work) = state.work.as_mut() {
            let failed = work.active.len();
            work.active.clear();
            work.counters.failed += failed;
            work.completed = work.completed.saturating_add(failed).min(work.total);
            maybe_send_work(&mut state, true);
        }
    }
}

fn ensure_work(state: &mut ReporterState, phase: WorkPhase, total: usize) {
    if !matches!(&state.work, Some(work) if work.phase == phase && work.total == total) {
        state.work = Some(WorkState::new(phase, total));
        state.last_work_send = None;
    }
}

fn finish_dsp(state: &mut ReporterState, task_id: String, total: usize, cached: bool) {
    ensure_work(state, WorkPhase::Dsp, total);
    let work = state.work.as_mut().unwrap();
    work.active.remove(&task_id);
    work.completed += 1;
    if cached {
        work.counters.cached += 1;
    } else {
        work.counters.computed += 1;
    }
    maybe_send_work(state, false);
}

fn maybe_send_work(state: &mut ReporterState, force: bool) {
    let Some(work) = &state.work else {
        return;
    };
    let now = Instant::now();
    state.work_dirty = true;
    let phase_complete = work.completed == work.total;
    let due = state
        .last_work_send
        .is_none_or(|last| now.duration_since(last) >= FAST_UPDATE_INTERVAL);
    if force || phase_complete || due {
        let event = work.snapshot();
        let _ = send_locked(state, event);
        state.last_work_send = Some(now);
        state.work_dirty = false;
    }
}

fn send_locked(state: &mut ReporterState, event: ProgressEvent) -> Result<(), String> {
    state.seq = state.seq.saturating_add(1);
    if state
        .channel
        .send(AppProgress {
            seq: state.seq,
            event,
        })
        .is_err()
    {
        state.delivery_error = true;
        Err("Progress updates could not be delivered to the window.".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: usize) -> ActiveTask {
        ActiveTask {
            task_id: format!("track-{id}"),
            source_index: id,
            title: format!("Track {id}"),
            state: TaskState::Analyzing,
        }
    }

    #[test]
    fn snapshot_caps_the_active_list() {
        let mut work = WorkState::new(WorkPhase::Dsp, 20);
        for index in 0..20 {
            work.active.insert(format!("track-{index:02}"), task(index));
        }
        let ProgressEvent::WorkSnapshot { active, .. } = work.snapshot() else {
            unreachable!()
        };
        assert_eq!(active.len(), MAX_ACTIVE_TASKS);
    }

    #[test]
    fn out_of_order_finishes_only_advance_aggregate_completion() {
        let mut work = WorkState::new(WorkPhase::Dsp, 2);
        work.active.insert("track-0".into(), task(0));
        work.active.insert("track-1".into(), task(1));
        work.active.remove("track-1");
        work.completed += 1;
        let ProgressEvent::WorkSnapshot { completed, .. } = work.snapshot() else {
            unreachable!()
        };
        assert_eq!(completed, 1);
        work.active.remove("track-0");
        work.completed += 1;
        let ProgressEvent::WorkSnapshot {
            completed, active, ..
        } = work.snapshot()
        else {
            unreachable!()
        };
        assert_eq!(completed, 2);
        assert!(active.is_empty());
    }
}
