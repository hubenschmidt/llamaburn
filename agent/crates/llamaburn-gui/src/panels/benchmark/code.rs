use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use tracing::{info, warn};

use llamaburn_benchmark::{load_all_problem_sets, CodeBenchmarkEvent, CodeBenchmarkRunner};
use llamaburn_core::{
    BenchmarkType, CodeBenchmarkConfig, CodeBenchmarkMetrics, CodeBenchmarkSummary, CodeProblem,
    Difficulty, Language, ProblemSet,
};
use llamaburn_services::{BatchCombo, BatchState, BatchStatus, CodeHistoryEntry};

use super::BenchmarkPanel;

/// Temperature bucket values
pub const TEMPERATURE_BUCKETS: &[f32] = &[0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2, 1.4];

/// Max tokens bucket values
pub const MAX_TOKENS_BUCKETS: &[u32] = &[512, 1024, 2048, 4096, 8192];

/// A single benchmark configuration combination
#[derive(Clone, Debug)]
pub struct BenchmarkCombo {
    pub model: String,
    pub language: Language,
    pub temperature: f32,
    pub max_tokens: u32,
}

/// Code benchmark specific state
#[derive(Default)]
pub struct CodeBenchmarkState {
    // Multi-select config fields
    pub selected_models: Vec<String>,
    pub selected_languages: Vec<Language>,
    pub selected_temperatures: Vec<f32>,
    pub custom_temperature: f32,  // For custom input field
    pub selected_max_tokens: Vec<u32>,

    // Problem selection
    pub problem_sets: Vec<ProblemSet>,
    pub selected_problem_set_idx: usize,
    pub selected_problem_ids: Vec<String>,
    pub auto_run_tests: bool,
    pub skip_on_error: bool,

    // Resume state
    pub pending_resume_batches: Vec<BatchState>,

    // Runtime state
    pub code_running: bool,
    pub code_rx: Option<Receiver<CodeBenchmarkEvent>>,
    pub current_problem: Option<String>,
    pub current_problem_id: Option<String>,
    pub generated_code: String,
    pub code_metrics: Vec<CodeBenchmarkMetrics>,
    pub code_summary: Option<CodeBenchmarkSummary>,
    pub code_output: String,

    // Rankings
    pub code_leaderboard: Vec<(String, f64)>,
    pub last_language_for_rankings: Option<Language>,

    // Combo queue for matrix execution
    pub combo_queue: VecDeque<BenchmarkCombo>,
    pub current_combo: Option<BenchmarkCombo>,
    pub queue_total: usize,
    pub queue_completed: usize,
    pub batch_session_id: Option<String>,
}

impl CodeBenchmarkState {
    pub fn new() -> Self {
        Self {
            // Default selections: Python, temp 0.0, tokens 2048
            selected_models: Vec::new(),
            selected_languages: vec![Language::Python],
            selected_temperatures: vec![0.0],
            custom_temperature: 0.0,
            selected_max_tokens: vec![2048],

            problem_sets: load_problem_sets_from_disk(),
            selected_problem_set_idx: 0,
            selected_problem_ids: Vec::new(),
            auto_run_tests: true,
            skip_on_error: false,

            pending_resume_batches: Vec::new(),

            code_running: false,
            code_rx: None,
            current_problem: None,
            current_problem_id: None,
            generated_code: String::new(),
            code_metrics: Vec::new(),
            code_summary: None,
            code_output: String::new(),

            code_leaderboard: Vec::new(),
            last_language_for_rankings: None,

            combo_queue: VecDeque::new(),
            current_combo: None,
            queue_total: 0,
            queue_completed: 0,
            batch_session_id: None,
        }
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
                            max_tokens: *tokens,
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

    pub fn current_problems(&self) -> &[CodeProblem] {
        self.problem_sets
            .get(self.selected_problem_set_idx)
            .map(|ps| ps.problems.as_slice())
            .unwrap_or(&[])
    }

    pub fn selected_problems(&self) -> Vec<&CodeProblem> {
        // Search across ALL problem sets, not just current
        self.problem_sets
            .iter()
            .flat_map(|ps| ps.problems.iter())
            .filter(|p| self.selected_problem_ids.contains(&p.id))
            .collect()
    }

    pub fn find_problem_by_title(&self, title: &str) -> Option<&CodeProblem> {
        self.problem_sets
            .iter()
            .flat_map(|ps| ps.problems.iter())
            .find(|p| p.title == title)
    }

    pub fn find_problem_by_id(&self, id: &str) -> Option<&CodeProblem> {
        self.problem_sets
            .iter()
            .flat_map(|ps| ps.problems.iter())
            .find(|p| p.id == id)
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
        let mut pending: Vec<BatchCombo> = self.current_combo.iter()
            .map(|c| BatchCombo {
                model: c.model.clone(),
                language: c.language,
                temperature: c.temperature,
                max_tokens: c.max_tokens,
            })
            .collect();
        pending.extend(self.combo_queue.iter().map(|c| BatchCombo {
            model: c.model.clone(),
            language: c.language,
            temperature: c.temperature,
            max_tokens: c.max_tokens,
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
        self.combo_queue = batch.pending_combos.iter().map(|c| BenchmarkCombo {
            model: c.model.clone(),
            language: c.language,
            temperature: c.temperature,
            max_tokens: c.max_tokens,
        }).collect();
        self.queue_total = batch.queue_total;
        self.queue_completed = batch.queue_completed;
        self.batch_session_id = Some(batch.session_id.clone());
    }
}

impl BenchmarkPanel {
    /// Render banner for incomplete/paused batch sessions
    fn render_incomplete_sessions_banner(&mut self, ui: &mut egui::Ui, interactive: bool) {
        if self.code_state.pending_resume_batches.is_empty() {
            return;
        }

        let batches = self.code_state.pending_resume_batches.clone();
        let mut resume_idx: Option<usize> = None;
        let mut discard_idx: Option<usize> = None;

        egui::Frame::group(ui.style())
            .inner_margin(egui::vec2(8.0, 6.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(format!("Incomplete Sessions ({})", batches.len())).strong());
                ui.add_space(4.0);

                for (idx, batch) in batches.iter().enumerate() {
                    ui.separator();
                    ui.add_space(2.0);

                    // Status and progress line
                    let status_text = match batch.status {
                        BatchStatus::Paused => "Paused",
                        BatchStatus::Running => "Interrupted",
                        BatchStatus::Completed => "Completed",
                    };
                    let progress_text = format!(
                        "{} - {}/{} complete",
                        status_text, batch.queue_completed, batch.queue_total
                    );
                    ui.label(progress_text);

                    // Configuration summary
                    let config_text = format!(
                        "{} models x {} langs x {} temps x {} tokens",
                        batch.selected_models.len(),
                        batch.selected_languages.len(),
                        batch.selected_temperatures.len(),
                        batch.selected_max_tokens.len(),
                    );
                    ui.label(egui::RichText::new(config_text).small().weak());

                    // Error info if present
                    if let Some(ref error) = batch.error_message {
                        ui.label(egui::RichText::new(format!("Error: {}", error)).small().color(egui::Color32::RED));
                    }

                    // Action buttons
                    ui.horizontal(|ui| {
                        if ui.add_enabled(interactive, egui::Button::new("Resume")).clicked() {
                            resume_idx = Some(idx);
                        }
                        if ui.add_enabled(interactive, egui::Button::new("Discard")).clicked() {
                            discard_idx = Some(idx);
                        }
                    });
                }
            });

        ui.add_space(8.0);

        // Handle resume action
        if let Some(idx) = resume_idx {
            self.resume_batch(idx);
        }

        // Handle discard action
        if let Some(idx) = discard_idx {
            self.discard_batch(idx);
        }
    }

    /// Resume a paused/incomplete batch session
    fn resume_batch(&mut self, idx: usize) {
        let Some(batch) = self.code_state.pending_resume_batches.get(idx).cloned() else {
            return;
        };

        // Restore state from batch
        self.code_state.restore_from_batch(&batch);

        // Update status to Running in database
        let mut updated_batch = batch.clone();
        updated_batch.status = BatchStatus::Running;
        updated_batch.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        updated_batch.error_message = None;
        updated_batch.failed_combo = None;

        if let Err(e) = self.history_service.update_batch(&updated_batch) {
            warn!("Failed to update batch status to running: {}", e);
        }

        // Remove from pending list
        self.code_state.pending_resume_batches.remove(idx);

        // Start execution
        self.live_output.push_str(&format!(
            "=== Resuming Batch {} ===\n{}/{} combinations remaining\n",
            batch.session_id,
            batch.queue_total - batch.queue_completed,
            batch.queue_total
        ));
        self.advance_to_next_combo();
    }

    /// Discard an incomplete batch session
    fn discard_batch(&mut self, idx: usize) {
        let Some(batch) = self.code_state.pending_resume_batches.get(idx) else {
            return;
        };

        // Delete from database
        if let Err(e) = self.history_service.delete_batch(&batch.session_id) {
            warn!("Failed to delete batch state: {}", e);
        }

        // Remove from pending list
        self.code_state.pending_resume_batches.remove(idx);
    }

    /// Render running controls (Pause/Cancel buttons and progress). Returns true if Pause clicked.
    fn render_running_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut pause_clicked = false;
        ui.horizontal(|ui| {
            pause_clicked = ui.button("Pause").clicked();
            let cancel_clicked = ui.button("Cancel").clicked();
            ui.spinner();

            let progress_label = self.code_state.current_combo.as_ref()
                .map(|combo| format!(
                    "{}/{}: {} | {} | T={:.1} | {}tok",
                    self.code_state.queue_completed + 1,
                    self.code_state.queue_total,
                    combo.model,
                    combo.language.label(),
                    combo.temperature,
                    combo.max_tokens
                ))
                .unwrap_or_else(|| self.progress.clone());
            ui.label(progress_label);

            if cancel_clicked {
                self.cancel_matrix_benchmark();
            }
        });
        pause_clicked
    }

    pub fn render_code_config(&mut self, ui: &mut egui::Ui) {
        let disabled = self.code_state.code_running || self.loading_models;
        let queue_running = !self.code_state.combo_queue.is_empty();
        let interactive = !disabled && !queue_running;

        // Show incomplete sessions banner if any exist
        self.render_incomplete_sessions_banner(ui, interactive);

        // Models dropdown with multi-select
        let models_label = format_selection_label(
            "Models",
            self.code_state.selected_models.len(),
            self.models.len(),
        );
        let models_popup_id = ui.make_persistent_id("models_popup");
        let models_btn = ui.add_enabled(interactive, egui::Button::new(&models_label).min_size(egui::vec2(290.0, 0.0)));
        if models_btn.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(models_popup_id));
        }
        egui::popup_below_widget(ui, models_popup_id, &models_btn, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(280.0);
            ui.horizontal(|ui| {
                if ui.small_button("All").clicked() {
                    self.code_state.selected_models = self.models.clone();
                }
                if ui.small_button("Clear").clicked() {
                    self.code_state.selected_models.clear();
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for model in &self.models.clone() {
                        let mut selected = self.code_state.selected_models.contains(model);
                        if ui.checkbox(&mut selected, model).changed() {
                            toggle_selection(&mut self.code_state.selected_models, model.clone(), selected);
                        }
                    }
                });
        });

        ui.add_space(3.0);

        // Languages dropdown
        let langs_label = format_selection_label(
            "Languages",
            self.code_state.selected_languages.len(),
            Language::all().len(),
        );
        let langs_popup_id = ui.make_persistent_id("langs_popup");
        let langs_btn = ui.add_enabled(interactive, egui::Button::new(&langs_label).min_size(egui::vec2(290.0, 0.0)));
        if langs_btn.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(langs_popup_id));
        }
        egui::popup_below_widget(ui, langs_popup_id, &langs_btn, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(200.0);
            ui.horizontal(|ui| {
                if ui.small_button("All").clicked() {
                    self.code_state.selected_languages = Language::all().to_vec();
                }
                if ui.small_button("Clear").clicked() {
                    self.code_state.selected_languages.clear();
                }
            });
            ui.separator();
            for lang in Language::all() {
                let mut selected = self.code_state.selected_languages.contains(lang);
                if ui.checkbox(&mut selected, lang.label()).changed() {
                    toggle_selection(&mut self.code_state.selected_languages, *lang, selected);
                }
            }
        });

        ui.add_space(3.0);

        // Temperature dropdown with custom input
        let temp_label = format_temp_label(&self.code_state.selected_temperatures);
        let temp_popup_id = ui.make_persistent_id("temp_popup");
        let temp_btn = ui.add_enabled(interactive, egui::Button::new(&temp_label).min_size(egui::vec2(290.0, 0.0)));
        if temp_btn.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(temp_popup_id));
        }
        egui::popup_below_widget(ui, temp_popup_id, &temp_btn, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(220.0);
            ui.horizontal(|ui| {
                if ui.small_button("All").clicked() {
                    self.code_state.selected_temperatures = TEMPERATURE_BUCKETS.to_vec();
                }
                if ui.small_button("Clear").clicked() {
                    self.code_state.selected_temperatures.clear();
                }
            });
            ui.separator();
            for temp in TEMPERATURE_BUCKETS {
                let mut selected = self.code_state.selected_temperatures.contains(temp);
                if ui.checkbox(&mut selected, format!("{:.1}", temp)).changed() {
                    toggle_selection(&mut self.code_state.selected_temperatures, *temp, selected);
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Custom:");
                ui.add(egui::DragValue::new(&mut self.code_state.custom_temperature)
                    .range(0.0..=2.0)
                    .speed(0.05));
                if ui.small_button("Add").clicked() {
                    let val = self.code_state.custom_temperature;
                    if !self.code_state.selected_temperatures.contains(&val) {
                        self.code_state.selected_temperatures.push(val);
                        self.code_state.selected_temperatures.sort_by(|a, b| a.partial_cmp(b).expect("NaN in temperatures"));
                    }
                }
            });
        });

        ui.add_space(3.0);

        // Max tokens dropdown
        let tokens_label = format_tokens_label(&self.code_state.selected_max_tokens);
        let tokens_popup_id = ui.make_persistent_id("tokens_popup");
        let tokens_btn = ui.add_enabled(interactive, egui::Button::new(&tokens_label).min_size(egui::vec2(290.0, 0.0)));
        if tokens_btn.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(tokens_popup_id));
        }
        egui::popup_below_widget(ui, tokens_popup_id, &tokens_btn, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
            ui.set_min_width(180.0);
            ui.horizontal(|ui| {
                if ui.small_button("All").clicked() {
                    self.code_state.selected_max_tokens = MAX_TOKENS_BUCKETS.to_vec();
                }
                if ui.small_button("Clear").clicked() {
                    self.code_state.selected_max_tokens.clear();
                }
            });
            ui.separator();
            for tokens in MAX_TOKENS_BUCKETS {
                let mut selected = self.code_state.selected_max_tokens.contains(tokens);
                if ui.checkbox(&mut selected, format!("{}", tokens)).changed() {
                    toggle_selection(&mut self.code_state.selected_max_tokens, *tokens, selected);
                }
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(5.0);

        // Problem set selection
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Problems").strong());
            ui.add_space(10.0);
            ui.add_enabled_ui(!disabled, |ui| {
                let current_set_name = self.code_state.problem_sets
                    .get(self.code_state.selected_problem_set_idx)
                    .map(|ps| ps.name.as_str())
                    .unwrap_or("None");
                egui::ComboBox::from_id_salt("problem_set_select")
                    .selected_text(current_set_name)
                    .show_ui(ui, |ui| {
                        for (idx, ps) in self.code_state.problem_sets.iter().enumerate() {
                            if ui.selectable_label(
                                self.code_state.selected_problem_set_idx == idx,
                                &ps.name,
                            ).clicked() {
                                self.code_state.selected_problem_set_idx = idx;
                                // Don't clear selections - preserve across set switches
                            }
                        }
                    });
            });
        });

        // Problem selection buttons
        let current_set_ids: Vec<String> = self.code_state.current_problems()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let total_problems: usize = self.code_state.problem_sets
            .iter()
            .map(|ps| ps.problems.len())
            .sum();
        ui.horizontal(|ui| {
            if ui.add_enabled(!disabled, egui::Button::new("Select All (Set)")).clicked() {
                // Add current set's problems to selection (don't replace)
                for id in current_set_ids {
                    if !self.code_state.selected_problem_ids.contains(&id) {
                        self.code_state.selected_problem_ids.push(id);
                    }
                }
            }
            if ui.add_enabled(!disabled, egui::Button::new("Clear All")).clicked() {
                self.code_state.selected_problem_ids.clear();
            }
            ui.label(format!(
                "{}/{} total",
                self.code_state.selected_problem_ids.len(),
                total_problems
            ));
        });
        ui.add_space(5.0);

        let problems = self.code_state.current_problems().to_vec();
        // Use available height minus space for buttons below
        let problems_height = (ui.available_height() - 50.0).max(80.0);
        egui::ScrollArea::vertical()
            .max_height(problems_height)
            .show(ui, |ui| {
                for problem in &problems {
                    let is_selected = self.code_state.selected_problem_ids.contains(&problem.id);
                    let difficulty_color = match problem.difficulty {
                        Difficulty::Easy => egui::Color32::GREEN,
                        Difficulty::Medium => egui::Color32::YELLOW,
                        Difficulty::Hard => egui::Color32::RED,
                    };

                    ui.horizontal(|ui| {
                        let mut selected = is_selected;
                        if ui.add_enabled(!disabled, egui::Checkbox::new(&mut selected, "")).changed() {
                            if selected {
                                self.code_state.selected_problem_ids.push(problem.id.clone());
                            } else {
                                self.code_state.selected_problem_ids.retain(|id| id != &problem.id);
                            }
                        }

                        ui.colored_label(difficulty_color, format!("[{}]", problem.difficulty.label()));
                        ui.label(&problem.title);
                    });
                }
            });

        ui.add_space(10.0);

        // Run button and options
        let running = self.code_state.code_running || queue_running;
        let pause_clicked = running && self.render_running_controls(ui);
        if pause_clicked {
            self.pause_matrix_benchmark();
        }
        if running {
            return;
        }
        ui.horizontal(|ui| {

            // Calculate combinations
            let combo_count = self.code_state.combination_count();
            let has_selections = !self.code_state.selected_models.is_empty()
                && !self.code_state.selected_languages.is_empty()
                && !self.code_state.selected_temperatures.is_empty()
                && !self.code_state.selected_max_tokens.is_empty()
                && !self.code_state.selected_problem_ids.is_empty();

            // Build descriptive label
            let button_label = format!(
                "Run {} combo{} ({} × {} × {} × {})",
                combo_count,
                if combo_count == 1 { "" } else { "s" },
                self.code_state.selected_models.len(),
                self.code_state.selected_languages.len(),
                self.code_state.selected_temperatures.len(),
                self.code_state.selected_max_tokens.len()
            );

            if ui.add_enabled(has_selections, egui::Button::new(button_label)).clicked() {
                self.start_matrix_benchmark();
            }
            ui.checkbox(&mut self.code_state.auto_run_tests, "Run Tests");
            ui.checkbox(&mut self.code_state.skip_on_error, "Skip on Error")
                .on_hover_text("Skip failed combos and continue (for unattended runs)");
        });
    }

    pub fn render_code_results(&self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Results").strong());

        let Some(summary) = &self.code_state.code_summary else {
            ui.label("No results yet");
            return;
        };

        ui.label(format!(
            "Pass Rate: {:.1}%",
            summary.pass_rate * 100.0
        ));
        ui.label(format!(
            "Solved: {}/{}",
            summary.problems_solved, summary.problems_total
        ));
        ui.label(format!("Avg TPS: {:.1}", summary.avg_tps));
        ui.label(format!(
            "Avg Exec Time: {:.1}ms",
            summary.avg_execution_time_ms
        ));

        if self.code_state.code_metrics.is_empty() {
            return;
        }

        ui.add_space(10.0);
        ui.label(egui::RichText::new("Per-Problem Results").small());

        for metrics in &self.code_state.code_metrics {
            let status = if metrics.tests_passed == metrics.tests_total {
                "✅"
            } else {
                "❌"
            };
            ui.label(format!(
                "{} {} ({}/{})",
                status, metrics.problem_id, metrics.tests_passed, metrics.tests_total
            ));
        }
    }

    pub fn render_code_rankings(&self, ui: &mut egui::Ui) {
        if self.code_state.code_leaderboard.is_empty() {
            ui.label("No rankings yet");
            return;
        }

        for (i, (model, pass_rate)) in self.code_state.code_leaderboard.iter().enumerate() {
            ui.label(format!("{}. {} ({:.1}%)", i + 1, model, pass_rate * 100.0));
        }
    }

    fn run_code_benchmark_with_combo(&mut self, combo: BenchmarkCombo, problems: Vec<CodeProblem>) {
        let config = CodeBenchmarkConfig {
            model_id: combo.model.clone(),
            language: combo.language,
            problem_ids: problems.iter().map(|p| p.id.clone()).collect(),
            temperature: combo.temperature,
            max_tokens: Some(combo.max_tokens),
            warmup_runs: self.warmup,
            run_tests: self.code_state.auto_run_tests,
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.code_state.code_rx = Some(rx);
        self.code_state.code_running = true;
        self.code_state.code_output.clear();
        self.code_state.code_metrics.clear();
        self.code_state.code_summary = None;
        self.code_state.generated_code.clear();
        self.error = None;

        let cancel_token = tokio_util::sync::CancellationToken::new();
        self.cancel_token = Some(std::sync::Arc::new(cancel_token.clone()));

        let ollama_host = self.ollama.host().to_string();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(async {
                let runner = CodeBenchmarkRunner::new(&ollama_host);
                let (async_tx, mut async_rx) = tokio::sync::mpsc::channel(100);

                tokio::spawn(async move {
                    runner
                        .run_streaming(&config, &problems, cancel_token, async_tx)
                        .await;
                });

                while let Some(event) = async_rx.recv().await {
                    if tx.send(event).is_err() {
                        break;
                    }
                }
            });
        });
    }

    pub fn cancel_code_benchmark(&mut self) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }
        self.code_state.code_running = false;
    }

    /// Start matrix benchmark: queue all combinations
    fn start_matrix_benchmark(&mut self) {
        let combos = self.code_state.generate_combinations();
        self.code_state.combo_queue = combos.into();
        self.code_state.queue_total = self.code_state.combo_queue.len();
        self.code_state.queue_completed = 0;
        self.code_state.batch_session_id = Some(uuid::Uuid::new_v4().to_string());

        // Save initial batch state for resume capability
        let batch = self.code_state.to_batch_state();
        let save_result = batch.map(|b| self.history_service.insert_batch(&b));
        if let Some(Err(e)) = save_result {
            warn!("Failed to save batch state: {}", e);
        }

        self.live_output.push_str(&format!(
            "\n=== Matrix Benchmark: {} combinations ===\n",
            self.code_state.queue_total
        ));

        self.advance_to_next_combo();
    }

    /// Advance to next combo in queue, starting preload if model changed
    pub(super) fn advance_to_next_combo(&mut self) {
        let Some(combo) = self.code_state.combo_queue.pop_front() else {
            // Queue complete - delete batch state
            let delete_result = self.code_state.batch_session_id.as_ref()
                .map(|sid| self.history_service.delete_batch(sid));
            if let Some(Err(e)) = delete_result {
                warn!("Failed to delete completed batch state: {}", e);
            }
            self.live_output.push_str("\n=== All Combinations Complete ===\n");
            self.code_state.current_combo = None;
            self.code_state.queue_total = 0;
            self.code_state.queue_completed = 0;
            self.code_state.batch_session_id = None;
            return;
        };

        // Check if model changed from previous combo
        let model_changed = self.code_state.current_combo
            .as_ref()
            .map(|c| c.model != combo.model)
            .unwrap_or(true);

        self.code_state.current_combo = Some(combo.clone());
        self.selected_model = combo.model.clone();

        self.live_output.push_str(&format!(
            "\n--- Combo {}/{}: {} | {} | T={:.1} | {}tok ---\n",
            self.code_state.queue_completed + 1,
            self.code_state.queue_total,
            combo.model,
            combo.language.label(),
            combo.temperature,
            combo.max_tokens
        ));

        if model_changed {
            // Start preloading the model
            self.model_preload_rx = Some(self.ollama.preload_model_async(&combo.model));
            self.model_preloading = true;
            self.preloading_model_name = combo.model.clone();
            self.live_output.push_str(&format!("⏳ Loading {} into VRAM...\n", combo.model));
            return;
        }

        // Same model, start benchmark directly
        self.run_current_combo();
    }

    /// Run benchmark for current combo
    pub(super) fn run_current_combo(&mut self) {
        let Some(combo) = self.code_state.current_combo.clone() else {
            return;
        };

        let problems: Vec<CodeProblem> = self
            .code_state
            .selected_problems()
            .into_iter()
            .cloned()
            .collect();

        if problems.is_empty() {
            self.error = Some("No problems selected".to_string());
            return;
        }

        self.run_code_benchmark_with_combo(combo, problems);
    }

    /// Cancel matrix benchmark and clear queue
    fn pause_matrix_benchmark(&mut self) {
        // Save state with Paused status
        let batch = self.code_state.to_batch_state();
        let save_result = batch.as_ref().map(|b| {
            let mut paused = b.clone();
            paused.status = BatchStatus::Paused;
            self.history_service.update_batch(&paused)
        });
        if let Some(Err(e)) = save_result {
            warn!("Failed to save paused batch state: {}", e);
        }

        // Add to pending resume list
        if let Some(b) = batch {
            let mut paused = b;
            paused.status = BatchStatus::Paused;
            self.code_state.pending_resume_batches.push(paused);
        }

        // Cancel current execution
        self.cancel_code_benchmark();

        // Clear running state
        self.code_state.combo_queue.clear();
        self.code_state.current_combo = None;
        self.code_state.queue_total = 0;
        self.code_state.queue_completed = 0;
        self.code_state.batch_session_id = None;
        self.live_output.push_str("\n=== Matrix Benchmark Paused ===\n");
    }

    fn cancel_matrix_benchmark(&mut self) {
        // Delete batch state on cancel
        let delete_result = self.code_state.batch_session_id.as_ref()
            .map(|sid| self.history_service.delete_batch(sid));
        if let Some(Err(e)) = delete_result {
            warn!("Failed to delete cancelled batch state: {}", e);
        }

        self.cancel_code_benchmark();
        self.code_state.combo_queue.clear();
        self.code_state.current_combo = None;
        self.code_state.queue_total = 0;
        self.code_state.queue_completed = 0;
        self.code_state.batch_session_id = None;
        self.live_output.push_str("\n=== Matrix Benchmark Cancelled ===\n");
    }

    pub fn poll_code_benchmark(&mut self) {
        let Some(rx) = self.code_state.code_rx.take() else {
            return;
        };

        let mut should_clear = false;

        while let Ok(event) = rx.try_recv() {
            match event {
                CodeBenchmarkEvent::Warmup { current, total } => {
                    self.progress = format!("Warmup {}/{}", current, total);
                }
                CodeBenchmarkEvent::Problem { current, total, title } => {
                    self.progress = format!("Problem {}/{}: {}", current, total, title);
                    let problem_id = self.code_state
                        .find_problem_by_title(&title)
                        .map(|p| p.id.clone());
                    self.code_state.current_problem = Some(title);
                    self.code_state.current_problem_id = problem_id;
                    self.code_state.generated_code.clear();
                }
                CodeBenchmarkEvent::GeneratingCode => {
                    self.code_state.code_output.push_str("Generating code...\n");
                }
                CodeBenchmarkEvent::Token { content } => {
                    self.code_state.generated_code.push_str(&content);
                    self.live_output.push_str(&content);
                }
                CodeBenchmarkEvent::ExecutingTests { total } => {
                    self.live_output.push_str(&format!("\nRunning {} tests...", total));
                }
                CodeBenchmarkEvent::TestResult { test_num, test_total, passed, expected, actual, error } => {
                    let status = if passed { "✅" } else { "❌" };
                    self.live_output.push_str(&format!("\n  Test {}/{}: {} ", test_num, test_total, status));
                    if !passed {
                        self.live_output.push_str(&format!("Expected: {} | Actual: {}", expected, actual));
                    } else {
                        self.live_output.push_str("PASS");
                    }
                    if let Some(e) = error {
                        self.live_output.push_str(&format!("\n    Error: {}", e));
                    }
                }
                CodeBenchmarkEvent::ProblemComplete { metrics } => {
                    self.live_output.push_str(&format!(
                        "\n\n--- {} complete: {}/{} tests passed ---\n",
                        metrics.problem_id, metrics.tests_passed, metrics.tests_total
                    ));
                    self.code_state.code_metrics.push(metrics);
                }
                CodeBenchmarkEvent::Done { summary } => {
                    self.code_state.code_summary = Some(summary.clone());
                    self.code_state.code_running = false;
                    self.save_code_to_history(&summary);
                    self.force_refresh_code_rankings();
                    self.live_output.push_str(&format!(
                        "\n=== Benchmark Complete ===\nPass Rate: {:.1}%\nSolved: {}/{}\n",
                        summary.pass_rate * 100.0,
                        summary.problems_solved,
                        summary.problems_total
                    ));
                    should_clear = true;

                    // Advance to next combo in queue
                    let has_more = !self.code_state.combo_queue.is_empty();
                    if !has_more { continue; }

                    self.code_state.queue_completed += 1;
                    // Update batch state for resume capability
                    if let Some(mut batch) = self.code_state.to_batch_state() {
                        batch.updated_at = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64;
                        let _ = self.history_service.update_batch(&batch);
                    }
                    self.advance_to_next_combo();
                }
                CodeBenchmarkEvent::Cancelled => {
                    self.code_state.code_running = false;
                    self.live_output.push_str("\nCancelled\n");
                    should_clear = true;
                }
                CodeBenchmarkEvent::Error { message } => {
                    self.code_state.code_running = false;
                    self.live_output.push_str(&format!("\nError: {}\n", message));
                    should_clear = true;

                    // Handle based on skip_on_error setting
                    let has_queue = !self.code_state.combo_queue.is_empty();
                    if !has_queue {
                        self.error = Some(message.clone());
                        continue;
                    }

                    // skip_on_error: skip this combo and continue
                    if self.code_state.skip_on_error {
                        self.live_output.push_str("(Skipping to next combo...)\n");
                        self.code_state.queue_completed += 1;
                        self.advance_to_next_combo();
                        continue;
                    }

                    // Auto-pause: save state and stop for user to resume
                    self.live_output.push_str("(Auto-paused - use Resume to continue)\n");
                    if let Some(mut batch) = self.code_state.to_batch_state() {
                        batch.status = BatchStatus::Paused;
                        batch.error_message = Some(message.clone());
                        batch.failed_combo = self.code_state.current_combo.as_ref().map(|c| BatchCombo {
                            model: c.model.clone(),
                            language: c.language,
                            temperature: c.temperature,
                            max_tokens: c.max_tokens,
                        });
                        let _ = self.history_service.update_batch(&batch);
                    }
                    // Clear queue to stop processing
                    self.code_state.combo_queue.clear();
                    self.code_state.current_combo = None;
                }
            }
        }

        if !should_clear {
            self.code_state.code_rx = Some(rx);
        }
    }

    fn save_code_to_history(&self, summary: &CodeBenchmarkSummary) {
        let Some(combo) = &self.code_state.current_combo else {
            warn!("No current combo to save to history");
            return;
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let config = CodeBenchmarkConfig {
            model_id: combo.model.clone(),
            language: combo.language,
            problem_ids: self
                .code_state
                .code_metrics
                .iter()
                .map(|m| m.problem_id.clone())
                .collect(),
            temperature: combo.temperature,
            max_tokens: Some(combo.max_tokens),
            warmup_runs: self.warmup,
            run_tests: self.code_state.auto_run_tests,
        };

        let entry = CodeHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            benchmark_type: BenchmarkType::Code,
            model_id: combo.model.clone(),
            language: combo.language,
            config,
            summary: summary.clone(),
            metrics: self.code_state.code_metrics.clone(),
            session_id: self.code_state.batch_session_id.clone(),
        };

        match self.history_service.insert_code(&entry) {
            Ok(()) => info!("Saved code benchmark result to history: {}", entry.id),
            Err(e) => warn!("Failed to save code benchmark history: {}", e),
        }
    }

    pub(super) fn refresh_code_rankings(&mut self) {
        if self.benchmark_type != BenchmarkType::Code {
            return;
        }

        let Some(lang) = self.code_state.selected_languages.first().copied() else {
            return;
        };

        if self.code_state.last_language_for_rankings == Some(lang) {
            return;
        }

        self.code_state.last_language_for_rankings = Some(lang);
        self.code_state.code_leaderboard = self
            .history_service
            .get_code_leaderboard(lang, 5)
            .unwrap_or_default();
    }

    fn force_refresh_code_rankings(&mut self) {
        self.code_state.last_language_for_rankings = None;
        let lang = self.code_state.selected_languages.first().copied()
            .unwrap_or(Language::Python);
        self.code_state.code_leaderboard = self
            .history_service
            .get_code_leaderboard(lang, 5)
            .unwrap_or_default();
    }
}

fn load_problem_sets_from_disk() -> Vec<ProblemSet> {
    let problems_dir = find_problems_dir();
    let Some(dir) = problems_dir else {
        tracing::warn!("Problems directory not found, using empty set");
        return Vec::new();
    };

    load_all_problem_sets(&dir).unwrap_or_else(|e| {
        tracing::error!("Failed to load problem sets: {}", e);
        Vec::new()
    })
}

fn find_problems_dir() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("problems"),
        PathBuf::from("../problems"),
        PathBuf::from("../../problems"),
    ];

    if let Some(found) = candidates.into_iter().find(|p| p.is_dir()) {
        return Some(found);
    }

    let exe_path = std::env::current_exe().ok()?;
    let from_exe = exe_path.parent()?.join("problems");
    from_exe.is_dir().then_some(from_exe)
}

/// Toggle an item in/out of a selection vec
fn toggle_selection<T: Clone + PartialEq>(vec: &mut Vec<T>, item: T, selected: bool) {
    if selected && !vec.contains(&item) {
        vec.push(item);
        return;
    }
    if !selected {
        vec.retain(|x| x != &item);
    }
}

/// Format dropdown label showing selection count
fn format_selection_label(name: &str, selected: usize, total: usize) -> String {
    match selected {
        0 => format!("{}: None", name),
        n if n == total => format!("{}: All ({})", name, total),
        1 => format!("{}: 1 selected", name),
        n => format!("{}: {} selected", name, n),
    }
}

/// Format temperature dropdown label
fn format_temp_label(temps: &[f32]) -> String {
    match temps.len() {
        0 => "Temp: None".to_string(),
        1 => format!("Temp: {:.1}", temps[0]),
        n if n == TEMPERATURE_BUCKETS.len() => format!("Temp: All ({})", n),
        n => format!("Temp: {} values", n),
    }
}

/// Format max tokens dropdown label
fn format_tokens_label(tokens: &[u32]) -> String {
    match tokens.len() {
        0 => "Tokens: None".to_string(),
        1 => format!("Tokens: {}", tokens[0]),
        n if n == MAX_TOKENS_BUCKETS.len() => format!("Tokens: All ({})", n),
        n => format!("Tokens: {} values", n),
    }
}
