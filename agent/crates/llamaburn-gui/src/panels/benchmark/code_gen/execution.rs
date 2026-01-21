//! Benchmark execution methods for code benchmark panel

use std::sync::Arc;

use llamaburn_benchmark::CodeBenchmarkRunner;
use llamaburn_services::{CodeBenchmarkConfig, CodeProblem};
use llamaburn_services::BatchStatus;

use super::{BenchmarkCombo, CodeGenAction, CodeGenBenchmarkPanel};

impl CodeGenBenchmarkPanel {
    /// Run a benchmark with the given combo and problems.
    /// Returns actions and optionally sets up the receiver internally.
    pub fn run_benchmark_with_combo(
        &mut self,
        combo: BenchmarkCombo,
        problems: Vec<CodeProblem>,
        ollama_host: &str,
    ) -> Vec<CodeGenAction> {
        let mut actions = Vec::new();

        let config = CodeBenchmarkConfig {
            model_id: combo.model.clone(),
            language: combo.language,
            problem_ids: problems.iter().map(|p| p.id.clone()).collect(),
            temperature: combo.temperature,
            max_tokens: combo.max_tokens,
            warmup_runs: 0,
            run_tests: self.auto_run_tests,
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.code_rx = Some(rx);
        self.running = true;
        self.code_output.clear();
        self.code_metrics.clear();
        self.code_summary = None;
        self.generated_code.clear();

        actions.push(CodeGenAction::SetError(None));

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let cancel_token_arc = Arc::new(cancel_token.clone());
        actions.push(CodeGenAction::SetCancelToken(cancel_token_arc));

        let ollama_host = ollama_host.to_string();

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

        actions
    }

    /// Cancel the running benchmark
    pub fn cancel(&mut self) -> Vec<CodeGenAction> {
        self.running = false;
        vec![CodeGenAction::ClearCancelToken]
    }

    /// Start matrix benchmark: queue all combinations.
    /// Returns actions for parent to process.
    pub fn start_matrix(&mut self) -> Vec<CodeGenAction> {
        let mut actions = Vec::new();

        let combos = self.generate_combinations();
        self.combo_queue = combos.into();
        self.queue_total = self.combo_queue.len();
        self.queue_completed = 0;
        self.batch_session_id = Some(uuid::Uuid::new_v4().to_string());
        self.combo_durations_ms.clear();

        // Save initial batch state for resume capability
        if let Some(batch) = self.to_batch_state() {
            actions.push(CodeGenAction::InsertBatch(batch));
        }

        actions.push(CodeGenAction::AppendOutput(format!(
            "\n=== Matrix Benchmark: {} combinations ===\n",
            self.queue_total
        )));

        // Request advance to first combo
        actions.push(CodeGenAction::AdvanceToNextCombo);

        actions
    }

    /// Advance to next combo in queue.
    /// Returns actions. Parent should call run_current_combo or preload model based on actions.
    pub fn advance_to_next(&mut self) -> Vec<CodeGenAction> {
        let mut actions = Vec::new();

        let Some(combo) = self.combo_queue.pop_front() else {
            // Queue complete - delete batch state
            if let Some(sid) = &self.batch_session_id {
                actions.push(CodeGenAction::DeleteBatch(sid.clone()));
            }
            actions.push(CodeGenAction::AppendOutput(
                "\n=== All Combinations Complete ===\n".into(),
            ));
            self.current_combo = None;
            self.queue_total = 0;
            self.queue_completed = 0;
            self.batch_session_id = None;
            return actions;
        };

        // Check if model changed from previous combo
        let model_changed = self
            .current_combo
            .as_ref()
            .map(|c| c.model != combo.model)
            .unwrap_or(true);

        self.current_combo = Some(combo.clone());
        actions.push(CodeGenAction::SetSelectedModel(combo.model.clone()));

        actions.push(CodeGenAction::AppendOutput(format!(
            "\n--- Combo {}/{}: {} | {} | T={:.1} | {}tok ---\n",
            self.queue_completed + 1,
            self.queue_total,
            combo.model,
            combo.language.label(),
            combo.temperature,
            combo.max_tokens.unwrap_or(2048)
        )));

        if model_changed {
            // Request model preload - parent will handle async loading
            actions.push(CodeGenAction::PreloadModel(combo.model.clone()));
            actions.push(CodeGenAction::AppendOutput(format!(
                "Loading {} into VRAM...\n",
                combo.model
            )));
        } else {
            // Same model, request benchmark start
            actions.push(CodeGenAction::RunCurrentCombo);
        }

        actions
    }

    /// Run benchmark for current combo.
    /// Returns actions. Parent provides ollama_host.
    pub fn run_current(&mut self, ollama_host: &str) -> Vec<CodeGenAction> {
        let Some(combo) = self.current_combo.clone() else {
            return vec![];
        };

        let problems: Vec<CodeProblem> = self
            .selected_problems()
            .into_iter()
            .cloned()
            .collect();

        if problems.is_empty() {
            return vec![CodeGenAction::SetError(Some("No problems selected".into()))];
        }

        // Record start time for ETA calculation
        self.combo_start_time = Some(std::time::Instant::now());

        self.run_benchmark_with_combo(combo, problems, ollama_host)
    }

    /// Pause matrix benchmark and save state.
    /// Returns actions for parent to process.
    pub fn pause_matrix(&mut self) -> Vec<CodeGenAction> {
        let mut actions = Vec::new();

        // Save state with Paused status
        if let Some(mut batch) = self.to_batch_state() {
            batch.status = BatchStatus::Paused;
            actions.push(CodeGenAction::UpdateBatch(batch.clone()));
            // Add to pending resume list
            self.pending_resume_batches.push(batch);
        }

        // Cancel current execution
        self.running = false;
        actions.push(CodeGenAction::ClearCancelToken);

        // Clear running state
        self.combo_queue.clear();
        self.current_combo = None;
        self.queue_total = 0;
        self.queue_completed = 0;
        self.batch_session_id = None;

        actions.push(CodeGenAction::AppendOutput(
            "\n=== Matrix Benchmark Paused ===\n".into(),
        ));

        actions
    }

    /// Cancel matrix benchmark.
    /// Returns actions for parent to process.
    pub fn cancel_matrix(&mut self) -> Vec<CodeGenAction> {
        let mut actions = Vec::new();

        // Delete batch state on cancel
        if let Some(sid) = &self.batch_session_id {
            actions.push(CodeGenAction::DeleteBatch(sid.clone()));
        }

        self.running = false;
        actions.push(CodeGenAction::ClearCancelToken);

        self.combo_queue.clear();
        self.current_combo = None;
        self.queue_total = 0;
        self.queue_completed = 0;
        self.batch_session_id = None;

        actions.push(CodeGenAction::AppendOutput(
            "\n=== Matrix Benchmark Cancelled ===\n".into(),
        ));

        actions
    }
}
