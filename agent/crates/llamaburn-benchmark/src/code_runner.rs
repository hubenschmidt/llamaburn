use crate::code_executor::{CodeExecutor, TestResult};
use crate::ollama::OllamaClient;
use futures::StreamExt;
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
    ExecutingTests { current: u32, total: u32 },
    TestResult { passed: bool, expected: String, actual: String, error: Option<String> },
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
                Err(e) => {
                    let _ = tx
                        .send(CodeBenchmarkEvent::Error {
                            message: e.to_string(),
                        })
                        .await;
                    return;
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
        let _ = tx.send(CodeBenchmarkEvent::GeneratingCode).await;

        let prompt = self.build_prompt(problem, config.language);
        let start = Instant::now();

        let stream_result = self
            .client
            .chat_stream(
                &config.model_id,
                &prompt,
                Some(config.temperature),
                config.max_tokens,
            )
            .await?;

        let mut chunk_stream = stream_result;
        let mut generated_code = String::new();
        let mut first_token_time: Option<f64> = None;
        let mut eval_count: u64 = 0;
        let mut eval_duration: i64 = 0;

        while let Some(chunk_result) = chunk_stream.next().await {
            if cancel_token.is_cancelled() {
                return Err(LlamaBurnError::Cancelled);
            }

            let chunk = chunk_result?;

            if !chunk.content.is_empty() {
                if first_token_time.is_none() {
                    first_token_time = Some(start.elapsed().as_secs_f64() * 1000.0);
                }
                generated_code.push_str(&chunk.content);
                let _ = tx
                    .send(CodeBenchmarkEvent::Token {
                        content: chunk.content,
                    })
                    .await;
            }

            if chunk.done {
                eval_count = chunk.eval_count.unwrap_or(0);
                eval_duration = chunk.eval_duration.unwrap_or(0);
            }
        }

        let generation_time_ms = start.elapsed().as_secs_f64() * 1000.0;
        let ttft_ms = first_token_time.unwrap_or(generation_time_ms);

        let eval_duration_ns = eval_duration.max(0) as f64;
        let tokens_per_sec = if eval_duration_ns > 0.0 {
            (eval_count as f64) / (eval_duration_ns / 1_000_000_000.0)
        } else {
            0.0
        };

        // Extract code from response (remove markdown fences if present)
        let code = extract_code(&generated_code);

        // Run tests if enabled
        let (tests_passed, tests_total, execution_time_ms, compilation_error, runtime_error) =
            if config.run_tests {
                let test_results = self
                    .run_tests(&code, config.language, problem, tx)
                    .await;

                match test_results {
                    Ok(results) => {
                        let passed = results.iter().filter(|r| r.passed).count() as u32;
                        let total = results.len() as u32;
                        let exec_time = results.iter().map(|r| r.execution_time_ms).sum();
                        let comp_err = results
                            .iter()
                            .find(|r| r.error.as_ref().map(|e| e.contains("Compilation")).unwrap_or(false))
                            .and_then(|r| r.error.clone());
                        let run_err = results
                            .iter()
                            .find(|r| r.error.is_some() && !r.error.as_ref().unwrap().contains("Compilation"))
                            .and_then(|r| r.error.clone());
                        (passed, total, exec_time, comp_err, run_err)
                    }
                    Err(e) => (0, problem.test_cases.len() as u32, 0.0, Some(e), None),
                }
            } else {
                (0, 0, 0.0, None, None)
            };

        Ok(CodeBenchmarkMetrics {
            problem_id: problem.id.clone(),
            difficulty: problem.difficulty,
            ttft_ms,
            tokens_per_sec,
            tests_passed,
            tests_total,
            execution_time_ms,
            generated_code: code,
            compilation_error,
            runtime_error,
        })
    }

    async fn run_tests(
        &self,
        code: &str,
        language: Language,
        problem: &CodeProblem,
        tx: &mpsc::Sender<CodeBenchmarkEvent>,
    ) -> std::result::Result<Vec<TestResult>, String> {
        let test_cases = &problem.test_cases;
        let mut results = Vec::with_capacity(test_cases.len());

        for (idx, _test_case) in test_cases.iter().enumerate() {
            let _ = tx
                .send(CodeBenchmarkEvent::ExecutingTests {
                    current: idx as u32 + 1,
                    total: test_cases.len() as u32,
                })
                .await;
        }

        match self
            .executor
            .run_tests(code, language, test_cases, problem.time_limit_ms)
            .await
        {
            Ok(test_results) => {
                for result in &test_results {
                    let _ = tx
                        .send(CodeBenchmarkEvent::TestResult {
                            passed: result.passed,
                            expected: result.expected_output.clone(),
                            actual: result.actual_output.clone(),
                            error: result.error.clone(),
                        })
                        .await;
                }
                results = test_results;
            }
            Err(e) => return Err(e.to_string()),
        }

        Ok(results)
    }

    fn build_prompt(&self, problem: &CodeProblem, language: Language) -> String {
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
            r#"Implement the following function in {}. Return ONLY the code, no explanation.

{}

{}

Examples:
{}
"#,
            language.label(),
            signature,
            problem.description,
            examples
        )
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

    for (idx, _) in test_cases.iter().enumerate() {
        let _ = tx
            .send(CodeBenchmarkEvent::ExecutingTests {
                current: idx as u32 + 1,
                total: test_cases.len() as u32,
            })
            .await;
    }

    let test_results = executor
        .run_tests(code, language, test_cases, problem.time_limit_ms)
        .await
        .map_err(|e| e.to_string())?;

    for result in &test_results {
        let _ = tx
            .send(CodeBenchmarkEvent::TestResult {
                passed: result.passed,
                expected: result.expected_output.clone(),
                actual: result.actual_output.clone(),
                error: result.error.clone(),
            })
            .await;
    }

    let passed = test_results.iter().filter(|r| r.passed).count() as u32;
    let total = test_results.len() as u32;
    let exec_time = test_results.iter().map(|r| r.execution_time_ms).sum();

    Ok((passed, total, exec_time))
}

fn extract_code(response: &str) -> String {
    let lines: Vec<&str> = response.lines().collect();

    // Try 1: Find code block with fences (```python, ```rust, etc.)
    if let Some(code) = extract_fenced_code(&lines) {
        return code;
    }

    // Try 2: Look for </think> tag and take everything after
    if let Some(code) = extract_after_think_tag(response) {
        return code;
    }

    // Try 3: Find function definition without fences
    if let Some(code) = extract_function_definition(&lines) {
        return code;
    }

    // Fallback: return trimmed response
    response.trim().to_string()
}

fn extract_fenced_code(lines: &[&str]) -> Option<String> {
    let start_idx = lines.iter().position(|l| l.trim().starts_with("```"))?;
    let end_idx = lines[start_idx + 1..]
        .iter()
        .position(|l| l.trim() == "```")
        .map(|i| start_idx + 1 + i)
        .unwrap_or(lines.len());
    Some(lines[start_idx + 1..end_idx].join("\n"))
}

fn extract_after_think_tag(response: &str) -> Option<String> {
    let after_think = response.split("</think>").nth(1)?.trim();
    if after_think.is_empty() {
        return None;
    }
    // If there's a code fence after </think>, extract it
    let lines: Vec<&str> = after_think.lines().collect();
    if let Some(code) = extract_fenced_code(&lines) {
        return Some(code);
    }
    // Otherwise return everything after </think>
    Some(after_think.to_string())
}

fn extract_function_definition(lines: &[&str]) -> Option<String> {
    // Look for common function definition patterns
    let patterns = ["function ", "def ", "fn ", "pub fn ", "async fn "];
    let start_idx = lines.iter().position(|l| {
        let trimmed = l.trim();
        patterns.iter().any(|p| trimmed.starts_with(p))
    })?;
    // Take from function definition to end (or until we hit obvious non-code)
    let code_lines: Vec<&str> = lines[start_idx..]
        .iter()
        .take_while(|l| !l.trim().starts_with("Return only") && !l.trim().starts_with("That's it"))
        .copied()
        .collect();
    if code_lines.is_empty() {
        return None;
    }
    Some(code_lines.join("\n"))
}
