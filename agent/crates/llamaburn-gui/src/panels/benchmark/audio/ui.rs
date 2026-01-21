use eframe::egui;
use tracing::info;

use llamaburn_services::{EffectDetectionTool, WhisperModel};

use super::{AudioBenchmarkPanel, AudioSampleFormat, AudioTestState, CHANNEL_OPTIONS, SAMPLE_RATES};

impl AudioBenchmarkPanel {
    pub fn render_quality_settings(&mut self, ui: &mut egui::Ui) {
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

    pub fn render_recording_settings(&mut self, ui: &mut egui::Ui) {
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

    pub fn apply_audio_settings(&mut self) {
        // Restart live monitor if running to apply new latency
        let monitor_running = matches!(self.audio_test_state, AudioTestState::Monitoring);
        if !monitor_running {
            return;
        }

        info!("Applying audio settings - restarting live monitor");
        self.stop_live_monitor();
        // Note: caller should start monitor again with appropriate error handling
    }

    pub fn render_audio_settings_content(&mut self, ui: &mut egui::Ui, error: &mut Option<String>) {
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
                    // Restart monitor if it was running
                    if matches!(self.audio_test_state, AudioTestState::Monitoring) {
                        self.start_live_monitor(error);
                    }
                }
                if ui.button("Cancel").clicked() {
                    self.show_audio_settings = false;
                }
            });
        });
    }

    pub fn render_audio_settings_dialog(&mut self, ctx: &egui::Context, error: &mut Option<String>) {
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
                self.render_audio_settings_content(ui, error);
            });

        // Preserve false if OK/Cancel was clicked, or set false if X was clicked
        self.show_audio_settings = self.show_audio_settings && open;
    }

    pub fn render_waveform_display(&mut self, ui: &mut egui::Ui) {
        // Header with recording indicator and countdown
        ui.horizontal(|ui| {
            let (label_text, label_color) = match self.live_recording {
                true => ("🔴 Recording", Some(egui::Color32::RED)),
                false => ("Waveform", None),
            };

            match label_color {
                Some(color) => ui.colored_label(color, label_text),
                None => ui.label(label_text),
            };

            // Show countdown timer during recording
            if self.live_recording {
                let Some(start) = self.recording_start else {
                    return;
                };
                let elapsed = start.elapsed().as_secs_f64();
                let total = self.capture_duration_secs as f64;
                let remaining = (total - elapsed).max(0.0);

                // Countdown display
                let countdown_color = match remaining < 3.0 {
                    true => egui::Color32::YELLOW,
                    false => egui::Color32::WHITE,
                };
                ui.colored_label(
                    countdown_color,
                    format!("{:.1}s", remaining),
                );

                ui.separator();

                // Progress bar
                let progress = (elapsed / total).min(1.0) as f32;
                let progress_bar = egui::ProgressBar::new(progress)
                    .desired_width(100.0)
                    .show_percentage();
                ui.add(progress_bar);

                // Request repaint for smooth countdown animation
                ui.ctx().request_repaint();
            }
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

        // Auto-scale: find max amplitude and normalize display
        let max_amplitude = self
            .waveform_peaks
            .iter()
            .map(|(min, max)| min.abs().max(max.abs()))
            .fold(0.0f32, f32::max)
            .max(0.001); // Avoid division by zero

        // Scale factor: quiet audio gets amplified, loud audio stays at 1.0
        let scale = (0.8 / max_amplitude).min(50.0); // Cap at 50x gain

        // Audacity-style: draw thin vertical lines, 1 pixel each
        // Map peaks to pixel positions, drawing one line per pixel
        let waveform_color = egui::Color32::from_rgb(100, 180, 255); // Light blue

        for (i, (min, max)) in self.waveform_peaks.iter().enumerate() {
            let x = rect.left() + (i as f32 / num_peaks.max(1) as f32) * width;

            // Scale samples with auto-gain
            let scaled_min = (min * scale).clamp(-1.0, 1.0);
            let scaled_max = (max * scale).clamp(-1.0, 1.0);
            let min_y = center_y - scaled_min * height;
            let max_y = center_y - scaled_max * height;

            // Draw thin vertical line from min to max
            painter.line_segment(
                [egui::pos2(x, min_y), egui::pos2(x, max_y)],
                egui::Stroke::new(1.0, waveform_color),
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

    pub fn render_model_downloads(&mut self, ui: &mut egui::Ui, live_output: &mut String) {
        // Whisper Models section
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
            let actions = self.download_whisper_model(model);
            // Apply actions locally for this UI context
            for action in actions {
                if let super::AudioAction::AppendOutput(s) = action {
                    *live_output = s;
                }
            }
        }

        // Effect Detection Tools section
        ui.add_space(15.0);
        ui.separator();
        ui.add_space(5.0);
        ui.label(egui::RichText::new("Effect Detection").strong());
        ui.add_space(5.0);

        let mut tool_to_install: Option<EffectDetectionTool> = None;

        egui::Grid::new("effect_detection_grid")
            .num_columns(3)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Tool").small());
                ui.label(egui::RichText::new("Status").small());
                ui.label("");
                ui.end_row();

                for tool in EffectDetectionTool::all() {
                    ui.label(tool.label());

                    let available = self.is_effect_tool_available(*tool);
                    if available {
                        ui.colored_label(egui::Color32::GREEN, "Ready");
                        ui.label(""); // Empty cell
                    } else {
                        ui.colored_label(egui::Color32::GRAY, "—");
                        if ui.link("Install").clicked() {
                            tool_to_install = Some(*tool);
                        }
                    }
                    ui.end_row();
                }
            });

        if let Some(tool) = tool_to_install {
            use llamaburn_services::EffectDetectionService;
            let instructions = EffectDetectionService::install_instructions(tool);
            info!("Showing install instructions for: {:?}", tool);
            *live_output = format!(
                "Install {} with:\n\n{}\n\nRun this in your terminal to install the Python package.",
                tool.label(),
                instructions
            );
        }

        // Refresh link
        ui.horizontal(|ui| {
            if ui.small_button("Refresh").clicked() {
                self.refresh_effect_tool_availability();
            }
            if self.effect_tool_availability.is_empty() {
                ui.small("(click to check)");
            }
        });
    }

    pub fn format_time_ms(ms: u64) -> String {
        let secs = ms / 1000;
        let millis = ms % 1000;
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{:02}:{:02}.{}", mins, secs, millis / 100)
    }
}
