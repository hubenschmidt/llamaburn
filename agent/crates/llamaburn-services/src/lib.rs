mod audio_input;
mod audio_output;
pub mod audio_effects;
mod benchmark;
mod effect_detection;
mod gpu_monitor;
mod history;
mod model_info;
mod ollama;
mod problem_loader;
pub mod runners;
mod services;
mod settings;
mod whisper;

pub use audio_input::{AudioInputError, AudioInputService, StreamHandle};
pub use audio_output::{AudioOutputError, AudioOutputService, MonitorHandle, PlaybackHandle};
pub use benchmark::BenchmarkService;
pub use services::Services;
pub use effect_detection::{EffectDetectionError, EffectDetectionService, get_llm_blind_analysis, build_llm_analysis_prompt};
pub use gpu_monitor::{GpuMonitor, GpuMonitorError};
pub use history::{HistoryError, HistoryService};
pub use model_info::{ModelInfo, ModelInfoService};
pub use ollama::{OllamaClient, OllamaError, OllamaModelDetails, OllamaShowResponse};
pub use settings::{keys as settings_keys, SettingsError, SettingsService};
pub use whisper::{get_audio_duration_ms, WhisperError, WhisperService};
pub use problem_loader::{load_all_problem_sets, load_problem_set, ProblemLoaderError};

// Re-export benchmark runner types
pub use runners::{
    BenchmarkEvent, BenchmarkResult, BenchmarkRunner, BenchmarkSummary,
    CodeBenchmarkEvent, CodeBenchmarkResult, CodeBenchmarkRunner,
    CodeExecutor, CodeExecutorError, TestResult,
    run_tests_only, code_output_schema, StructuredCodeResponse,
};

// Re-export core types for GUI (GUI should only import from services)
pub use llamaburn_core::{
    // Config and metrics
    TextBenchmarkConfig, BenchmarkMetrics, BenchmarkType,
    // Models (app state)
    AppModels, ModelList, TextBenchmark, TextBenchmarkResult,
    AudioBenchmark, CodeBenchmark, BenchmarkCombo,
    // Audio types
    AudioBenchmarkConfig, AudioBenchmarkMetrics, AudioBenchmarkResult, AudioCaptureConfig,
    AudioDevice, AudioMode, AudioSampleFormat, AudioSource, AudioSourceMode, DeviceType,
    EffectDetectionConfig, EffectDetectionResult, EffectDetectionTool, Segment,
    TranscriptionResult, WhisperEvent, WhisperModel,
    // Code types
    CodeBenchmarkConfig, CodeBenchmarkMetrics, CodeBenchmarkSummary, Language,
    CodeProblem, ProblemSet, Difficulty,
    // History types
    AudioHistoryEntry, BatchCombo, BatchState, BatchStatus, BenchmarkHistoryEntry,
    CodeHistoryEntry, EffectDetectionHistoryEntry, HistoryFilter, Preset, RunStatus,
    // System types
    GpuMetrics,
    // Model types - aliased to avoid conflict with services::ModelInfo
    ModelConfig, ModelInfo as CoreModelInfo,
};
