mod audio;
mod code_gen;
mod components;
mod text;

use std::sync::mpsc::Receiver;
use std::sync::Arc;

use eframe::egui;
use llamaburn_services::AppModels;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use llamaburn_services::{
    BenchmarkEvent, BenchmarkType, ModelInfo, OllamaError, Services,
};

// Re-export panel types
pub use audio::AudioBenchmarkPanel;
pub use code_gen::{CodeGenAction, CodeGenBenchmarkPanel, CodeGenRenderContext};

pub struct BenchmarkPanel {
    // =========================================
    // Async Receivers (can't be in services - mpsc::Receiver)
    // =========================================
    model_rx: Option<Receiver<Result<Vec<String>, OllamaError>>>,
    model_preload_rx: Option<Receiver<Result<(), OllamaError>>>,
    model_info_rx: Option<Receiver<Option<ModelInfo>>>,
    text_rx: Option<Receiver<BenchmarkEvent>>,

    // Legacy field (async cancellation)
    cancel_token: Option<Arc<CancellationToken>>,

    // Sub-panels
    audio_panel: AudioBenchmarkPanel,
    code_panel: CodeGenBenchmarkPanel,

    // =========================================
    // UI State
    // =========================================
    benchmark_type: BenchmarkType,
    config_panel_expanded: bool,
    config_panel_height: f32,
    live_output_expanded: bool,
    live_output_height: f32,
}

impl BenchmarkPanel {
    /// Create a new BenchmarkPanel with services reference.
    /// Models are passed to ui() each frame instead of stored.
    pub fn new(services: &Services) -> Self {
        // Start loading models
        let model_rx = Some(services.ollama.fetch_models_async());

        // Check for incomplete batch sessions on startup
        let pending_resume_batches = services
            .history
            .get_incomplete_batches()
            .unwrap_or_else(|e| {
                warn!("Failed to load incomplete batches: {}", e);
                vec![]
            });

        // Load presets on startup
        let presets = services.history.list_presets().unwrap_or_else(|e| {
            warn!("Failed to load presets: {}", e);
            vec![]
        });

        let mut code_panel = CodeGenBenchmarkPanel::new();
        code_panel.pending_resume_batches = pending_resume_batches;
        code_panel.set_presets(presets);

        Self {
            // Async receivers
            model_rx,
            model_preload_rx: None,
            model_info_rx: None,
            text_rx: None,

            // Legacy field (async cancellation)
            cancel_token: None,

            // Sub-panels
            audio_panel: AudioBenchmarkPanel::new(),
            code_panel,

            // UI state
            benchmark_type: BenchmarkType::default(),
            config_panel_expanded: true,
            config_panel_height: 280.0,
            live_output_expanded: true,
            live_output_height: 2000.0,
        }
    }

    /// Set the benchmark type (Text, Audio, Code, etc.)
    pub fn set_benchmark_type(&mut self, bt: BenchmarkType) {
        self.benchmark_type = bt;
    }

    /// Load code benchmark params from history
    pub fn load_code_from_history(
        &mut self,
        model_id: String,
        language: llamaburn_services::Language,
        temperature: f32,
        max_tokens: Option<u32>,
        problem_ids: Vec<String>,
    ) {
        self.benchmark_type = BenchmarkType::Code;
        self.code_panel
            .load_from_history(model_id, language, temperature, max_tokens, problem_ids);
    }

    /// Load presets from database
    fn load_presets(&mut self, services: &Services) {
        match services.history.list_presets() {
            Ok(presets) => self.code_panel.set_presets(presets),
            Err(e) => warn!("Failed to load presets: {}", e),
        }
    }

    /// Get live output for current benchmark type
    fn current_live_output(&self, app_models: &AppModels, services: &Services) -> String {
        match self.benchmark_type {
            BenchmarkType::Text => services.get_text_output(app_models),
            BenchmarkType::Audio => services.get_audio_output(app_models),
            BenchmarkType::Code => services.get_code_output(app_models),
            _ => String::new(), // Not implemented yet
        }
    }

    /// Get progress for current benchmark type
    fn current_progress(&self, app_models: &AppModels, services: &Services) -> String {
        match self.benchmark_type {
            BenchmarkType::Text => services.get_text_progress(app_models),
            BenchmarkType::Audio => services.get_audio_progress(app_models),
            BenchmarkType::Code => services.get_code_progress(app_models),
            _ => String::new(), // Not implemented yet
        }
    }

    /// Clear output for current benchmark type
    fn clear_current_output(&self, app_models: &mut AppModels, services: &Services) {
        match self.benchmark_type {
            BenchmarkType::Text => services.clear_text_output(app_models),
            BenchmarkType::Audio => services.clear_audio_output(app_models),
            BenchmarkType::Code => services.clear_code_output(app_models),
            _ => {} // Not implemented yet
        }
    }

    fn refresh_models(&mut self, app_models: &mut AppModels, services: &Services) {
        services.start_loading_models(app_models);
        self.model_rx = Some(services.ollama.fetch_models_async());
    }

    fn poll_models(&mut self, app_models: &mut AppModels, services: &Services) {
        let Some(rx) = &self.model_rx else { return };

        if let Ok(result) = rx.try_recv() {
            match result {
                Ok(models) => {
                    services.set_models(app_models, models);
                    // Clear error on current benchmark type
                    match self.benchmark_type {
                        BenchmarkType::Text => services.set_text_error(app_models, None),
                        BenchmarkType::Audio => services.set_audio_error(app_models, None),
                        BenchmarkType::Code => services.set_code_error(app_models, None),
                        _ => {}
                    }
                }
                Err(e) => {
                    services.set_loading(app_models, false);
                    let err_msg = Some(e.to_string());
                    match self.benchmark_type {
                        BenchmarkType::Text => services.set_text_error(app_models, err_msg.clone()),
                        BenchmarkType::Audio => services.set_audio_error(app_models, err_msg.clone()),
                        BenchmarkType::Code => services.set_code_error(app_models, err_msg),
                        _ => {}
                    }
                }
            }
        }
    }

    fn poll_model_preload(&mut self, app_models: &mut AppModels, services: &Services) {
        let Some(rx) = self.model_preload_rx.take() else {
            return;
        };

        match rx.try_recv() {
            Ok(Ok(())) => {
                let preloading_name = services.get_preloading_name(app_models);
                services.finish_preload(app_models);
                let msg = format!("✅ {} loaded into VRAM\n", preloading_name);
                match self.benchmark_type {
                    BenchmarkType::Text => services.append_text_output(app_models, &msg),
                    BenchmarkType::Audio => services.append_audio_output(app_models, &msg),
                    BenchmarkType::Code => services.append_code_output(app_models, &msg),
                    _ => {}
                }
                self.maybe_auto_start_combo(app_models, services);
            }
            Ok(Err(e)) => {
                let preloading_name = services.get_preloading_name(app_models);
                services.finish_preload(app_models);
                let msg = format!("❌ Failed to load {}: {}\n", preloading_name, e);
                match self.benchmark_type {
                    BenchmarkType::Text => services.append_text_output(app_models, &msg),
                    BenchmarkType::Audio => services.append_audio_output(app_models, &msg),
                    BenchmarkType::Code => services.append_code_output(app_models, &msg),
                    _ => {}
                }
                self.maybe_skip_to_next_combo(app_models, services);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.model_preload_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let preloading_name = services.get_preloading_name(app_models);
                services.finish_preload(app_models);
                let msg = format!("❌ Model preload disconnected for {}\n", preloading_name);
                match self.benchmark_type {
                    BenchmarkType::Text => services.append_text_output(app_models, &msg),
                    BenchmarkType::Audio => services.append_audio_output(app_models, &msg),
                    BenchmarkType::Code => services.append_code_output(app_models, &msg),
                    _ => {}
                }
                self.maybe_skip_to_next_combo(app_models, services);
            }
        }
    }

    fn poll_model_info(&mut self, app_models: &mut AppModels, services: &Services) {
        let Some(rx) = self.model_info_rx.take() else {
            return;
        };

        match rx.try_recv() {
            Ok(info) => {
                let core_info = info.map(|i| llamaburn_services::CoreModelInfo {
                    config: llamaburn_services::ModelConfig {
                        id: i.model_id.clone(),
                        name: i.model_id.clone(),
                        model: i.model_id,
                        api_base: None,
                        quantization: i.quantization,
                    },
                    size_bytes: None,
                    parameter_count: i.parameter_size,
                    context_length: None,
                });
                services.set_model_info(app_models, core_info);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.model_info_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
        }
    }

    fn maybe_auto_start_combo(&mut self, app_models: &mut AppModels, services: &Services) {
        if self.code_panel.current_combo.is_none() {
            return;
        }
        if self.code_panel.selected_problem_ids.is_empty() {
            return;
        }
        let actions = self.code_panel.run_current(services.ollama.host());
        self.process_code_actions(actions, app_models, services);
    }

    fn maybe_skip_to_next_combo(&mut self, app_models: &mut AppModels, services: &Services) {
        if self.code_panel.combo_queue.is_empty() {
            return;
        }
        self.code_panel.queue_completed += 1;
        let actions = self.code_panel.advance_to_next();
        self.process_code_actions(actions, app_models, services);
    }

    // =========================================================================
    // Action Processing (Reducer pattern)
    // =========================================================================

    /// Process actions from CodeGenBenchmarkPanel (like a Redux reducer)
    fn process_code_actions(&mut self, actions: Vec<CodeGenAction>, app_models: &mut AppModels, services: &Services) {
        for action in actions {
            match action {
                CodeGenAction::AppendOutput(s) => {
                    services.append_code_output(app_models, &s);
                }
                CodeGenAction::SetProgress(s) => {
                    services.set_code_progress(app_models, s);
                }
                CodeGenAction::SetError(e) => {
                    services.set_code_error(app_models, e);
                }
                CodeGenAction::SaveCodeHistory(entry) => {
                    if let Err(e) = services.history.insert_code(&entry) {
                        warn!("Failed to save code history: {}", e);
                    } else {
                        info!("Saved code benchmark result: {}", entry.id);
                    }
                }
                CodeGenAction::SaveFailedHistory {
                    error_message,
                    status,
                } => {
                    if let Some(entry) =
                        self.code_panel.build_failed_history_entry(&error_message, status)
                    {
                        if let Err(e) = services.history.insert_code(&entry) {
                            warn!("Failed to save failed history: {}", e);
                        } else {
                            info!(
                                "Saved {} entry: {} - {}",
                                status.as_str(),
                                entry.id,
                                error_message
                            );
                        }
                    }
                }
                CodeGenAction::InsertBatch(batch) => {
                    if let Err(e) = services.history.insert_batch(&batch) {
                        warn!("Failed to insert batch: {}", e);
                    }
                }
                CodeGenAction::UpdateBatch(batch) => {
                    if let Err(e) = services.history.update_batch(&batch) {
                        warn!("Failed to update batch: {}", e);
                    }
                }
                CodeGenAction::DeleteBatch(session_id) => {
                    if let Err(e) = services.history.delete_batch(&session_id) {
                        warn!("Failed to delete batch: {}", e);
                    }
                }
                CodeGenAction::InsertPreset(preset) => {
                    if let Err(e) = services.history.insert_preset(&preset) {
                        warn!("Failed to insert preset: {}", e);
                    } else {
                        info!("Saved preset: {}", preset.name);
                    }
                }
                CodeGenAction::DeletePreset(preset_id) => {
                    if let Err(e) = services.history.delete_preset(&preset_id) {
                        warn!("Failed to delete preset: {}", e);
                    }
                }
                CodeGenAction::LoadPresets => {
                    self.load_presets(services);
                }
                CodeGenAction::AdvanceToNextCombo => {
                    let next_actions = self.code_panel.advance_to_next();
                    self.process_code_actions(next_actions, app_models, services);
                }
                CodeGenAction::RunCurrentCombo => {
                    let run_actions = self.code_panel.run_current(services.ollama.host());
                    self.process_code_actions(run_actions, app_models, services);
                }
                CodeGenAction::RefreshModels => {
                    self.refresh_models(app_models, services);
                }
                CodeGenAction::RefreshRankings => {
                    self.code_panel.force_refresh_rankings(&mut app_models.code, &services.history);
                }
                CodeGenAction::PreloadModel(model_name) => {
                    services.start_preload(app_models, &model_name);
                    self.model_preload_rx = Some(services.ollama.preload_model_async(&model_name));
                }
                CodeGenAction::SetSelectedModel(model_name) => {
                    services.select_model(app_models, model_name);
                }
                CodeGenAction::SetCancelToken(token) => {
                    self.cancel_token = Some(token);
                }
                CodeGenAction::ClearCancelToken => {
                    if let Some(token) = self.cancel_token.take() {
                        token.cancel();
                    }
                }
            }
        }
    }

    /// Process actions from AudioBenchmarkPanel (like a Redux reducer)
    fn process_audio_actions(&mut self, actions: Vec<audio::AudioAction>, app_models: &mut AppModels, services: &Services) {
        for action in actions {
            match action {
                audio::AudioAction::AppendOutput(s) => {
                    services.append_audio_output(app_models, &s);
                }
                audio::AudioAction::ClearOutput => {
                    services.clear_audio_output(app_models);
                }
                audio::AudioAction::SetProgress(s) => {
                    services.set_audio_progress(app_models, s);
                }
                audio::AudioAction::SetError(e) => {
                    services.set_audio_error(app_models, e);
                }
                audio::AudioAction::SaveHistory(entry) => {
                    if let Err(e) = services.history.insert_audio(&entry) {
                        warn!("Failed to save audio benchmark history: {}", e);
                    } else {
                        info!("Saved audio benchmark result to history: {}", entry.id);
                    }
                }
                audio::AudioAction::RefreshRankings => {
                    services
                        .benchmark
                        .force_refresh_audio_rankings(&mut app_models.audio, &services.history);
                }
                audio::AudioAction::RefreshModels => {
                    self.refresh_models(app_models, services);
                }
                audio::AudioAction::PreloadLlmModel(model_name) => {
                    services.start_preload(app_models, &model_name);
                    self.model_preload_rx = Some(services.ollama.preload_model_async(&model_name));
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, app_models: &mut AppModels, services: &Services) {
        // Poll for updates
        self.poll_models(app_models, services);
        self.poll_model_preload(app_models, services);
        self.poll_model_info(app_models, services);

        // Poll code panel and process actions
        let code_actions = self.code_panel.poll(&mut app_models.code);
        self.process_code_actions(code_actions, app_models, services);

        // Audio panel polling - process actions
        let audio_actions = self.audio_panel.poll_audio_benchmark();
        self.process_audio_actions(audio_actions, app_models, services);

        let fx_actions = self.audio_panel.poll_effect_detection();
        self.process_audio_actions(fx_actions, app_models, services);

        let live_actions = self.audio_panel.poll_live_transcription();
        self.process_audio_actions(live_actions, app_models, services);

        self.audio_panel.poll_effect_tool_availability();
        self.audio_panel.poll_audio_test(&mut app_models.audio.error);
        self.audio_panel.check_playback_completion();
        self.audio_panel.poll_level_monitor();
        self.audio_panel.check_capture_duration(&mut app_models.audio.live_output);

        // Model info polling (audio only)
        self.audio_panel.poll_model_info();

        // Sync audio model with audio_panel state (until fully migrated)
        app_models.audio.whisper_model = self.audio_panel.whisper_model;

        // Rankings refresh
        services
            .benchmark
            .maybe_refresh_audio_rankings(&mut app_models.audio, &services.history);
        self.code_panel.refresh_rankings(&mut app_models.code, &services.history);

        // Audio model info refresh
        self.audio_panel.refresh_model_info();

        // Collapsible config panel
        let config_panel_expanded = self.config_panel_expanded;
        let config_panel_height = self.config_panel_height;
        let benchmark_type = self.benchmark_type;

        let config_header = egui::CollapsingHeader::new(
            egui::RichText::new("⚙️ Benchmark Runner").strong(),
        )
        .default_open(config_panel_expanded)
        .show(ui, |ui| {
            self.config_panel_expanded = true;

            self.render_type_selector(ui, app_models, services);
            ui.add_space(10.0);

            // Scrollable config area
            let panel_height = config_panel_height.clamp(100.0, 1000.0);
            egui::ScrollArea::vertical()
                .max_height(panel_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_height(panel_height - 20.0);

                    let available = ui.available_width();
                    let spacing = 15.0;
                    let separator_width = 10.0;
                    let total_spacing = (spacing * 4.0) + (separator_width * 2.0);
                    let content_width = (available - total_spacing).max(300.0);

                    let config_width = content_width * 0.35;
                    let info_width = content_width * 0.30;
                    let results_width = content_width * 0.35;

                    let column_height = panel_height - 30.0;
                    ui.horizontal(|ui| {
                        // Left: Config
                        ui.vertical(|ui| {
                            ui.set_width(config_width);
                            ui.set_min_height(column_height);
                            self.render_config(ui, app_models, services);
                        });

                        ui.add_space(spacing);
                        ui.separator();
                        ui.add_space(spacing);

                        // Center: Model Info
                        ui.vertical(|ui| {
                            ui.set_width(info_width);
                            ui.set_min_height(column_height);
                            self.render_model_info(ui, app_models, services);

                            if benchmark_type == BenchmarkType::Audio {
                                ui.add_space(10.0);
                                ui.separator();
                                ui.add_space(5.0);
                                self.audio_panel
                                    .render_model_downloads(ui, &mut app_models.audio.live_output);
                            }
                        });

                        ui.add_space(spacing);
                        ui.separator();
                        ui.add_space(spacing);

                        // Right: Results
                        ui.vertical(|ui| {
                            ui.set_width(results_width);
                            ui.set_min_height(column_height);
                            self.render_results(ui, app_models, services);
                        });
                    });
                });

            // Resize handle
            self.render_config_resize_handle(ui);

            ui.add_space(8.0);
        });

        if !config_header.fully_open() {
            self.config_panel_expanded = false;
        }

        // Show error for current benchmark type
        let err_msg = match self.benchmark_type {
            BenchmarkType::Text => services.get_text_error(app_models),
            BenchmarkType::Audio => services.get_audio_error(app_models),
            BenchmarkType::Code => services.get_code_error(app_models),
            _ => None,
        };
        if let Some(err) = err_msg {
            ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
            ui.add_space(5.0);
        }

        // Waveform display
        if self.audio_panel.live_recording || !self.audio_panel.waveform_peaks.is_empty() {
            self.audio_panel.render_waveform_display(ui);
            ui.add_space(5.0);
        }

        // Reserve space for effects rack and logs
        let effects_rack_height = {
            let is_audio = self.benchmark_type == BenchmarkType::Audio;
            let heights = [0.0, [40.0, 230.0][self.audio_panel.effects_rack_expanded as usize]];
            heights[is_audio as usize]
        };

        let log_height = match self.benchmark_type {
            BenchmarkType::Code => {
                let mut height = 0.0;
                if !self.code_panel.test_failure_log.is_empty() {
                    height += if self.code_panel.test_failure_log_expanded {
                        300.0
                    } else {
                        30.0
                    };
                }
                if !self.code_panel.error_log.is_empty() {
                    height += if self.code_panel.error_log_expanded {
                        300.0
                    } else {
                        30.0
                    };
                }
                height
            }
            _ => 0.0,
        };

        self.render_live_output_with_reserved(ui, app_models, services, effects_rack_height + log_height);

        // Error log panel (Code mode only)
        if self.benchmark_type == BenchmarkType::Code {
            self.code_panel.render_error_log(ui);
        }

        // Effects rack panel (Audio mode only)
        if self.benchmark_type == BenchmarkType::Audio {
            ui.add_space(10.0);
            self.audio_panel.render_effects_rack(ui);
        }

        // Audio settings dialog
        self.audio_panel
            .render_audio_settings_dialog(ui.ctx(), &mut app_models.audio.error);
    }

    fn render_config_resize_handle(&mut self, ui: &mut egui::Ui) {
        let resize_id = ui.id().with("config_panel_resize");
        let resize_rect = ui.available_rect_before_wrap();
        let handle_rect = egui::Rect::from_min_size(
            egui::pos2(resize_rect.left(), resize_rect.top()),
            egui::vec2(resize_rect.width(), 10.0),
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
    }

    fn render_type_selector(&mut self, ui: &mut egui::Ui, app_models: &mut AppModels, services: &Services) {
        ui.horizontal(|ui| {
            for bt in BenchmarkType::all() {
                let selected = self.benchmark_type == *bt;
                let running = self.audio_panel.running || self.code_panel.running;
                let enabled = bt.is_implemented() && !running;

                let response =
                    ui.add_enabled(enabled, egui::SelectableLabel::new(selected, bt.label()));

                if response.clicked() && self.benchmark_type != *bt {
                    self.benchmark_type = *bt;
                    // Clear model info when switching tabs
                    services.clear_model_info(app_models);
                    services.clear_text_state(app_models);
                    self.audio_panel.model_info = None;
                    self.audio_panel.last_model_for_info = None;
                    self.audio_panel.model_best_rtf = None;
                    self.audio_panel.all_time_best_audio = None;
                    self.audio_panel.audio_leaderboard.clear();
                    self.audio_panel.last_whisper_model_for_rankings = None;
                }
            }
        });
    }

    fn render_config(&mut self, ui: &mut egui::Ui, app_models: &mut AppModels, services: &Services) {
        match self.benchmark_type {
            BenchmarkType::Audio => {
                let actions = {
                    let llamaburn_services::AppModels { models: model_list, audio, .. } = app_models;
                    let mut shared = audio::AudioSharedState {
                        model_list,
                        audio,
                        ollama: &services.ollama,
                        history_service: &services.history,
                    };
                    self.audio_panel.render_config(ui, &mut shared)
                };
                self.process_audio_actions(actions, app_models, services);
            }
            BenchmarkType::Code => {
                let actions = {
                    let ctx = CodeGenRenderContext {
                        model_list: &app_models.models,
                    };
                    self.code_panel.render_config(ui, &ctx)
                };
                self.process_code_actions(actions, app_models, services);
            }
            _ => {
                // Text: MVC pattern
                let llamaburn_services::AppModels { text, models, .. } = app_models;
                ui.add(text::ConfigView::new(
                    text,
                    &services.benchmark,
                    models,
                    &mut self.text_rx,
                    &mut self.model_preload_rx,
                    &mut self.model_info_rx,
                    &services.ollama,
                    &services.model_info,
                    &services.history,
                ));
            }
        }
    }

    fn render_live_output_with_reserved(&mut self, ui: &mut egui::Ui, app_models: &mut AppModels, services: &Services, reserved_height: f32) {
        let progress = self.current_progress(app_models, services);
        let header_text = match progress.is_empty() {
            true => "📋 Live Output".to_string(),
            false => format!("📋 Live Output — {}", progress),
        };

        let output = self.current_live_output(app_models, services);
        let should_clear = ui.horizontal(|ui| {
            let toggle = ui.selectable_label(
                false,
                egui::RichText::new(if self.live_output_expanded {
                    "▼"
                } else {
                    "▶"
                }),
            );
            if toggle.clicked() {
                self.live_output_expanded = !self.live_output_expanded;
            }

            ui.strong(&header_text);

            ui.add_space(10.0);
            let clear = ui.small_button("Clear").clicked();
            if ui.small_button("Export").clicked() && !output.is_empty() {
                self.export_live_output(app_models, services);
            }
            clear
        }).inner;

        if should_clear {
            self.clear_current_output(app_models, services);
        }

        if !self.live_output_expanded {
            return;
        }

        let available_height = (ui.available_height() - reserved_height - 20.0).max(100.0);
        let content_height = self.live_output_height.min(available_height).max(80.0);

        let text_width = ui.clip_rect().width().min(ui.available_width()) - 30.0;
        egui::ScrollArea::vertical()
            .max_height(content_height)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut output.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(text_width)
                        .layouter(&mut |ui, string, wrap_width| {
                            let mut layout_job = egui::text::LayoutJob::default();
                            layout_job.wrap = egui::text::TextWrapping {
                                max_width: wrap_width,
                                ..Default::default()
                            };

                            let normal_color = ui.visuals().text_color();
                            let error_color = egui::Color32::from_rgb(255, 100, 100);
                            let font = egui::FontId::monospace(12.0);

                            for line in string.split_inclusive('\n') {
                                let is_error = line.contains("Error:")
                                    || line.starts_with("error")
                                    || line.contains("TypeError")
                                    || line.contains("SyntaxError")
                                    || line.contains("panic!");
                                let color = if is_error { error_color } else { normal_color };
                                layout_job.append(
                                    line,
                                    0.0,
                                    egui::text::TextFormat {
                                        font_id: font.clone(),
                                        color,
                                        ..Default::default()
                                    },
                                );
                            }

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
            self.config_panel_height -= delta;
            self.config_panel_height = self.config_panel_height.clamp(100.0, 1000.0);
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
    }

    fn render_results(&self, ui: &mut egui::Ui, app_models: &AppModels, services: &Services) {
        ui.label(
            egui::RichText::new("Results")
                .heading()
                .color(egui::Color32::GRAY),
        );

        match self.benchmark_type {
            BenchmarkType::Code => {
                self.code_panel.render_code_results(ui, &app_models.code);
            }
            BenchmarkType::Text => {
                if let Some(r) = services.get_text_result(app_models) {
                    ui.label(format!("Avg TPS: {:.2} t/s", r.avg_tps));
                    ui.label(format!("Avg TTFT: {:.2} ms", r.avg_ttft_ms));
                    ui.label(format!("Avg Total: {:.2} ms", r.avg_total_ms));
                    ui.label(format!("Min/Max TPS: {:.1}/{:.1}", r.min_tps, r.max_tps));
                    ui.label(format!("Iterations: {}", r.iterations));
                }
            }
            _ => {}
        }

        self.render_rankings(ui, app_models);
    }

    #[allow(dead_code)]
    fn unload_model(&mut self, app_models: &mut AppModels, services: &Services) {
        let model = services.get_selected_model(app_models);
        if model.is_empty() {
            return;
        }

        info!("Unloading model: {}", model);

        let _ = services.ollama.unload_model_async(&model);

        services.clear_selected_model(app_models);
        services.clear_model_info(app_models);
        services.clear_text_state(app_models);
    }

    fn render_model_info(&self, ui: &mut egui::Ui, app_models: &AppModels, services: &Services) {
        ui.label(
            egui::RichText::new("Model Info")
                .heading()
                .color(egui::Color32::GRAY),
        );
        ui.add_space(5.0);

        match self.benchmark_type {
            BenchmarkType::Audio => {
                let Some(info) = self.audio_panel.model_info.as_ref() else {
                    ui.label("Select a model to view details");
                    return;
                };

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
                if let Some(url) = info.hf_url() {
                    ui.add_space(5.0);
                    if ui.link("View on HuggingFace").clicked() {
                        let _ = open::that(&url);
                    }
                }
            }
            BenchmarkType::Text => {
                let Some(info) = services.get_model_info(app_models) else {
                    ui.label("Select a model to view details");
                    return;
                };

                ui.label(egui::RichText::new("Ollama").strong());
                if let Some(size) = &info.parameter_count {
                    ui.label(format!("Size: {}", size));
                }
                if let Some(quant) = &info.config.quantization {
                    ui.label(format!("Quant: {}", quant));
                }
            }
            _ => {
                let Some(info) = services.get_model_info(app_models) else {
                    ui.label("Select a model to view details");
                    return;
                };

                ui.label(egui::RichText::new("Ollama").strong());
                if let Some(size) = &info.parameter_count {
                    ui.label(format!("Size: {}", size));
                }
                if let Some(quant) = &info.config.quantization {
                    ui.label(format!("Quant: {}", quant));
                }
            }
        }
    }

    fn render_rankings(&self, ui: &mut egui::Ui, app_models: &AppModels) {
        ui.add_space(15.0);
        ui.label(
            egui::RichText::new("Rankings")
                .heading()
                .color(egui::Color32::GRAY),
        );

        match self.benchmark_type {
            BenchmarkType::Text => { ui.add(text::RankingsView::new(&app_models.text)); }
            BenchmarkType::Audio => audio::render_audio_rankings(&app_models.audio, ui),
            BenchmarkType::Code => self.code_panel.render_code_rankings(ui, &app_models.code),
            _ => {}
        }
    }

    fn export_live_output(&self, app_models: &AppModels, services: &Services) {
        let content = self.current_live_output(app_models, services);
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("Export Live Output")
                .add_filter("Text Files", &["txt"])
                .set_file_name("benchmark_output.txt")
                .save_file();
            let Some(path) = path else { return };
            if let Err(e) = std::fs::write(&path, &content) {
                tracing::error!("Failed to export live output: {}", e);
            }
        });
    }
}

fn format_number(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{:.1}K", n as f64 / 1_000.0),
        _ => n.to_string(),
    }
}
