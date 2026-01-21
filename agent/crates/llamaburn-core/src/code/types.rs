use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{CodeBenchmarkConfig, Language};

// =============================================================================
// Simple Types (no internal dependencies)
// =============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestCase {
    pub input: String,
    pub expected: String,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CodeBenchmarkSummary {
    pub pass_rate: f64,
    pub problems_solved: u32,
    pub problems_total: u32,
    pub avg_tps: f64,
    pub avg_execution_time_ms: f64,
    #[serde(default)]
    pub easy_solved: u32,
    #[serde(default)]
    pub easy_total: u32,
    #[serde(default)]
    pub medium_solved: u32,
    #[serde(default)]
    pub medium_total: u32,
    #[serde(default)]
    pub hard_solved: u32,
    #[serde(default)]
    pub hard_total: u32,
}

// =============================================================================
// Types with internal dependencies
// =============================================================================

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CodeBenchmarkMetrics {
    pub problem_id: String,
    #[serde(default)]
    pub difficulty: Difficulty,
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

fn default_time_limit() -> u32 {
    5000
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProblemSet {
    pub name: String,
    #[serde(default)]
    pub evaluation_mode: EvaluationMode,
    pub problems: Vec<CodeProblem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCombo {
    pub model: String,
    pub language: Language,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLogEntry {
    pub problem_id: String,
    pub language: Language,
    pub model: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub model_id: String,
    pub language: Language,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub problem_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeBenchmarkResult {
    pub config: CodeBenchmarkConfig,
    pub metrics: Vec<CodeBenchmarkMetrics>,
    pub summary: CodeBenchmarkSummary,
}
