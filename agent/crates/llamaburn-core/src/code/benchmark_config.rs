use serde::{Deserialize, Serialize};

use super::Language;

fn default_run_tests() -> bool {
    true
}

fn default_temperature() -> f32 {
    0.0
}

fn default_warmup() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeBenchmarkConfig {
    pub model_id: String,
    pub language: Language,
    pub problem_ids: Vec<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default = "default_warmup")]
    pub warmup_runs: u32,
    #[serde(default = "default_run_tests")]
    pub run_tests: bool,
}

impl Default for CodeBenchmarkConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            language: Language::default(),
            problem_ids: Vec::new(),
            temperature: default_temperature(),
            max_tokens: None,
            warmup_runs: default_warmup(),
            run_tests: default_run_tests(),
        }
    }
}
