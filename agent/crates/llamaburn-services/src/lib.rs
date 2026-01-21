mod audio_input;
mod audio_output;
pub mod audio_effects;
mod benchmark;
mod effect_detection;
mod gpu_monitor;
mod history;
mod model_info;
mod ollama;
mod services;
mod settings;
mod whisper;

pub use audio_input::{AudioCaptureConfig, AudioDevice, AudioInputError, AudioInputService, AudioSampleFormat, DeviceType, StreamHandle};
pub use audio_output::{AudioOutputError, AudioOutputService, MonitorHandle, PlaybackHandle};
pub use benchmark::BenchmarkService;
pub use services::Services;
pub use effect_detection::{EffectDetectionError, EffectDetectionService, get_llm_blind_analysis, build_llm_analysis_prompt};
pub use gpu_monitor::{GpuMetrics, GpuMonitor, GpuMonitorError};
pub use history::{AudioHistoryEntry, BatchCombo, BatchState, BatchStatus, BenchmarkHistoryEntry, CodeHistoryEntry, EffectDetectionHistoryEntry, HistoryError, HistoryFilter, HistoryService, Preset, RunStatus};
pub use model_info::{ModelInfo, ModelInfoService};
pub use ollama::{OllamaClient, OllamaError, OllamaModelDetails, OllamaShowResponse};
pub use settings::{keys as settings_keys, SettingsError, SettingsService};
pub use whisper::{get_audio_duration_ms, Segment, TranscriptionResult, WhisperError, WhisperEvent, WhisperService};

// Re-export benchmark types for convenience
pub use llamaburn_benchmark::{BenchmarkEvent, BenchmarkRunner, BenchmarkSummary};

// Re-export core types for GUI (GUI should only import from services)
pub use llamaburn_core::{
    // Config and metrics
    TextBenchmarkConfig, BenchmarkMetrics, BenchmarkType,
    // Models (app state)
    AppModels, ModelList, TextBenchmark, TextBenchmarkResult,
    AudioBenchmark, CodeBenchmark, BenchmarkCombo,
    // Audio types
    AudioBenchmarkConfig, AudioBenchmarkMetrics, AudioBenchmarkResult, AudioMode, AudioSource, AudioSourceMode, WhisperModel,
    EffectDetectionConfig, EffectDetectionResult, EffectDetectionTool,
    // Code types
    CodeBenchmarkConfig, CodeBenchmarkMetrics, CodeBenchmarkSummary, Language,
    CodeProblem, ProblemSet, Difficulty,
    // Model types - aliased to avoid conflict with services::ModelInfo
    ModelConfig, ModelInfo as CoreModelInfo,
};
