use crate::BenchmarkType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBenchmarkConfig {
    #[serde(default)]
    pub benchmark_type: BenchmarkType,
    pub model_id: String,
    pub iterations: u32,
    pub warmup_runs: u32,
    pub prompt_set: String,
    pub temperature: f32,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub top_k: Option<u32>,
}

impl Default for TextBenchmarkConfig {
    fn default() -> Self {
        Self {
            benchmark_type: BenchmarkType::Text,
            model_id: String::new(),
            iterations: 5,
            warmup_runs: 2,
            prompt_set: "default".to_string(),
            temperature: 0.7,
            max_tokens: None,
            top_p: None,
            top_k: None,
        }
    }
}
