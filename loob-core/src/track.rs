use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub global_embedding: Vec<f64>,
    pub head_embedding: Vec<f64>,
    pub tail_embedding: Vec<f64>,
}
