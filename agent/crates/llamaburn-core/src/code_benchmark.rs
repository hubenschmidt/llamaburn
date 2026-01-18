use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvaluationMode {
    TestExecution,
    LlmJudge { rubric: String },
}

impl Default for EvaluationMode {
    fn default() -> Self {
        EvaluationMode::TestExecution
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Python,
    JavaScript,
    Rust,
    Go,
}

impl Language {
    pub fn label(&self) -> &'static str {
        match self {
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::Rust => "Rust",
            Language::Go => "Go",
        }
    }

    pub fn file_extension(&self) -> &'static str {
        match self {
            Language::Python => "py",
            Language::JavaScript => "js",
            Language::Rust => "rs",
            Language::Go => "go",
        }
    }

    pub fn all() -> &'static [Language] {
        &[
            Language::Python,
            Language::JavaScript,
            Language::Rust,
            Language::Go,
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    #[default]
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    pub fn label(&self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestCase {
    pub input: String,
    pub expected: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeProblem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub difficulty: Difficulty,
    #[serde(default = "default_time_limit")]
    pub time_limit_ms: u32,
    pub signatures: HashMap<Language, String>,
    pub test_cases: Vec<TestCase>,
}

fn default_time_limit() -> u32 {
    5000
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProblemSet {
    pub name: String,
    #[serde(default)]
    pub evaluation_mode: EvaluationMode,
    pub problems: Vec<CodeProblem>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CodeBenchmarkMetrics {
    pub problem_id: String,
    pub ttft_ms: f64,
    pub tokens_per_sec: f64,
    pub tests_passed: u32,
    pub tests_total: u32,
    pub execution_time_ms: f64,
    pub generated_code: String,
    #[serde(default)]
    pub compilation_error: Option<String>,
    #[serde(default)]
    pub runtime_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CodeBenchmarkSummary {
    pub pass_rate: f64,
    pub problems_solved: u32,
    pub problems_total: u32,
    pub avg_tps: f64,
    pub avg_execution_time_ms: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeBenchmarkConfig {
    pub model_id: String,
    pub language: Language,
    pub problem_ids: Vec<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default = "default_warmup")]
    pub warmup_runs: u32,
    #[serde(default = "default_run_tests")]
    pub run_tests: bool,
}

fn default_run_tests() -> bool {
    true
}

fn default_temperature() -> f32 {
    0.2
}

fn default_warmup() -> u32 {
    1
}

impl Default for CodeBenchmarkConfig {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            language: Language::default(),
            problem_ids: Vec::new(),
            temperature: default_temperature(),
            max_tokens: None,
            warmup_runs: default_warmup(),
            run_tests: default_run_tests(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeBenchmarkResult {
    pub config: CodeBenchmarkConfig,
    pub metrics: Vec<CodeBenchmarkMetrics>,
    pub summary: CodeBenchmarkSummary,
}
