use std::time::{Duration, Instant};

use llamaburn_core::{Language, TestCase};
use tempfile::TempDir;
use thiserror::Error;
use tokio::process::Command;
use std::process::Stdio;

#[derive(Debug, Error)]
pub enum CodeExecutorError {
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    #[error("Timeout after {0}ms")]
    Timeout(u64),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CodeExecutorError>;

#[derive(Debug, Clone)]
pub struct TestResult {
    pub passed: bool,
    pub actual_output: String,
    pub expected_output: String,
    pub execution_time_ms: f64,
    pub error: Option<String>,
}

pub struct CodeExecutor {
    temp_dir: TempDir,
}

impl CodeExecutor {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    pub async fn run_tests(
        &self,
        code: &str,
        language: Language,
        test_cases: &[TestCase],
        timeout_ms: u32,
    ) -> Result<Vec<TestResult>> {
        let mut results = Vec::with_capacity(test_cases.len());

        for test_case in test_cases {
            let result = self
                .run_single_test(code, language, test_case, timeout_ms)
                .await?;
            results.push(result);
        }

        Ok(results)
    }

    async fn run_single_test(
        &self,
        code: &str,
        language: Language,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        match language {
            Language::Python => self.run_python(code, test_case, timeout_ms).await,
            Language::JavaScript => self.run_javascript(code, test_case, timeout_ms).await,
            Language::Rust => self.run_rust(code, test_case, timeout_ms).await,
            Language::Go => self.run_go(code, test_case, timeout_ms).await,
        }
    }

    async fn run_python(
        &self,
        code: &str,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let func_name = extract_function_name(code, Language::Python);
        let escaped_input = test_case.input.replace('\\', "\\\\").replace('\'', "\\'");

        let test_code = format!(
            "{code}\n\nimport json\nimport sys\n\nargs = json.loads('{escaped_input}')\nresult = {func_name}(*args)\nprint(json.dumps(result))",
            code = code,
            escaped_input = escaped_input,
            func_name = func_name
        );

        let start = Instant::now();
        let output = self
            .execute_command("python3", &["-c", &test_code], timeout_ms)
            .await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_javascript(
        &self,
        code: &str,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let func_name = extract_function_name(code, Language::JavaScript);
        let escaped_input = test_case.input.replace('\\', "\\\\").replace('\'', "\\'");

        let test_code = format!(
            "{code}\n\nconst args = JSON.parse('{escaped_input}');\nconst result = {func_name}(...args);\nconsole.log(JSON.stringify(result));",
            code = code,
            escaped_input = escaped_input,
            func_name = func_name
        );

        let start = Instant::now();
        let output = self
            .execute_command("node", &["-e", &test_code], timeout_ms)
            .await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_rust(
        &self,
        code: &str,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let func_name = extract_function_name(code, Language::Rust);
        let source_path = self.temp_dir.path().join("solution.rs");
        let binary_path = self.temp_dir.path().join("solution");

        // Build the main wrapper - simplified version that just calls the function
        let full_code = format!(
            r##"#![allow(unused)]
{code}

fn main() {{
    // Test input: {input}
    // This is a simplified runner - real impl would parse JSON args
    println!("{{:?}}", "test");
}}
"##,
            code = code,
            input = test_case.input,
        );

        std::fs::write(&source_path, &full_code)?;

        // Compile
        let compile_output = self
            .execute_command(
                "rustc",
                &[
                    source_path.to_str().unwrap(),
                    "-o",
                    binary_path.to_str().unwrap(),
                    "--edition=2021",
                ],
                30000,
            )
            .await;

        if let Err(e) = &compile_output {
            return Ok(TestResult {
                passed: false,
                actual_output: String::new(),
                expected_output: test_case.expected.clone(),
                execution_time_ms: 0.0,
                error: Some(format!("Compilation failed: {}", e)),
            });
        }

        let start = Instant::now();
        let output = self
            .execute_command(binary_path.to_str().unwrap(), &[], timeout_ms)
            .await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_go(
        &self,
        code: &str,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let func_name = extract_function_name(code, Language::Go);
        let source_path = self.temp_dir.path().join("main.go");
        let escaped_input = test_case.input.replace('`', "'");

        let full_code = format!(
            r#"package main

import (
    "encoding/json"
    "fmt"
)

{code}

func main() {{
    var args []interface{{}}
    json.Unmarshal([]byte("{escaped_input}"), &args)
    result := {func_name}()
    output, _ := json.Marshal(result)
    fmt.Println(string(output))
}}
"#,
            code = code,
            escaped_input = escaped_input.replace('"', "\\\""),
            func_name = func_name,
        );

        std::fs::write(&source_path, &full_code)?;

        let start = Instant::now();
        let output = self
            .execute_command("go", &["run", source_path.to_str().unwrap()], timeout_ms)
            .await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn execute_command(
        &self,
        program: &str,
        args: &[&str],
        timeout_ms: u32,
    ) -> std::result::Result<String, String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .current_dir(self.temp_dir.path());

        let child = cmd.spawn().map_err(|e| e.to_string())?;

        let timeout = Duration::from_millis(timeout_ms as u64);
        let result = tokio::time::timeout(timeout, child.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Err(format!("{}\n{}", stderr, stdout).trim().to_string())
                }
            }
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err(format!("Timeout after {}ms", timeout_ms)),
        }
    }

    fn build_test_result(
        output: std::result::Result<String, String>,
        test_case: &TestCase,
        execution_time_ms: f64,
    ) -> Result<TestResult> {
        match output {
            Ok(actual) => {
                let passed = normalize_output(&actual) == normalize_output(&test_case.expected);
                Ok(TestResult {
                    passed,
                    actual_output: actual,
                    expected_output: test_case.expected.clone(),
                    execution_time_ms,
                    error: None,
                })
            }
            Err(e) => Ok(TestResult {
                passed: false,
                actual_output: String::new(),
                expected_output: test_case.expected.clone(),
                execution_time_ms,
                error: Some(e),
            }),
        }
    }
}

impl Default for CodeExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create temp directory")
    }
}

fn extract_function_name(code: &str, language: Language) -> String {
    let pattern = match language {
        Language::Python => r"def\s+(\w+)\s*\(",
        Language::JavaScript => r"function\s+(\w+)\s*\(|const\s+(\w+)\s*=",
        Language::Rust => r"fn\s+(\w+)\s*[<(]",
        Language::Go => r"func\s+(\w+)\s*\(",
    };

    let re = regex::Regex::new(pattern).unwrap();
    re.captures(code)
        .and_then(|c| c.get(1).or_else(|| c.get(2)))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "solution".to_string())
}

fn normalize_output(s: &str) -> String {
    s.trim()
        .replace(' ', "")
        .replace('\n', "")
        .replace('\r', "")
}
