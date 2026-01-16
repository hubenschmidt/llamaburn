#[cfg(feature = "audio-input")]
mod audio_input;
#[cfg(feature = "audio-input")]
mod audio_output;
#[cfg(feature = "audio-input")]
pub mod audio_effects;
mod benchmark;
mod effect_detection;
mod gpu_monitor;
mod history;
mod model_info;
mod ollama;
mod settings;
mod whisper;

#[cfg(feature = "audio-input")]
pub use audio_input::{AudioCaptureConfig, AudioDevice, AudioInputError, AudioInputService, AudioSampleFormat, DeviceType, StreamHandle};
#[cfg(feature = "audio-input")]
pub use audio_output::{AudioOutputError, AudioOutputService, MonitorHandle, PlaybackHandle};
pub use benchmark::BenchmarkService;
pub use effect_detection::{EffectDetectionError, EffectDetectionService, get_llm_blind_analysis, build_llm_analysis_prompt};
pub use gpu_monitor::{GpuMetrics, GpuMonitor, GpuMonitorError};
pub use history::{AudioHistoryEntry, BenchmarkHistoryEntry, EffectDetectionHistoryEntry, HistoryError, HistoryFilter, HistoryService};
pub use model_info::{ModelInfo, ModelInfoService};
pub use ollama::{OllamaClient, OllamaError, OllamaModelDetails, OllamaShowResponse};
pub use settings::{keys as settings_keys, SettingsError, SettingsService};
pub use whisper::{get_audio_duration_ms, Segment, TranscriptionResult, WhisperError, WhisperEvent, WhisperService};

// Re-export benchmark types for convenience
pub use llamaburn_benchmark::{BenchmarkEvent, BenchmarkRunner, BenchmarkSummary};
