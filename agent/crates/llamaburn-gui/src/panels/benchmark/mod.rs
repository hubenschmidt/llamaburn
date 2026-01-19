mod audio;
mod code;
mod components;
mod text;

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use eframe::egui;
use tokio_util::sync::CancellationToken;
use tracing::info;

use llamaburn_core::{
    AudioBenchmarkResult, BenchmarkConfig, BenchmarkMetrics, BenchmarkType,
    EffectDetectionResult, EffectDetectionTool, WhisperModel,
};
use llamaburn_services::{
    BenchmarkEvent, BenchmarkHistoryEntry, BenchmarkService, BenchmarkSummary,
    EffectDetectionService, HistoryService, ModelInfo, ModelInfoService, OllamaClient,
    OllamaError, WhisperService,
};

pub use code::CodeBenchmarkState;

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
#[derive(Debug, Clone)]
pub struct TranscriptionSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub rtf: f64,
}

/// Events from live transcription stream
pub enum LiveTranscriptionEvent {
    /// Waveform peaks for display (min, max pairs)
    AudioPeaks(Vec<(f32, f32)>),
    /// Completed transcription segment
    Transcription(TranscriptionSegment),
    /// Streaming output line (verbose token/segment info)
    StreamOutput(String),
    /// GPU metrics update
    GpuMetrics(llamaburn_services::GpuMetrics),
    /// Effect detection result
    FxDetection(EffectDetectionResult),
    /// Error occurred
    Error(String),
    /// Recording stopped
    Stopped,
}

/// Audio test state for mic testing and monitoring
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
    model_preload_rx: Option<Receiver<Result<(), OllamaError>>>,
    model_preloading: bool,
    preloading_model_name: String,
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
    audio_devices: Vec<llamaburn_services::AudioDevice>,
    selected_device_id: Option<String>,
    capture_duration_secs: u32,
    loading_devices: bool,

    // Audio rankings
    model_best_rtf: Option<f64>,
    all_time_best_audio: Option<(String, f64)>,
    audio_leaderboard: Vec<(String, f64)>,
    last_whisper_model_for_rankings: Option<WhisperModel>,

    // Live transcription state (DAW mode)
    live_recording: bool,
    waveform_peaks: std::collections::VecDeque<(f32, f32)>,
    recording_start: Option<std::time::Instant>,
    transcription_segments: Vec<TranscriptionSegment>,
    live_transcription_rx: Option<Receiver<LiveTranscriptionEvent>>,
    live_stream_handle: Option<llamaburn_services::StreamHandle>,

    // Audio test state (mic test & monitoring)
    audio_test_state: AudioTestState,
    audio_test_rx: Option<Receiver<AudioTestEvent>>,
    monitor_handle: Option<llamaburn_services::MonitorHandle>,

    // Input level monitor (VU meter)
    level_monitor_handle: Option<llamaburn_services::StreamHandle>,
    level_monitor_rx: Option<Receiver<(f32, f32)>>, // (left_peak, right_peak) 0.0-1.0
    waveform_monitor_rx: Option<Receiver<Vec<(f32, f32)>>>, // Dense waveform peaks
    input_levels: (f32, f32), // Current display levels with decay

    // Audio settings dialog
    show_audio_settings: bool,
    audio_sample_rate: u32,
    audio_sample_format: AudioSampleFormat,
    audio_channels: u16,
    playback_device_id: Option<String>,
    playback_latency_ms: u32,

    // Audio effects chain
    effect_chain: std::sync::Arc<std::sync::Mutex<llamaburn_services::audio_effects::EffectChain>>,
    show_effects_ui: bool,
    effects_rack_expanded: bool,

    // Panel layout state
    config_panel_expanded: bool,
    config_panel_height: f32,
    live_output_expanded: bool,
    live_output_height: f32,

    // Effect detection state
    selected_effect_tool: EffectDetectionTool,
    reference_audio_path: Option<std::path::PathBuf>,  // Dry audio for LLM2Fx
    effect_detection_result: Option<EffectDetectionResult>,
    effect_detection_running: bool,
    effect_detection_rx: Option<Receiver<Result<EffectDetectionResult, String>>>,
    effect_tool_availability: std::collections::HashMap<EffectDetectionTool, bool>,
    effect_tool_check_rx: Option<Receiver<(EffectDetectionTool, bool)>>,

    // Code benchmark state
    code_state: CodeBenchmarkState,
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

        let mut panel = Self {
            models: vec![],
            selected_model: String::new(),
            loading_models: true,
            model_rx,
            model_preload_rx: None,
            model_preloading: false,
            preloading_model_name: String::new(),
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
            audio_devices: Vec::new(),
            selected_device_id: None,
            capture_duration_secs: 10,
            loading_devices: false,
            // Audio rankings
            model_best_rtf: None,
            all_time_best_audio: None,
            audio_leaderboard: Vec::new(),
            last_whisper_model_for_rankings: None,
            // Live transcription (DAW mode)
            live_recording: false,
            waveform_peaks: std::collections::VecDeque::new(),
            recording_start: None,
            transcription_segments: Vec::new(),
            live_transcription_rx: None,
            live_stream_handle: None,

            audio_test_state: AudioTestState::Idle,
            audio_test_rx: None,
            monitor_handle: None,

            level_monitor_handle: None,
            level_monitor_rx: None,
            waveform_monitor_rx: None,
            input_levels: (0.0, 0.0),

            show_audio_settings: false,
            audio_sample_rate: 44100,
            audio_sample_format: AudioSampleFormat::default(),
            audio_channels: 2,
            playback_device_id: None,
            playback_latency_ms: 100,

            effect_chain: std::sync::Arc::new(std::sync::Mutex::new(
                llamaburn_services::audio_effects::EffectChain::new(),
            )),
            show_effects_ui: false,
            effects_rack_expanded: true,

            // Panel layout
            config_panel_expanded: true,
            config_panel_height: 280.0,
            live_output_expanded: true,
            live_output_height: 2000.0, // Large default to fill available space

            // Effect detection
            selected_effect_tool: EffectDetectionTool::default(),
            reference_audio_path: None,
            effect_detection_result: None,
            effect_detection_running: false,
            effect_detection_rx: None,
            effect_tool_availability: std::collections::HashMap::new(),
            effect_tool_check_rx: None,

            // Code benchmark
            code_state: CodeBenchmarkState::new(),
        };

        // Start async tool availability check on startup
        panel.refresh_effect_tool_availability();
        panel
    }

    /// Check tool availability from cache, or return false if not yet checked
    fn is_effect_tool_available(&self, tool: EffectDetectionTool) -> bool {
        self.effect_tool_availability.get(&tool).copied().unwrap_or(false)
    }

    /// Refresh effect detection tool availability (runs in background thread)
    fn refresh_effect_tool_availability(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.effect_tool_check_rx = Some(rx);

        std::thread::spawn(move || {
            for tool in EffectDetectionTool::all() {
                let available = EffectDetectionService::is_tool_available(*tool);
                let _ = tx.send((*tool, available));
            }
        });
    }

    /// Poll for tool availability check results
    fn poll_effect_tool_availability(&mut self) {
        let Some(rx) = &self.effect_tool_check_rx else {
            return;
        };

        while let Ok((tool, available)) = rx.try_recv() {
            self.effect_tool_availability.insert(tool, available);
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

    fn poll_model_preload(&mut self) {
        let Some(rx) = self.model_preload_rx.take() else { return };

        match rx.try_recv() {
            Ok(result) => {
                self.model_preloading = false;
                match result {
                    Ok(()) => {
                        self.live_output.push_str(&format!(
                            "✅ {} loaded into VRAM\n",
                            self.preloading_model_name
                        ));
                    }
                    Err(e) => {
                        self.live_output.push_str(&format!(
                            "❌ Failed to load {}: {}\n",
                            self.preloading_model_name, e
                        ));
                    }
                }
                self.preloading_model_name.clear();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Still loading, put receiver back
                self.model_preload_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.model_preloading = false;
                self.live_output.push_str(&format!(
                    "❌ Model preload disconnected for {}\n",
                    self.preloading_model_name
                ));
                self.preloading_model_name.clear();
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll_models();
        self.poll_model_preload();
        self.poll_benchmark();
        self.poll_audio_benchmark();
        self.poll_code_benchmark();
        self.poll_effect_detection();
        self.poll_effect_tool_availability();
        self.poll_live_transcription();
        self.poll_audio_test();
        self.check_playback_completion();
        self.poll_level_monitor();
        self.check_capture_duration();
        self.poll_model_info();
        self.poll_audio_model_info();
        self.refresh_rankings();
        self.refresh_audio_rankings();
        self.refresh_model_info();
        self.refresh_audio_model_info();

        // Collapsible config panel
        let config_header = egui::CollapsingHeader::new(
            egui::RichText::new("⚙️ Benchmark Runner").strong(),
        )
        .default_open(self.config_panel_expanded)
        .show(ui, |ui| {
            self.config_panel_expanded = true;

            self.render_type_selector(ui);
            ui.add_space(10.0);

            // Scrollable config area - height adjusts with Live Output
            let panel_height = self.config_panel_height.clamp(100.0, 1000.0);
            egui::ScrollArea::vertical()
                .max_height(panel_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Set minimum height so content fills the panel
                    ui.set_min_height(panel_height - 20.0);

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

                    let column_height = panel_height - 30.0;
                    ui.horizontal(|ui| {
                        // Left: Config
                        ui.vertical(|ui| {
                            ui.set_width(config_width);
                            ui.set_min_height(column_height);
                            self.render_config(ui);
                        });

                        ui.add_space(spacing);
                        ui.separator();
                        ui.add_space(spacing);

                        // Center: Model Info + Whisper Models (for Audio mode)
                        ui.vertical(|ui| {
                            ui.set_width(info_width);
                            ui.set_min_height(column_height);
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
                            ui.set_min_height(column_height);
                            self.render_results(ui);
                        });
                    });
                });

            // Resize handle at bottom of config panel
            let resize_id = ui.id().with("config_panel_resize");
            let resize_rect = ui.available_rect_before_wrap();
            let handle_rect = egui::Rect::from_min_size(
                egui::pos2(resize_rect.left(), resize_rect.top()),
                egui::vec2(resize_rect.width(), 10.0),
            );

            let response = ui.interact(handle_rect, resize_id, egui::Sense::drag());

            // Visual indicator
            let handle_color = match response.hovered() || response.dragged() {
                true => ui.style().visuals.strong_text_color(),
                false => ui.style().visuals.weak_text_color(),
            };
            ui.painter().hline(
                handle_rect.x_range(),
                handle_rect.center().y,
                egui::Stroke::new(2.0, handle_color),
            );
            ui.painter().text(
                handle_rect.center(),
                egui::Align2::CENTER_CENTER,
                "⋯",
                egui::FontId::proportional(12.0),
                handle_color,
            );

            if response.dragged() {
                self.config_panel_height += response.drag_delta().y;
                self.config_panel_height = self.config_panel_height.clamp(100.0, 1000.0);
            }

            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }

            ui.add_space(8.0);
        });

        // Track collapsed state
        if !config_header.fully_open() {
            self.config_panel_expanded = false;
        }

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
            ui.add_space(5.0);
        }

        // Waveform display - visible during and after recording
        if self.live_recording || !self.waveform_peaks.is_empty() {
            self.render_waveform_display(ui);
            ui.add_space(5.0);
        }

        // Effects rack panel at bottom (Audio mode only) - reserve space
        let effects_rack_height = {
            let is_audio = self.benchmark_type == BenchmarkType::Audio;
            // 40px collapsed, 230px expanded (180px panel + header/padding)
            let heights = [0.0, [40.0, 230.0][self.effects_rack_expanded as usize]];
            heights[is_audio as usize]
        };

        // Live output takes remaining space (minus effects rack)
        self.render_live_output_with_reserved(ui, effects_rack_height);

        // Effects rack panel at bottom (Audio mode only)
        if self.benchmark_type == BenchmarkType::Audio {
            ui.add_space(10.0);
            self.render_effects_rack(ui);
        }

        // Audio settings dialog (rendered as egui Window)
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
        match self.benchmark_type {
            BenchmarkType::Audio => self.render_audio_config(ui),
            BenchmarkType::Code => self.render_code_config(ui),
            _ => self.render_text_config(ui),
        }
    }

    fn render_live_output_with_reserved(&mut self, ui: &mut egui::Ui, reserved_height: f32) {
        // Header row with Clear button
        let header_text = match self.progress.is_empty() {
            true => "📋 Live Output".to_string(),
            false => format!("📋 Live Output — {}", self.progress),
        };

        ui.horizontal(|ui| {
            // Toggle expand/collapse on click
            let toggle = ui.selectable_label(false, egui::RichText::new(
                if self.live_output_expanded { "▼" } else { "▶" }
            ));
            if toggle.clicked() {
                self.live_output_expanded = !self.live_output_expanded;
            }

            ui.strong(&header_text);

            ui.add_space(10.0);
            if ui.small_button("Clear").clicked() {
                self.live_output.clear();
            }
        });

        if !self.live_output_expanded {
            return;
        }

        // Fill available height (minus padding and reserved space for panels below)
        let available_height = (ui.available_height() - reserved_height - 20.0).max(100.0);
        let content_height = self.live_output_height.min(available_height).max(80.0);

        // Use clip_rect width to account for side panels (like GPU Monitor)
        let text_width = ui.clip_rect().width().min(ui.available_width()) - 30.0;
        egui::ScrollArea::vertical()
            .max_height(content_height)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.live_output.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(text_width)
                        .layouter(&mut |ui, string, wrap_width| {
                            let mut layout_job = egui::text::LayoutJob::simple(
                                string.to_owned(),
                                egui::FontId::monospace(12.0),
                                ui.visuals().text_color(),
                                wrap_width,
                            );
                            layout_job.wrap = egui::text::TextWrapping {
                                max_width: wrap_width,
                                ..Default::default()
                            };
                            ui.fonts(|f| f.layout_job(layout_job))
                        }),
                );
            });

        // Resize handle
        let resize_id = ui.id().with("live_output_resize");
        let resize_rect = ui.available_rect_before_wrap();
        let handle_rect = egui::Rect::from_min_size(
            egui::pos2(resize_rect.left(), resize_rect.top()),
            egui::vec2(resize_rect.width(), 8.0),
        );

        let response = ui.interact(handle_rect, resize_id, egui::Sense::drag());

        let handle_color = match response.hovered() || response.dragged() {
            true => ui.style().visuals.strong_text_color(),
            false => ui.style().visuals.weak_text_color(),
        };
        ui.painter().hline(
            handle_rect.x_range(),
            handle_rect.center().y,
            egui::Stroke::new(2.0, handle_color),
        );

        if response.dragged() {
            let delta = response.drag_delta().y;
            self.live_output_height += delta;
            self.live_output_height = self.live_output_height.clamp(80.0, available_height);
            // Inversely adjust config panel height
            self.config_panel_height -= delta;
            self.config_panel_height = self.config_panel_height.clamp(100.0, 1000.0);
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
    }

    fn render_results(&self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("Results")
                .heading()
                .color(egui::Color32::GRAY),
        );

        match self.benchmark_type {
            BenchmarkType::Code => {
                self.render_code_results(ui);
            }
            _ => {
                if let Some(r) = &self.result {
                    ui.label(format!("Avg TPS: {:.2} t/s", r.avg_tps));
                    ui.label(format!("Avg TTFT: {:.2} ms", r.avg_ttft_ms));
                    ui.label(format!("Avg Total: {:.2} ms", r.avg_total_ms));
                    ui.label(format!("Min/Max TPS: {:.1}/{:.1}", r.min_tps, r.max_tps));
                    ui.label(format!("Iterations: {}", r.iterations));
                }
            }
        }

        self.render_rankings(ui);
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
            BenchmarkType::Code => self.render_code_rankings(ui),
            _ => {}
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
