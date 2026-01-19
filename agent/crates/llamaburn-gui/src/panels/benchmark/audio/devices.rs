use std::collections::BTreeMap;

use eframe::egui;
use tracing::{info, warn};

use llamaburn_services::DeviceType;

use super::super::{AudioTestEvent, AudioTestState, BenchmarkPanel};

impl BenchmarkPanel {
    pub(in super::super) fn render_audio_device_menu(&mut self, ui: &mut egui::Ui) {
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

    pub(in super::super) fn refresh_audio_devices(&mut self) {
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

    pub(in super::super) fn start_audio_test(&mut self) {
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

    pub(in super::super) fn poll_audio_test(&mut self) {
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

    pub(in super::super) fn start_live_monitor(&mut self) {
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

    pub(in super::super) fn stop_live_monitor(&mut self) {
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

    pub(in super::super) fn start_level_monitor(&mut self) {
        use llamaburn_services::AudioInputService;

        // Stop existing monitor if any
        self.stop_level_monitor();

        let Some(device_id) = self.selected_device_id.clone() else {
            return;
        };

        info!("Starting input level monitor: device={}", device_id);

        let (audio_tx, audio_rx) = std::sync::mpsc::channel::<Vec<f32>>();
        let (level_tx, level_rx) = std::sync::mpsc::channel::<(f32, f32)>();
        let (waveform_tx, waveform_rx) = std::sync::mpsc::channel::<Vec<(f32, f32)>>();

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
        self.waveform_monitor_rx = Some(waveform_rx);

        // Spawn thread to calculate levels and waveform peaks
        std::thread::spawn(move || {
            let mut sample_buffer: Vec<f32> = Vec::with_capacity(3200);

            loop {
                let samples = match audio_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(s) => s,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                };

                // Calculate peak levels for VU meter
                let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                if level_tx.send((peak, peak)).is_err() {
                    break;
                }

                // Buffer samples for dense waveform peaks
                sample_buffer.extend(&samples);

                // When we have ~100ms of audio (1600 samples at 16kHz), compute peaks
                const SAMPLES_PER_BATCH: usize = 1600;
                if sample_buffer.len() < SAMPLES_PER_BATCH {
                    continue;
                }

                // Compute 40 peaks per batch = 400 peaks/second for dense waveform
                let peaks = Self::compute_waveform_peaks(&sample_buffer, 40);
                sample_buffer.clear();

                if waveform_tx.send(peaks).is_err() {
                    break;
                }
            }
        });
    }

    pub(in super::super) fn stop_level_monitor(&mut self) {
        if let Some(handle) = self.level_monitor_handle.take() {
            handle.stop();
        }
        self.level_monitor_rx = None;
        self.waveform_monitor_rx = None;
        self.input_levels = (0.0, 0.0);
    }

    pub(in super::super) fn poll_level_monitor(&mut self) {
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
            // Slow decay when no data
            self.input_levels.0 *= 0.95;
            self.input_levels.1 *= 0.95;
            return;
        };

        // Exponential smoothing: fast attack, slow release
        let attack = 0.6;  // Rise quickly to peaks
        let release = 0.15; // Fall slowly

        let smooth = |current: f32, target: f32| -> f32 {
            let factor = if target > current { attack } else { release };
            current + (target - current) * factor
        };

        self.input_levels.0 = smooth(self.input_levels.0, l);
        self.input_levels.1 = smooth(self.input_levels.1, r);

        // Receive dense waveform peaks when recording
        if !self.live_recording {
            return;
        }

        let Some(waveform_rx) = &self.waveform_monitor_rx else {
            return;
        };

        // Receive all available waveform peaks
        while let Ok(peaks) = waveform_rx.try_recv() {
            self.waveform_peaks.extend(peaks);
        }

        // Cap waveform size (~30 seconds at 400 peaks/sec)
        const MAX_PEAKS: usize = 12000;
        while self.waveform_peaks.len() > MAX_PEAKS {
            self.waveform_peaks.pop_front();
        }
    }

    pub(in super::super) fn render_level_meter(&self, ui: &mut egui::Ui) {
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

    pub(in super::super) fn check_capture_duration(&mut self) {
        // Only check if we're in capture mode (not live streaming)
        let dominated_by_capture = self.effect_detection_running
            && self.live_recording
            && self.audio_source_mode == super::super::AudioSourceMode::Capture;

        if !dominated_by_capture {
            return;
        }

        let Some(start) = self.recording_start else {
            return;
        };

        let elapsed = start.elapsed();
        let duration = std::time::Duration::from_secs(self.capture_duration_secs as u64);

        // Recording duration exceeded - stop the visual recording state
        if elapsed < duration {
            return;
        }

        // Stop level monitor but keep waveform visible
        if let Some(handle) = self.level_monitor_handle.take() {
            handle.stop();
        }
        self.live_recording = false;
        self.recording_start = None;
        self.live_output.push_str("Recording complete. Processing...\n");
    }

    pub(in super::super) fn check_playback_completion(&mut self) {
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

    pub(in super::super) fn pick_audio_file(&mut self) {
        use llamaburn_services::get_audio_duration_ms;

        let file = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "mp3", "flac", "m4a", "ogg"])
            .pick_file();

        let Some(path) = file else {
            return;
        };

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
}
