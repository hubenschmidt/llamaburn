use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use tracing::{debug, info, warn};

use llamaburn_core::BenchmarkConfig;
use llamaburn_services::{BenchmarkEvent, BenchmarkHistoryEntry, BenchmarkSummary};

use super::components::render_model_selector;
use super::BenchmarkPanel;

impl BenchmarkPanel {
    pub(super) fn render_text_config(&mut self, ui: &mut egui::Ui) {
        let disabled = self.running || self.loading_models;

        egui::Grid::new("config_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Model:");
                let resp = render_model_selector(
                    ui,
                    "text_model_select",
                    &self.models,
                    &self.selected_model,
                    self.loading_models,
                    self.model_preloading,
                    disabled,
                );
                if let Some(model) = resp.selected {
                    self.selected_model = model.clone();
                    self.model_preload_rx = Some(self.ollama.preload_model_async(&model));
                    self.model_preloading = true;
                    self.preloading_model_name = model.clone();
                    self.live_output.push_str(&format!("⏳ Loading {} into VRAM...\n", model));
                }
                if resp.unload_clicked {
                    self.unload_model();
                }
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

    pub(super) fn render_text_rankings(&self, ui: &mut egui::Ui) {
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

    pub(super) fn start_benchmark(&mut self) {
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

    pub(super) fn poll_benchmark(&mut self) {
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

    pub(super) fn save_to_history(&mut self, summary: &BenchmarkSummary) {
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

    pub(super) fn cancel_benchmark(&mut self) {
        info!("Cancelling benchmark");
        if let Some(token) = &self.cancel_token {
            token.cancel();
        }
        self.progress = "Cancelling...".to_string();
    }

    pub(super) fn refresh_rankings(&mut self) {
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

    pub(super) fn force_refresh_rankings(&mut self) {
        self.last_model_for_rankings.clear();
        self.refresh_rankings();
    }
}
