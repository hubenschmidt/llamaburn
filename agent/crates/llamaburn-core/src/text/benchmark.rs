use serde::{Deserialize, Serialize};

use super::{TextBenchmarkConfig, BenchmarkMetrics, TextBenchmarkResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextBenchmark {
    pub config: TextBenchmarkConfig,

    pub running: bool,

    #[serde(skip)]
    pub live_output: String,
    #[serde(skip)]
    pub progress: String,
    #[serde(skip)]
    pub error: Option<String>,

    pub result: Option<TextBenchmarkResult>,
    pub collected_metrics: Vec<BenchmarkMetrics>,

    pub model_best_tps: Option<f64>,
    pub all_time_best: Option<(String, f64)>,
    pub leaderboard: Vec<(String, f64)>,
    pub last_model_for_rankings: String,

    pub last_model_for_info: String,
}

impl TextBenchmark {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self, model_id: &str) {
        self.config.model_id = model_id.to_string();
        self.running = true;
        self.result = None;
        self.collected_metrics.clear();
        self.clear_output();
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn append_output(&mut self, s: &str) {
        self.live_output.push_str(s);
    }

    pub fn set_progress(&mut self, s: String) {
        self.progress = s;
    }

    pub fn set_error(&mut self, e: Option<String>) {
        self.error = e;
    }

    pub fn clear_output(&mut self) {
        self.live_output.clear();
        self.progress.clear();
        self.error = None;
    }

    pub fn set_result(&mut self, result: TextBenchmarkResult) {
        self.result = Some(result);
        self.running = false;
    }

    pub fn add_metrics(&mut self, metrics: BenchmarkMetrics) {
        self.collected_metrics.push(metrics);
    }

    pub fn set_rankings(
        &mut self,
        model_best: Option<f64>,
        all_time: Option<(String, f64)>,
        leaderboard: Vec<(String, f64)>,
    ) {
        self.model_best_tps = model_best;
        self.all_time_best = all_time;
        self.leaderboard = leaderboard;
    }
}
