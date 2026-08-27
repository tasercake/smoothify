use loob_core::Progress;
use loob_yt::{UnavailabilityReason, YoutubeProgress};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressSource {
    LocalFiles,
    YoutubePlaylist,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioOutcome {
    Cached,
    Downloaded,
    SkippedCached,
    Skipped,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DspOutcome {
    Cached,
    Computed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum AppProgress {
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
    PreparingAudio {
        current: usize,
        total: usize,
        title: String,
    },
    AudioReady {
        current: usize,
        total: usize,
        title: String,
        outcome: AudioOutcome,
    },
    CheckingDsp {
        current: usize,
        total: usize,
        title: String,
    },
    DspReady {
        current: usize,
        total: usize,
        title: String,
        outcome: DspOutcome,
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

impl AppProgress {
    fn from_youtube(progress: YoutubeProgress) -> Self {
        match progress {
            YoutubeProgress::FetchingManifest => Self::CheckingManifest,
            YoutubeProgress::ManifestReady {
                title,
                total,
                was_cached,
            } => Self::ManifestReady {
                title,
                total,
                was_cached,
            },
            YoutubeProgress::ResolvingAudio {
                current,
                total,
                title,
            } => Self::PreparingAudio {
                current,
                total,
                title,
            },
            YoutubeProgress::AudioReady {
                current,
                total,
                title,
                was_cached,
            } => Self::AudioReady {
                current,
                total,
                title,
                outcome: if was_cached {
                    AudioOutcome::Cached
                } else {
                    AudioOutcome::Downloaded
                },
            },
            YoutubeProgress::AudioSkipped {
                current,
                total,
                title,
                reason,
                was_cached,
            } => Self::AudioReady {
                current,
                total,
                title: skipped_title(title, reason),
                outcome: if was_cached {
                    AudioOutcome::SkippedCached
                } else {
                    AudioOutcome::Skipped
                },
            },
        }
    }

    fn from_core(progress: Progress) -> Self {
        match progress {
            Progress::Analyzing {
                current,
                total,
                title,
            } => Self::CheckingDsp {
                current,
                total,
                title,
            },
            Progress::CacheHit {
                current,
                total,
                title,
            } => Self::DspReady {
                current,
                total,
                title,
                outcome: DspOutcome::Cached,
            },
            Progress::Analyzed {
                current,
                total,
                title,
            } => Self::DspReady {
                current,
                total,
                title,
                outcome: DspOutcome::Computed,
            },
            Progress::Optimizing { total } => Self::Optimizing { total },
        }
    }
}

fn skipped_title(title: String, reason: UnavailabilityReason) -> String {
    format!("{} ({})", title, reason.user_message().to_ascii_lowercase())
}

#[derive(Clone)]
pub struct ProgressReporter {
    channel: Channel<AppProgress>,
    delivery_error: Arc<Mutex<bool>>,
}

impl ProgressReporter {
    pub fn new(channel: Channel<AppProgress>) -> Self {
        Self {
            channel,
            delivery_error: Arc::new(Mutex::new(false)),
        }
    }

    pub fn send(&self, progress: AppProgress) -> Result<(), String> {
        if self.channel.send(progress).is_err() {
            *self
                .delivery_error
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = true;
            Err("Progress updates could not be delivered to the window.".into())
        } else {
            Ok(())
        }
    }

    pub fn youtube(&self, progress: YoutubeProgress) {
        let _ = self.send(AppProgress::from_youtube(progress));
    }

    pub fn core(&self, progress: Progress) {
        let _ = self.send(AppProgress::from_core(progress));
    }

    pub fn ensure_delivery(&self) -> Result<(), String> {
        if *self
            .delivery_error
            .lock()
            .unwrap_or_else(|error| error.into_inner())
        {
            Err("Progress updates could not be delivered to the window.".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_adapter_distinguishes_cache_download_and_cached_skip() {
        assert!(matches!(
            AppProgress::from_youtube(YoutubeProgress::AudioReady {
                current: 1,
                total: 2,
                title: "Cached".into(),
                was_cached: true,
            }),
            AppProgress::AudioReady {
                outcome: AudioOutcome::Cached,
                ..
            }
        ));
        assert!(matches!(
            AppProgress::from_youtube(YoutubeProgress::AudioReady {
                current: 2,
                total: 2,
                title: "New".into(),
                was_cached: false,
            }),
            AppProgress::AudioReady {
                outcome: AudioOutcome::Downloaded,
                ..
            }
        ));
        assert!(matches!(
            AppProgress::from_youtube(YoutubeProgress::AudioSkipped {
                current: 2,
                total: 2,
                title: "Gone".into(),
                reason: UnavailabilityReason::Removed,
                was_cached: true,
            }),
            AppProgress::AudioReady {
                outcome: AudioOutcome::SkippedCached,
                ..
            }
        ));
    }

    #[test]
    fn core_adapter_distinguishes_computed_and_cached_dsp() {
        let cached = AppProgress::from_core(Progress::CacheHit {
            current: 1,
            total: 2,
            title: "One".into(),
        });
        let computed = AppProgress::from_core(Progress::Analyzed {
            current: 2,
            total: 2,
            title: "Two".into(),
        });
        assert!(matches!(
            cached,
            AppProgress::DspReady {
                outcome: DspOutcome::Cached,
                ..
            }
        ));
        assert!(matches!(
            computed,
            AppProgress::DspReady {
                outcome: DspOutcome::Computed,
                ..
            }
        ));
    }

    #[test]
    fn source_and_dsp_totals_are_independent_phases() {
        let source = AppProgress::from_youtube(YoutubeProgress::AudioReady {
            current: 3,
            total: 5,
            title: "Source".into(),
            was_cached: true,
        });
        let dsp = AppProgress::from_core(Progress::Analyzing {
            current: 1,
            total: 4,
            title: "Usable".into(),
        });
        assert!(matches!(
            source,
            AppProgress::AudioReady {
                current: 3,
                total: 5,
                ..
            }
        ));
        assert!(matches!(
            dsp,
            AppProgress::CheckingDsp {
                current: 1,
                total: 4,
                ..
            }
        ));
    }
}
