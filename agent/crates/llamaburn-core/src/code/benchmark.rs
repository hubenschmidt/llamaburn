use serde::{Deserialize, Serialize};

use super::{
    BenchmarkCombo, CodeBenchmarkConfig, CodeBenchmarkMetrics, CodeBenchmarkSummary, ErrorLogEntry,
    Language, Preset,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeBenchmark {
    pub selected_models: Vec<String>,
    pub selected_languages: Vec<Language>,
    pub selected_temperatures: Vec<f32>,
    pub selected_max_tokens: Vec<u32>,
    pub selected_problem_ids: Vec<String>,

    pub warmup_runs: u32,
    pub auto_run_tests: bool,
    pub skip_on_error: bool,

    pub combo_queue: Vec<BenchmarkCombo>,
    pub current_combo: Option<BenchmarkCombo>,
    pub queue_total: u32,
    pub queue_completed: u32,

    pub batch_session_id: Option<String>,

    pub running: bool,

    pub current_problem: Option<String>,
    pub current_problem_id: Option<String>,
    pub generated_code: String,

    #[serde(skip)]
    pub live_output: String,
    #[serde(skip)]
    pub progress: String,
    #[serde(skip)]
    pub error: Option<String>,

    pub metrics: Vec<CodeBenchmarkMetrics>,
    pub summary: Option<CodeBenchmarkSummary>,

    pub test_failure_log: Vec<ErrorLogEntry>,
    pub error_log: Vec<ErrorLogEntry>,

    pub presets: Vec<Preset>,
    pub active_preset_id: Option<String>,

    pub model_best_pass_rate: Option<f64>,
    pub all_time_best: Option<(String, f64)>,
    pub leaderboard: Vec<(String, f64)>,
    pub last_language_for_rankings: Option<Language>,
}

impl CodeBenchmark {
    pub fn new() -> Self {
        Self {
            warmup_runs: 1,
            auto_run_tests: true,
            skip_on_error: true,
            selected_temperatures: vec![0.0],
            selected_max_tokens: vec![2048],
            ..Default::default()
        }
    }

    pub fn set_warmup(&mut self, n: u32) {
        self.warmup_runs = n;
    }

    pub fn set_auto_run_tests(&mut self, enabled: bool) {
        self.auto_run_tests = enabled;
    }

    pub fn set_skip_on_error(&mut self, enabled: bool) {
        self.skip_on_error = enabled;
    }

    pub fn start(&mut self) {
        self.running = true;
        self.metrics.clear();
        self.summary = None;
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn append_output(&mut self, text: &str) {
        self.live_output.push_str(text);
    }

    pub fn clear_output(&mut self) {
        self.live_output.clear();
        self.progress.clear();
    }

    pub fn set_progress(&mut self, progress: String) {
        self.progress = progress;
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    pub fn set_current_problem(&mut self, title: Option<String>, id: Option<String>) {
        self.current_problem = title;
        self.current_problem_id = id;
    }

    pub fn append_generated_code(&mut self, code: &str) {
        self.generated_code.push_str(code);
    }

    pub fn clear_generated_code(&mut self) {
        self.generated_code.clear();
    }

    pub fn set_summary(&mut self, summary: CodeBenchmarkSummary) {
        self.summary = Some(summary);
    }

    pub fn add_metrics(&mut self, metrics: CodeBenchmarkMetrics) {
        self.metrics.push(metrics);
    }

    pub fn set_rankings(
        &mut self,
        model_best: Option<f64>,
        all_time: Option<(String, f64)>,
        leaderboard: Vec<(String, f64)>,
    ) {
        self.model_best_pass_rate = model_best;
        self.all_time_best = all_time;
        self.leaderboard = leaderboard;
    }

    pub fn to_config(&self) -> Option<CodeBenchmarkConfig> {
        let combo = self.current_combo.as_ref()?;

        Some(CodeBenchmarkConfig {
            model_id: combo.model.clone(),
            language: combo.language,
            problem_ids: self.selected_problem_ids.clone(),
            temperature: combo.temperature,
            max_tokens: combo.max_tokens,
            warmup_runs: self.warmup_runs,
            run_tests: self.auto_run_tests,
        })
    }

    pub fn generate_combos(&self) -> Vec<BenchmarkCombo> {
        let mut combos = Vec::new();

        for model in &self.selected_models {
            for &language in &self.selected_languages {
                for &temperature in &self.selected_temperatures {
                    for &max_tokens in &self.selected_max_tokens {
                        combos.push(BenchmarkCombo {
                            model: model.clone(),
                            language,
                            temperature,
                            max_tokens: Some(max_tokens),
                        });
                    }
                }
            }
        }

        combos
    }

    pub fn start_matrix(&mut self) {
        self.combo_queue = self.generate_combos();
        self.queue_total = self.combo_queue.len() as u32;
        self.queue_completed = 0;
        self.current_combo = None;
        self.metrics.clear();
        self.summary = None;
        self.running = true;
    }

    pub fn advance_to_next(&mut self) -> Option<BenchmarkCombo> {
        self.current_combo = self.combo_queue.pop();
        self.current_combo.clone()
    }

    pub fn complete_current(&mut self) {
        self.queue_completed += 1;
        self.current_combo = None;
    }

    pub fn is_matrix_complete(&self) -> bool {
        self.combo_queue.is_empty() && self.current_combo.is_none()
    }

    pub fn load_preset(&mut self, preset: &Preset) {
        self.selected_models = vec![preset.model_id.clone()];
        self.selected_languages = vec![preset.language];
        self.selected_temperatures = vec![preset.temperature];
        self.selected_max_tokens = preset
            .max_tokens
            .map(|t| vec![t])
            .unwrap_or_else(|| vec![2048]);
        self.selected_problem_ids = preset.problem_ids.clone();
        self.active_preset_id = Some(preset.id.clone());
    }

    pub fn clear_preset(&mut self) {
        self.active_preset_id = None;
    }

    pub fn set_presets(&mut self, presets: Vec<Preset>) {
        self.presets = presets;
    }

    pub fn log_error(&mut self, entry: ErrorLogEntry) {
        self.error_log.push(entry);
    }

    pub fn log_test_failure(&mut self, entry: ErrorLogEntry) {
        self.test_failure_log.push(entry);
    }
}
