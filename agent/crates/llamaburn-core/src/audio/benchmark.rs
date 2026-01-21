use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{
    AudioBenchmarkConfig, AudioBenchmarkMetrics, AudioBenchmarkResult, AudioMode, AudioSampleFormat,
    AudioSource, AudioSourceMode, EffectDetectionResult, EffectDetectionTool, TranscriptionSegment,
    WhisperModel,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioBenchmark {
    pub iterations: u32,
    pub warmup: u32,
    pub source_mode: AudioSourceMode,
    pub audio_file_path: Option<PathBuf>,
    pub audio_duration_ms: Option<f64>,
    pub whisper_model: Option<WhisperModel>,
    pub capture_duration_secs: u32,

    pub selected_device_id: Option<String>,
    pub playback_device_id: Option<String>,

    pub sample_rate: u32,
    pub sample_format: AudioSampleFormat,
    pub channels: u16,
    pub playback_latency_ms: u32,

    pub selected_effect_tool: EffectDetectionTool,
    pub reference_audio_path: Option<PathBuf>,
    pub effect_detection_result: Option<EffectDetectionResult>,
    pub effect_detection_running: bool,

    pub running: bool,
    pub live_recording: bool,

    #[serde(skip)]
    pub live_output: String,
    #[serde(skip)]
    pub progress: String,
    #[serde(skip)]
    pub error: Option<String>,

    #[serde(skip)]
    pub current_config: Option<AudioBenchmarkConfig>,

    pub result: Option<AudioBenchmarkResult>,
    pub collected_metrics: Vec<AudioBenchmarkMetrics>,

    pub model_best_rtf: Option<f64>,
    pub all_time_best: Option<(String, f64)>,
    pub leaderboard: Vec<(String, f64)>,
    pub last_model_for_rankings: Option<WhisperModel>,

    pub last_model_for_info: Option<WhisperModel>,

    #[serde(skip)]
    pub transcription_segments: Vec<TranscriptionSegment>,
    #[serde(skip)]
    pub input_levels: (f32, f32),
}

impl AudioBenchmark {
    pub fn new() -> Self {
        Self {
            iterations: 5,
            warmup: 2,
            capture_duration_secs: 10,
            sample_rate: 44100,
            sample_format: AudioSampleFormat::default(),
            channels: 2,
            playback_latency_ms: 100,
            selected_effect_tool: EffectDetectionTool::default(),
            ..Default::default()
        }
    }

    pub fn set_iterations(&mut self, n: u32) {
        self.iterations = n;
    }

    pub fn set_warmup(&mut self, n: u32) {
        self.warmup = n;
    }

    pub fn set_whisper_model(&mut self, model: Option<WhisperModel>) {
        self.whisper_model = model;
    }

    pub fn set_audio_file(&mut self, path: Option<PathBuf>, duration_ms: Option<f64>) {
        self.audio_file_path = path;
        self.audio_duration_ms = duration_ms;
    }

    pub fn start(&mut self) {
        self.running = true;
        self.result = None;
        self.collected_metrics.clear();
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn set_result(&mut self, result: AudioBenchmarkResult) {
        self.result = Some(result);
        self.running = false;
    }

    pub fn add_metrics(&mut self, metrics: AudioBenchmarkMetrics) {
        self.collected_metrics.push(metrics);
    }

    pub fn set_rankings(
        &mut self,
        model_best: Option<f64>,
        all_time: Option<(String, f64)>,
        leaderboard: Vec<(String, f64)>,
    ) {
        self.model_best_rtf = model_best;
        self.all_time_best = all_time;
        self.leaderboard = leaderboard;
    }

    pub fn append_output(&mut self, s: &str) {
        self.live_output.push_str(s);
    }

    pub fn set_progress(&mut self, s: String) {
        self.progress = s;
    }

    pub fn set_error(&mut self, e: Option<String>) {
        self.error = e;
    }

    pub fn clear_output(&mut self) {
        self.live_output.clear();
        self.progress.clear();
        self.error = None;
    }

    pub fn to_config(&self) -> AudioBenchmarkConfig {
        let audio_source = match self.source_mode {
            AudioSourceMode::File => AudioSource::File,
            AudioSourceMode::Capture => AudioSource::Capture {
                device_id: "default".to_string(),
                duration_secs: self.capture_duration_secs,
            },
            AudioSourceMode::LiveStream => AudioSource::LiveStream {
                device_id: "default".to_string(),
            },
        };

        AudioBenchmarkConfig {
            audio_mode: AudioMode::Stt,
            audio_source,
            model_size: self.whisper_model,
            audio_path: self.audio_file_path.clone().unwrap_or_default(),
            language: None,
            iterations: self.iterations,
            warmup_runs: self.warmup,
        }
    }
}
