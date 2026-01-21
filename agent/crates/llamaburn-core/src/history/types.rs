use serde::{Deserialize, Serialize};

use crate::{
    AudioBenchmarkConfig, AudioBenchmarkMetrics, AudioBenchmarkSummary, AudioMode,
    BenchmarkMetrics, BenchmarkType, CodeBenchmarkConfig, CodeBenchmarkMetrics,
    CodeBenchmarkSummary, EffectDetectionResult, EffectDetectionTool, Language,
    TextBenchmarkConfig, TextBenchmarkSummary,
};

// Re-export Preset from code module (already defined there)
pub use crate::code::Preset;

// =============================================================================
// Status Enums
// =============================================================================

/// Status of an individual benchmark run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    #[default]
    Success,
    Error,
    Skipped,
    Paused,
    Cancelled,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Success => "success",
            RunStatus::Error => "error",
            RunStatus::Skipped => "skipped",
            RunStatus::Paused => "paused",
            RunStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "error" => RunStatus::Error,
            "skipped" => RunStatus::Skipped,
            "paused" => RunStatus::Paused,
            "cancelled" => RunStatus::Cancelled,
            _ => RunStatus::Success,
        }
    }
}

/// Status of a batch benchmark run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BatchStatus {
    Running,
    Paused,
    Completed,
}

impl BatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BatchStatus::Running => "running",
            BatchStatus::Paused => "paused",
            BatchStatus::Completed => "completed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => BatchStatus::Running,
            "paused" => BatchStatus::Paused,
            "completed" => BatchStatus::Completed,
            _ => BatchStatus::Paused,
        }
    }
}

// =============================================================================
// History Entry Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkHistoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub benchmark_type: BenchmarkType,
    pub model_id: String,
    pub config: TextBenchmarkConfig,
    pub summary: TextBenchmarkSummary,
    pub metrics: Vec<BenchmarkMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioHistoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub benchmark_type: BenchmarkType,
    pub audio_mode: AudioMode,
    pub model_id: String,
    pub config: AudioBenchmarkConfig,
    pub summary: AudioBenchmarkSummary,
    pub metrics: Vec<AudioBenchmarkMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeHistoryEntry {
    pub id: String,
    pub timestamp: i64,
    pub benchmark_type: BenchmarkType,
    pub model_id: String,
    pub language: Language,
    pub config: CodeBenchmarkConfig,
    pub summary: CodeBenchmarkSummary,
    pub metrics: Vec<CodeBenchmarkMetrics>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub status: RunStatus,
    #[serde(default)]
    pub preset_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectDetectionHistoryEntry {
    pub id: i64,
    pub tool: EffectDetectionTool,
    pub audio_path: String,
    pub result: EffectDetectionResult,
    pub created_at: i64,
}

// =============================================================================
// Batch Types
// =============================================================================

/// A single benchmark combination in a matrix run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCombo {
    pub model: String,
    pub language: Language,
    pub temperature: f32,
    pub max_tokens: u32,
}

/// Persisted state for a resumable batch benchmark session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchState {
    pub session_id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub status: BatchStatus,
    pub selected_models: Vec<String>,
    pub selected_languages: Vec<Language>,
    pub selected_temperatures: Vec<f32>,
    pub selected_max_tokens: Vec<u32>,
    pub selected_problem_ids: Vec<String>,
    pub auto_run_tests: bool,
    pub skip_on_error: bool,
    pub pending_combos: Vec<BatchCombo>,
    pub queue_total: usize,
    pub queue_completed: usize,
    pub failed_combo: Option<BatchCombo>,
    pub error_message: Option<String>,
}

// =============================================================================
// Filter Types
// =============================================================================

#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    pub model_id: Option<String>,
    pub benchmark_type: Option<BenchmarkType>,
    pub limit: Option<u32>,
}
