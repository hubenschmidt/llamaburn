use eframe::egui;
use tracing::{info, warn};

use llamaburn_core::{EffectDetectionResult, EffectDetectionTool};
use llamaburn_services::EffectDetectionService;

use super::super::BenchmarkPanel;

impl BenchmarkPanel {
    /// Ableton-style horizontal effects rack panel at bottom of UI
    pub(in super::super) fn render_effects_rack(&mut self, ui: &mut egui::Ui) {
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
                        if !ui.button(name).clicked() {
                            continue;
                        }
                        chain.add(create_effect());
                        ui.close_menu();
                    }
                });

                if chain.effects().is_empty() {
                    return;
                }

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

    pub(in super::super) fn render_effect_list(&self, ui: &mut egui::Ui) {
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

    pub(in super::super) fn render_add_effect_menu(&self, ui: &mut egui::Ui) {
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

    pub(in super::super) fn render_effects_menu(&mut self, ui: &mut egui::Ui) {
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

    pub(in super::super) fn start_effect_detection(&mut self) {
        let Some(audio_path) = self.audio_file_path.clone() else {
            return;
        };

        info!("Starting effect detection: {:?}", audio_path);

        self.effect_detection_running = true;
        self.effect_detection_result = None;

        let tool = self.selected_effect_tool;
        let reference_path = self.reference_audio_path.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.effect_detection_rx = Some(rx);

        std::thread::spawn(move || {
            let service = EffectDetectionService::new(tool);
            let result = service.detect(&audio_path, reference_path.as_deref());

            let result = match result {
                Ok(r) => Ok(r),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(result);
        });
    }

    pub(in super::super) fn start_effect_detection_capture(&mut self) {
        use llamaburn_services::AudioInputService;

        let Some(device_id) = self.selected_device_id.clone() else {
            return;
        };
        let duration = self.capture_duration_secs;
        let tool = self.selected_effect_tool;
        let effect_chain = self.effect_chain.clone();
        let llm_model = (!self.selected_model.is_empty()).then(|| self.selected_model.clone());

        // Check if using LLM2Fx dry+wet mode
        let is_dry_wet_mode = tool == EffectDetectionTool::Llm2FxTools;

        info!(
            "Starting effect detection capture: device={}, duration={}s, tool={:?}, dry_wet={}",
            device_id, duration, tool, is_dry_wet_mode
        );

        self.effect_detection_running = true;
        self.effect_detection_result = None;
        self.live_output.clear();
        self.live_recording = true;
        self.waveform_peaks.clear();
        self.recording_start = Some(std::time::Instant::now());

        // Start level monitor to show waveform during capture
        self.start_level_monitor();

        let mode_text = match is_dry_wet_mode {
            true => "Mode: Recording raw input + applying effects rack\n\n",
            false => "\n",
        };
        let header = match is_dry_wet_mode {
            true => "Effect Detection (Dry+Wet Capture)\n===================================\n",
            false => "Effect Detection (Capture)\n===========================\n",
        };
        self.live_output.push_str(&format!(
            "{}Tool: {}\nDevice: {}\nDuration: {}s\n{}Recording audio...\n",
            header,
            tool.label(),
            device_id,
            duration,
            mode_text,
        ));

        // Get applied effects (ground truth) before spawning thread
        let applied_effects = if is_dry_wet_mode {
            effect_chain.lock().ok().map(|chain| chain.get_applied_effects())
        } else {
            None
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.effect_detection_rx = Some(rx);

        std::thread::spawn(move || {
            // Step 1: Capture audio (raw/dry samples)
            let dry_samples = match AudioInputService::capture(&device_id, duration) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(format!("Capture error: {}", e)));
                    return;
                }
            };

            // Step 2: Save files and run detection
            let service = EffectDetectionService::new(tool);

            // Standard mode: single file (early return)
            if !is_dry_wet_mode {
                let temp_path = std::env::temp_dir().join("llamaburn_capture.wav");
                if let Err(e) = Self::save_samples_to_wav(&dry_samples, 16000, &temp_path) {
                    let _ = tx.send(Err(format!("Failed to save audio: {}", e)));
                    return;
                }

                let result = service.detect(&temp_path, None);
                let _ = std::fs::remove_file(&temp_path);

                let result = result.map_err(|e| e.to_string());
                let _ = tx.send(result);
                return;
            }

            // LLM2Fx dry+wet mode: save both files
            let dry_path = std::env::temp_dir().join("llamaburn_dry.wav");
            let wet_path = std::env::temp_dir().join("llamaburn_wet.wav");

            // Save dry (original)
            if let Err(e) = Self::save_samples_to_wav(&dry_samples, 16000, &dry_path) {
                let _ = tx.send(Err(format!("Failed to save dry audio: {}", e)));
                return;
            }

            // Clone and apply effects for wet
            let mut wet_samples = dry_samples.clone();
            if let Ok(mut chain) = effect_chain.lock() {
                chain.set_sample_rate(16000.0); // Match capture sample rate
                chain.process(&mut wet_samples);
            }

            // Save wet (with effects)
            if let Err(e) = Self::save_samples_to_wav(&wet_samples, 16000, &wet_path) {
                let _ = std::fs::remove_file(&dry_path);
                let _ = tx.send(Err(format!("Failed to save wet audio: {}", e)));
                return;
            }

            // Run detection with both files
            let mut result = service.detect(&wet_path, Some(dry_path.as_path()));

            // Cleanup both temp files
            let _ = std::fs::remove_file(&dry_path);
            let _ = std::fs::remove_file(&wet_path);

            // Add ground truth applied effects to result
            if let Ok(ref mut r) = result {
                r.applied_effects = applied_effects;
            }

            // LLM blind analysis (if model selected)
            if let (Ok(ref mut r), Some(ref model)) = (&mut result, &llm_model) {
                match llamaburn_services::get_llm_blind_analysis(r, model, "http://localhost:11434") {
                    Ok(description) => {
                        r.llm_description = Some(description);
                        r.llm_model_used = Some(model.clone());
                    }
                    Err(e) => tracing::warn!("LLM blind analysis failed: {}", e),
                }
            }

            let result = result.map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
    }

    pub(in super::super) fn start_effect_detection_live(&mut self) {
        use llamaburn_services::AudioInputService;

        let Some(device_id) = self.selected_device_id.clone() else {
            return;
        };
        let tool = self.selected_effect_tool;
        let chunk_duration = 5; // Analyze 5-second chunks

        info!(
            "Starting live effect detection: device={}, tool={:?}",
            device_id, tool
        );

        self.effect_detection_running = true;
        self.effect_detection_result = None;
        self.live_recording = true;
        self.waveform_peaks.clear();
        self.recording_start = Some(std::time::Instant::now());
        self.live_output.clear();
        self.live_output.push_str(&format!(
            "Live Effect Detection\n\
             =====================\n\
             Tool: {}\n\
             Device: {}\n\
             Analyzing {}s chunks...\n\n",
            tool.label(),
            device_id,
            chunk_duration,
        ));

        // Start level monitor for waveform display
        self.start_level_monitor();

        let (tx, rx) = std::sync::mpsc::channel();
        self.effect_detection_rx = Some(rx);

        // For live mode, capture one chunk and analyze it
        // (continuous live detection would need a more complex streaming architecture)
        std::thread::spawn(move || {
            // Capture a chunk
            let samples = match AudioInputService::capture(&device_id, chunk_duration) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(format!("Capture error: {}", e)));
                    return;
                }
            };

            // Save to temp file
            let temp_path = std::env::temp_dir().join("llamaburn_live.wav");
            if let Err(e) = Self::save_samples_to_wav(&samples, 16000, &temp_path) {
                let _ = tx.send(Err(format!("Failed to save audio: {}", e)));
                return;
            }

            // Run effect detection (no reference for live mode)
            let service = EffectDetectionService::new(tool);
            let result = service.detect(&temp_path, None);

            // Cleanup
            let _ = std::fs::remove_file(&temp_path);

            let result = match result {
                Ok(r) => Ok(r),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(result);
        });
    }

    pub(in super::super) fn poll_effect_detection(&mut self) {
        let Some(rx) = self.effect_detection_rx.take() else {
            return;
        };

        match rx.try_recv() {
            Ok(result) => {
                self.effect_detection_running = false;
                self.live_recording = false;
                self.recording_start = None;

                // Stop level monitor but keep waveform peaks visible
                if let Some(handle) = self.level_monitor_handle.take() {
                    handle.stop();
                }

                match result {
                    Ok(detection_result) => {
                        info!(
                            "Effect detection complete: {} effects found",
                            detection_result.effects.len()
                        );
                        // Format results for Live Output
                        self.format_detection_results(&detection_result);
                        self.effect_detection_result = Some(detection_result);
                    }
                    Err(e) => {
                        warn!("Effect detection failed: {}", e);
                        self.live_output.push_str(&format!("\n❌ Error: {}\n", e));
                        self.error = Some(format!("Effect detection failed: {}", e));
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Still running, put the receiver back
                self.effect_detection_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.effect_detection_running = false;
                self.live_recording = false;
                self.error = Some("Effect detection thread disconnected".to_string());
            }
        }
    }

    pub(in super::super) fn format_detection_results(&mut self, result: &EffectDetectionResult) {
        self.live_output.push_str("\n════════════════════════════════════\n");
        self.live_output.push_str("DETECTION RESULTS\n");
        self.live_output.push_str("════════════════════════════════════\n\n");

        // Ground Truth (Applied Effects)
        if let Some(ref applied) = result.applied_effects {
            self.live_output.push_str("📋 APPLIED EFFECTS (Ground Truth)\n");
            self.live_output.push_str("──────────────────────────────────\n");
            if applied.is_empty() {
                self.live_output.push_str("  (none)\n");
            }
            for effect in applied {
                let status = if effect.bypassed { " [BYPASSED]" } else { "" };
                self.live_output.push_str(&format!("  • {}{}\n", effect.name, status));
                for (param, value) in &effect.parameters {
                    self.live_output.push_str(&format!("      {}: {:.2}\n", param, value));
                }
            }
            self.live_output.push('\n');
        }

        // Signal Analysis
        if let Some(ref sa) = result.signal_analysis {
            self.live_output.push_str("📊 SIGNAL ANALYSIS\n");
            self.live_output.push_str("──────────────────────────────────\n");
            if let Some(delay) = sa.detected_delay_ms {
                self.live_output.push_str(&format!("  • Delay detected: {:.1}ms\n", delay));
            }
            if let Some(dr) = sa.dynamic_range_change_db {
                self.live_output.push_str(&format!("  • Dynamic range change: {:.1}dB\n", dr));
            }
            if let Some(freq) = sa.frequency_change_db {
                self.live_output.push_str(&format!("  • Frequency change: {:.1}dB\n", freq));
            }
            if let Some(crest) = sa.crest_factor_change {
                self.live_output.push_str(&format!("  • Crest factor change: {:.2}\n", crest));
            }
            self.live_output.push('\n');
        }

        // Detected Effects
        self.live_output.push_str("🎯 DETECTED EFFECTS\n");
        self.live_output.push_str("──────────────────────────────────\n");
        for effect in &result.effects {
            self.live_output.push_str(&format!(
                "  • {} (confidence: {:.0}%)\n",
                effect.name,
                effect.confidence * 100.0
            ));
        }
        self.live_output.push('\n');

        // Embedding Metrics
        if let (Some(dist), Some(sim)) = (result.embedding_distance, result.cosine_similarity) {
            self.live_output.push_str("📈 EMBEDDING METRICS\n");
            self.live_output.push_str("──────────────────────────────────\n");
            self.live_output.push_str(&format!("  • Distance: {:.4}\n", dist));
            self.live_output.push_str(&format!("  • Cosine similarity: {:.4}\n", sim));
            self.live_output.push('\n');
        }

        // LLM Blind Analysis
        if let Some(ref description) = result.llm_description {
            let model_info = result.llm_model_used.as_ref()
                .map(|m| format!(" ({})", m))
                .unwrap_or_default();
            self.live_output.push_str(&format!("🤖 LLM BLIND ANALYSIS{}\n", model_info));
            self.live_output.push_str("──────────────────────────────────\n");
            self.live_output.push_str(&format!("  {}\n\n", description));
        }

        // Processing time
        self.live_output.push_str(&format!(
            "⏱️ Processing time: {:.1}ms\n",
            result.processing_time_ms
        ));
    }

    pub(in super::super) fn install_effect_tool(&mut self, tool: EffectDetectionTool) {
        // Stop any active recording session
        self.stop_recording();

        // Clear session state
        self.effect_detection_result = None;
        self.error = None;
        self.progress.clear();
        {
            self.waveform_peaks.clear();
            self.transcription_segments.clear();
        }

        let instructions = EffectDetectionService::install_instructions(tool);

        info!("Showing install instructions for: {:?}", tool);
        self.live_output = format!(
            "Install {} with:\n\n{}\n\nRun this in your terminal to install the Python package.",
            tool.label(),
            instructions
        );
    }
}
