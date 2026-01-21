// Domain modules
pub mod ai;
pub mod ai_selector;
pub mod audio;
pub mod benchmark_type;
pub mod code;
pub mod error;
pub mod history;
pub mod system;
pub mod text;

pub use ai::{Modality, ModelConfig, ModelInfo};
pub use ai_selector::ModelList;
pub use audio::{
    AppliedEffect, AudioBenchmark, AudioBenchmarkConfig, AudioBenchmarkMetrics,
    AudioBenchmarkResult, AudioBenchmarkSummary, AudioCaptureConfig, AudioDevice, AudioMode,
    AudioSampleFormat, AudioSource, AudioSourceMode, DetectedEffect, DeviceType,
    EffectDetectionConfig, EffectDetectionResult, EffectDetectionTool, Segment, SignalAnalysis,
    TranscriptionResult, TranscriptionSegment, WhisperEvent, WhisperModel, CHANNEL_OPTIONS,
    SAMPLE_RATES,
};
pub use benchmark_type::BenchmarkType;
pub use code::{
    BenchmarkCombo, CodeBenchmark, CodeBenchmarkConfig, CodeBenchmarkMetrics, CodeBenchmarkResult,
    CodeBenchmarkSummary, CodeProblem, Difficulty, ErrorLogEntry, EvaluationMode, Language, Preset,
    ProblemSet, TestCase,
};
pub use error::{LlamaBurnError, Result};
pub use history::{
    AudioHistoryEntry, BatchCombo, BatchState, BatchStatus, BenchmarkHistoryEntry,
    CodeHistoryEntry, EffectDetectionHistoryEntry, HistoryFilter, RunStatus,
};
pub use system::GpuMetrics;
pub use text::{
    BenchmarkMetrics, TextBenchmark, TextBenchmarkConfig, TextBenchmarkResult, TextBenchmarkSummary,
};

/// Root application models container
#[derive(Debug, Clone, Default)]
pub struct AppModels {
    pub models: ModelList,
    pub text: TextBenchmark,
    pub audio: AudioBenchmark,
    pub code: CodeBenchmark,
}

impl AppModels {
    pub fn new() -> Self {
        Self {
            models: ModelList::new(),
            text: TextBenchmark::new(),
            audio: AudioBenchmark::new(),
            code: CodeBenchmark::new(),
        }
    }
}
