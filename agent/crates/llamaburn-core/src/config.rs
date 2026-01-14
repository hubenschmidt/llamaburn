use crate::BenchmarkType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaBurnConfig {
    #[serde(default)]
    pub defaults: DefaultsConfig,
    #[serde(default)]
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub cost: CostConfig,
    #[serde(default)]
    pub audio: AudioConfig,
}

impl Default for LlamaBurnConfig {
    fn default() -> Self {
        Self {
            defaults: DefaultsConfig::default(),
            ollama: OllamaConfig::default(),
            cost: CostConfig::default(),
            audio: AudioConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    pub judge_provider: String,
    pub benchmark_iterations: u32,
    pub warmup_runs: u32,
    pub stress_duration_sec: u32,
    pub temperature: f32,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            judge_provider: "claude".to_string(),
            benchmark_iterations: 5,
            warmup_runs: 2,
            stress_duration_sec: 60,
            temperature: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub host: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:11434".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    pub kwh_rate: f64,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self { kwh_rate: 0.12 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub input_device: String,
    pub output_device: String,
    pub sample_rate: u32,
    pub channels: u8,
    pub buffer_size: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input_device: "default".to_string(),
            output_device: "default".to_string(),
            sample_rate: 44100,
            channels: 1,
            buffer_size: 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StressMode {
    Ramp,
    Sweep,
    Sustained,
    Spike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArrivalPattern {
    Static,
    Poisson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StressConfig {
    pub model_id: String,
    pub mode: StressMode,
    pub arrival: ArrivalPattern,
    pub max_concurrency: u32,
    pub duration_sec: u32,
    #[serde(default)]
    pub think_time_ms: Option<u32>,
}
