use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{AudioBenchmarkConfig, EffectDetectionTool};

// =============================================================================
// Simple Types (no internal dependencies)
// =============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub rtf: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBenchmarkMetrics {
    pub real_time_factor: f64,
    pub processing_time_ms: f64,
    pub audio_duration_ms: f64,
    pub transcription: String,
    pub word_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignalAnalysis {
    pub detected_delay_ms: Option<f64>,
    pub detected_reverb_rt60_ms: Option<f64>,
    pub frequency_change_db: Option<f64>,
    pub dynamic_range_change_db: Option<f64>,
    pub crest_factor_change: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioBenchmarkSummary {
    pub avg_rtf: f64,
    pub min_rtf: f64,
    pub max_rtf: f64,
    pub avg_processing_ms: f64,
    pub iterations: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioSourceMode {
    #[default]
    File,
    Capture,
    LiveStream,
}

impl AudioSourceMode {
    pub fn label(&self) -> &'static str {
        match self {
            AudioSourceMode::File => "File",
            AudioSourceMode::Capture => "Capture",
            AudioSourceMode::LiveStream => "Live",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioSource {
    #[default]
    File,
    Capture {
        device_id: String,
        duration_secs: u32,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioSampleFormat {
    I16,
    I24,
    #[default]
    F32,
}

impl AudioSampleFormat {
    pub fn label(&self) -> &'static str {
        match self {
            AudioSampleFormat::I16 => "16-bit",
            AudioSampleFormat::I24 => "24-bit",
            AudioSampleFormat::F32 => "32-bit float",
        }
    }

    pub fn all() -> &'static [AudioSampleFormat] {
        &[
            AudioSampleFormat::I16,
            AudioSampleFormat::I24,
            AudioSampleFormat::F32,
        ]
    }
}

pub const SAMPLE_RATES: &[u32] = &[
    8000, 11025, 16000, 22050, 44100, 48000, 88200, 96000, 176400, 192000,
];

pub const CHANNEL_OPTIONS: &[(u16, &str)] = &[(1, "1 (Mono)"), (2, "2 (Stereo)")];

// =============================================================================
// Whisper/Transcription Types
// =============================================================================

#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub text: String,
    pub segments: Vec<Segment>,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum WhisperEvent {
    LoadingModel { model: super::WhisperModel },
    ModelLoaded { load_time_ms: u64 },
    LoadingAudio { path: PathBuf },
    AudioLoaded { duration_ms: u64 },
    Transcribing,
    TranscriptionComplete { result: TranscriptionResult, processing_ms: u64 },
    Error { message: String },
}

// =============================================================================
// Audio Input Types
// =============================================================================

/// Device type for grouping and display
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    /// Direct hardware (hw:)
    Hardware,
    /// Plugin hardware with conversion (plughw:) - recommended
    PluginHardware,
    /// PulseAudio/PipeWire
    PulseAudio,
    /// System default
    Default,
    /// Other (surround, dsnoop, etc.)
    Other,
}

/// Audio input device information
#[derive(Debug, Clone)]
pub struct AudioDevice {
    /// Raw ALSA device name (e.g., "plughw:CARD=R24,DEV=0")
    pub name: String,
    /// Device ID for selection
    pub id: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub is_default: bool,
    /// Card identifier (e.g., "R24", "Generic_1")
    pub card_id: Option<String>,
    /// Friendly card name from ALSA (e.g., "ZOOM R24", "HD-Audio Generic")
    pub card_name: Option<String>,
    /// Device type hint for display
    pub device_type: DeviceType,
}

/// Configuration for audio capture
#[derive(Debug, Clone)]
pub struct AudioCaptureConfig {
    pub sample_rate: u32,
    pub sample_format: AudioSampleFormat,
    pub channels: u16,
}

impl Default for AudioCaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            sample_format: AudioSampleFormat::F32,
            channels: 2,
        }
    }
}

// =============================================================================
// Effect Types (some internal dependencies)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedEffect {
    pub name: String,
    pub parameters: HashMap<String, f32>,
    pub bypassed: bool,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectDetectionConfig {
    pub tool: EffectDetectionTool,
    pub audio_path: PathBuf,
    pub reference_audio_path: Option<PathBuf>,
    pub iterations: u32,
}

impl Default for EffectDetectionConfig {
    fn default() -> Self {
        Self {
            tool: EffectDetectionTool::default(),
            audio_path: PathBuf::new(),
            reference_audio_path: None,
            iterations: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectDetectionResult {
    pub tool: EffectDetectionTool,
    pub effects: Vec<DetectedEffect>,
    pub processing_time_ms: f64,
    pub audio_duration_ms: f64,
    pub embeddings: Option<Vec<f32>>,
    pub applied_effects: Option<Vec<AppliedEffect>>,
    pub signal_analysis: Option<SignalAnalysis>,
    pub llm_description: Option<String>,
    pub llm_model_used: Option<String>,
    pub embedding_distance: Option<f64>,
    pub cosine_similarity: Option<f64>,
}

// =============================================================================
// Result Types (depends on config/metrics/summary)
// =============================================================================

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
