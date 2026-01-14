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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StressMetrics {
    pub requests_per_sec: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub p999_latency_ms: f64,
    pub error_rate: f64,
    pub degradation_point: Option<u32>,
    pub failure_point: Option<u32>,
    pub recovery_time_ms: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub timestamp_ms: u64,
    pub cpu_usage_percent: f64,
    pub ram_usage_mb: f64,
    pub gpu_utilization_percent: Option<f64>,
    pub gpu_vram_mb: Option<f64>,
    pub gpu_power_watts: Option<f64>,
    pub gpu_temp_celsius: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalScore {
    pub accuracy: u8,
    pub completeness: u8,
    pub coherence: u8,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMetrics {
    pub generation_time_ms: f64,
    pub audio_duration_ms: f64,
    pub real_time_factor: f64,
    #[serde(default)]
    pub word_error_rate: Option<f64>,
    #[serde(default)]
    pub quality_score: Option<u8>,
}
