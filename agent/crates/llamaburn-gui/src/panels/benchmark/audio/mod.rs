mod devices;
mod effects;
mod stt;
mod ui;

use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use tracing::{info, warn};

use llamaburn_core::{
    AudioBenchmarkResult, AudioMode, BenchmarkType, EffectDetectionTool, WhisperModel,
};
use llamaburn_services::{AudioHistoryEntry, ModelInfoService};

use super::{AudioSourceMode, AudioTestState, BenchmarkPanel};

impl BenchmarkPanel {
    pub(super) fn render_audio_config(&mut self, ui: &mut egui::Ui) {
        let disabled = self.running;

        // Audio Setup dropdown button
        {
            if self.audio_devices.is_empty() && !self.loading_devices {
                self.refresh_audio_devices();
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
                        self.render_audio_device_menu(ui);
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
                        false => self.start_live_monitor(),
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
            let needs_capture = self.audio_source_mode != super::AudioSourceMode::File;
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
                        ui.add_enabled_ui(!disabled, |ui| {
                            let selected_text: &str = match self.selected_model.is_empty() {
                                true => "Select model (optional)...",
                                false => &self.selected_model,
                            };

                            egui::ComboBox::from_id_salt("llm_model_audio")
                                .selected_text(selected_text)
                                .show_ui(ui, |ui| {
                                    for model in &self.models {
                                        let clicked = ui.selectable_label(&self.selected_model == model, model).clicked();
                                        if !clicked {
                                            continue;
                                        }

                                        self.selected_model = model.clone();
                                        self.model_preload_rx = Some(self.ollama.preload_model_async(model));
                                        self.model_preloading = true;
                                        self.preloading_model_name = model.clone();
                                        self.live_output.push_str(&format!("⏳ Loading {} into VRAM...\n", model));
                                    }
                                });
                        });

                        if self.loading_models || self.model_preloading {
                            ui.spinner();
                        }

                        if self.model_preloading {
                            ui.colored_label(egui::Color32::YELLOW, "Loading...");
                        }

                        if ui.small_button("↻").on_hover_text("Refresh models").clicked() {
                            self.refresh_models();
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
                    self.refresh_audio_devices();
                }
                ui.end_row();

                // File picker (File mode only)
                if self.audio_source_mode == AudioSourceMode::File {
                    ui.label("Audio:");
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!disabled, egui::Button::new("Select File...")).clicked() {
                            self.pick_audio_file();
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
        self.render_transport_controls(ui);

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
    }

    pub(super) fn render_audio_rankings(&self, ui: &mut egui::Ui) {
        let best = self.model_best_rtf
            .map(|r| format!("{:.3}x RTF", r))
            .unwrap_or_else(|| "—".to_string());
        ui.label(format!("Model Best: {}", best));

        let all_time = self.all_time_best_audio.as_ref()
            .map(|(m, r)| format!("{:.3}x ({m})", r))
            .unwrap_or_else(|| "—".to_string());
        ui.label(format!("All-Time: {}", all_time));

        if self.audio_leaderboard.is_empty() {
            return;
        }

        ui.add_space(10.0);
        ui.label(egui::RichText::new("Leaderboard").small().color(egui::Color32::GRAY));

        for (i, (model, rtf)) in self.audio_leaderboard.iter().enumerate() {
            ui.label(format!("{}. {} ({:.3}x)", i + 1, model, rtf));
        }
    }

    pub(super) fn render_transport_controls(&mut self, ui: &mut egui::Ui) {
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
                self.stop_recording();
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
                self.start_recording();
            }

            ui.add_enabled(false, egui::Button::new("⏭")).on_disabled_hover_text("Forward (coming soon)");

            ui.add_space(10.0);

            if is_recording {
                ui.spinner();
                ui.label("Recording...");
            }
        });
    }

    pub(super) fn start_recording(&mut self) {
        let whisper_ready = self.whisper_model
            .map(|m| self.whisper_service.is_model_downloaded(m))
            .unwrap_or(false);
        let fx_ready = self.is_effect_tool_available(self.selected_effect_tool);

        match self.audio_source_mode {
            AudioSourceMode::File => {
                if whisper_ready {
                    self.start_audio_benchmark();
                }
                if fx_ready {
                    self.start_effect_detection();
                }
            }
            AudioSourceMode::Capture => {
                if whisper_ready {
                    self.start_capture_benchmark();
                }
                if fx_ready {
                    self.start_effect_detection_capture();
                }
            }
            AudioSourceMode::LiveStream => {
                if whisper_ready {
                    self.start_live_transcription_with_fx(fx_ready);
                } else if fx_ready {
                    self.start_effect_detection_live();
                }
            }
        }
    }

    pub(super) fn stop_recording(&mut self) {
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
        self.progress = "Stopped".to_string();
    }

    pub(super) fn save_audio_to_history(&mut self, result: &AudioBenchmarkResult) {
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

    pub(super) fn refresh_audio_rankings(&mut self) {
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

        self.model_best_rtf = self.history_service
            .get_best_audio_for_model(&model_id, AudioMode::Stt)
            .ok()
            .flatten();

        self.all_time_best_audio = self.history_service
            .get_all_time_best_audio(AudioMode::Stt)
            .ok()
            .flatten();

        self.audio_leaderboard = self.history_service
            .get_audio_leaderboard(AudioMode::Stt, 5)
            .unwrap_or_default();
    }

    pub(super) fn force_refresh_audio_rankings(&mut self) {
        self.last_whisper_model_for_rankings = None;
        self.refresh_audio_rankings();
    }

    pub(super) fn refresh_audio_model_info(&mut self) {
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

    pub(super) fn poll_audio_model_info(&mut self) {
        let Some(rx) = &self.audio_model_info_rx else {
            return;
        };

        let Ok(info) = rx.try_recv() else {
            return;
        };

        self.model_info = info;
        self.audio_model_info_rx = None;
    }
}
