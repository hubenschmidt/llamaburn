use crate::code_executor::{CodeExecutor, TestResult};
use crate::ollama::{code_output_schema, OllamaClient, StructuredCodeResponse};
use llamaburn_core::{
    CodeBenchmarkConfig, CodeBenchmarkMetrics, CodeBenchmarkSummary, CodeProblem, Language,
    LlamaBurnError, Result,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodeBenchmarkEvent {
    Warmup { current: u32, total: u32 },
    Problem { current: u32, total: u32, title: String },
    GeneratingCode,
    Token { content: String },
    ExecutingTests { total: u32 },
    TestResult { test_num: u32, test_total: u32, passed: bool, expected: String, actual: String, error: Option<String> },
    ProblemComplete { metrics: CodeBenchmarkMetrics },
    Done { summary: CodeBenchmarkSummary },
    Cancelled,
    Error { message: String },
}

pub struct CodeBenchmarkRunner {
    client: OllamaClient,
    executor: CodeExecutor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBenchmarkResult {
    pub config: CodeBenchmarkConfig,
    pub metrics: Vec<CodeBenchmarkMetrics>,
    pub summary: CodeBenchmarkSummary,
}

impl CodeBenchmarkRunner {
    pub fn new(ollama_host: &str) -> Self {
        Self {
            client: OllamaClient::new(ollama_host),
            executor: CodeExecutor::default(),
        }
    }

    pub async fn run_streaming(
        &self,
        config: &CodeBenchmarkConfig,
        problems: &[CodeProblem],
        cancel_token: CancellationToken,
        tx: mpsc::Sender<CodeBenchmarkEvent>,
    ) {
        // Warmup
        for i in 0..config.warmup_runs {
            if cancel_token.is_cancelled() {
                let _ = tx.send(CodeBenchmarkEvent::Cancelled).await;
                return;
            }
            let _ = tx
                .send(CodeBenchmarkEvent::Warmup {
                    current: i + 1,
                    total: config.warmup_runs,
                })
                .await;

            if let Err(e) = self.client.warmup(&config.model_id).await {
                let _ = tx
                    .send(CodeBenchmarkEvent::Error {
                        message: e.to_string(),
                    })
                    .await;
                return;
            }
        }

        let mut all_metrics = Vec::with_capacity(problems.len());

        for (idx, problem) in problems.iter().enumerate() {
            if cancel_token.is_cancelled() {
                let _ = tx.send(CodeBenchmarkEvent::Cancelled).await;
                return;
            }

            let _ = tx
                .send(CodeBenchmarkEvent::Problem {
                    current: idx as u32 + 1,
                    total: problems.len() as u32,
                    title: problem.title.clone(),
                })
                .await;

            let metrics = match self
                .run_problem(config, problem, &cancel_token, &tx)
                .await
            {
                Ok(m) => m,
                Err(LlamaBurnError::Cancelled) => {
                    let _ = tx.send(CodeBenchmarkEvent::Cancelled).await;
                    return;
                }
                Err(e) => {
                    // Log error and skip to next problem
                    let _ = tx
                        .send(CodeBenchmarkEvent::Error {
                            message: format!("Problem '{}' failed: {}", problem.title, e),
                        })
                        .await;
                    // Create failed metrics for this problem
                    CodeBenchmarkMetrics {
                        problem_id: problem.id.clone(),
                        difficulty: problem.difficulty,
                        ttft_ms: 0.0,
                        tokens_per_sec: 0.0,
                        tests_passed: 0,
                        tests_total: problem.test_cases.len() as u32,
                        execution_time_ms: 0.0,
                        generated_code: String::new(),
                        compilation_error: Some(e.to_string()),
                        runtime_error: None,
                    }
                }
            };

            let _ = tx
                .send(CodeBenchmarkEvent::ProblemComplete {
                    metrics: metrics.clone(),
                })
                .await;
            all_metrics.push(metrics);
        }

        let summary = Self::calculate_summary(&all_metrics);
        let _ = tx.send(CodeBenchmarkEvent::Done { summary }).await;
    }

    async fn run_problem(
        &self,
        config: &CodeBenchmarkConfig,
        problem: &CodeProblem,
        cancel_token: &CancellationToken,
        tx: &mpsc::Sender<CodeBenchmarkEvent>,
    ) -> Result<CodeBenchmarkMetrics> {
        if cancel_token.is_cancelled() {
            return Err(LlamaBurnError::Cancelled);
        }

        let _ = tx.send(CodeBenchmarkEvent::GeneratingCode).await;
        let start = Instant::now();

        // SINGLE CALL: Get structured output (single source of truth)
        let structured = self
            .get_structured_code(config, problem)
            .await
            .map_err(|e| LlamaBurnError::OllamaError(format!("Structured output failed: {}", e)))?;

        let generation_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Display the code to Live Output (same code that will be tested)
        let _ = tx.send(CodeBenchmarkEvent::Token {
            content: structured.code.clone(),
        }).await;

        // Run tests if enabled - uses same structured response
        let (tests_passed, tests_total, execution_time_ms, compilation_error, runtime_error) =
            match config.run_tests {
                false => (0, 0, 0.0, None, None),
                true => {
                    let results = self
                        .run_tests_structured(&structured, config.language, problem, tx)
                        .await;

                    match results {
                        Err(e) => (0, problem.test_cases.len() as u32, 0.0, Some(e), None),
                        Ok(r) => {
                            let passed = r.iter().filter(|t| t.passed).count() as u32;
                            let total = r.len() as u32;
                            let exec_time = r.iter().map(|t| t.execution_time_ms).sum();
                            let comp_err = r.iter()
                                .filter_map(|t| t.error.as_ref())
                                .find(|e| e.contains("Compilation"))
                                .cloned();
                            let run_err = r.iter()
                                .filter_map(|t| t.error.as_ref())
                                .find(|e| !e.contains("Compilation"))
                                .cloned();
                            (passed, total, exec_time, comp_err, run_err)
                        }
                    }
                }
            };

        Ok(CodeBenchmarkMetrics {
            problem_id: problem.id.clone(),
            difficulty: problem.difficulty,
            ttft_ms: generation_time_ms,  // No streaming, TTFT = total generation time
            tokens_per_sec: 0.0,          // Not measured without streaming
            tests_passed,
            tests_total,
            execution_time_ms,
            generated_code: structured.code,  // Same code that was displayed
            compilation_error,
            runtime_error,
        })
    }

    /// Get structured code output for reliable test execution (CALL 2)
    async fn get_structured_code(
        &self,
        config: &CodeBenchmarkConfig,
        problem: &CodeProblem,
    ) -> Result<StructuredCodeResponse> {
        let prompt = self.build_structured_prompt(problem, config.language);
        let schema = code_output_schema();

        self.client
            .chat_structured(&config.model_id, &prompt, schema, Some(0.0))
            .await
    }

    /// Build prompt for structured output - requests clean JSON response
    fn build_structured_prompt(&self, problem: &CodeProblem, language: Language) -> String {
        let signature = problem
            .signatures
            .get(&language)
            .cloned()
            .unwrap_or_else(|| format!("// Implement {} solution", problem.id));

        let examples = problem
            .test_cases
            .iter()
            .take(2)
            .map(|tc| format!("Input: {}\nOutput: {}", tc.input, tc.expected))
            .collect::<Vec<_>>()
            .join("\n\n");

        format!(
            r#"Implement a solution for this problem in {}.

{}

{}

Examples:
{}

Return a JSON object with exactly these fields:
- "function_name": the name of your solution function (string)
- "imports": array of required imports/packages, names only without 'import' keyword (array of strings)
- "code": the complete function code only - NO package declaration, NO main function, NO example usage (string)"#,
            language.label(),
            signature,
            problem.description,
            examples
        )
    }

    /// Run tests using structured output - clean code components
    async fn run_tests_structured(
        &self,
        structured: &StructuredCodeResponse,
        language: Language,
        problem: &CodeProblem,
        tx: &mpsc::Sender<CodeBenchmarkEvent>,
    ) -> std::result::Result<Vec<TestResult>, String> {
        let test_cases = &problem.test_cases;
        let total = test_cases.len() as u32;

        let _ = tx.send(CodeBenchmarkEvent::ExecutingTests { total }).await;

        let test_results = self
            .executor
            .run_tests_structured(structured, language, test_cases, problem.time_limit_ms)
            .await
            .map_err(|e| e.to_string())?;

        for (idx, result) in test_results.iter().enumerate() {
            let _ = tx
                .send(CodeBenchmarkEvent::TestResult {
                    test_num: idx as u32 + 1,
                    test_total: total,
                    passed: result.passed,
                    expected: result.expected_output.clone(),
                    actual: result.actual_output.clone(),
                    error: result.error.clone(),
                })
                .await;
        }

        Ok(test_results)
    }

    fn calculate_summary(metrics: &[CodeBenchmarkMetrics]) -> CodeBenchmarkSummary {
        use llamaburn_core::Difficulty::{self, *};

        let problems_total = metrics.len() as u32;
        let problems_solved = metrics.iter().filter(|m| m.tests_passed == m.tests_total).count() as u32;
        let pass_rate = match problems_total {
            0 => 0.0,
            _ => problems_solved as f64 / problems_total as f64,
        };

        let avg_tps = match metrics.is_empty() {
            true => 0.0,
            false => metrics.iter().map(|m| m.tokens_per_sec).sum::<f64>() / metrics.len() as f64,
        };

        let avg_execution_time_ms = match metrics.is_empty() {
            true => 0.0,
            false => metrics.iter().map(|m| m.execution_time_ms).sum::<f64>() / metrics.len() as f64,
        };

        // Calculate difficulty breakdown
        let count_by_difficulty = |diff: Difficulty| -> (u32, u32) {
            let matching: Vec<_> = metrics.iter().filter(|m| m.difficulty == diff).collect();
            let total = matching.len() as u32;
            let solved = matching.iter().filter(|m| m.tests_passed == m.tests_total).count() as u32;
            (solved, total)
        };

        let (easy_solved, easy_total) = count_by_difficulty(Easy);
        let (medium_solved, medium_total) = count_by_difficulty(Medium);
        let (hard_solved, hard_total) = count_by_difficulty(Hard);

        CodeBenchmarkSummary {
            pass_rate,
            problems_solved,
            problems_total,
            avg_tps,
            avg_execution_time_ms,
            easy_solved,
            easy_total,
            medium_solved,
            medium_total,
            hard_solved,
            hard_total,
        }
    }
}

/// Run tests only on existing code (no code generation).
/// Returns (tests_passed, tests_total, execution_time_ms)
pub async fn run_tests_only(
    code: &str,
    language: Language,
    problem: &CodeProblem,
    tx: mpsc::Sender<CodeBenchmarkEvent>,
) -> std::result::Result<(u32, u32, f64), String> {
    let executor = CodeExecutor::default();
    let test_cases = &problem.test_cases;
    let total = test_cases.len() as u32;

    let _ = tx.send(CodeBenchmarkEvent::ExecutingTests { total }).await;

    let test_results = executor
        .run_tests(code, language, test_cases, problem.time_limit_ms)
        .await
        .map_err(|e| e.to_string())?;

    for (idx, result) in test_results.iter().enumerate() {
        let _ = tx
            .send(CodeBenchmarkEvent::TestResult {
                test_num: idx as u32 + 1,
                test_total: total,
                passed: result.passed,
                expected: result.expected_output.clone(),
                actual: result.actual_output.clone(),
                error: result.error.clone(),
            })
            .await;
    }

    let passed = test_results.iter().filter(|r| r.passed).count() as u32;
    let exec_time = test_results.iter().map(|r| r.execution_time_ms).sum();

    Ok((passed, total, exec_time))
}
