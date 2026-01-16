use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use llamaburn_core::{
    AudioBenchmarkConfig, AudioBenchmarkResult, AudioMode, AudioSource, BenchmarkConfig,
    BenchmarkMetrics, BenchmarkType, WhisperModel,
};
use llamaburn_services::{
    get_audio_duration_ms, AudioHistoryEntry, BenchmarkEvent, BenchmarkHistoryEntry,
    BenchmarkService, BenchmarkSummary, HistoryService, ModelInfo, ModelInfoService, OllamaClient,
    OllamaError, WhisperService,
};

/// UI-level audio source mode (simpler than AudioSource for UI state)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// Audio sample format for recording
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

    #[cfg(feature = "audio-input")]
    pub fn to_service_format(self) -> llamaburn_services::AudioSampleFormat {
        match self {
            AudioSampleFormat::I16 => llamaburn_services::AudioSampleFormat::I16,
            AudioSampleFormat::I24 => llamaburn_services::AudioSampleFormat::I24,
            AudioSampleFormat::F32 => llamaburn_services::AudioSampleFormat::F32,
        }
    }
}

/// Common sample rates
pub const SAMPLE_RATES: &[u32] = &[
    8000, 11025, 16000, 22050, 44100, 48000, 88200, 96000, 176400, 192000,
];

/// Recording channel options
pub const CHANNEL_OPTIONS: &[(u16, &str)] = &[(1, "1 (Mono)"), (2, "2 (Stereo)")];

/// Transcription segment with timing info
#[cfg(feature = "audio-input")]
#[derive(Debug, Clone)]
pub struct TranscriptionSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub rtf: f64,
}

/// Events from live transcription stream
#[cfg(feature = "audio-input")]
pub enum LiveTranscriptionEvent {
    /// Waveform peaks for display (min, max pairs)
    AudioPeaks(Vec<(f32, f32)>),
    /// Completed transcription segment
    Transcription(TranscriptionSegment),
    /// Streaming output line (verbose token/segment info)
    StreamOutput(String),
    /// GPU metrics update
    GpuMetrics(llamaburn_services::GpuMetrics),
    /// Error occurred
    Error(String),
    /// Recording stopped
    Stopped,
}

/// Audio test state for mic testing and monitoring
#[cfg(feature = "audio-input")]
#[derive(Default)]
pub enum AudioTestState {
    #[default]
    Idle,
    Recording {
        start: std::time::Instant,
    },
    Playing {
        handle: Option<llamaburn_services::PlaybackHandle>,
    },
    Monitoring,
}

/// Events from audio test
#[cfg(feature = "audio-input")]
pub enum AudioTestEvent {
    RecordingComplete {
        samples: Vec<f32>,
        sample_rate: u32,
        channels: u16,
    },
    Error(String),
}

pub struct BenchmarkPanel {
    // Model selection
    models: Vec<String>,
    selected_model: String,
    loading_models: bool,
    model_rx: Option<Receiver<Result<Vec<String>, OllamaError>>>,
    ollama: OllamaClient,

    // Benchmark config
    benchmark_type: BenchmarkType,
    iterations: u32,
    warmup: u32,
    temperature: f32,

    // Benchmark state
    running: bool,
    benchmark_rx: Option<Receiver<BenchmarkEvent>>,
    cancel_token: Option<Arc<CancellationToken>>,
    benchmark_service: BenchmarkService,
    current_config: Option<BenchmarkConfig>,
    collected_metrics: Vec<BenchmarkMetrics>,

    // History
    history_service: Arc<HistoryService>,

    // Rankings
    model_best_tps: Option<f64>,
    all_time_best: Option<(String, f64)>,
    leaderboard: Vec<(String, f64)>,
    last_model_for_rankings: String,

    // Model info
    model_info_service: ModelInfoService,
    model_info: Option<ModelInfo>,
    model_info_rx: Option<Receiver<Option<ModelInfo>>>,
    last_model_for_info: String,

    // Output
    live_output: String,
    progress: String,
    result: Option<BenchmarkSummary>,
    error: Option<String>,

    // Audio benchmark state
    audio_file_path: Option<PathBuf>,
    audio_duration_ms: Option<f64>,
    whisper_model: Option<WhisperModel>,
    whisper_service: WhisperService,
    audio_result: Option<AudioBenchmarkResult>,
    audio_rx: Option<Receiver<AudioBenchmarkEvent>>,
    last_whisper_model_for_info: Option<WhisperModel>,
    audio_model_info_rx: Option<Receiver<Option<ModelInfo>>>,

    // Audio recording state (requires audio-input feature)
    audio_source_mode: AudioSourceMode,
    #[cfg(feature = "audio-input")]
    audio_devices: Vec<llamaburn_services::AudioDevice>,
    selected_device_id: Option<String>,
    capture_duration_secs: u32,
    #[cfg(feature = "audio-input")]
    loading_devices: bool,

    // Audio rankings
    model_best_rtf: Option<f64>,
    all_time_best_audio: Option<(String, f64)>,
    audio_leaderboard: Vec<(String, f64)>,
    last_whisper_model_for_rankings: Option<WhisperModel>,

    // Live transcription state (DAW mode)
    #[cfg(feature = "audio-input")]
    live_recording: bool,
    #[cfg(feature = "audio-input")]
    waveform_peaks: std::collections::VecDeque<(f32, f32)>,
    #[cfg(feature = "audio-input")]
    recording_start: Option<std::time::Instant>,
    #[cfg(feature = "audio-input")]
    transcription_segments: Vec<TranscriptionSegment>,
    #[cfg(feature = "audio-input")]
    live_transcription_rx: Option<Receiver<LiveTranscriptionEvent>>,
    #[cfg(feature = "audio-input")]
    live_stream_handle: Option<llamaburn_services::StreamHandle>,

    // Audio test state (mic test & monitoring)
    #[cfg(feature = "audio-input")]
    audio_test_state: AudioTestState,
    #[cfg(feature = "audio-input")]
    audio_test_rx: Option<Receiver<AudioTestEvent>>,
    #[cfg(feature = "audio-input")]
    monitor_handle: Option<llamaburn_services::MonitorHandle>,

    // Input level monitor (VU meter)
    #[cfg(feature = "audio-input")]
    level_monitor_handle: Option<llamaburn_services::StreamHandle>,
    #[cfg(feature = "audio-input")]
    level_monitor_rx: Option<Receiver<(f32, f32)>>, // (left_peak, right_peak) 0.0-1.0
    #[cfg(feature = "audio-input")]
    input_levels: (f32, f32), // Current display levels with decay

    // Audio settings dialog
    #[cfg(feature = "audio-input")]
    show_audio_settings: bool,
    #[cfg(feature = "audio-input")]
    audio_sample_rate: u32,
    #[cfg(feature = "audio-input")]
    audio_sample_format: AudioSampleFormat,
    #[cfg(feature = "audio-input")]
    audio_channels: u16,
    #[cfg(feature = "audio-input")]
    playback_device_id: Option<String>,
    #[cfg(feature = "audio-input")]
    playback_latency_ms: u32,

    // Audio effects chain
    #[cfg(feature = "audio-input")]
    effect_chain: std::sync::Arc<std::sync::Mutex<llamaburn_services::audio_effects::EffectChain>>,
    #[cfg(feature = "audio-input")]
    show_effects_ui: bool,
    #[cfg(feature = "audio-input")]
    effects_rack_expanded: bool,
}

/// Events from async audio benchmark
pub enum AudioBenchmarkEvent {
    Progress(String),
    IterationComplete {
        iteration: u32,
        metrics: llamaburn_core::AudioBenchmarkMetrics,
    },
    Done {
        metrics: Vec<llamaburn_core::AudioBenchmarkMetrics>,
    },
    Error(String),
}

impl BenchmarkPanel {
    pub fn new(history_service: Arc<HistoryService>) -> Self {
        let ollama = OllamaClient::default();
        let model_rx = Some(ollama.fetch_models_async());

        Self {
            models: vec![],
            selected_model: String::new(),
            loading_models: true,
            model_rx,
            ollama,
            benchmark_type: BenchmarkType::default(),
            iterations: 5,
            warmup: 2,
            temperature: 0.7,
            running: false,
            benchmark_rx: None,
            cancel_token: None,
            benchmark_service: BenchmarkService::default(),
            current_config: None,
            collected_metrics: Vec::new(),
            history_service,
            model_best_tps: None,
            all_time_best: None,
            leaderboard: Vec::new(),
            last_model_for_rankings: String::new(),
            model_info_service: ModelInfoService::default(),
            model_info: None,
            model_info_rx: None,
            last_model_for_info: String::new(),
            live_output: String::new(),
            progress: String::new(),
            result: None,
            error: None,
            // Audio
            audio_file_path: None,
            audio_duration_ms: None,
            whisper_model: None,
            whisper_service: WhisperService::default(),
            audio_result: None,
            audio_rx: None,
            last_whisper_model_for_info: None,
            audio_model_info_rx: None,
            // Audio recording
            audio_source_mode: AudioSourceMode::default(),
            #[cfg(feature = "audio-input")]
            audio_devices: Vec::new(),
            selected_device_id: None,
            capture_duration_secs: 10,
            #[cfg(feature = "audio-input")]
            loading_devices: false,
            // Audio rankings
            model_best_rtf: None,
            all_time_best_audio: None,
            audio_leaderboard: Vec::new(),
            last_whisper_model_for_rankings: None,
            // Live transcription (DAW mode)
            #[cfg(feature = "audio-input")]
            live_recording: false,
            #[cfg(feature = "audio-input")]
            waveform_peaks: std::collections::VecDeque::new(),
            #[cfg(feature = "audio-input")]
            recording_start: None,
            #[cfg(feature = "audio-input")]
            transcription_segments: Vec::new(),
            #[cfg(feature = "audio-input")]
            live_transcription_rx: None,
            #[cfg(feature = "audio-input")]
            live_stream_handle: None,

            #[cfg(feature = "audio-input")]
            audio_test_state: AudioTestState::Idle,
            #[cfg(feature = "audio-input")]
            audio_test_rx: None,
            #[cfg(feature = "audio-input")]
            monitor_handle: None,

            #[cfg(feature = "audio-input")]
            level_monitor_handle: None,
            #[cfg(feature = "audio-input")]
            level_monitor_rx: None,
            #[cfg(feature = "audio-input")]
            input_levels: (0.0, 0.0),

            #[cfg(feature = "audio-input")]
            show_audio_settings: false,
            #[cfg(feature = "audio-input")]
            audio_sample_rate: 44100,
            #[cfg(feature = "audio-input")]
            audio_sample_format: AudioSampleFormat::default(),
            #[cfg(feature = "audio-input")]
            audio_channels: 2,
            #[cfg(feature = "audio-input")]
            playback_device_id: None,
            #[cfg(feature = "audio-input")]
            playback_latency_ms: 100,

            #[cfg(feature = "audio-input")]
            effect_chain: std::sync::Arc::new(std::sync::Mutex::new(
                llamaburn_services::audio_effects::EffectChain::new(),
            )),
            #[cfg(feature = "audio-input")]
            show_effects_ui: false,
            #[cfg(feature = "audio-input")]
            effects_rack_expanded: true,
        }
    }

    fn refresh_models(&mut self) {
        self.loading_models = true;
        self.model_rx = Some(self.ollama.fetch_models_async());
    }

    fn poll_models(&mut self) {
        let Some(rx) = &self.model_rx else { return };

        if let Ok(result) = rx.try_recv() {
            match result {
                Ok(models) => {
                    self.models = models;
                    self.error = None;
                }
                Err(e) => {
                    self.error = Some(e.to_string());
                }
            }
            self.loading_models = false;
        }
    }

    fn poll_benchmark(&mut self) {
        let Some(rx) = &self.benchmark_rx else { return };

        let mut should_clear = false;
        let mut summary_to_save: Option<BenchmarkSummary> = None;

        while let Ok(event) = rx.try_recv() {
            match event {
                BenchmarkEvent::Warmup { current, total } => {
                    self.progress = format!("Warmup {}/{}", current, total);
                    debug!("Warmup {}/{}", current, total);
                }
                BenchmarkEvent::Iteration {
                    current,
                    total,
                    prompt: _,
                } => {
                    self.progress = format!("Iteration {}/{}", current, total);
                    self.live_output.push_str("\n\n--- New Iteration ---\n");
                    debug!("Iteration {}/{}", current, total);
                }
                BenchmarkEvent::Token { content } => {
                    self.live_output.push_str(&content);
                }
                BenchmarkEvent::IterationComplete { metrics } => {
                    self.live_output.push_str(&format!(
                        "\n[{:.2} tokens/sec, {:.0}ms]\n",
                        metrics.tokens_per_sec, metrics.total_generation_ms
                    ));
                    self.collected_metrics.push(metrics);
                }
                BenchmarkEvent::Done { summary } => {
                    info!("Benchmark complete: {:.2} avg TPS", summary.avg_tps);
                    self.progress = "Complete".to_string();
                    self.running = false;
                    self.result = Some(summary.clone());
                    summary_to_save = Some(summary);
                    should_clear = true;
                }
                BenchmarkEvent::Cancelled => {
                    info!("Benchmark cancelled");
                    self.progress = "Cancelled".to_string();
                    self.running = false;
                    should_clear = true;
                }
                BenchmarkEvent::Error { message } => {
                    self.error = Some(message);
                    self.running = false;
                    self.progress = "Error".to_string();
                    should_clear = true;
                }
            }
        }

        if should_clear {
            self.benchmark_rx = None;
            self.cancel_token = None;
        }

        if let Some(summary) = summary_to_save {
            self.save_to_history(&summary);
            self.force_refresh_rankings();
        }
    }

    fn poll_audio_benchmark(&mut self) {
        let Some(rx) = self.audio_rx.take() else {
            return;
        };

        let mut should_clear = false;

        while let Ok(event) = rx.try_recv() {
            match event {
                AudioBenchmarkEvent::Progress(msg) => {
                    self.live_output.push_str(&msg);
                    self.live_output.push('\n');
                }
                AudioBenchmarkEvent::IterationComplete { iteration, metrics } => {
                    self.progress = format!("Iteration {}", iteration);
                    self.live_output.push_str(&format!(
                        "Run {}: RTF={:.3}x ({:.0}ms) | {} words\n",
                        iteration,
                        metrics.real_time_factor,
                        metrics.processing_time_ms,
                        metrics.word_count
                    ));
                }
                AudioBenchmarkEvent::Done { metrics } => {
                    let summary = AudioBenchmarkResult::calculate_summary(&metrics);

                    self.live_output.push_str(&format!(
                        "\nSummary\n\
                         -------\n\
                         Avg RTF: {:.3}x ({:.0}x real-time)\n\
                         Avg Time: {:.0}ms\n\
                         Min/Max RTF: {:.3}/{:.3}\n",
                        summary.avg_rtf,
                        1.0 / summary.avg_rtf,
                        summary.avg_processing_ms,
                        summary.min_rtf,
                        summary.max_rtf,
                    ));

                    if let Some(first) = metrics.first() {
                        self.live_output.push_str(&format!(
                            "\nTranscription ({} words):\n{}\n",
                            first.word_count, first.transcription
                        ));
                    }

                    let result = AudioBenchmarkResult {
                        config: AudioBenchmarkConfig {
                            audio_mode: AudioMode::Stt,
                            audio_source: AudioSource::File,
                            model_size: self.whisper_model,
                            audio_path: self.audio_file_path.clone().unwrap_or_default(),
                            language: None,
                            iterations: self.iterations,
                            warmup_runs: self.warmup,
                        },
                        metrics,
                        summary,
                    };

                    // Save to history
                    self.save_audio_to_history(&result);

                    self.audio_result = Some(result);
                    self.progress = "Complete".to_string();
                    self.running = false;
                    should_clear = true;

                    // Refresh rankings after saving
                    self.force_refresh_audio_rankings();
                }
                AudioBenchmarkEvent::Error(msg) => {
                    self.live_output.push_str(&format!("\nError: {}\n", msg));
                    self.error = Some(msg);
                    self.progress = "Error".to_string();
                    self.running = false;
                    should_clear = true;
                }
            }
        }

        if !should_clear {
            self.audio_rx = Some(rx);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll_models();
        self.poll_benchmark();
        self.poll_audio_benchmark();
        #[cfg(feature = "audio-input")]
        self.poll_live_transcription();
        #[cfg(feature = "audio-input")]
        self.poll_audio_test();
        #[cfg(feature = "audio-input")]
        self.check_playback_completion();
        #[cfg(feature = "audio-input")]
        self.poll_level_monitor();
        self.poll_model_info();
        self.poll_audio_model_info();
        self.refresh_rankings();
        self.refresh_audio_rankings();
        self.refresh_model_info();
        self.refresh_audio_model_info();

        ui.label(
            egui::RichText::new("Benchmark Runner")
                .heading()
                .color(egui::Color32::GRAY),
        );
        ui.add_space(10.0);

        self.render_type_selector(ui);
        ui.add_space(10.0);

        // Config, Model Info, and Results - responsive columns
        let available = ui.available_width();
        let spacing = 15.0;
        let separator_width = 10.0;
        let total_spacing = (spacing * 4.0) + (separator_width * 2.0);
        let content_width = (available - total_spacing).max(300.0);

        // Proportional widths: Config 35%, Model Info 30%, Results 35%
        let config_width = content_width * 0.35;
        let info_width = content_width * 0.30;
        let results_width = content_width * 0.35;

        ui.horizontal(|ui| {
            // Left: Config
            ui.vertical(|ui| {
                ui.set_width(config_width);
                self.render_config(ui);
            });

            ui.add_space(spacing);
            ui.separator();
            ui.add_space(spacing);

            // Center: Model Info + Whisper Models (for Audio mode)
            ui.vertical(|ui| {
                ui.set_width(info_width);
                self.render_model_info(ui);

                // Whisper Models for Audio mode
                if self.benchmark_type == BenchmarkType::Audio {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(5.0);
                    self.render_model_downloads(ui);
                }
            });

            ui.add_space(spacing);
            ui.separator();
            ui.add_space(spacing);

            // Right: Results
            ui.vertical(|ui| {
                ui.set_width(results_width);
                self.render_results(ui);
            });
        });

        // Full-width waveform display when live recording
        #[cfg(feature = "audio-input")]
        if self.live_recording || !self.waveform_peaks.is_empty() {
            ui.add_space(10.0);
            self.render_waveform_display(ui);
        }

        ui.add_space(10.0);

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
            ui.add_space(10.0);
        }

        // Effects rack panel at bottom (Audio mode only) - reserve space
        #[cfg(feature = "audio-input")]
        let effects_rack_height = {
            let is_audio = self.benchmark_type == BenchmarkType::Audio;
            // 40px collapsed, 230px expanded (180px panel + header/padding)
            let heights = [0.0, [40.0, 230.0][self.effects_rack_expanded as usize]];
            heights[is_audio as usize]
        };
        #[cfg(not(feature = "audio-input"))]
        let effects_rack_height = 0.0;

        // Live output takes remaining space (minus effects rack)
        self.render_live_output_with_reserved(ui, effects_rack_height);

        // Effects rack panel at bottom (Audio mode only)
        #[cfg(feature = "audio-input")]
        if self.benchmark_type == BenchmarkType::Audio {
            ui.add_space(10.0);
            self.render_effects_rack(ui);
        }

        // Audio settings dialog (rendered as egui Window)
        #[cfg(feature = "audio-input")]
        self.render_audio_settings_dialog(ui.ctx());
    }

    fn render_type_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for bt in BenchmarkType::all() {
                let selected = self.benchmark_type == *bt;
                let enabled = bt.is_implemented() && !self.running;

                let response =
                    ui.add_enabled(enabled, egui::SelectableLabel::new(selected, bt.label()));

                if response.clicked() && self.benchmark_type != *bt {
                    self.benchmark_type = *bt;
                    // Clear model info when switching tabs
                    self.model_info = None;
                    self.last_model_for_info.clear();
                    self.last_whisper_model_for_info = None;
                    // Clear rankings
                    self.model_best_tps = None;
                    self.model_best_rtf = None;
                    self.all_time_best = None;
                    self.all_time_best_audio = None;
                    self.leaderboard.clear();
                    self.audio_leaderboard.clear();
                    self.last_model_for_rankings.clear();
                    self.last_whisper_model_for_rankings = None;
                }
            }
        });
    }

    fn render_config(&mut self, ui: &mut egui::Ui) {
        if self.benchmark_type == BenchmarkType::Audio {
            self.render_audio_config(ui);
            return;
        }

        self.render_text_config(ui);
    }

    fn render_text_config(&mut self, ui: &mut egui::Ui) {
        let disabled = self.running || self.loading_models;

        egui::Grid::new("config_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Model:");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!disabled, |ui| {
                        let selected_text = match (
                            self.loading_models,
                            self.models.is_empty(),
                            self.selected_model.is_empty(),
                        ) {
                            (true, _, _) => "Loading models...",
                            (_, true, _) => "No models found",
                            (_, _, true) => "Select model...",
                            _ => &self.selected_model,
                        };

                        egui::ComboBox::from_id_salt("model_select")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for model in &self.models {
                                    ui.selectable_value(
                                        &mut self.selected_model,
                                        model.clone(),
                                        model,
                                    );
                                }
                            });
                    });

                    if self.loading_models {
                        ui.spinner();
                    }

                    let can_unload = !self.selected_model.is_empty() && !self.running;
                    if ui
                        .add_enabled(can_unload, egui::Button::new("Unload"))
                        .clicked()
                    {
                        self.unload_model();
                    }
                });
                ui.end_row();

                ui.label("Iterations:");
                ui.add_enabled(
                    !disabled,
                    egui::DragValue::new(&mut self.iterations).range(1..=100),
                );
                ui.end_row();

                ui.label("Warmup:");
                ui.add_enabled(
                    !disabled,
                    egui::DragValue::new(&mut self.warmup).range(0..=10),
                );
                ui.end_row();

                ui.label("Temperature:");
                ui.add_enabled(
                    !disabled,
                    egui::DragValue::new(&mut self.temperature)
                        .range(0.0..=2.0)
                        .speed(0.1),
                );
                ui.end_row();
            });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let can_run = !self.running && !self.loading_models && !self.selected_model.is_empty();

            if ui
                .add_enabled(can_run, egui::Button::new("Run Benchmark"))
                .clicked()
            {
                self.start_benchmark();
            }

            if ui.button("Refresh Models").clicked() && !self.loading_models {
                self.refresh_models();
            }

            if self.running {
                if ui.button("Cancel").clicked() {
                    self.cancel_benchmark();
                }
                ui.spinner();
            }
        });
    }

    fn render_audio_config(&mut self, ui: &mut egui::Ui) {
        let disabled = self.running;

        // Audio Setup dropdown button (Audacity-style)
        #[cfg(feature = "audio-input")]
        {
            // Auto-load devices on first render
            if self.audio_devices.is_empty() && !self.loading_devices {
                self.refresh_audio_devices();
            }

            ui.horizontal(|ui| {
                let button_text = self
                    .selected_device_id
                    .as_ref()
                    .and_then(|id| self.audio_devices.iter().find(|d| &d.id == id))
                    .map(|d| {
                        // Show friendly card name if available
                        let display_name = d.card_name.as_ref().unwrap_or(&d.name);
                        format!("🔊 {}", display_name)
                    })
                    .unwrap_or_else(|| "🔊 Audio Setup".to_string());

                ui.add_enabled_ui(!disabled, |ui| {
                    ui.menu_button(button_text, |ui| {
                        self.render_audio_device_menu(ui);
                    });
                });

                if self.loading_devices {
                    ui.spinner();
                }

                ui.add_space(8.0);

                // Input monitor toggle - red square button (toggles live monitoring)
                let monitor_active = matches!(self.audio_test_state, AudioTestState::Monitoring);

                // Draw custom square button with hollow center
                let btn_size = egui::vec2(18.0, 18.0);
                let (response, painter) = ui.allocate_painter(btn_size, egui::Sense::click());
                let rect = response.rect;

                let colors = [
                    (
                        egui::Color32::from_rgb(180, 60, 60),
                        egui::Color32::TRANSPARENT,
                    ), // Off: red outline, hollow
                    (
                        egui::Color32::from_rgb(220, 50, 50),
                        egui::Color32::from_rgb(220, 50, 50),
                    ), // On: red filled
                ];
                let (stroke_color, fill_color) = colors[monitor_active as usize];

                painter.rect(
                    rect.shrink(2.0),
                    2.0,
                    fill_color,
                    egui::Stroke::new(2.0, stroke_color),
                );

                if response.clicked() {
                    [Self::start_live_monitor, Self::stop_live_monitor][monitor_active as usize](
                        self,
                    );
                }

                let tooltips = ["Enable live monitoring", "Disable live monitoring"];
                response.on_hover_text(tooltips[monitor_active as usize]);

                // Show "Input Monitor" label when active
                if monitor_active {
                    ui.label(
                        egui::RichText::new("Input Monitor")
                            .small()
                            .color(egui::Color32::from_rgb(220, 50, 50)),
                    );
                }
            });

            // Render VU meter when level monitor is active
            if self.level_monitor_handle.is_some() || self.input_levels.0 > 0.001 {
                self.render_level_meter(ui);
            }

            ui.add_space(8.0);
        }

        egui::Grid::new("audio_config_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Whisper model selector
                ui.label("Model:");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!disabled, |ui| {
                        let selected_text = self
                            .whisper_model
                            .map(|m| m.label())
                            .unwrap_or("Select model...");

                        egui::ComboBox::from_id_salt("whisper_model")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for model in WhisperModel::all() {
                                    let label =
                                        format!("{} (~{}MB)", model.label(), model.size_mb());
                                    if ui
                                        .selectable_label(self.whisper_model == Some(*model), label)
                                        .clicked()
                                    {
                                        self.whisper_model = Some(*model);
                                    }
                                }
                            });
                    });

                    // Unload button
                    let can_unload = self.whisper_model.is_some() && !self.running;
                    if ui
                        .add_enabled(can_unload, egui::Button::new("Unload"))
                        .clicked()
                    {
                        self.unload_whisper_model();
                    }
                });
                ui.end_row();

                // Audio source mode selector
                ui.label("Source:");
                let prev_mode = self.audio_source_mode;
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!disabled, |ui| {
                        ui.selectable_value(
                            &mut self.audio_source_mode,
                            AudioSourceMode::File,
                            "File",
                        );
                        #[cfg(feature = "audio-input")]
                        {
                            ui.selectable_value(
                                &mut self.audio_source_mode,
                                AudioSourceMode::Capture,
                                "Capture",
                            );
                            ui.selectable_value(
                                &mut self.audio_source_mode,
                                AudioSourceMode::LiveStream,
                                "Live",
                            );
                        }
                        #[cfg(not(feature = "audio-input"))]
                        {
                            ui.add_enabled(false, egui::SelectableLabel::new(false, "Capture"))
                                .on_disabled_hover_text("Build with --features audio-input");
                            ui.add_enabled(false, egui::SelectableLabel::new(false, "Live"))
                                .on_disabled_hover_text("Build with --features audio-input");
                        }
                    });
                });
                // Auto-refresh devices when switching to recording mode
                #[cfg(feature = "audio-input")]
                if self.audio_source_mode != prev_mode
                    && self.audio_source_mode != AudioSourceMode::File
                    && self.audio_devices.is_empty()
                {
                    self.refresh_audio_devices();
                }
                ui.end_row();

                // File picker (File mode only)
                if self.audio_source_mode == AudioSourceMode::File {
                    ui.label("Audio:");
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!disabled, egui::Button::new("Select File..."))
                            .clicked()
                        {
                            self.pick_audio_file();
                        }

                        if let Some(path) = &self.audio_file_path {
                            let filename = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown");
                            ui.label(filename);
                        }
                    });
                    ui.end_row();

                    // Show duration if file selected
                    if let Some(duration_ms) = self.audio_duration_ms {
                        ui.label("Duration:");
                        ui.label(format!("{:.1}s", duration_ms / 1000.0));
                        ui.end_row();
                    }
                }

                // Duration slider (Capture mode only)
                #[cfg(feature = "audio-input")]
                if self.audio_source_mode == AudioSourceMode::Capture {
                    ui.label("Duration:");
                    ui.add_enabled(
                        !disabled,
                        egui::Slider::new(&mut self.capture_duration_secs, 5..=60).suffix("s"),
                    );
                    ui.end_row();
                }

                // Model download status
                if let Some(model) = self.whisper_model {
                    ui.label("Status:");
                    let downloaded = self.whisper_service.is_model_downloaded(model);
                    if downloaded {
                        ui.colored_label(egui::Color32::GREEN, "Model ready");
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "Model not downloaded");
                    }
                    ui.end_row();
                }

                ui.label("Iterations:");
                ui.add_enabled(
                    !disabled,
                    egui::DragValue::new(&mut self.iterations).range(1..=20),
                );
                ui.end_row();

                ui.label("Warmup:");
                ui.add_enabled(
                    !disabled,
                    egui::DragValue::new(&mut self.warmup).range(0..=5),
                );
                ui.end_row();
            });

        ui.add_space(10.0);

        // Whisper feature check
        if !WhisperService::is_whisper_enabled() {
            ui.colored_label(
                egui::Color32::RED,
                "Whisper not enabled. Build with --features whisper-gpu",
            );
            return;
        }

        ui.horizontal(|ui| {
            let model_ready = self
                .whisper_model
                .map(|m| self.whisper_service.is_model_downloaded(m))
                .unwrap_or(false);

            let source_ready = match self.audio_source_mode {
                AudioSourceMode::File => self.audio_file_path.is_some(),
                #[cfg(feature = "audio-input")]
                AudioSourceMode::Capture | AudioSourceMode::LiveStream => {
                    self.selected_device_id.is_some()
                }
                #[cfg(not(feature = "audio-input"))]
                _ => false,
            };

            let can_run =
                !self.running && source_ready && self.whisper_model.is_some() && model_ready;

            let button_text = match self.audio_source_mode {
                AudioSourceMode::File => "Run Audio Benchmark",
                AudioSourceMode::Capture => "Record & Benchmark",
                AudioSourceMode::LiveStream => "Start Live Transcription",
            };

            if ui
                .add_enabled(can_run, egui::Button::new(button_text))
                .clicked()
            {
                match self.audio_source_mode {
                    AudioSourceMode::File => self.start_audio_benchmark(),
                    #[cfg(feature = "audio-input")]
                    AudioSourceMode::Capture => self.start_capture_benchmark(),
                    #[cfg(feature = "audio-input")]
                    AudioSourceMode::LiveStream => self.start_live_transcription(),
                    #[cfg(not(feature = "audio-input"))]
                    _ => {}
                }
            }

            #[cfg(feature = "audio-input")]
            if self.running && self.live_recording && ui.button("Stop Recording").clicked() {
                self.stop_live_transcription();
            }

            #[cfg(feature = "audio-input")]
            if self.running && !self.live_recording && ui.button("Cancel").clicked() {
                self.cancel_benchmark();
            }

            #[cfg(not(feature = "audio-input"))]
            if self.running && ui.button("Cancel").clicked() {
                self.cancel_benchmark();
            }

            if self.running {
                ui.spinner();
            }
        });


        // Show audio results if available
        if let Some(result) = &self.audio_result {
            ui.add_space(10.0);
            ui.separator();
            ui.label(egui::RichText::new("Audio Results").strong());

            // Calculate words per second from first metric
            let wps = result
                .metrics
                .first()
                .map(|m| {
                    if m.processing_time_ms > 0.0 {
                        m.word_count as f64 / (m.processing_time_ms / 1000.0)
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0);

            let speed_label = if result.summary.avg_rtf > 0.0 {
                format!("{:.0}x real-time", 1.0 / result.summary.avg_rtf)
            } else {
                "—".to_string()
            };

            ui.label(format!(
                "Avg RTF: {:.3}x ({})",
                result.summary.avg_rtf, speed_label
            ));
            ui.label(format!(
                "Avg Time: {:.0} ms",
                result.summary.avg_processing_ms
            ));
            ui.label(format!(
                "Min/Max RTF: {:.3}/{:.3}",
                result.summary.min_rtf, result.summary.max_rtf
            ));
            ui.label(format!("WPS: {:.1} words/sec", wps));
            ui.label(format!("Iterations: {}", result.summary.iterations));

            if let Some(first) = result.metrics.first() {
                ui.label(format!(
                    "Audio: {:.1}s | Words: {}",
                    first.audio_duration_ms / 1000.0,
                    first.word_count
                ));
                ui.add_space(5.0);
                ui.label("Transcription:");
                let preview = if first.transcription.len() > 200 {
                    format!("{}...", &first.transcription[..200])
                } else {
                    first.transcription.clone()
                };
                ui.label(egui::RichText::new(preview).small().italics());
            }
        }
    }

    fn pick_audio_file(&mut self) {
        let file = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "mp3", "flac", "m4a", "ogg"])
            .pick_file();

        let Some(path) = file else { return };

        // Get duration
        match get_audio_duration_ms(&path) {
            Ok(duration) => {
                self.audio_duration_ms = Some(duration);
                self.audio_file_path = Some(path);
                self.error = None;
            }
            Err(e) => {
                self.error = Some(format!("Failed to read audio: {}", e));
            }
        }
    }

    fn start_audio_benchmark(&mut self) {
        let Some(audio_path) = self.audio_file_path.clone() else {
            return;
        };
        let Some(model) = self.whisper_model else {
            return;
        };

        info!("Starting audio benchmark: {:?}", audio_path);

        self.running = true;
        self.error = None;
        self.audio_result = None;
        self.live_output.clear();
        self.progress = "Loading model...".to_string();

        // Show config in live output
        let model_path = self.whisper_service.model_path(model);
        self.live_output.push_str(&format!(
            "Audio Benchmark\n\
             ===============\n\
             Model: {} (~{}MB)\n\
             Path: {}\n\
             Audio: {}\n\
             Iterations: {}\n\
             Warmup: {}\n\n",
            model.label(),
            model.size_mb(),
            model_path.display(),
            audio_path.display(),
            self.iterations,
            self.warmup,
        ));

        // Create channel for async communication
        let (tx, rx) = std::sync::mpsc::channel();
        self.audio_rx = Some(rx);
        let iterations = self.iterations;
        let warmup = self.warmup;

        // Spawn background thread with stderr capture
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};

            // Create pipe to capture stderr
            let (stderr_read, stderr_write) = match os_pipe::pipe() {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(AudioBenchmarkEvent::Error(format!("Pipe error: {}", e)));
                    return;
                }
            };

            // Redirect stderr to our pipe
            let old_stderr = unsafe { libc::dup(2) };
            if old_stderr == -1 {
                let _ = tx.send(AudioBenchmarkEvent::Error("Failed to dup stderr".into()));
                return;
            }
            let dup2_result = unsafe {
                use std::os::fd::AsRawFd;
                libc::dup2(stderr_write.as_raw_fd(), 2)
            };
            if dup2_result == -1 {
                unsafe { libc::close(old_stderr) };
                let _ = tx.send(AudioBenchmarkEvent::Error(
                    "Failed to redirect stderr".into(),
                ));
                return;
            }
            drop(stderr_write); // Close write end in this thread

            // Spawn reader thread for stderr
            let tx_stderr = tx.clone();
            let reader_handle = std::thread::spawn(move || {
                let reader = BufReader::new(stderr_read);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    // Filter and send interesting lines
                    if line.contains("whisper_")
                        || line.contains("ggml_")
                        || line.contains("ROCm")
                        || line.contains("loading")
                        || line.contains("MB")
                        || line.contains("backend")
                        || line.starts_with("  Device")
                    {
                        let _ = tx_stderr.send(AudioBenchmarkEvent::Progress(line));
                    }
                }
            });

            let _ = tx.send(AudioBenchmarkEvent::Progress(
                "Loading model...".to_string(),
            ));

            let mut service = WhisperService::default();
            let result = service.run_benchmark(model, &audio_path, iterations, warmup, None);

            // Restore stderr
            unsafe {
                libc::dup2(old_stderr, 2);
                libc::close(old_stderr);
            }

            // Wait for reader to finish
            let _ = reader_handle.join();

            match result {
                Ok(metrics) => {
                    // Send iteration results
                    for (i, m) in metrics.iter().enumerate() {
                        let _ = tx.send(AudioBenchmarkEvent::IterationComplete {
                            iteration: (i + 1) as u32,
                            metrics: m.clone(),
                        });
                    }
                    let _ = tx.send(AudioBenchmarkEvent::Done { metrics });
                }
                Err(e) => {
                    let _ = tx.send(AudioBenchmarkEvent::Error(e.to_string()));
                }
            }
        });
    }

    #[cfg(feature = "audio-input")]
    fn start_capture_benchmark(&mut self) {
        use llamaburn_services::AudioInputService;

        let Some(device_id) = self.selected_device_id.clone() else {
            return;
        };
        let Some(model) = self.whisper_model else {
            return;
        };
        let duration = self.capture_duration_secs;

        info!(
            "Starting capture benchmark: device={}, duration={}s",
            device_id, duration
        );

        self.running = true;
        self.error = None;
        self.audio_result = None;
        self.live_output.clear();
        self.progress = "Recording...".to_string();

        // Show config in live output
        let model_path = self.whisper_service.model_path(model);
        self.live_output.push_str(&format!(
            "Capture Benchmark\n\
             =================\n\
             Model: {} (~{}MB)\n\
             Path: {}\n\
             Device: {}\n\
             Duration: {}s\n\
             Iterations: {}\n\n\
             Recording audio...\n",
            model.label(),
            model.size_mb(),
            model_path.display(),
            device_id,
            duration,
            self.iterations,
        ));

        let (tx, rx) = std::sync::mpsc::channel();
        self.audio_rx = Some(rx);
        let iterations = self.iterations;

        std::thread::spawn(move || {
            // Step 1: Capture audio
            let _ = tx.send(AudioBenchmarkEvent::Progress(
                "Recording audio...".to_string(),
            ));

            let samples = match AudioInputService::capture(&device_id, duration) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(AudioBenchmarkEvent::Error(format!("Capture error: {}", e)));
                    return;
                }
            };

            let _ = tx.send(AudioBenchmarkEvent::Progress(format!(
                "Captured {} samples ({:.1}s at 16kHz)",
                samples.len(),
                samples.len() as f64 / 16000.0
            )));

            // Step 2: Transcribe with benchmark iterations
            let _ = tx.send(AudioBenchmarkEvent::Progress(
                "Loading model...".to_string(),
            ));

            let service = WhisperService::default();
            let mut metrics_vec = Vec::new();

            for i in 0..iterations {
                let _ = tx.send(AudioBenchmarkEvent::Progress(format!(
                    "Iteration {} of {}...",
                    i + 1,
                    iterations
                )));

                match service.transcribe_samples(&samples) {
                    Ok((result, duration)) => {
                        let audio_duration_ms = (samples.len() as f64 / 16000.0) * 1000.0;
                        let processing_time_ms = duration.as_secs_f64() * 1000.0;
                        let real_time_factor = processing_time_ms / audio_duration_ms;
                        let word_count = result.text.split_whitespace().count() as u32;

                        let metrics = llamaburn_core::AudioBenchmarkMetrics {
                            real_time_factor,
                            processing_time_ms,
                            audio_duration_ms,
                            transcription: result.text.clone(),
                            word_count,
                        };

                        let _ = tx.send(AudioBenchmarkEvent::IterationComplete {
                            iteration: (i + 1) as u32,
                            metrics: metrics.clone(),
                        });

                        metrics_vec.push(metrics);
                    }
                    Err(e) => {
                        let _ = tx.send(AudioBenchmarkEvent::Error(format!(
                            "Transcription error: {}",
                            e
                        )));
                        return;
                    }
                }
            }

            let _ = tx.send(AudioBenchmarkEvent::Done {
                metrics: metrics_vec,
            });
        });
    }

    #[cfg(feature = "audio-input")]
    fn start_live_transcription(&mut self) {
        use llamaburn_services::AudioInputService;

        let Some(device_id) = self.selected_device_id.clone() else {
            self.error = Some("No audio device selected".to_string());
            return;
        };
        let Some(model) = self.whisper_model else {
            self.error = Some("No Whisper model selected".to_string());
            return;
        };

        info!("Starting live transcription: device={}", device_id);

        // Reset state
        self.live_recording = true;
        self.running = true;
        self.error = None;
        self.waveform_peaks.clear();
        self.transcription_segments.clear();
        self.recording_start = Some(std::time::Instant::now());
        self.live_output.clear();
        self.progress = "Recording...".to_string();

        // Create channels
        let (audio_tx, audio_rx) = std::sync::mpsc::channel::<Vec<f32>>();
        let (event_tx, event_rx) = std::sync::mpsc::channel::<LiveTranscriptionEvent>();
        self.live_transcription_rx = Some(event_rx);

        // Start audio stream
        let stream_handle = match AudioInputService::start_stream(&device_id, audio_tx) {
            Ok(h) => h,
            Err(e) => {
                self.error = Some(format!("Failed to start audio stream: {}", e));
                self.live_recording = false;
                self.running = false;
                return;
            }
        };
        self.live_stream_handle = Some(stream_handle);

        // Spawn processing thread with effect chain
        let event_tx_clone = event_tx.clone();
        let effect_chain = self.effect_chain.clone();
        std::thread::spawn(move || {
            let mut service = WhisperService::default();

            // Load the model
            if let Err(e) = service.load_model(model) {
                let _ = event_tx_clone.send(LiveTranscriptionEvent::Error(format!("Failed to load model: {}", e)));
                return;
            }

            let mut accumulated_samples: Vec<f32> = Vec::new();
            let mut chunk_start_ms: u64 = 0;
            let max_chunk_samples = 16000 * 5; // 5 seconds max at 16kHz
            let min_chunk_samples = 16000 * 1; // 1 second min for VAD trigger

            // VAD parameters
            let silence_threshold = 0.01_f32; // RMS threshold for silence
            let silence_duration_samples = 16000 / 2; // 500ms of silence to trigger
            let mut consecutive_silence_samples = 0_usize;

            loop {
                // Receive audio chunk (with timeout to check for stop)
                let mut samples = match audio_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(s) => s,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                };

                // Apply effects chain to audio before processing
                if let Ok(mut chain) = effect_chain.lock() {
                    chain.process(&mut samples);
                }

                // Compute peaks for waveform display (downsample to ~100 peaks per chunk)
                let peaks = Self::compute_waveform_peaks(&samples, 100);
                let _ = event_tx_clone.send(LiveTranscriptionEvent::AudioPeaks(peaks));

                // Calculate RMS for VAD
                let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();

                // Track silence duration
                if rms < silence_threshold {
                    consecutive_silence_samples += samples.len();
                } else {
                    consecutive_silence_samples = 0;
                }

                // Accumulate samples
                accumulated_samples.extend(samples);

                // Determine if we should process now:
                // 1. Max chunk size reached (5 seconds), OR
                // 2. VAD triggered: silence detected for 500ms AND we have at least 1 second of audio
                let max_reached = accumulated_samples.len() >= max_chunk_samples;
                let vad_triggered = consecutive_silence_samples >= silence_duration_samples
                    && accumulated_samples.len() >= min_chunk_samples;

                if !max_reached && !vad_triggered {
                    continue;
                }

                // Take all accumulated samples (up to max)
                let chunk_samples = accumulated_samples.len().min(max_chunk_samples);
                let chunk: Vec<f32> = accumulated_samples.drain(..chunk_samples).collect();
                let chunk_duration_ms = (chunk.len() as u64 * 1000) / 16000;
                let chunk_end_ms = chunk_start_ms + chunk_duration_ms;

                // Reset silence counter after processing
                consecutive_silence_samples = 0;

                // Set up streaming output channel
                let (stream_tx, stream_rx) = std::sync::mpsc::channel::<String>();
                let event_tx_stream = event_tx_clone.clone();

                // Spawn thread to forward streaming output
                std::thread::spawn(move || {
                    while let Ok(line) = stream_rx.recv() {
                        let _ = event_tx_stream.send(LiveTranscriptionEvent::StreamOutput(line));
                    }
                });

                // Transcribe chunk with streaming output
                let chunk_duration_secs = chunk_duration_ms as f64 / 1000.0;
                match service.transcribe_samples_streaming(&chunk, stream_tx) {
                    Ok((result, duration)) => {
                        let rtf = duration.as_secs_f64() / chunk_duration_secs.max(0.001);
                        let segment = TranscriptionSegment {
                            start_ms: chunk_start_ms,
                            end_ms: chunk_end_ms,
                            text: result.text,
                            rtf,
                        };
                        let _ = event_tx_clone.send(LiveTranscriptionEvent::Transcription(segment));
                    }
                    Err(e) => {
                        let _ = event_tx_clone.send(LiveTranscriptionEvent::Error(e.to_string()));
                    }
                }

                chunk_start_ms = chunk_end_ms;
            }

            let _ = event_tx_clone.send(LiveTranscriptionEvent::Stopped);
        });
    }

    #[cfg(feature = "audio-input")]
    fn compute_waveform_peaks(samples: &[f32], num_peaks: usize) -> Vec<(f32, f32)> {
        let samples_per_peak = (samples.len() / num_peaks).max(1);
        samples
            .chunks(samples_per_peak)
            .map(|chunk| {
                let min = chunk.iter().cloned().fold(f32::INFINITY, f32::min);
                let max = chunk.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                (min, max)
            })
            .collect()
    }

    #[cfg(feature = "audio-input")]
    fn stop_live_transcription(&mut self) {
        if let Some(handle) = self.live_stream_handle.take() {
            handle.stop();
        }
        self.live_recording = false;
        self.running = false;
        self.recording_start = None;
        self.progress = "Stopped".to_string();
    }

    #[cfg(feature = "audio-input")]
    fn poll_live_transcription(&mut self) {
        let Some(rx) = &self.live_transcription_rx else {
            return;
        };

        while let Ok(event) = rx.try_recv() {
            match event {
                LiveTranscriptionEvent::AudioPeaks(peaks) => {
                    self.waveform_peaks.extend(peaks);
                    // Keep last ~10 seconds worth of peaks (assuming ~100 peaks per 5s chunk)
                    while self.waveform_peaks.len() > 400 {
                        self.waveform_peaks.pop_front();
                    }
                }
                LiveTranscriptionEvent::Transcription(segment) => {
                    self.transcription_segments.push(segment);
                }
                LiveTranscriptionEvent::StreamOutput(line) => {
                    self.live_output.push_str(&line);
                    self.live_output.push('\n');
                }
                LiveTranscriptionEvent::GpuMetrics(_metrics) => {
                    // TODO: Display GPU metrics
                }
                LiveTranscriptionEvent::Error(e) => {
                    self.error = Some(e);
                }
                LiveTranscriptionEvent::Stopped => {
                    self.live_recording = false;
                    self.running = false;
                }
            }
        }
    }

    #[cfg(feature = "audio-input")]
    fn format_time_ms(ms: u64) -> String {
        let secs = ms / 1000;
        let millis = ms % 1000;
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{:02}:{:02}.{}", mins, secs, millis / 100)
    }

    #[cfg(feature = "audio-input")]
    fn render_waveform_display(&mut self, ui: &mut egui::Ui) {
        // Header with recording indicator and duration
        ui.horizontal(|ui| {
            let (label_text, label_color) = match self.live_recording {
                true => ("🔴 Recording", Some(egui::Color32::RED)),
                false => ("Waveform", None),
            };

            match label_color {
                Some(color) => ui.colored_label(color, label_text),
                None => ui.label(label_text),
            };

            let Some(start) = self.recording_start else {
                return;
            };
            let elapsed = start.elapsed().as_secs_f64();
            ui.label(format!(
                "{:02}:{:02}.{}",
                (elapsed / 60.0) as u32,
                (elapsed % 60.0) as u32,
                ((elapsed * 10.0) % 10.0) as u32
            ));
        });

        // Waveform canvas
        let desired_size = egui::vec2(ui.available_width(), 80.0);
        let (response, painter) = ui.allocate_painter(desired_size, egui::Sense::hover());
        let rect = response.rect;

        // Background
        painter.rect_filled(rect, 4.0, egui::Color32::from_gray(30));

        // Center line
        let center_y = rect.center().y;
        painter.line_segment(
            [
                egui::pos2(rect.left(), center_y),
                egui::pos2(rect.right(), center_y),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
        );

        // Draw waveform peaks
        let num_peaks = self.waveform_peaks.len();
        let width = rect.width();
        let height = rect.height() / 2.0 - 4.0;

        for (i, (min, max)) in self.waveform_peaks.iter().enumerate() {
            let x = rect.left() + (i as f32 / num_peaks.max(1) as f32) * width;

            // Scale samples (-1.0 to 1.0) to pixel heights
            let min_y = center_y - min * height;
            let max_y = center_y - max * height;

            // Color based on amplitude (louder = brighter, red on clipping)
            let amplitude = (max - min).abs();
            let clipping = amplitude > 1.8;
            let intensity = (amplitude * 200.0).min(255.0) as u8;
            let color = if clipping {
                egui::Color32::from_rgb(255, 50, 50) // Red for clipping
            } else {
                egui::Color32::from_rgb(50, 150_u8.saturating_add(intensity / 2), 50_u8.saturating_add(intensity))
            };

            painter.line_segment(
                [egui::pos2(x, min_y), egui::pos2(x, max_y)],
                egui::Stroke::new(1.5, color),
            );
        }

        // Metrics one-liner below waveform
        if self.transcription_segments.is_empty() {
            return;
        }

        let segments = &self.transcription_segments;
        let avg_rtf: f64 = segments.iter().map(|s| s.rtf).sum::<f64>() / segments.len() as f64;
        let speed = 1.0 / avg_rtf;
        let total_audio_secs = segments.last().map(|s| s.end_ms).unwrap_or(0) as f64 / 1000.0;
        let total_words: usize = segments.iter().map(|s| s.text.split_whitespace().count()).sum();
        let wps = total_words as f64 / total_audio_secs.max(0.001);

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(format!("RTF: {:.3}x ({:.0}x real-time)", avg_rtf, speed));
            ui.separator();
            ui.label(format!("Audio: {:.1}s", total_audio_secs));
            ui.separator();
            ui.label(format!("Words: {} ({:.1} WPS)", total_words, wps));
            ui.separator();
            ui.label(format!("Segments: {}", segments.len()));
        });
    }

    fn render_model_downloads(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Whisper Models").strong());
        ui.add_space(5.0);

        let mut model_to_download: Option<WhisperModel> = None;

        egui::Grid::new("whisper_models_grid")
            .num_columns(4)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Model").small());
                ui.label(egui::RichText::new("Size").small());
                ui.label(egui::RichText::new("Status").small());
                ui.label("");
                ui.end_row();

                for model in WhisperModel::all() {
                    ui.label(model.label());
                    ui.label(format!("{}MB", model.size_mb()));

                    let downloaded = self.whisper_service.is_model_downloaded(*model);
                    if downloaded {
                        ui.colored_label(egui::Color32::GREEN, "Ready");
                        ui.label(""); // Empty cell
                    } else {
                        ui.colored_label(egui::Color32::GRAY, "—");
                        if ui.link("Download").clicked() {
                            model_to_download = Some(*model);
                        }
                    }
                    ui.end_row();
                }
            });

        if let Some(model) = model_to_download {
            self.download_whisper_model(model);
        }
    }

    fn download_whisper_model(&mut self, model: WhisperModel) {
        let url = model.download_url();
        let path = self.whisper_service.model_path(model);

        info!("Opening download URL: {}", url);
        self.live_output = format!(
            "Download {} from:\n{}\n\nSave to:\n{}",
            model.label(),
            url,
            path.display()
        );

        // Open URL in browser
        let _ = open::that(&url);
    }

    fn render_live_output_with_reserved(&self, ui: &mut egui::Ui, reserved_height: f32) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Live Output")
                    .heading()
                    .color(egui::Color32::GRAY),
            );
            if !self.progress.is_empty() {
                ui.separator();
                ui.label(&self.progress);
            }
        });

        ui.separator();

        // Use remaining vertical space minus reserved area for effects rack
        let available_height = ui.available_height() - 10.0 - reserved_height;
        egui::ScrollArea::vertical()
            .max_height(available_height.max(100.0))
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.live_output.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(20)
                        .interactive(false),
                );
            });
    }

    fn render_results(&self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("Results")
                .heading()
                .color(egui::Color32::GRAY),
        );

        if let Some(r) = &self.result {
            ui.label(format!("Avg TPS: {:.2} t/s", r.avg_tps));
            ui.label(format!("Avg TTFT: {:.2} ms", r.avg_ttft_ms));
            ui.label(format!("Avg Total: {:.2} ms", r.avg_total_ms));
            ui.label(format!("Min/Max TPS: {:.1}/{:.1}", r.min_tps, r.max_tps));
            ui.label(format!("Iterations: {}", r.iterations));
        }

        self.render_rankings(ui);
    }

    fn start_benchmark(&mut self) {
        info!("Starting benchmark for model: {}", self.selected_model);

        self.running = true;
        self.error = None;
        self.result = None;
        self.live_output.clear();
        self.collected_metrics.clear();
        self.progress = "Starting...".to_string();

        let config = BenchmarkConfig {
            benchmark_type: self.benchmark_type,
            model_id: self.selected_model.clone(),
            iterations: self.iterations,
            warmup_runs: self.warmup,
            prompt_set: "default".to_string(),
            temperature: self.temperature,
            max_tokens: Some(256),
            top_p: None,
            top_k: None,
        };

        self.current_config = Some(config.clone());

        let (rx, cancel_token) = self.benchmark_service.run_streaming(config);
        self.benchmark_rx = Some(rx);
        self.cancel_token = Some(cancel_token);
    }

    fn save_to_history(&mut self, summary: &BenchmarkSummary) {
        let Some(config) = self.current_config.take() else {
            warn!("No config available for history entry");
            return;
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let entry = BenchmarkHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            benchmark_type: config.benchmark_type,
            model_id: config.model_id.clone(),
            config,
            summary: summary.clone(),
            metrics: std::mem::take(&mut self.collected_metrics),
        };

        if let Err(e) = self.history_service.insert(&entry) {
            warn!("Failed to save benchmark history: {}", e);
        } else {
            info!("Saved benchmark result to history: {}", entry.id);
        }
    }

    fn cancel_benchmark(&mut self) {
        info!("Cancelling benchmark");
        if let Some(token) = &self.cancel_token {
            token.cancel();
        }
        self.progress = "Cancelling...".to_string();
    }

    fn unload_model(&mut self) {
        let model = self.selected_model.clone();
        if model.is_empty() {
            return;
        }

        info!("Unloading model: {}", model);

        // Fire and forget - unload in background
        let _ = self.ollama.unload_model_async(&model);

        // Clear selection and model info
        self.selected_model.clear();
        self.model_info = None;
        self.last_model_for_info.clear();
        self.last_model_for_rankings.clear();
        self.model_best_tps = None;
    }

    fn unload_whisper_model(&mut self) {
        let Some(model) = self.whisper_model else {
            return;
        };

        info!("Unloading whisper model: {}", model.label());

        self.whisper_service.unload_model();
        self.whisper_model = None;
        self.model_info = None;
        self.last_whisper_model_for_info = None;
        self.audio_model_info_rx = None;
    }

    #[cfg(feature = "audio-input")]
    fn render_audio_device_menu(&mut self, ui: &mut egui::Ui) {
        use llamaburn_services::DeviceType;
        use std::collections::BTreeMap;

        // Track device before menu to detect changes
        let device_before = self.selected_device_id.clone();

        // Recording Device submenu
        ui.menu_button("Recording Device", |ui| {
            if self.audio_devices.is_empty() {
                ui.label("No devices found");
                return;
            }

            // Group devices by card
            let mut groups: BTreeMap<String, Vec<&llamaburn_services::AudioDevice>> =
                BTreeMap::new();

            for device in &self.audio_devices {
                let group_key = device
                    .card_name
                    .clone()
                    .or_else(|| device.card_id.clone())
                    .unwrap_or_else(|| "System".to_string());
                groups.entry(group_key).or_default().push(device);
            }

            // Render grouped devices
            for (group_name, devices) in &groups {
                ui.label(egui::RichText::new(group_name).strong().size(12.0));
                ui.separator();

                for device in devices {
                    let selected = self.selected_device_id.as_ref() == Some(&device.id);
                    let prefix = ["  ", "• "][selected as usize];

                    // Friendly device type label
                    let type_suffix = match device.device_type {
                        DeviceType::PluginHardware => " (Recommended)",
                        DeviceType::Hardware => " (Direct)",
                        DeviceType::Default => " (Default)",
                        DeviceType::PulseAudio | DeviceType::Other => "",
                    };

                    let label = format!("{}{}{}", prefix, device.name, type_suffix);

                    if !ui.button(label).clicked() {
                        continue;
                    }
                    self.selected_device_id = Some(device.id.clone());
                    ui.close_menu();
                }

                ui.add_space(4.0);
            }
        });

        // Start VU meter if device changed
        if self.selected_device_id != device_before {
            self.start_level_monitor();
        }

        ui.separator();

        // Test Mic (Record & Play) button
        let test_label = match &self.audio_test_state {
            AudioTestState::Recording { start } => {
                let elapsed = start.elapsed().as_secs_f32();
                format!("🎙️ Recording... ({:.1}s)", 3.0 - elapsed)
            }
            AudioTestState::Playing { .. } => "🔊 Playing...".to_string(),
            AudioTestState::Monitoring => "🎧 Stop Monitor".to_string(),
            AudioTestState::Idle => "🎙️ Test Mic (Record & Play)".to_string(),
        };

        let can_test = self.selected_device_id.is_some()
            && matches!(self.audio_test_state, AudioTestState::Idle);

        if ui
            .add_enabled(can_test, egui::Button::new(&test_label))
            .clicked()
        {
            self.start_audio_test();
            ui.close_menu();
        }

        // Live Monitor toggle
        let is_monitoring = matches!(self.audio_test_state, AudioTestState::Monitoring);
        let monitor_label = if is_monitoring {
            "🎧 Live Monitor ✓"
        } else {
            "🎧 Live Monitor"
        };
        let can_monitor = self.selected_device_id.is_some()
            && matches!(
                self.audio_test_state,
                AudioTestState::Idle | AudioTestState::Monitoring
            );

        if ui
            .add_enabled(can_monitor, egui::Button::new(monitor_label))
            .clicked()
        {
            [Self::start_live_monitor, Self::stop_live_monitor][is_monitoring as usize](self);
            ui.close_menu();
        }

        ui.separator();

        // Effects Chain submenu
        self.render_effects_menu(ui);

        ui.separator();

        // Rescan Audio Devices
        if ui.button("Rescan Audio Devices").clicked() {
            self.refresh_audio_devices();
            ui.close_menu();
        }

        // Audio Settings dialog
        if ui.button("Audio Settings...").clicked() {
            self.show_audio_settings = true;
            ui.close_menu();
        }
    }

    #[cfg(feature = "audio-input")]
    fn refresh_audio_devices(&mut self) {
        use llamaburn_services::AudioInputService;

        self.loading_devices = true;

        let devices = match AudioInputService::list_devices() {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to list audio devices: {}", e);
                self.error = Some(format!("Audio device error: {}", e));
                self.loading_devices = false;
                return;
            }
        };

        info!("Found {} audio devices", devices.len());

        // Auto-select default device if none selected, start VU meter
        let had_device = self.selected_device_id.is_some();
        if !had_device {
            let default_device = devices.iter().find(|d| d.is_default);
            let fallback = devices.first();
            self.selected_device_id = default_device.or(fallback).map(|d| d.id.clone());
        }

        self.audio_devices = devices;
        self.loading_devices = false;

        // Auto-start VU meter when device is first selected
        if !had_device && self.selected_device_id.is_some() {
            self.start_level_monitor();
        }
    }

    #[cfg(feature = "audio-input")]
    fn start_audio_test(&mut self) {
        use llamaburn_services::{AudioCaptureConfig, AudioInputService};

        let Some(device_id) = self.selected_device_id.clone() else {
            return;
        };

        info!("Starting audio test: device={}", device_id);

        self.audio_test_state = AudioTestState::Recording {
            start: std::time::Instant::now(),
        };

        // Build config from user settings
        let config = AudioCaptureConfig {
            sample_rate: self.audio_sample_rate,
            sample_format: self.audio_sample_format.to_service_format(),
            channels: self.audio_channels,
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.audio_test_rx = Some(rx);

        // Spawn recording thread
        std::thread::spawn(move || {
            match AudioInputService::capture_with_config(&device_id, 3, &config) {
                Ok((samples, sample_rate, channels)) => {
                    let _ = tx.send(AudioTestEvent::RecordingComplete {
                        samples,
                        sample_rate,
                        channels,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AudioTestEvent::Error(e.to_string()));
                }
            }
        });
    }

    #[cfg(feature = "audio-input")]
    fn start_live_monitor(&mut self) {
        use llamaburn_services::{AudioInputService, AudioOutputService};

        let Some(device_id) = self.selected_device_id.clone() else {
            return;
        };

        info!("Starting live audio monitor: device={}", device_id);

        // Start raw audio input stream (no resampling, native format)
        let (audio_tx, audio_rx) = std::sync::mpsc::channel();
        let (stream_handle, sample_rate, channels) =
            match AudioInputService::start_stream_raw(&device_id, audio_tx) {
                Ok(result) => result,
                Err(e) => {
                    self.error = Some(format!("Failed to start audio stream: {}", e));
                    return;
                }
            };

        // Start output monitor with matching format, latency, and effects chain
        let latency = self.playback_latency_ms;
        let effect_chain = Some(self.effect_chain.clone());
        let monitor_handle = match AudioOutputService::start_monitor_with_effects(
            audio_rx,
            sample_rate,
            channels,
            latency,
            effect_chain,
        ) {
            Ok(h) => h,
            Err(e) => {
                stream_handle.stop();
                self.error = Some(format!("Failed to start monitor output: {}", e));
                return;
            }
        };

        // Store handles
        self.live_stream_handle = Some(stream_handle);
        self.monitor_handle = Some(monitor_handle);
        self.audio_test_state = AudioTestState::Monitoring;
    }

    #[cfg(feature = "audio-input")]
    fn stop_live_monitor(&mut self) {
        info!("Stopping live audio monitor");

        // Stop input stream
        if let Some(handle) = self.live_stream_handle.take() {
            handle.stop();
        }

        // Stop output monitor
        if let Some(handle) = self.monitor_handle.take() {
            handle.stop();
        }

        self.audio_test_state = AudioTestState::Idle;
    }

    #[cfg(feature = "audio-input")]
    fn poll_audio_test(&mut self) {
        use llamaburn_services::AudioOutputService;

        // Check recording completion
        let Some(rx) = &self.audio_test_rx else {
            return;
        };

        let event = match rx.try_recv() {
            Ok(e) => e,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Check if recording has timed out (3s)
                if let AudioTestState::Recording { start } = &self.audio_test_state {
                    if start.elapsed().as_secs() > 4 {
                        self.audio_test_state = AudioTestState::Idle;
                        self.audio_test_rx = None;
                    }
                }
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.audio_test_state = AudioTestState::Idle;
                self.audio_test_rx = None;
                return;
            }
        };

        match event {
            AudioTestEvent::RecordingComplete {
                samples,
                sample_rate,
                channels,
            } => {
                info!(
                    samples = samples.len(),
                    sample_rate, channels, "Recording complete, starting playback"
                );

                // Convert to mono for playback if needed (playback handles stereo expansion)
                let mono_samples = match channels {
                    1 => samples,
                    _ => samples
                        .chunks(channels as usize)
                        .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                        .collect(),
                };

                match AudioOutputService::play_samples(mono_samples, sample_rate) {
                    Ok(handle) => {
                        self.audio_test_state = AudioTestState::Playing {
                            handle: Some(handle),
                        };
                    }
                    Err(e) => {
                        self.error = Some(format!("Playback failed: {}", e));
                        self.audio_test_state = AudioTestState::Idle;
                        self.audio_test_rx = None;
                    }
                }
            }
            AudioTestEvent::Error(e) => {
                self.error = Some(format!("Audio test error: {}", e));
                self.audio_test_state = AudioTestState::Idle;
                self.audio_test_rx = None;
            }
        }
    }

    #[cfg(feature = "audio-input")]
    fn check_playback_completion(&mut self) {
        let AudioTestState::Playing { handle } = &mut self.audio_test_state else {
            return;
        };

        let Some(h) = handle else {
            self.audio_test_state = AudioTestState::Idle;
            return;
        };

        if !h.is_done() {
            return;
        }

        info!("Playback complete");
        self.audio_test_state = AudioTestState::Idle;
        self.audio_test_rx = None;
    }

    #[cfg(feature = "audio-input")]
    fn start_level_monitor(&mut self) {
        use llamaburn_services::AudioInputService;

        // Stop existing monitor if any
        self.stop_level_monitor();

        let Some(device_id) = self.selected_device_id.clone() else {
            return;
        };

        info!("Starting input level monitor: device={}", device_id);

        let (audio_tx, audio_rx) = std::sync::mpsc::channel::<Vec<f32>>();
        let (level_tx, level_rx) = std::sync::mpsc::channel::<(f32, f32)>();

        // Start audio stream
        let stream_handle = match AudioInputService::start_stream(&device_id, audio_tx) {
            Ok(h) => h,
            Err(e) => {
                warn!("Failed to start level monitor: {}", e);
                return;
            }
        };

        self.level_monitor_handle = Some(stream_handle);
        self.level_monitor_rx = Some(level_rx);

        // Spawn thread to calculate levels
        std::thread::spawn(move || {
            loop {
                let samples = match audio_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(s) => s,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                };

                // Calculate peak levels (assuming stereo interleaved, but our stream is mono 16kHz)
                // Since AudioInputService converts to mono, we'll just use the same value for L/R
                let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);

                if level_tx.send((peak, peak)).is_err() {
                    break;
                }
            }
        });
    }

    #[cfg(feature = "audio-input")]
    fn stop_level_monitor(&mut self) {
        if let Some(handle) = self.level_monitor_handle.take() {
            handle.stop();
        }
        self.level_monitor_rx = None;
        self.input_levels = (0.0, 0.0);
    }

    #[cfg(feature = "audio-input")]
    fn poll_level_monitor(&mut self) {
        let Some(rx) = &self.level_monitor_rx else {
            // Apply decay when no monitor running
            self.input_levels.0 *= 0.85;
            self.input_levels.1 *= 0.85;
            return;
        };

        // Get latest levels (drain channel to get most recent)
        let mut latest: Option<(f32, f32)> = None;
        while let Ok(levels) = rx.try_recv() {
            latest = Some(levels);
        }

        let Some((l, r)) = latest else {
            // Decay when no new data
            self.input_levels.0 *= 0.9;
            self.input_levels.1 *= 0.9;
            return;
        };

        // Peak hold behavior - take max of current and new
        self.input_levels.0 = self.input_levels.0.max(l);
        self.input_levels.1 = self.input_levels.1.max(r);
    }

    #[cfg(feature = "audio-input")]
    fn render_level_meter(&self, ui: &mut egui::Ui) {
        let (left, right) = self.input_levels;

        // Convert to dB (-60 to 0 range)
        let to_db = |level: f32| -> f32 {
            if level < 0.001 {
                -60.0
            } else {
                20.0 * level.log10()
            }
        };

        let left_db = to_db(left);
        let right_db = to_db(right);

        // Normalize to 0.0-1.0 for display (-60dB = 0.0, 0dB = 1.0)
        let db_to_normalized = |db: f32| -> f32 { ((db + 60.0) / 60.0).clamp(0.0, 1.0) };

        let left_norm = db_to_normalized(left_db);
        let right_norm = db_to_normalized(right_db);

        let bar_height = 8.0;
        let bar_width = ui.available_width().min(200.0);

        // Helper to draw a single meter bar
        let draw_meter = |ui: &mut egui::Ui, level: f32, label: &str| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).small().monospace());

                let (response, painter) =
                    ui.allocate_painter(egui::vec2(bar_width, bar_height), egui::Sense::hover());
                let rect = response.rect;

                // Background
                painter.rect_filled(rect, 2.0, egui::Color32::from_gray(40));

                // Level bar with gradient colors
                let level_width = rect.width() * level;
                let level_rect =
                    egui::Rect::from_min_size(rect.min, egui::vec2(level_width, rect.height()));

                // Color thresholds: (max_level, color)
                // Green < -12dB, Yellow -12 to -6dB, Orange -6 to -3dB, Red > -3dB
                let color_zones: [(f32, egui::Color32); 4] = [
                    (0.50, egui::Color32::from_rgb(50, 205, 50)), // Green: below -12dB
                    (0.80, egui::Color32::from_rgb(255, 200, 0)), // Yellow: -12dB to -6dB
                    (0.95, egui::Color32::from_rgb(255, 140, 0)), // Orange: -6dB to -3dB
                    (1.00, egui::Color32::from_rgb(255, 50, 50)), // Red: above -3dB
                ];
                let color = color_zones
                    .iter()
                    .find(|(threshold, _)| level < *threshold)
                    .map(|(_, c)| *c)
                    .unwrap_or(color_zones[3].1);

                painter.rect_filled(level_rect, 2.0, color);

                // dB markers
                let marker_positions = [(0.0, "-∞"), (0.5, "-12"), (0.8, "-6"), (1.0, "0")];
                for (pos, _label) in marker_positions {
                    let x = rect.left() + rect.width() * pos;
                    painter.line_segment(
                        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                        egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
                    );
                }
            });
        };

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            draw_meter(ui, left_norm, "L");
            draw_meter(ui, right_norm, "R");

            // dB scale labels
            ui.horizontal(|ui| {
                ui.add_space(12.0); // Offset for "L"/"R" label
                ui.label(egui::RichText::new("-∞").small().weak());
                ui.add_space(bar_width * 0.45);
                ui.label(egui::RichText::new("-12").small().weak());
                ui.add_space(bar_width * 0.25);
                ui.label(egui::RichText::new("-6").small().weak());
                ui.add_space(bar_width * 0.1);
                ui.label(egui::RichText::new("0").small().weak());
            });
        });
    }

    #[cfg(feature = "audio-input")]
    fn render_audio_settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_audio_settings {
            return;
        }

        let mut open = true;
        egui::Window::new("Audio Settings")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                self.render_audio_settings_content(ui);
            });

        // Preserve false if OK/Cancel was clicked, or set false if X was clicked
        self.show_audio_settings = self.show_audio_settings && open;
    }

    #[cfg(feature = "audio-input")]
    fn render_audio_settings_content(&mut self, ui: &mut egui::Ui) {
        // Interface section
        ui.heading("Interface");
        ui.add_space(4.0);
        egui::Grid::new("interface_grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Host:");
                ui.label("ALSA (via cpal)");
                ui.end_row();
            });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        // Playback section
        ui.heading("Playback");
        ui.add_space(4.0);
        egui::Grid::new("playback_grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Device:");
                let playback_label = self.playback_device_id.as_deref().unwrap_or("default");
                egui::ComboBox::from_id_salt("playback_device")
                    .selected_text(playback_label)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.playback_device_id.is_none(), "default")
                            .clicked()
                        {
                            self.playback_device_id = None;
                        }
                    });
                ui.end_row();
            });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        // Recording section
        ui.heading("Recording");
        ui.add_space(4.0);
        self.render_recording_settings(ui);

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        // Quality section
        ui.heading("Quality");
        ui.add_space(4.0);
        self.render_quality_settings(ui);

        ui.add_space(16.0);

        // OK / Cancel buttons
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("OK").clicked() {
                    self.show_audio_settings = false;
                    self.apply_audio_settings();
                }
                if ui.button("Cancel").clicked() {
                    self.show_audio_settings = false;
                }
            });
        });
    }

    #[cfg(feature = "audio-input")]
    fn apply_audio_settings(&mut self) {
        // Restart live monitor if running to apply new latency
        let monitor_running = matches!(self.audio_test_state, AudioTestState::Monitoring);
        if !monitor_running {
            return;
        }

        info!("Applying audio settings - restarting live monitor");
        self.stop_live_monitor();
        self.start_live_monitor();
    }

    #[cfg(feature = "audio-input")]
    fn render_recording_settings(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("recording_grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                // Device selector
                ui.label("Device:");
                let rec_label = self.selected_device_id.as_deref().unwrap_or("default");
                egui::ComboBox::from_id_salt("recording_device_settings")
                    .selected_text(rec_label)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for device in &self.audio_devices {
                            let selected = self.selected_device_id.as_ref() == Some(&device.id);
                            if ui.selectable_label(selected, &device.name).clicked() {
                                self.selected_device_id = Some(device.id.clone());
                            }
                        }
                    });
                ui.end_row();

                // Channels selector
                ui.label("Channels:");
                let ch_label = CHANNEL_OPTIONS
                    .iter()
                    .find(|(ch, _)| *ch == self.audio_channels)
                    .map(|(_, l)| *l)
                    .unwrap_or("2 (Stereo)");
                egui::ComboBox::from_id_salt("recording_channels")
                    .selected_text(ch_label)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for &(ch, label) in CHANNEL_OPTIONS {
                            if ui
                                .selectable_label(self.audio_channels == ch, label)
                                .clicked()
                            {
                                self.audio_channels = ch;
                            }
                        }
                    });
                ui.end_row();
            });
    }

    #[cfg(feature = "audio-input")]
    fn render_quality_settings(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("quality_grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                // Sample rate
                ui.label("Sample Rate:");
                let rate_label = format!("{} Hz", self.audio_sample_rate);
                egui::ComboBox::from_id_salt("sample_rate")
                    .selected_text(rate_label)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for &rate in SAMPLE_RATES {
                            let label = format!("{} Hz", rate);
                            if ui
                                .selectable_label(self.audio_sample_rate == rate, label)
                                .clicked()
                            {
                                self.audio_sample_rate = rate;
                            }
                        }
                    });
                ui.end_row();

                // Sample format
                ui.label("Sample Format:");
                egui::ComboBox::from_id_salt("sample_format")
                    .selected_text(self.audio_sample_format.label())
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for &fmt in AudioSampleFormat::all() {
                            if ui
                                .selectable_label(self.audio_sample_format == fmt, fmt.label())
                                .clicked()
                            {
                                self.audio_sample_format = fmt;
                            }
                        }
                    });
                ui.end_row();
            });
    }

    #[cfg(feature = "audio-input")]
    fn render_effects_menu(&mut self, ui: &mut egui::Ui) {
        let effect_count = self.effect_chain.lock().map(|c| c.len()).unwrap_or(0);

        let label = match effect_count {
            0 => "🎛️ Effects Chain".to_string(),
            n => format!("🎛️ Effects Chain ({})", n),
        };

        ui.menu_button(label, |ui| {
            self.render_add_effect_menu(ui);
            ui.separator();
            self.render_effect_list(ui);
        });
    }

    #[cfg(feature = "audio-input")]
    fn render_add_effect_menu(&self, ui: &mut egui::Ui) {
        use llamaburn_services::audio_effects::{
            CompressorEffect, DelayEffect, GainEffect, HighPassEffect, LowPassEffect, ReverbEffect,
        };

        ui.menu_button("➕ Add Effect", |ui| {
            let effects: Vec<(&str, Box<dyn FnOnce() -> Box<dyn llamaburn_services::audio_effects::AudioEffect>>)> = vec![
                ("Gain", Box::new(|| Box::new(GainEffect::new(0.0)))),
                ("High Pass Filter", Box::new(|| Box::new(HighPassEffect::new(80.0, 44100.0)))),
                ("Low Pass Filter", Box::new(|| Box::new(LowPassEffect::new(12000.0, 44100.0)))),
                ("Compressor", Box::new(|| Box::new(CompressorEffect::new(-20.0, 10.0, 100.0)))),
                ("Delay", Box::new(|| Box::new(DelayEffect::new(250.0, 0.4, 0.3, 44100.0)))),
                ("Reverb", Box::new(|| Box::new(ReverbEffect::new(0.5, 0.5, 0.3, 44100.0)))),
            ];

            for (name, create_effect) in effects {
                if !ui.button(name).clicked() {
                    continue;
                }
                let Ok(mut c) = self.effect_chain.lock() else {
                    continue;
                };
                c.add(create_effect());
                ui.close_menu();
            }
        });
    }

    #[cfg(feature = "audio-input")]
    fn render_effect_list(&self, ui: &mut egui::Ui) {
        let Ok(mut chain) = self.effect_chain.lock() else {
            return;
        };

        let mut to_remove: Option<usize> = None;

        for (i, effect) in chain.effects().iter().enumerate() {
            ui.horizontal(|ui| {
                let suffix = ["", " [OFF]"][effect.is_bypassed() as usize];
                ui.label(format!("{}. {}{}", i + 1, effect.name(), suffix));
                if ui.small_button("❌").clicked() {
                    to_remove = Some(i);
                }
            });
        }

        if let Some(idx) = to_remove {
            chain.remove(idx);
        }

        if chain.is_empty() {
            ui.label("No effects added");
        }

        ui.separator();

        // Bypass all toggle
        let bypass_all = chain.is_bypass_all();
        let bypass_label = ["🔇 Bypass All", "🔇 Bypass All ✓"][bypass_all as usize];
        if ui.button(bypass_label).clicked() {
            chain.set_bypass_all(!bypass_all);
        }

        // Clear all
        if chain.is_empty() {
            return;
        }
        if !ui.button("🗑️ Clear All").clicked() {
            return;
        }
        chain.clear();
        ui.close_menu();
    }

    /// Ableton-style horizontal effects rack panel at bottom of UI
    #[cfg(feature = "audio-input")]
    fn render_effects_rack(&mut self, ui: &mut egui::Ui) {
        use llamaburn_services::audio_effects::{
            CompressorEffect, DelayEffect, GainEffect, HighPassEffect, LowPassEffect, ReverbEffect,
        };

        let header = egui::CollapsingHeader::new(
            egui::RichText::new("🎛️ Effects Rack").strong(),
        )
        .default_open(self.effects_rack_expanded)
        .show(ui, |ui| {
            self.effects_rack_expanded = true;

            // Header row with controls and Add button
            ui.horizontal(|ui| {
                let Ok(mut chain) = self.effect_chain.lock() else {
                    return;
                };

                let bypass_all = chain.is_bypass_all();
                let label = ["🔊 Active", "🔇 Bypassed"][bypass_all as usize];
                if ui.selectable_label(bypass_all, label).clicked() {
                    chain.set_bypass_all(!bypass_all);
                }

                ui.separator();

                // Add effect menu in header
                ui.menu_button("➕ Add", |ui| {
                    let effects: Vec<(&str, Box<dyn FnOnce() -> Box<dyn llamaburn_services::audio_effects::AudioEffect>>)> = vec![
                        ("Gain", Box::new(|| Box::new(GainEffect::new(0.0)))),
                        ("High Pass", Box::new(|| Box::new(HighPassEffect::new(80.0, 44100.0)))),
                        ("Low Pass", Box::new(|| Box::new(LowPassEffect::new(12000.0, 44100.0)))),
                        ("Compressor", Box::new(|| Box::new(CompressorEffect::new(-20.0, 10.0, 100.0)))),
                        ("Delay", Box::new(|| Box::new(DelayEffect::new(250.0, 0.4, 0.3, 44100.0)))),
                        ("Reverb", Box::new(|| Box::new(ReverbEffect::new(0.5, 0.5, 0.3, 44100.0)))),
                    ];

                    for (name, create_effect) in effects {
                        if !ui.button(name).clicked() { continue; }
                        chain.add(create_effect());
                        ui.close_menu();
                    }
                });

                if chain.effects().is_empty() { return; }

                ui.separator();

                if ui.small_button("🗑️ Clear All").clicked() {
                    chain.clear();
                }
            });

            let Ok(mut chain) = self.effect_chain.lock() else {
                return;
            };

            if chain.effects().is_empty() {
                ui.label("No effects - click Add to insert effects");
                return;
            }

            ui.add_space(5.0);

            // Fixed height panel with horizontal effect cards
            let panel_height = 160.0;
            let card_width = 150.0;

            ui.horizontal_top(|ui| {
                let mut to_remove: Option<usize> = None;

                for (i, effect) in chain.effects_mut().iter_mut().enumerate() {
                    egui::Frame::group(ui.style())
                        .fill(ui.style().visuals.extreme_bg_color)
                        .inner_margin(egui::Margin::same(6.0))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.set_min_size(egui::vec2(card_width, panel_height));
                                ui.set_max_width(card_width);

                                // Header row: bypass + name + remove
                                ui.horizontal(|ui| {
                                    let bypassed = effect.is_bypassed();
                                    let bypass_label = ["▶", "⏸"][bypassed as usize];
                                    if ui.small_button(bypass_label).clicked() {
                                        effect.set_bypass(!bypassed);
                                    }

                                    let colors = [egui::Color32::LIGHT_GREEN, egui::Color32::GRAY];
                                    ui.colored_label(colors[bypassed as usize], egui::RichText::new(effect.name()).strong());

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("✕").clicked() {
                                            to_remove = Some(i);
                                        }
                                    });
                                });

                                ui.separator();

                                // Parameters - each param gets label row + slider row
                                for param in effect.get_params() {
                                    let mut value = param.value;
                                    ui.label(format!("{}: {:.2}", param.name, value));
                                    let slider = egui::Slider::new(&mut value, param.min..=param.max)
                                        .show_value(false);
                                    if ui.add_sized([card_width - 14.0, 16.0], slider).changed() {
                                        effect.set_param(&param.name, value);
                                    }
                                }
                            });
                        });

                    ui.add_space(4.0);
                }

                drop(chain);

                if let Some(idx) = to_remove {
                    let _ = self.effect_chain.lock().map(|mut c| c.remove(idx));
                }
            });
        });

        // Track collapsed state
        if !header.fully_open() {
            self.effects_rack_expanded = false;
        }
    }

    fn refresh_rankings(&mut self) {
        if self.selected_model.is_empty() {
            return;
        }

        if self.selected_model == self.last_model_for_rankings {
            return;
        }

        self.last_model_for_rankings = self.selected_model.clone();

        // Get best TPS for selected model
        self.model_best_tps = self
            .history_service
            .get_best_for_model(&self.selected_model, self.benchmark_type)
            .ok()
            .flatten();

        // Get all-time best
        self.all_time_best = self
            .history_service
            .get_all_time_best(self.benchmark_type)
            .ok()
            .flatten();

        // Get leaderboard
        self.leaderboard = self
            .history_service
            .get_leaderboard(self.benchmark_type, 5)
            .unwrap_or_default();
    }

    fn force_refresh_rankings(&mut self) {
        self.last_model_for_rankings.clear();
        self.refresh_rankings();
    }

    fn save_audio_to_history(&mut self, result: &AudioBenchmarkResult) {
        let Some(model) = self.whisper_model else {
            return;
        };

        let model_id = format!("whisper-{}", model.label().to_lowercase());

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let entry = AudioHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            benchmark_type: BenchmarkType::Audio,
            audio_mode: AudioMode::Stt,
            model_id,
            config: result.config.clone(),
            summary: result.summary.clone(),
            metrics: result.metrics.clone(),
        };

        if let Err(e) = self.history_service.insert_audio(&entry) {
            warn!("Failed to save audio benchmark history: {}", e);
        } else {
            info!("Saved audio benchmark result to history: {}", entry.id);
        }
    }

    fn refresh_audio_rankings(&mut self) {
        if self.benchmark_type != BenchmarkType::Audio {
            return;
        }

        let Some(model) = self.whisper_model else {
            return;
        };

        if self.last_whisper_model_for_rankings == Some(model) {
            return;
        }

        self.last_whisper_model_for_rankings = Some(model);

        let model_id = format!("whisper-{}", model.label().to_lowercase());

        // Get best RTF for this model
        self.model_best_rtf = self
            .history_service
            .get_best_audio_for_model(&model_id, AudioMode::Stt)
            .ok()
            .flatten();

        // Get all-time best
        self.all_time_best_audio = self
            .history_service
            .get_all_time_best_audio(AudioMode::Stt)
            .ok()
            .flatten();

        // Get leaderboard
        self.audio_leaderboard = self
            .history_service
            .get_audio_leaderboard(AudioMode::Stt, 5)
            .unwrap_or_default();
    }

    fn force_refresh_audio_rankings(&mut self) {
        self.last_whisper_model_for_rankings = None;
        self.refresh_audio_rankings();
    }

    fn refresh_model_info(&mut self) {
        if self.selected_model.is_empty() {
            return;
        }

        if self.selected_model == self.last_model_for_info {
            return;
        }

        self.last_model_for_info = self.selected_model.clone();
        self.model_info = None;
        self.model_info_rx = Some(
            self.model_info_service
                .fetch_info_async(&self.selected_model),
        );
    }

    fn poll_model_info(&mut self) {
        let Some(rx) = &self.model_info_rx else {
            return;
        };

        let Ok(info) = rx.try_recv() else {
            return;
        };

        self.model_info = info;
        self.model_info_rx = None;
    }

    fn refresh_audio_model_info(&mut self) {
        if self.benchmark_type != BenchmarkType::Audio {
            return;
        }

        let Some(model) = self.whisper_model else {
            return;
        };

        if self.last_whisper_model_for_info == Some(model) {
            return;
        }

        self.last_whisper_model_for_info = Some(model);
        self.model_info = None;
        self.audio_model_info_rx = Some(ModelInfoService::fetch_whisper_info_async(model));
    }

    fn poll_audio_model_info(&mut self) {
        let Some(rx) = &self.audio_model_info_rx else {
            return;
        };

        let Ok(info) = rx.try_recv() else { return };

        self.model_info = info;
        self.audio_model_info_rx = None;
    }

    fn render_model_info(&self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("Model Info")
                .heading()
                .color(egui::Color32::GRAY),
        );
        ui.add_space(5.0);

        let Some(info) = &self.model_info else {
            ui.label("Select a model to view details");
            return;
        };

        // Ollama metadata - only show for Text mode
        if self.benchmark_type == BenchmarkType::Text {
            ui.label(egui::RichText::new("Ollama").strong());
            if let Some(size) = &info.parameter_size {
                ui.label(format!("Size: {}", size));
            }
            if let Some(quant) = &info.quantization {
                ui.label(format!("Quant: {}", quant));
            }
            if let Some(family) = &info.family {
                ui.label(format!("Family: {}", family));
            }
            if let Some(format) = &info.format {
                ui.label(format!("Format: {}", format));
            }
            ui.add_space(10.0);
        }

        // HuggingFace metadata
        let has_hf = info.hf_repo.is_some();
        if !has_hf {
            return;
        }

        ui.label(egui::RichText::new("HuggingFace").strong());

        if let Some(author) = &info.hf_author {
            ui.label(format!("Author: {}", author));
        }
        if let Some(license) = &info.hf_license {
            ui.label(format!("License: {}", license));
        }
        if let Some(downloads) = info.hf_downloads {
            ui.label(format!("Downloads: {}", format_number(downloads)));
        }
        if let Some(likes) = info.hf_likes {
            ui.label(format!("Likes: {}", format_number(likes)));
        }
        if let Some(pipeline) = &info.hf_pipeline {
            ui.label(format!("Pipeline: {}", pipeline));
        }
        if let Some(gated) = &info.hf_gated {
            ui.label(format!("Gated: {}", gated));
        }
        if let Some(modified) = &info.hf_last_modified {
            ui.label(format!("Updated: {}", modified));
        }

        // Clickable repo link
        if let Some(url) = info.hf_url() {
            ui.add_space(5.0);
            if ui.link("View on HuggingFace").clicked() {
                let _ = open::that(&url);
            }
        }
    }

    fn render_rankings(&self, ui: &mut egui::Ui) {
        ui.add_space(15.0);
        ui.label(
            egui::RichText::new("Rankings")
                .heading()
                .color(egui::Color32::GRAY),
        );

        match self.benchmark_type {
            BenchmarkType::Text => self.render_text_rankings(ui),
            BenchmarkType::Audio => self.render_audio_rankings(ui),
            _ => {}
        }
    }

    fn render_text_rankings(&self, ui: &mut egui::Ui) {
        let best = self
            .model_best_tps
            .map(|t| format!("{:.1} TPS", t))
            .unwrap_or_else(|| "—".to_string());
        ui.label(format!("Model Best: {}", best));

        let all_time = self
            .all_time_best
            .as_ref()
            .map(|(m, t)| format!("{:.1} ({m})", t))
            .unwrap_or_else(|| "—".to_string());
        ui.label(format!("All-Time: {}", all_time));

        if self.leaderboard.is_empty() {
            return;
        }

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("Leaderboard")
                .small()
                .color(egui::Color32::GRAY),
        );

        for (i, (model, tps)) in self.leaderboard.iter().enumerate() {
            ui.label(format!("{}. {} ({:.1})", i + 1, model, tps));
        }
    }

    fn render_audio_rankings(&self, ui: &mut egui::Ui) {
        let best = self
            .model_best_rtf
            .map(|r| format!("{:.3}x RTF", r))
            .unwrap_or_else(|| "—".to_string());
        ui.label(format!("Model Best: {}", best));

        let all_time = self
            .all_time_best_audio
            .as_ref()
            .map(|(m, r)| format!("{:.3}x ({m})", r))
            .unwrap_or_else(|| "—".to_string());
        ui.label(format!("All-Time: {}", all_time));

        if self.audio_leaderboard.is_empty() {
            return;
        }

        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("Leaderboard")
                .small()
                .color(egui::Color32::GRAY),
        );

        for (i, (model, rtf)) in self.audio_leaderboard.iter().enumerate() {
            ui.label(format!("{}. {} ({:.3}x)", i + 1, model, rtf));
        }
    }
}

fn format_number(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{:.1}K", n as f64 / 1_000.0),
        _ => n.to_string(),
    }
}
