//! State management methods for CodeGenBenchmarkPanel

use std::time::{SystemTime, UNIX_EPOCH};

use llamaburn_services::Language;
use llamaburn_services::{BatchCombo, BatchState, BatchStatus, Preset};

use super::{BenchmarkCombo, CodeGenBenchmarkPanel};

impl CodeGenBenchmarkPanel {
    /// Load a preset's configuration
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

    /// Clear active preset (when config is modified)
    pub fn clear_active_preset(&mut self) {
        self.active_preset_id = None;
    }

    /// Generate all combinations from selected configs
    pub fn generate_combinations(&self) -> Vec<BenchmarkCombo> {
        let mut combos = Vec::new();
        for model in &self.selected_models {
            for lang in &self.selected_languages {
                for temp in &self.selected_temperatures {
                    for tokens in &self.selected_max_tokens {
                        combos.push(BenchmarkCombo {
                            model: model.clone(),
                            language: *lang,
                            temperature: *temp,
                            max_tokens: Some(*tokens),
                        });
                    }
                }
            }
        }
        combos
    }

    /// Calculate total number of combinations
    pub fn combination_count(&self) -> usize {
        let models = self.selected_models.len().max(1);
        let langs = self.selected_languages.len().max(1);
        let temps = self.selected_temperatures.len().max(1);
        let tokens = self.selected_max_tokens.len().max(1);
        models * langs * temps * tokens
    }

    /// Load params from a history entry
    pub fn load_from_history(
        &mut self,
        model_id: String,
        language: Language,
        temperature: f32,
        max_tokens: Option<u32>,
        problem_ids: Vec<String>,
    ) {
        self.selected_models = vec![model_id];
        self.selected_languages = vec![language];
        self.selected_temperatures = vec![temperature];
        self.selected_max_tokens = max_tokens.map(|t| vec![t]).unwrap_or_else(|| vec![2048]);
        self.selected_problem_ids = problem_ids;
    }

    /// Create BatchState from current state for persistence
    pub fn to_batch_state(&self) -> Option<BatchState> {
        let session_id = self.batch_session_id.clone()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Include current_combo at front (it was popped from queue)
        let mut pending: Vec<BatchCombo> = self
            .current_combo
            .iter()
            .map(|c| BatchCombo {
                model: c.model.clone(),
                language: c.language,
                temperature: c.temperature,
                max_tokens: c.max_tokens.unwrap_or(2048),
            })
            .collect();
        pending.extend(self.combo_queue.iter().map(|c| BatchCombo {
            model: c.model.clone(),
            language: c.language,
            temperature: c.temperature,
            max_tokens: c.max_tokens.unwrap_or(2048),
        }));

        Some(BatchState {
            session_id,
            created_at: now,
            updated_at: now,
            status: BatchStatus::Running,
            selected_models: self.selected_models.clone(),
            selected_languages: self.selected_languages.clone(),
            selected_temperatures: self.selected_temperatures.clone(),
            selected_max_tokens: self.selected_max_tokens.clone(),
            selected_problem_ids: self.selected_problem_ids.clone(),
            auto_run_tests: self.auto_run_tests,
            skip_on_error: self.skip_on_error,
            pending_combos: pending,
            queue_total: self.queue_total,
            queue_completed: self.queue_completed,
            failed_combo: None,
            error_message: None,
        })
    }

    /// Restore state from a BatchState
    pub fn restore_from_batch(&mut self, batch: &BatchState) {
        self.selected_models = batch.selected_models.clone();
        self.selected_languages = batch.selected_languages.clone();
        self.selected_temperatures = batch.selected_temperatures.clone();
        self.selected_max_tokens = batch.selected_max_tokens.clone();
        self.selected_problem_ids = batch.selected_problem_ids.clone();
        self.auto_run_tests = batch.auto_run_tests;
        self.skip_on_error = batch.skip_on_error;
        self.combo_queue = batch
            .pending_combos
            .iter()
            .map(|c| BenchmarkCombo {
                model: c.model.clone(),
                language: c.language,
                temperature: c.temperature,
                max_tokens: Some(c.max_tokens),
            })
            .collect();
        self.queue_total = batch.queue_total;
        self.queue_completed = batch.queue_completed;
        self.batch_session_id = Some(batch.session_id.clone());
    }
}
