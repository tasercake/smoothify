use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OptimizerChoice { GreedyNn, SimulatedAnnealing }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub alpha: f64,
    pub beta: f64,
    pub head_seconds: f64,
    pub tail_seconds: f64,
    pub optimizer: OptimizerChoice,
}

impl Default for Config {
    fn default() -> Self {
        Self { alpha: 0.6, beta: 0.3, head_seconds: 10.0, tail_seconds: 10.0, optimizer: OptimizerChoice::SimulatedAnnealing }
    }
}
