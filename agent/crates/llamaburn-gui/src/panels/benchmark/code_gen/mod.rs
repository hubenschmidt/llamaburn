//! Code generation benchmark panel - decomposed into submodules

mod config_ui;
mod error_log;
mod execution;
mod history;
mod polling;
mod results_ui;
mod state;
mod util;

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use llamaburn_services::CodeBenchmarkEvent;
use llamaburn_services::{
    BenchmarkCombo, CodeBenchmarkMetrics, CodeBenchmarkSummary, CodeProblem, Language, ProblemSet,
};
use llamaburn_services::{BatchState, CodeHistoryEntry, Preset, RunStatus};
use tokio_util::sync::CancellationToken;

pub use error_log::ErrorLogEntry;

// ============================================================================
// Action Pattern - CodeGenAction (like Redux actions)
// ============================================================================

/// Actions emitted by CodeGenBenchmarkPanel for parent to process.
/// Panel decides WHAT should happen, parent decides HOW.
#[derive(Debug)]
pub enum CodeGenAction {
    // Output mutations
    AppendOutput(String),
    SetProgress(String),
    SetError(Option<String>),

    // History operations
    SaveCodeHistory(CodeHistoryEntry),
    SaveFailedHistory {
        error_message: String,
        status: RunStatus,
    },
    InsertBatch(BatchState),
    UpdateBatch(BatchState),
    DeleteBatch(String),
    InsertPreset(Preset),
    DeletePreset(String),
    LoadPresets,

    // Flow control
    AdvanceToNextCombo,
    RunCurrentCombo,
    RefreshModels,
    RefreshRankings,

    // Model management
    PreloadModel(String),
    SetSelectedModel(String),

    // Cancellation token
    SetCancelToken(Arc<CancellationToken>),
    ClearCancelToken,
}

/// Read-only context for rendering config UI
pub struct CodeGenRenderContext<'a> {
    pub model_list: &'a llamaburn_services::ModelList,
}

// ============================================================================
// CodeGenBenchmarkPanel
// ============================================================================

/// Code generation benchmark panel state
pub struct CodeGenBenchmarkPanel {
    // Multi-select config fields
    pub selected_models: Vec<String>,
    pub selected_languages: Vec<Language>,
    pub selected_temperatures: Vec<f32>,
    pub custom_temperature: f32,
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
    pub running: bool,
    pub code_rx: Option<Receiver<CodeBenchmarkEvent>>,
    pub current_problem: Option<String>,
    pub current_problem_id: Option<String>,
    pub generated_code: String,
    pub code_metrics: Vec<CodeBenchmarkMetrics>,
    pub code_summary: Option<CodeBenchmarkSummary>,
    pub code_output: String,

    // Error log for harness/infrastructure errors (need fixing)
    pub error_log: Vec<ErrorLogEntry>,
    pub error_log_expanded: bool,
    // Test failure log for LLM code failures (expected benchmark results)
    pub test_failure_log: Vec<ErrorLogEntry>,
    pub test_failure_log_expanded: bool,

    // Rankings
    pub code_leaderboard: Vec<(String, f64)>,
    pub last_language_for_rankings: Option<Language>,

    // Combo queue for matrix execution
    pub combo_queue: VecDeque<BenchmarkCombo>,
    pub current_combo: Option<BenchmarkCombo>,
    pub queue_total: usize,
    pub queue_completed: usize,
    pub batch_session_id: Option<String>,

    // Timing for ETA calculation
    pub combo_start_time: Option<std::time::Instant>,
    pub combo_durations_ms: Vec<u64>,

    // Preset management
    pub presets: Vec<Preset>,
    pub active_preset_id: Option<String>,
    pub preset_name_input: String,
    pub show_save_preset_modal: bool,
}

impl Default for CodeGenBenchmarkPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeGenBenchmarkPanel {
    pub fn new() -> Self {
        Self {
            selected_models: Vec::new(),
            selected_languages: vec![Language::Python],
            selected_temperatures: vec![0.0],
            custom_temperature: 0.0,
            selected_max_tokens: vec![2048],

            problem_sets: util::load_problem_sets_from_disk(),
            selected_problem_set_idx: 0,
            selected_problem_ids: Vec::new(),
            auto_run_tests: true,
            skip_on_error: false,

            pending_resume_batches: Vec::new(),

            running: false,
            code_rx: None,
            current_problem: None,
            current_problem_id: None,
            generated_code: String::new(),
            code_metrics: Vec::new(),
            code_summary: None,
            code_output: String::new(),

            error_log: Vec::new(),
            error_log_expanded: false,
            test_failure_log: Vec::new(),
            test_failure_log_expanded: false,

            code_leaderboard: Vec::new(),
            last_language_for_rankings: None,

            combo_queue: VecDeque::new(),
            current_combo: None,
            queue_total: 0,
            queue_completed: 0,
            batch_session_id: None,

            combo_start_time: None,
            combo_durations_ms: Vec::new(),

            presets: Vec::new(),
            active_preset_id: None,
            preset_name_input: String::new(),
            show_save_preset_modal: false,
        }
    }

    pub fn current_problems(&self) -> &[CodeProblem] {
        self.problem_sets
            .get(self.selected_problem_set_idx)
            .map(|ps| ps.problems.as_slice())
            .unwrap_or(&[])
    }

    pub fn selected_problems(&self) -> Vec<&CodeProblem> {
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
}

