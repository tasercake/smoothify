use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Objective {
    /// Preserve Smoothify's original objective: minimize the worst transition.
    Bottleneck,
    /// Explicit opt-in compromise between worst transition and mean transition.
    Hybrid { bottleneck_weight: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub intro_seconds: f64,
    pub outro_seconds: f64,
    pub whole_track_weight: f64,
    pub objective: Objective,
    pub iterations: usize,
    pub seed: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            intro_seconds: 10.0,
            outro_seconds: 10.0,
            whole_track_weight: 0.2,
            objective: Objective::Bottleneck,
            iterations: 50_000,
            seed: 0,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !self.intro_seconds.is_finite()
            || !self.outro_seconds.is_finite()
            || self.intro_seconds <= 0.0
            || self.outro_seconds <= 0.0
        {
            return Err("intro and outro windows must be finite and positive".into());
        }
        if !self.whole_track_weight.is_finite() || !(0.0..=1.0).contains(&self.whole_track_weight) {
            return Err("whole-track weight must be between zero and one".into());
        }
        if let Objective::Hybrid { bottleneck_weight } = self.objective {
            if !bottleneck_weight.is_finite() || !(0.0..=1.0).contains(&bottleneck_weight) {
                return Err("hybrid bottleneck weight must be between zero and one".into());
            }
        }
        Ok(())
    }
}
