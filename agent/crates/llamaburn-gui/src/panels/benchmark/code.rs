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
use llamaburn_services::CodeHistoryEntry;

use super::components::render_model_selector;
use super::BenchmarkPanel;

/// Code benchmark specific state
#[derive(Default)]
pub struct CodeBenchmarkState {
    pub language: Language,
    pub problem_sets: Vec<ProblemSet>,
    pub selected_problem_set_idx: usize,
    pub selected_problem_ids: Vec<String>,
    pub code_temperature: f32,
    pub code_max_tokens: u32,
    pub auto_run_tests: bool,

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
}

impl CodeBenchmarkState {
    pub fn new() -> Self {
        Self {
            language: Language::Python,
            problem_sets: load_problem_sets_from_disk(),
            selected_problem_set_idx: 0,
            selected_problem_ids: Vec::new(),
            code_temperature: 0.0,
            code_max_tokens: 2048,
            auto_run_tests: true,
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
        }
    }

    pub fn current_problems(&self) -> &[CodeProblem] {
        self.problem_sets
            .get(self.selected_problem_set_idx)
            .map(|ps| ps.problems.as_slice())
            .unwrap_or(&[])
    }

    pub fn selected_problems(&self) -> Vec<&CodeProblem> {
        self.current_problems()
            .iter()
            .filter(|p| self.selected_problem_ids.contains(&p.id))
            .collect()
    }

    pub fn find_problem_by_title(&self, title: &str) -> Option<&CodeProblem> {
        self.current_problems().iter().find(|p| p.title == title)
    }

    pub fn find_problem_by_id(&self, id: &str) -> Option<&CodeProblem> {
        self.current_problems().iter().find(|p| p.id == id)
    }
}

impl BenchmarkPanel {
    pub fn render_code_config(&mut self, ui: &mut egui::Ui) {
        let disabled = self.code_state.code_running || self.loading_models;

        egui::Grid::new("code_config_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Model selection
                ui.label("Model:");
                let resp = render_model_selector(
                    ui,
                    "code_model_select",
                    &self.models,
                    &self.selected_model,
                    self.loading_models,
                    self.model_preloading,
                    disabled,
                );
                if let Some(model) = resp.selected {
                    self.selected_model = model.clone();
                    self.model_preload_rx = Some(self.ollama.preload_model_async(&model));
                    self.model_preloading = true;
                    self.preloading_model_name = model.clone();
                    self.live_output.push_str(&format!("⏳ Loading {} into VRAM...\n", model));
                }
                if resp.unload_clicked {
                    self.unload_model();
                }
                ui.end_row();

                // Language selection
                ui.label("Language:");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!disabled, |ui| {
                        egui::ComboBox::from_id_salt("language_select")
                            .selected_text(self.code_state.language.label())
                            .show_ui(ui, |ui| {
                                for lang in Language::all() {
                                    ui.selectable_value(
                                        &mut self.code_state.language,
                                        *lang,
                                        lang.label(),
                                    );
                                }
                            });
                    });
                });
                ui.end_row();

                // Temperature
                ui.label("Temperature:");
                ui.add_enabled(
                    !disabled,
                    egui::DragValue::new(&mut self.code_state.code_temperature)
                        .range(0.0..=2.0)
                        .speed(0.05),
                );
                ui.end_row();

                // Max tokens
                ui.label("Max Tokens:");
                ui.add_enabled(
                    !disabled,
                    egui::DragValue::new(&mut self.code_state.code_max_tokens)
                        .range(256..=8192),
                );
                ui.end_row();
            });

        ui.add_space(10.0);
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
                                self.code_state.selected_problem_ids.clear();
                            }
                        }
                    });
            });
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
        let can_run = !self.code_state.code_running
            && !self.selected_model.is_empty()
            && !self.code_state.selected_problem_ids.is_empty();

        let can_run_all = !self.code_state.code_running
            && !self.selected_model.is_empty()
            && !self.code_state.problem_sets.is_empty();

        let total_problems: usize = self.code_state.problem_sets.iter().map(|ps| ps.problems.len()).sum();

        ui.horizontal(|ui| {
            if self.code_state.code_running {
                if ui.button("Cancel").clicked() {
                    self.cancel_code_benchmark();
                }
                ui.spinner();
                ui.label(&self.progress);
            } else {
                if ui.add_enabled(can_run, egui::Button::new("Run Selected")).clicked() {
                    self.start_code_benchmark();
                }
                if ui.add_enabled(can_run_all, egui::Button::new("Run All")).clicked() {
                    self.start_all_code_benchmark();
                }
                ui.checkbox(&mut self.code_state.auto_run_tests, "Run Tests");
            }

            if !disabled {
                ui.label(format!(
                    "{} selected / {} total",
                    self.code_state.selected_problem_ids.len(),
                    total_problems
                ));
            }
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

    pub fn start_code_benchmark(&mut self) {
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

        self.run_code_benchmark_with_problems(problems);
    }

    pub fn start_all_code_benchmark(&mut self) {
        // Collect all problems from all problem sets, sorted by difficulty
        let mut problems: Vec<CodeProblem> = self
            .code_state
            .problem_sets
            .iter()
            .flat_map(|ps| ps.problems.clone())
            .collect();

        if problems.is_empty() {
            self.error = Some("No problems available".to_string());
            return;
        }

        // Sort by difficulty: Easy (0) -> Medium (1) -> Hard (2)
        problems.sort_by_key(|p| match p.difficulty {
            Difficulty::Easy => 0,
            Difficulty::Medium => 1,
            Difficulty::Hard => 2,
        });

        let easy_count = problems.iter().filter(|p| p.difficulty == Difficulty::Easy).count();
        let medium_count = problems.iter().filter(|p| p.difficulty == Difficulty::Medium).count();
        let hard_count = problems.iter().filter(|p| p.difficulty == Difficulty::Hard).count();

        self.live_output.push_str(&format!(
            "=== Running ALL {} problems ===\n  Easy: {}  |  Medium: {}  |  Hard: {}\n\n",
            problems.len(), easy_count, medium_count, hard_count
        ));

        self.run_code_benchmark_with_problems(problems);
    }

    fn run_code_benchmark_with_problems(&mut self, problems: Vec<CodeProblem>) {
        let config = CodeBenchmarkConfig {
            model_id: self.selected_model.clone(),
            language: self.code_state.language,
            problem_ids: problems.iter().map(|p| p.id.clone()).collect(),
            temperature: self.code_state.code_temperature,
            max_tokens: Some(self.code_state.code_max_tokens),
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
            let rt = tokio::runtime::Runtime::new().unwrap();
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
                CodeBenchmarkEvent::ExecutingTests { current, total } => {
                    let msg = format!("\nExecuting test {}/{}...", current, total);
                    self.live_output.push_str(&msg);
                }
                CodeBenchmarkEvent::TestResult { passed, expected, actual, error } => {
                    let status = if passed { " ✅ PASS" } else { " ❌ FAIL" };
                    self.live_output.push_str(status);
                    if !passed {
                        self.live_output.push_str(&format!("\n    Expected: {}", expected));
                        self.live_output.push_str(&format!("\n    Actual:   {}", actual));
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
                }
                CodeBenchmarkEvent::Cancelled => {
                    self.code_state.code_running = false;
                    self.live_output.push_str("\nCancelled\n");
                    should_clear = true;
                }
                CodeBenchmarkEvent::Error { message } => {
                    self.error = Some(message.clone());
                    self.code_state.code_running = false;
                    self.live_output.push_str(&format!("\nError: {}\n", message));
                    should_clear = true;
                }
            }
        }

        if !should_clear {
            self.code_state.code_rx = Some(rx);
        }
    }

    fn save_code_to_history(&self, summary: &CodeBenchmarkSummary) {
        let model_id = self.selected_model.clone();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let config = CodeBenchmarkConfig {
            model_id: model_id.clone(),
            language: self.code_state.language,
            problem_ids: self
                .code_state
                .code_metrics
                .iter()
                .map(|m| m.problem_id.clone())
                .collect(),
            temperature: self.code_state.code_temperature,
            max_tokens: Some(self.code_state.code_max_tokens),
            warmup_runs: self.warmup,
            run_tests: self.code_state.auto_run_tests,
        };

        let entry = CodeHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            benchmark_type: BenchmarkType::Code,
            model_id,
            language: self.code_state.language,
            config,
            summary: summary.clone(),
            metrics: self.code_state.code_metrics.clone(),
        };

        if let Err(e) = self.history_service.insert_code(&entry) {
            warn!("Failed to save code benchmark history: {}", e);
        } else {
            info!("Saved code benchmark result to history: {}", entry.id);
        }
    }

    pub(super) fn refresh_code_rankings(&mut self) {
        if self.benchmark_type != BenchmarkType::Code {
            return;
        }

        let lang = self.code_state.language;
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
        self.code_state.code_leaderboard = self
            .history_service
            .get_code_leaderboard(self.code_state.language, 5)
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
