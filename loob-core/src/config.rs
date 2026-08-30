use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Objective {
    /// Preserve Smoothify's original objective: minimize the worst transition.
    Bottleneck,
    /// Explicit opt-in compromise between worst transition and mean transition.
    Hybrid { bottleneck_weight: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JetConfig {
    /// Highest polynomial order to attempt: 0 = position, 1 = velocity,
    /// 2 = acceleration. Ill-conditioned or short inputs fall back safely.
    pub max_order: u8,
    /// Maximum number of boundary chunks used by each local fit.
    pub samples: usize,
    /// Multiplicative regression weight per step away from the seam.
    pub seam_weight_decay: f64,
    /// Optional forward projection after the seam. Gapless playback uses zero.
    pub delta_seconds: f64,
    pub lambda_position: f64,
    pub lambda_velocity: f64,
    pub lambda_acceleration: f64,
}

impl Default for JetConfig {
    fn default() -> Self {
        Self {
            max_order: 2,
            samples: 8,
            seam_weight_decay: 0.85,
            delta_seconds: 0.0,
            lambda_position: 1.0,
            lambda_velocity: 0.5,
            lambda_acceleration: 0.25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub whole_track_weight: f64,
    pub jet: JetConfig,
    pub objective: Objective,
    pub iterations: usize,
    pub seed: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            whole_track_weight: 0.2,
            jet: JetConfig::default(),
            objective: Objective::Bottleneck,
            iterations: 50_000,
            seed: 0,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !self.whole_track_weight.is_finite() || !(0.0..=1.0).contains(&self.whole_track_weight) {
            return Err("whole-track weight must be between zero and one".into());
        }
        if self.jet.max_order > 2 {
            return Err("jet order must be zero, one, or two".into());
        }
        if self.jet.samples == 0 {
            return Err("jet sample count must be positive".into());
        }
        if !self.jet.seam_weight_decay.is_finite()
            || !(0.0..=1.0).contains(&self.jet.seam_weight_decay)
            || self.jet.seam_weight_decay == 0.0
        {
            return Err("jet seam-weight decay must be finite and in (0, 1]".into());
        }
        if !self.jet.delta_seconds.is_finite() || self.jet.delta_seconds < 0.0 {
            return Err("jet delta must be finite and non-negative".into());
        }
        let lambdas = [
            self.jet.lambda_position,
            self.jet.lambda_velocity,
            self.jet.lambda_acceleration,
        ];
        if lambdas
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || self.jet.lambda_position == 0.0
        {
            return Err(
                "jet weights must be finite and non-negative, with positive position weight".into(),
            );
        }
        if let Objective::Hybrid { bottleneck_weight } = self.objective {
            if !bottleneck_weight.is_finite() || !(0.0..=1.0).contains(&bottleneck_weight) {
                return Err("hybrid bottleneck weight must be between zero and one".into());
            }
        }
        Ok(())
    }
}
