mod devices;
mod effects;
mod stt;
mod ui;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use eframe::egui;

use llamaburn_services::{
    AudioBenchmarkResult, AudioSourceMode, BenchmarkType, EffectDetectionResult,
    EffectDetectionTool, WhisperModel,
};
use llamaburn_services::{
    AudioHistoryEntry, EffectDetectionService, HistoryService, ModelInfo, ModelInfoService,
    OllamaClient, OllamaError, WhisperService,
};


// ============================================================================
// Audio Types
// ============================================================================

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
        start: Instant,
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

/// Events from async audio benchmark
pub enum AudioBenchmarkEvent {
    Progress(String),
    IterationComplete {
        iteration: u32,
        metrics: llamaburn_services::AudioBenchmarkMetrics,
    },
    Done {
        metrics: Vec<llamaburn_services::AudioBenchmarkMetrics>,
    },
    Error(String),
}

// ============================================================================
// Shared State for Audio Panel
// ============================================================================

/// Shared state passed from parent BenchmarkPanel to AudioBenchmarkPanel
pub struct AudioSharedState<'a> {
    /// Model list (for LLM selection in effect detection)
    pub model_list: &'a mut llamaburn_services::ModelList,
    /// Audio benchmark model (has live_output, progress, error)
    pub audio: &'a mut llamaburn_services::AudioBenchmark,
    /// Ollama client for model operations
    pub ollama: &'a OllamaClient,
    /// History service
    pub history_service: &'a HistoryService,
}

// ============================================================================
// Action Pattern - AudioAction (like Redux actions)
// ============================================================================

/// Actions emitted by AudioBenchmarkPanel for parent to process
#[derive(Debug)]
pub enum AudioAction {
    // Output mutations
    AppendOutput(String),
    ClearOutput,
    SetProgress(String),
    SetError(Option<String>),

    // History operations
    SaveHistory(AudioHistoryEntry),

    // Model management
    RefreshModels,
    /// Preload an LLM model into VRAM
    PreloadLlmModel(String),
}

/// Read-only context for rendering config UI
pub struct AudioRenderContext<'a> {
    pub models: &'a [String],
    pub selected_model: &'a str,
    pub loading_models: bool,
    pub model_preloading: bool,
}

// ============================================================================
// AudioBenchmarkPanel
// ============================================================================

/// Audio benchmark panel state
pub struct AudioBenchmarkPanel {
    // Config
    pub iterations: u32,
    pub warmup: u32,

    // Execution state
    pub running: bool,
    pub audio_file_path: Option<PathBuf>,
    pub audio_duration_ms: Option<f64>,
    pub whisper_model: Option<WhisperModel>,
    pub whisper_service: WhisperService,
    pub audio_result: Option<AudioBenchmarkResult>,
    pub audio_rx: Option<Receiver<AudioBenchmarkEvent>>,

    // Audio recording state
    pub audio_source_mode: AudioSourceMode,
    pub audio_devices: Vec<llamaburn_services::AudioDevice>,
    pub selected_device_id: Option<String>,
    pub capture_duration_secs: u32,
    pub loading_devices: bool,

    // Model info
    pub model_info: Option<ModelInfo>,
    pub last_model_for_info: Option<WhisperModel>,
    pub model_info_rx: Option<Receiver<Option<ModelInfo>>>,

    // Live transcription state (DAW mode)
    pub live_recording: bool,
    pub waveform_peaks: std::collections::VecDeque<(f32, f32)>,
    pub recording_start: Option<Instant>,
    pub transcription_segments: Vec<TranscriptionSegment>,
    pub live_transcription_rx: Option<Receiver<LiveTranscriptionEvent>>,
    pub live_stream_handle: Option<llamaburn_services::StreamHandle>,

    // Audio test state (mic test & monitoring)
    pub audio_test_state: AudioTestState,
    pub audio_test_rx: Option<Receiver<AudioTestEvent>>,
    pub monitor_handle: Option<llamaburn_services::MonitorHandle>,

    // Input level monitor (VU meter)
    pub level_monitor_handle: Option<llamaburn_services::StreamHandle>,
    pub level_monitor_rx: Option<Receiver<(f32, f32)>>,
    pub waveform_monitor_rx: Option<Receiver<Vec<(f32, f32)>>>,
    pub input_levels: (f32, f32),

    // Audio settings dialog
    pub show_audio_settings: bool,
    pub audio_sample_rate: u32,
    pub audio_sample_format: AudioSampleFormat,
    pub audio_channels: u16,
    pub playback_device_id: Option<String>,
    pub playback_latency_ms: u32,

    // Audio effects chain
    pub effect_chain: Arc<std::sync::Mutex<llamaburn_services::audio_effects::EffectChain>>,
    pub show_effects_ui: bool,
    pub effects_rack_expanded: bool,

    // Effect detection state
    pub selected_effect_tool: EffectDetectionTool,
    pub reference_audio_path: Option<PathBuf>,
    pub effect_detection_result: Option<EffectDetectionResult>,
    pub effect_detection_running: bool,
    pub effect_detection_rx: Option<Receiver<Result<EffectDetectionResult, String>>>,
    pub effect_tool_availability: HashMap<EffectDetectionTool, bool>,
    pub effect_tool_check_rx: Option<Receiver<(EffectDetectionTool, bool)>>,
}

impl Default for AudioBenchmarkPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioBenchmarkPanel {
    pub fn new() -> Self {
        let mut panel = Self {
            iterations: 5,
            warmup: 2,

            running: false,
            audio_file_path: None,
            audio_duration_ms: None,
            whisper_model: None,
            whisper_service: WhisperService::default(),
            audio_result: None,
            audio_rx: None,

            audio_source_mode: AudioSourceMode::default(),
            audio_devices: Vec::new(),
            selected_device_id: None,
            capture_duration_secs: 10,
            loading_devices: false,

            model_info: None,
            last_model_for_info: None,
            model_info_rx: None,

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

            effect_chain: Arc::new(std::sync::Mutex::new(
                llamaburn_services::audio_effects::EffectChain::new(),
            )),
            show_effects_ui: false,
            effects_rack_expanded: true,

            selected_effect_tool: EffectDetectionTool::default(),
            reference_audio_path: None,
            effect_detection_result: None,
            effect_detection_running: false,
            effect_detection_rx: None,
            effect_tool_availability: HashMap::new(),
            effect_tool_check_rx: None,
        };

        // Start async tool availability check on startup
        panel.refresh_effect_tool_availability();

        panel
    }

    /// Get the benchmark type for this panel
    pub fn benchmark_type() -> BenchmarkType {
        BenchmarkType::Audio
    }

    /// Check tool availability from cache
    pub fn is_effect_tool_available(&self, tool: EffectDetectionTool) -> bool {
        self.effect_tool_availability.get(&tool).copied().unwrap_or(false)
    }

    /// Refresh effect detection tool availability (runs in background thread)
    pub fn refresh_effect_tool_availability(&mut self) {
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
    pub fn poll_effect_tool_availability(&mut self) {
        let Some(rx) = &self.effect_tool_check_rx else {
            return;
        };

        while let Ok((tool, available)) = rx.try_recv() {
            self.effect_tool_availability.insert(tool, available);
        }
    }

    pub fn render_config(&mut self, ui: &mut egui::Ui, shared: &mut AudioSharedState<'_>) -> Vec<AudioAction> {
        let mut actions = Vec::new();
        let disabled = self.running;

        // Audio Setup dropdown button
        {
            if self.audio_devices.is_empty() && !self.loading_devices {
                self.refresh_audio_devices(&mut shared.audio.error);
            }

            ui.horizontal(|ui| {
                let button_text = self
                    .selected_device_id
                    .as_ref()
                    .and_then(|id| self.audio_devices.iter().find(|d| &d.id == id))
                    .map(|d| {
                        let display_name = d.card_name.as_ref().unwrap_or(&d.name);
                        format!("🔊 {}", display_name)
                    })
                    .unwrap_or_else(|| "🔊 Audio Setup".to_string());

                ui.add_enabled_ui(!disabled, |ui| {
                    ui.menu_button(button_text, |ui| {
                        self.render_audio_device_menu(ui, &mut shared.audio.error);
                    });
                });

                if self.loading_devices {
                    ui.spinner();
                }

                ui.add_space(8.0);

                // Input monitor toggle
                let monitor_active = matches!(self.audio_test_state, AudioTestState::Monitoring);
                let btn_size = egui::vec2(18.0, 18.0);
                let (response, painter) = ui.allocate_painter(btn_size, egui::Sense::click());
                let rect = response.rect;

                let colors = [
                    (egui::Color32::from_rgb(180, 60, 60), egui::Color32::TRANSPARENT),
                    (egui::Color32::from_rgb(220, 50, 50), egui::Color32::from_rgb(220, 50, 50)),
                ];
                let (stroke_color, fill_color) = colors[monitor_active as usize];

                painter.rect(rect.shrink(2.0), 2.0, fill_color, egui::Stroke::new(2.0, stroke_color));

                if response.clicked() {
                    match monitor_active {
                        true => self.stop_live_monitor(),
                        false => self.start_live_monitor(&mut shared.audio.error),
                    }
                }

                let tooltips = ["Enable live monitoring", "Disable live monitoring"];
                response.on_hover_text(tooltips[monitor_active as usize]);

                if monitor_active {
                    ui.label(
                        egui::RichText::new("Input Monitor")
                            .small()
                            .color(egui::Color32::from_rgb(220, 50, 50)),
                    );
                }
            });

            // Only show VU meter when device is selected AND in Capture/Live mode
            let has_device = self.selected_device_id.is_some();
            let needs_capture = self.audio_source_mode != AudioSourceMode::File;
            let needs_monitor = has_device && needs_capture && self.level_monitor_handle.is_none();
            if needs_monitor {
                self.start_level_monitor();
            }
            // Stop monitor if switched to File mode
            if !needs_capture && self.level_monitor_handle.is_some() {
                self.stop_level_monitor();
            }
            if has_device && needs_capture {
                self.render_level_meter(ui);
            }

            ui.add_space(8.0);
        }

        egui::Grid::new("audio_config_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                // Whisper model selector
                ui.label("STT Model:");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!disabled, |ui| {
                        let selected_text = self.whisper_model.map(|m| m.label()).unwrap_or("Select model...");

                        egui::ComboBox::from_id_salt("whisper_model")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for model in WhisperModel::all() {
                                    let label = format!("{} (~{}MB)", model.label(), model.size_mb());
                                    if ui.selectable_label(self.whisper_model == Some(*model), label).clicked() {
                                        self.whisper_model = Some(*model);
                                    }
                                }
                            });
                    });

                    let can_unload = self.whisper_model.is_some() && !self.running;
                    if ui.add_enabled(can_unload, egui::Button::new("Unload")).clicked() {
                        self.unload_whisper_model();
                    }
                });
                ui.end_row();

                // Effect detection tool selector
                ui.label("FX Detect:");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!disabled, |ui| {
                        egui::ComboBox::from_id_salt("effect_tool")
                            .selected_text(self.selected_effect_tool.label())
                            .show_ui(ui, |ui| {
                                for tool in EffectDetectionTool::all() {
                                    if ui.selectable_label(self.selected_effect_tool == *tool, tool.label())
                                        .on_hover_text(tool.description())
                                        .clicked()
                                    {
                                        self.selected_effect_tool = *tool;
                                    }
                                }
                            });
                    });

                    let available = self.is_effect_tool_available(self.selected_effect_tool);
                    let (color, text) = if available {
                        (egui::Color32::GREEN, "Ready")
                    } else {
                        (egui::Color32::YELLOW, "Not installed")
                    };
                    ui.colored_label(color, text);
                });
                ui.end_row();

                // LLM2Fx specific options
                if self.selected_effect_tool == EffectDetectionTool::Llm2FxTools {
                    ui.label("Dry Audio:");
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!disabled, |ui| {
                            let display = self.reference_audio_path.as_ref()
                                .and_then(|p| p.file_name())
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Select reference (dry) audio...".to_string());

                            let clicked = ui.button(display).clicked();
                            let picked = clicked.then(|| {
                                rfd::FileDialog::new()
                                    .add_filter("Audio", &["wav", "mp3", "flac", "ogg", "m4a"])
                                    .pick_file()
                            }).flatten();
                            if let Some(path) = picked {
                                self.reference_audio_path = Some(path);
                            }

                            let should_clear = self.reference_audio_path.is_some() && ui.small_button("✕").clicked();
                            if should_clear {
                                self.reference_audio_path = None;
                            }
                        });
                    });
                    ui.end_row();

                    ui.label("LLM Model:");
                    ui.horizontal(|ui| {
                        // Collect state before rendering to avoid borrow conflicts
                        let selected = shared.model_list.selected.clone();
                        let models: Vec<String> = shared.model_list.models.clone();
                        let loading = shared.model_list.loading;
                        let preloading = shared.model_list.preloading;

                        let mut new_selection: Option<String> = None;
                        ui.add_enabled_ui(!disabled, |ui| {
                            let selected_text: &str = match selected.is_empty() {
                                true => "Select model (optional)...",
                                false => &selected,
                            };

                            egui::ComboBox::from_id_salt("llm_model_audio")
                                .selected_text(selected_text)
                                .show_ui(ui, |ui| {
                                    for model in &models {
                                        let clicked = ui.selectable_label(selected == *model, model).clicked();
                                        if clicked {
                                            new_selection = Some(model.clone());
                                        }
                                    }
                                });
                        });

                        // Apply selection after iteration
                        if let Some(model) = new_selection {
                            shared.model_list.select(model.clone());
                            shared.audio.live_output.push_str(&format!("⏳ Loading {} into VRAM...\n", model));
                            actions.push(AudioAction::PreloadLlmModel(model));
                        }

                        if loading || preloading {
                            ui.spinner();
                        }

                        if preloading {
                            ui.colored_label(egui::Color32::YELLOW, "Loading...");
                        }

                        if ui.small_button("↻").on_hover_text("Refresh models").clicked() {
                            actions.push(AudioAction::RefreshModels);
                        }
                    });
                    ui.end_row();
                }

                // Audio source mode selector
                ui.label("Source:");
                let prev_mode = self.audio_source_mode;
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!disabled, |ui| {
                        ui.selectable_value(&mut self.audio_source_mode, AudioSourceMode::File, "File");
                        ui.selectable_value(&mut self.audio_source_mode, AudioSourceMode::Capture, "Capture");
                        ui.selectable_value(&mut self.audio_source_mode, AudioSourceMode::LiveStream, "Live");
                    });
                });
                if self.audio_source_mode != prev_mode
                    && self.audio_source_mode != AudioSourceMode::File
                    && self.audio_devices.is_empty()
                {
                    self.refresh_audio_devices(&mut shared.audio.error);
                }
                ui.end_row();

                // File picker (File mode only)
                if self.audio_source_mode == AudioSourceMode::File {
                    ui.label("Audio:");
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!disabled, egui::Button::new("Select File...")).clicked() {
                            self.pick_audio_file(&mut shared.audio.error);
                        }

                        if let Some(path) = &self.audio_file_path {
                            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
                            ui.label(filename);
                        }
                    });
                    ui.end_row();

                    if let Some(duration_ms) = self.audio_duration_ms {
                        ui.label("Duration:");
                        ui.label(format!("{:.1}s", duration_ms / 1000.0));
                        ui.end_row();
                    }
                }

                // Duration slider (Capture mode only)
                if self.audio_source_mode == AudioSourceMode::Capture {
                    ui.label("Duration:");
                    ui.add_enabled(!disabled, egui::Slider::new(&mut self.capture_duration_secs, 5..=60).suffix("s"));
                    ui.end_row();
                }

                // STT model download status
                if let Some(model) = self.whisper_model {
                    ui.label("STT Status:");
                    let downloaded = self.whisper_service.is_model_downloaded(model);
                    if downloaded {
                        ui.colored_label(egui::Color32::GREEN, "Model ready");
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "Model not downloaded");
                    }
                    ui.end_row();
                }

                ui.label("Iterations:");
                ui.add_enabled(!disabled, egui::DragValue::new(&mut self.iterations).range(1..=20));
                ui.end_row();

                ui.label("Warmup:");
                ui.add_enabled(!disabled, egui::DragValue::new(&mut self.warmup).range(0..=5));
                ui.end_row();
            });

        ui.add_space(10.0);
        actions.extend(self.render_transport_controls(ui, &shared.model_list.selected));

        // Effect detection results
        if let Some(result) = &self.effect_detection_result {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Detected Effects").strong());
            ui.separator();

            if result.effects.is_empty() {
                ui.label("No effects detected");
            } else {
                egui::Grid::new("effects_results_grid")
                    .num_columns(2)
                    .spacing([20.0, 4.0])
                    .show(ui, |ui| {
                        ui.strong("Effect");
                        ui.strong("Confidence");
                        ui.end_row();

                        for effect in &result.effects {
                            ui.label(&effect.name);
                            ui.add(egui::ProgressBar::new(effect.confidence).text(format!("{:.0}%", effect.confidence * 100.0)));
                            ui.end_row();
                        }
                    });
            }

            ui.add_space(5.0);
            ui.label(format!("Processing: {:.1}ms | Tool: {}", result.processing_time_ms, result.tool.label()));
        }

        // Show audio results if available
        if let Some(result) = &self.audio_result {
            ui.add_space(10.0);
            ui.separator();
            ui.label(egui::RichText::new("Audio Results").strong());

            let wps = result.metrics.first()
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

            ui.label(format!("Avg RTF: {:.3}x ({})", result.summary.avg_rtf, speed_label));
            ui.label(format!("Avg Time: {:.0} ms", result.summary.avg_processing_ms));
            ui.label(format!("Min/Max RTF: {:.3}/{:.3}", result.summary.min_rtf, result.summary.max_rtf));
            ui.label(format!("WPS: {:.1} words/sec", wps));
            ui.label(format!("Iterations: {}", result.summary.iterations));

            if let Some(first) = result.metrics.first() {
                ui.label(format!("Audio: {:.1}s | Words: {}", first.audio_duration_ms / 1000.0, first.word_count));
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

        actions
    }

    fn render_transport_controls(&mut self, ui: &mut egui::Ui, selected_model: &str) -> Vec<AudioAction> {
        let mut actions = Vec::new();
        let is_recording = self.running || self.effect_detection_running || self.live_recording;

        let source_ready = match self.audio_source_mode {
            AudioSourceMode::File => self.audio_file_path.is_some(),
            AudioSourceMode::Capture | AudioSourceMode::LiveStream => self.selected_device_id.is_some(),
        };

        let whisper_ready = self.whisper_model
            .map(|m| self.whisper_service.is_model_downloaded(m))
            .unwrap_or(false);
        let fx_ready = self.is_effect_tool_available(self.selected_effect_tool);
        let any_model_ready = whisper_ready || fx_ready;

        ui.horizontal(|ui| {
            ui.add_space(10.0);

            ui.add_enabled(false, egui::Button::new("⏮")).on_disabled_hover_text("Rewind (coming soon)");
            ui.add_enabled(false, egui::Button::new("▶")).on_disabled_hover_text("Play (coming soon)");
            ui.add_enabled(false, egui::Button::new("⏸")).on_disabled_hover_text("Pause (coming soon)");

            let can_stop = is_recording;
            if ui.add_enabled(can_stop, egui::Button::new("⏹")).on_hover_text("Stop").clicked() {
                actions.extend(self.stop_recording());
            }

            let record_color = if is_recording {
                egui::Color32::RED
            } else {
                egui::Color32::from_rgb(180, 60, 60)
            };

            let can_record = !is_recording && source_ready && any_model_ready;
            let record_btn = egui::Button::new(egui::RichText::new("⏺").color(record_color).size(18.0));

            let record_hover = match (source_ready, any_model_ready) {
                (false, _) => "Select audio source first",
                (_, false) => "Select and download a model first",
                _ => "Start recording",
            };

            if ui.add_enabled(can_record, record_btn).on_hover_text(record_hover).clicked() {
                actions.extend(self.start_recording(selected_model));
            }

            ui.add_enabled(false, egui::Button::new("⏭")).on_disabled_hover_text("Forward (coming soon)");

            ui.add_space(10.0);

            if is_recording {
                ui.spinner();
                ui.label("Recording...");
            }
        });

        actions
    }

    pub fn start_recording(&mut self, selected_model: &str) -> Vec<AudioAction> {
        let whisper_ready = self.whisper_model
            .map(|m| self.whisper_service.is_model_downloaded(m))
            .unwrap_or(false);
        let fx_ready = self.is_effect_tool_available(self.selected_effect_tool);

        let mut actions = Vec::new();

        match self.audio_source_mode {
            AudioSourceMode::File => {
                if whisper_ready {
                    actions.extend(self.start_audio_benchmark());
                }
                if fx_ready {
                    actions.extend(self.start_effect_detection());
                }
            }
            AudioSourceMode::Capture => {
                if whisper_ready {
                    actions.extend(self.start_capture_benchmark());
                }
                if fx_ready {
                    actions.extend(self.start_effect_detection_capture(selected_model));
                }
            }
            AudioSourceMode::LiveStream => {
                if whisper_ready {
                    actions.extend(self.start_live_transcription_with_fx(fx_ready));
                } else if fx_ready {
                    actions.extend(self.start_effect_detection_live());
                }
            }
        }

        actions
    }

    pub fn stop_recording(&mut self) -> Vec<AudioAction> {
        self.running = false;
        self.effect_detection_running = false;
        self.live_recording = false;
        self.recording_start = None;
        if let Some(handle) = self.live_stream_handle.take() {
            handle.stop();
        }
        if let Some(handle) = self.monitor_handle.take() {
            handle.stop();
        }
        self.audio_test_state = AudioTestState::Idle;

        vec![AudioAction::SetProgress("Stopped".to_string())]
    }

    pub fn refresh_model_info(&mut self) {
        let Some(model) = self.whisper_model else {
            return;
        };

        if self.last_model_for_info == Some(model) {
            return;
        }

        self.last_model_for_info = Some(model);
        self.model_info = None;
        self.model_info_rx = Some(ModelInfoService::fetch_whisper_info_async(model));
    }

    pub fn poll_model_info(&mut self) {
        let Some(rx) = &self.model_info_rx else {
            return;
        };

        let Ok(info) = rx.try_recv() else {
            return;
        };

        self.model_info = info;
        self.model_info_rx = None;
    }
}
