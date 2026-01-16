use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Audio benchmark modes - designed for future expansion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioMode {
    #[default]
    Stt,               // Speech-to-Text (Whisper)
    EffectDetection,   // Audio effect detection (Fx-Encoder++, OpenAmp, LLM2Fx)
    Tts,               // Text-to-Speech
    MusicSeparation,   // Demucs stem isolation
    MusicTranscription,// Basic Pitch note detection
    MusicGeneration,   // AudioCraft/MusicGen
    LlmMusicAnalysis,  // LLM audio understanding
}

impl AudioMode {
    pub fn label(&self) -> &'static str {
        match self {
            AudioMode::Stt => "STT",
            AudioMode::EffectDetection => "Effect Detection",
            AudioMode::Tts => "TTS",
            AudioMode::MusicSeparation => "Music Separation",
            AudioMode::MusicTranscription => "Music Transcription",
            AudioMode::MusicGeneration => "Music Generation",
            AudioMode::LlmMusicAnalysis => "LLM Music Analysis",
        }
    }

    pub fn is_implemented(&self) -> bool {
        matches!(self, AudioMode::Stt | AudioMode::EffectDetection)
    }

    pub fn all() -> &'static [AudioMode] {
        &[
            AudioMode::Stt,
            AudioMode::EffectDetection,
            AudioMode::Tts,
            AudioMode::MusicSeparation,
            AudioMode::MusicTranscription,
            AudioMode::MusicGeneration,
            AudioMode::LlmMusicAnalysis,
        ]
    }
}

/// Audio source for STT benchmarking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioSource {
    /// Load from file (default behavior)
    #[default]
    File,
    /// Record for fixed duration, then benchmark
    Capture {
        device_id: String,
        duration_secs: u32,
    },
    /// Stream live to Whisper in real-time
    LiveStream {
        device_id: String,
    },
}

impl AudioSource {
    pub fn label(&self) -> &'static str {
        match self {
            AudioSource::File => "File",
            AudioSource::Capture { .. } => "Capture",
            AudioSource::LiveStream { .. } => "Live Stream",
        }
    }

    pub fn is_recording(&self) -> bool {
        !matches!(self, AudioSource::File)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WhisperModel {
    Tiny,
    Base,
    Small,
    #[default]
    Medium,
    Large,
    LargeV3,
    LargeV3Turbo,
}

impl WhisperModel {
    pub fn label(&self) -> &'static str {
        match self {
            WhisperModel::Tiny => "Tiny",
            WhisperModel::Base => "Base",
            WhisperModel::Small => "Small",
            WhisperModel::Medium => "Medium",
            WhisperModel::Large => "Large",
            WhisperModel::LargeV3 => "Large-v3",
            WhisperModel::LargeV3Turbo => "Turbo",
        }
    }

    pub fn filename(&self) -> &'static str {
        match self {
            WhisperModel::Tiny => "ggml-tiny.bin",
            WhisperModel::Base => "ggml-base.bin",
            WhisperModel::Small => "ggml-small.bin",
            WhisperModel::Medium => "ggml-medium.bin",
            WhisperModel::Large => "ggml-large.bin",
            WhisperModel::LargeV3 => "ggml-large-v3.bin",
            WhisperModel::LargeV3Turbo => "ggml-large-v3-turbo.bin",
        }
    }

    pub fn download_url(&self) -> String {
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            self.filename()
        )
    }

    pub fn size_mb(&self) -> u32 {
        match self {
            WhisperModel::Tiny => 75,
            WhisperModel::Base => 142,
            WhisperModel::Small => 466,
            WhisperModel::Medium => 1500,
            WhisperModel::Large => 3100,
            WhisperModel::LargeV3 => 3100,
            WhisperModel::LargeV3Turbo => 1600,
        }
    }

    pub fn all() -> &'static [WhisperModel] {
        &[
            WhisperModel::Tiny,
            WhisperModel::Base,
            WhisperModel::Small,
            WhisperModel::Medium,
            WhisperModel::Large,
            WhisperModel::LargeV3,
            WhisperModel::LargeV3Turbo,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBenchmarkConfig {
    pub audio_mode: AudioMode,
    pub audio_source: AudioSource,
    pub model_size: Option<WhisperModel>,
    pub audio_path: PathBuf,
    pub language: Option<String>,
    pub iterations: u32,
    pub warmup_runs: u32,
}

impl Default for AudioBenchmarkConfig {
    fn default() -> Self {
        Self {
            audio_mode: AudioMode::default(),
            audio_source: AudioSource::default(),
            model_size: None,
            audio_path: PathBuf::new(),
            language: None,
            iterations: 3,
            warmup_runs: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBenchmarkMetrics {
    pub real_time_factor: f64,
    pub processing_time_ms: f64,
    pub audio_duration_ms: f64,
    pub transcription: String,
    pub word_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBenchmarkSummary {
    pub avg_rtf: f64,
    pub min_rtf: f64,
    pub max_rtf: f64,
    pub avg_processing_ms: f64,
    pub iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBenchmarkResult {
    pub config: AudioBenchmarkConfig,
    pub metrics: Vec<AudioBenchmarkMetrics>,
    pub summary: AudioBenchmarkSummary,
}

impl AudioBenchmarkResult {
    pub fn calculate_summary(metrics: &[AudioBenchmarkMetrics]) -> AudioBenchmarkSummary {
        let n = metrics.len() as f64;

        let avg_rtf = metrics.iter().map(|m| m.real_time_factor).sum::<f64>() / n;
        let avg_processing_ms = metrics.iter().map(|m| m.processing_time_ms).sum::<f64>() / n;

        let min_rtf = metrics
            .iter()
            .map(|m| m.real_time_factor)
            .fold(f64::INFINITY, f64::min);
        let max_rtf = metrics
            .iter()
            .map(|m| m.real_time_factor)
            .fold(f64::NEG_INFINITY, f64::max);

        AudioBenchmarkSummary {
            avg_rtf,
            min_rtf,
            max_rtf,
            avg_processing_ms,
            iterations: metrics.len() as u32,
        }
    }
}

// === Effect Detection Types ===

/// Available audio effect detection tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EffectDetectionTool {
    #[default]
    FxEncoderPlusPlus,  // Sony Research - best documented
    OpenAmp,            // Crowd-sourced effect models
    Llm2FxTools,        // LLM-based effect prediction
}

impl EffectDetectionTool {
    pub fn label(&self) -> &'static str {
        match self {
            EffectDetectionTool::FxEncoderPlusPlus => "Fx-Encoder++ (Sony)",
            EffectDetectionTool::OpenAmp => "OpenAmp",
            EffectDetectionTool::Llm2FxTools => "LLM2Fx-Tools",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            EffectDetectionTool::FxEncoderPlusPlus => "Contrastive learning for effect representation",
            EffectDetectionTool::OpenAmp => "Framework for effect detection models",
            EffectDetectionTool::Llm2FxTools => "LLM-based effect chain prediction",
        }
    }

    pub fn all() -> &'static [EffectDetectionTool] {
        &[
            EffectDetectionTool::FxEncoderPlusPlus,
            EffectDetectionTool::OpenAmp,
            EffectDetectionTool::Llm2FxTools,
        ]
    }
}

/// A detected audio effect with confidence and optional parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedEffect {
    pub name: String,
    pub confidence: f32,
    pub parameters: Option<HashMap<String, f32>>,
}

impl DetectedEffect {
    pub fn new(name: impl Into<String>, confidence: f32) -> Self {
        Self {
            name: name.into(),
            confidence,
            parameters: None,
        }
    }

    pub fn with_params(mut self, params: HashMap<String, f32>) -> Self {
        self.parameters = Some(params);
        self
    }
}

/// Result from effect detection analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectDetectionResult {
    pub tool: EffectDetectionTool,
    pub effects: Vec<DetectedEffect>,
    pub processing_time_ms: f64,
    pub audio_duration_ms: f64,
    pub embeddings: Option<Vec<f32>>,
}

/// Configuration for effect detection benchmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectDetectionConfig {
    pub tool: EffectDetectionTool,
    pub audio_path: PathBuf,
    pub iterations: u32,
}

impl Default for EffectDetectionConfig {
    fn default() -> Self {
        Self {
            tool: EffectDetectionTool::default(),
            audio_path: PathBuf::new(),
            iterations: 1,
        }
    }
}
