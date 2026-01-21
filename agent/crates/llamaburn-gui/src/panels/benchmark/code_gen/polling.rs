//! Event polling for code benchmark panel

use std::time::{SystemTime, UNIX_EPOCH};

use llamaburn_benchmark::CodeBenchmarkEvent;
use llamaburn_services::{CodeBenchmark, Language};
use llamaburn_services::{BatchCombo, BatchStatus, RunStatus};

use super::error_log::ErrorLogEntry;
use super::util::is_harness_error;
use super::{CodeGenAction, CodeGenBenchmarkPanel};

impl CodeGenBenchmarkPanel {
    /// Poll for benchmark events and return actions for parent to process
    pub fn poll(&mut self, model: &mut CodeBenchmark) -> Vec<CodeGenAction> {
        let Some(rx) = self.code_rx.take() else {
            return vec![];
        };

        let mut actions = Vec::new();
        let mut should_clear = false;

        while let Ok(event) = rx.try_recv() {
            match event {
                CodeBenchmarkEvent::Warmup { current, total } => {
                    actions.push(CodeGenAction::SetProgress(format!(
                        "Warmup {}/{}",
                        current, total
                    )));
                }
                CodeBenchmarkEvent::Problem {
                    current,
                    total,
                    title,
                } => {
                    actions.push(CodeGenAction::SetProgress(format!(
                        "Problem {}/{}: {}",
                        current, total, title
                    )));
                    let problem_id = self.find_problem_by_title(&title).map(|p| p.id.clone());
                    // Write to model
                    model.set_current_problem(Some(title), problem_id);
                    model.clear_generated_code();
                }
                CodeBenchmarkEvent::GeneratingCode => {
                    actions.push(CodeGenAction::AppendOutput("Generating code...\n".into()));
                }
                CodeBenchmarkEvent::Token { content } => {
                    model.append_generated_code(&content);
                    actions.push(CodeGenAction::AppendOutput(content));
                }
                CodeBenchmarkEvent::ExecutingTests { total } => {
                    actions.push(CodeGenAction::AppendOutput(format!(
                        "\nRunning {} tests...",
                        total
                    )));
                }
                CodeBenchmarkEvent::TestResult {
                    test_num,
                    test_total,
                    passed,
                    expected,
                    actual,
                    error,
                } => {
                    let status = if passed { "PASS" } else { "FAIL" };
                    actions.push(CodeGenAction::AppendOutput(format!(
                        "\n  Test {}/{}: {} ",
                        test_num, test_total, status
                    )));

                    if !passed {
                        actions.push(CodeGenAction::AppendOutput(format!(
                            "Expected: {} | Actual: {}",
                            expected, actual
                        )));
                        self.record_test_failure(
                            test_num,
                            expected.clone(),
                            actual.clone(),
                            error.clone(),
                        );
                    }

                    if let Some(e) = &error {
                        actions.push(CodeGenAction::AppendOutput(format!("\n    Error: {}", e)));
                    }
                }
                CodeBenchmarkEvent::ProblemComplete { metrics } => {
                    actions.push(CodeGenAction::AppendOutput(format!(
                        "\n\n--- {} complete: {}/{} tests passed ---\n",
                        metrics.problem_id, metrics.tests_passed, metrics.tests_total
                    )));
                    // Write to model
                    model.add_metrics(metrics.clone());
                    // Keep in panel for history building
                    self.code_metrics.push(metrics);
                }
                CodeBenchmarkEvent::Done { summary } => {
                    // Write to model
                    model.set_summary(summary.clone());
                    model.stop();
                    self.running = false;

                    // Build history entry for parent to save
                    if let Some(entry) = self.build_history_entry(&summary) {
                        actions.push(CodeGenAction::SaveCodeHistory(entry));
                    }
                    actions.push(CodeGenAction::RefreshRankings);
                    actions.push(CodeGenAction::AppendOutput(format!(
                        "\n=== Benchmark Complete ===\nPass Rate: {:.1}%\nSolved: {}/{}\n",
                        summary.pass_rate * 100.0,
                        summary.problems_solved,
                        summary.problems_total
                    )));
                    should_clear = true;

                    // Record combo duration for ETA calculation
                    if let Some(start) = self.combo_start_time.take() {
                        self.combo_durations_ms.push(start.elapsed().as_millis() as u64);
                    }

                    // Increment completed count
                    self.queue_completed += 1;

                    // Check if more combos in queue
                    if self.combo_queue.is_empty() {
                        // Queue complete - request batch deletion
                        if let Some(sid) = &self.batch_session_id {
                            actions.push(CodeGenAction::DeleteBatch(sid.clone()));
                        }
                        self.current_combo = None;
                    } else {
                        // Update batch state for resume capability
                        if let Some(batch) = self.to_batch_state() {
                            actions.push(CodeGenAction::UpdateBatch(batch));
                        }
                        actions.push(CodeGenAction::AdvanceToNextCombo);
                    }
                }
                CodeBenchmarkEvent::Cancelled => {
                    model.stop();
                    self.running = false;
                    actions.push(CodeGenAction::AppendOutput("\nCancelled\n".into()));
                    should_clear = true;
                }
                CodeBenchmarkEvent::Error { message } => {
                    model.stop();
                    self.running = false;
                    actions.push(CodeGenAction::AppendOutput(format!(
                        "\nError: {}\n",
                        message
                    )));
                    should_clear = true;

                    // Handle based on skip_on_error setting and queue state
                    let has_queue = !self.combo_queue.is_empty();

                    if !has_queue {
                        actions.push(CodeGenAction::SaveFailedHistory {
                            error_message: message.clone(),
                            status: RunStatus::Error,
                        });
                        actions.push(CodeGenAction::SetError(Some(message)));
                        continue;
                    }

                    if self.skip_on_error {
                        // Skip this combo and continue
                        actions.push(CodeGenAction::SaveFailedHistory {
                            error_message: message,
                            status: RunStatus::Skipped,
                        });
                        actions.push(CodeGenAction::AppendOutput(
                            "(Skipping to next combo...)\n".into(),
                        ));
                        self.queue_completed += 1;
                        actions.push(CodeGenAction::AdvanceToNextCombo);
                        continue;
                    }

                    // Auto-pause: save as Error status
                    actions.push(CodeGenAction::SaveFailedHistory {
                        error_message: message.clone(),
                        status: RunStatus::Error,
                    });
                    actions.push(CodeGenAction::AppendOutput(
                        "(Auto-paused - use Resume to continue)\n".into(),
                    ));

                    // Save paused batch state
                    if let Some(mut batch) = self.to_batch_state() {
                        batch.status = BatchStatus::Paused;
                        batch.error_message = Some(message);
                        batch.failed_combo = self.current_combo.as_ref().map(|c| BatchCombo {
                            model: c.model.clone(),
                            language: c.language,
                            temperature: c.temperature,
                            max_tokens: c.max_tokens.unwrap_or(2048),
                        });
                        actions.push(CodeGenAction::UpdateBatch(batch));
                    }

                    // Clear queue to stop processing
                    self.combo_queue.clear();
                    self.current_combo = None;
                }
            }
        }

        if !should_clear {
            self.code_rx = Some(rx);
        }

        actions
    }

    /// Record a test failure to the appropriate log
    fn record_test_failure(
        &mut self,
        test_num: u32,
        expected: String,
        actual: String,
        error: Option<String>,
    ) {
        let combo = self.current_combo.as_ref();
        let problem_id = self.current_problem_id.clone().unwrap_or_default();
        let test_input = self
            .find_problem_by_id(&problem_id)
            .and_then(|p| p.test_cases.get((test_num - 1) as usize))
            .map(|tc| tc.input.clone())
            .unwrap_or_default();

        let entry = ErrorLogEntry {
            timestamp: std::time::Instant::now(),
            model_id: combo.map(|c| c.model.clone()).unwrap_or_default(),
            language: combo.map(|c| c.language).unwrap_or(Language::Python),
            temperature: combo.map(|c| c.temperature).unwrap_or(0.0),
            max_tokens: combo.and_then(|c| c.max_tokens).unwrap_or(2048),
            problem_id,
            test_num,
            test_input,
            expected,
            actual,
            error: error.clone(),
        };

        // Route to appropriate log: harness errors vs test failures
        if is_harness_error(&error) {
            self.error_log.push(entry);
        } else {
            self.test_failure_log.push(entry);
        }
    }

    /// Build a CodeHistoryEntry from current state and summary
    fn build_history_entry(
        &self,
        summary: &llamaburn_services::CodeBenchmarkSummary,
    ) -> Option<llamaburn_services::CodeHistoryEntry> {
        let combo = self.current_combo.as_ref()?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let config = llamaburn_services::CodeBenchmarkConfig {
            model_id: combo.model.clone(),
            language: combo.language,
            problem_ids: self.code_metrics.iter().map(|m| m.problem_id.clone()).collect(),
            temperature: combo.temperature,
            max_tokens: combo.max_tokens,
            warmup_runs: 0,
            run_tests: self.auto_run_tests,
        };

        Some(llamaburn_services::CodeHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            benchmark_type: llamaburn_services::BenchmarkType::Code,
            model_id: combo.model.clone(),
            language: combo.language,
            config,
            summary: summary.clone(),
            metrics: self.code_metrics.clone(),
            session_id: self.batch_session_id.clone(),
            status: RunStatus::Success,
            preset_id: self.active_preset_id.clone(),
        })
    }

    /// Build a failed history entry
    pub fn build_failed_history_entry(
        &self,
        error_message: &str,
        status: RunStatus,
    ) -> Option<llamaburn_services::CodeHistoryEntry> {
        let combo = self.current_combo.as_ref()?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let config = llamaburn_services::CodeBenchmarkConfig {
            model_id: combo.model.clone(),
            language: combo.language,
            problem_ids: self.selected_problem_ids.clone(),
            temperature: combo.temperature,
            max_tokens: combo.max_tokens,
            warmup_runs: 0,
            run_tests: self.auto_run_tests,
        };

        let summary = llamaburn_services::CodeBenchmarkSummary {
            pass_rate: 0.0,
            problems_solved: 0,
            problems_total: 0,
            avg_tps: 0.0,
            avg_execution_time_ms: 0.0,
            easy_solved: 0,
            easy_total: 0,
            medium_solved: 0,
            medium_total: 0,
            hard_solved: 0,
            hard_total: 0,
        };

        Some(llamaburn_services::CodeHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            benchmark_type: llamaburn_services::BenchmarkType::Code,
            model_id: combo.model.clone(),
            language: combo.language,
            config,
            summary,
            metrics: vec![],
            session_id: self.batch_session_id.clone(),
            status,
            preset_id: self.active_preset_id.clone(),
        })
    }
}
