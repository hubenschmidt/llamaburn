use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BenchmarkMetrics {
    pub time_to_first_token_ms: f64,
    pub inter_token_latency_ms: f64,
    pub tokens_per_sec: f64,
    pub total_generation_ms: f64,
    pub prompt_eval_ms: f64,
    pub load_duration_ms: f64,
    pub input_sequence_length: u32,
    pub output_sequence_length: u32,
    #[serde(default)]
    pub power_draw_watts: Option<f64>,
    #[serde(default)]
    pub energy_wh: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBenchmarkResult {
    pub avg_tps: f64,
    pub avg_ttft_ms: f64,
    pub avg_total_ms: f64,
    pub min_tps: f64,
    pub max_tps: f64,
    pub iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBenchmarkSummary {
    pub avg_tps: f64,
    pub min_tps: f64,
    pub max_tps: f64,
    pub avg_ttft_ms: f64,
    pub avg_total_ms: f64,
    pub iterations: u32,
}
