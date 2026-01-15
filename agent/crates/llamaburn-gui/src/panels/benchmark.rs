use eframe::egui;
use llamaburn_core::{BenchmarkConfig, BenchmarkMetrics, BenchmarkType};
use llamaburn_services::{
    BenchmarkEvent, BenchmarkHistoryEntry, BenchmarkService, BenchmarkSummary, HistoryService,
    ModelInfo, ModelInfoService, OllamaClient, OllamaError,
};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

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

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll_models();
        self.poll_benchmark();
        self.poll_model_info();
        self.refresh_rankings();
        self.refresh_model_info();

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

            // Center: Model Info
            ui.vertical(|ui| {
                ui.set_width(info_width);
                self.render_model_info(ui);
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

        ui.add_space(10.0);

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::RED, format!("Error: {}", err));
            ui.add_space(10.0);
        }

        // Live output takes remaining space
        self.render_live_output(ui);
    }

    fn render_type_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for bt in BenchmarkType::all() {
                let selected = self.benchmark_type == *bt;
                let enabled = bt.is_implemented() && !self.running;

                let response =
                    ui.add_enabled(enabled, egui::SelectableLabel::new(selected, bt.label()));

                if response.clicked() {
                    self.benchmark_type = *bt;
                }
            }
        });
    }

    fn render_config(&mut self, ui: &mut egui::Ui) {
        let disabled = self.running || self.loading_models;

        egui::Grid::new("config_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Model:");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!disabled, |ui| {
                        let selected_text = if self.loading_models {
                            "Loading models..."
                        } else if self.models.is_empty() {
                            "No models found"
                        } else if self.selected_model.is_empty() {
                            "Select model..."
                        } else {
                            &self.selected_model
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

    fn render_live_output(&self, ui: &mut egui::Ui) {
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

        // Use all remaining vertical space
        let available_height = ui.available_height() - 10.0;
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

        // Ollama metadata - compact vertical layout
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

        // HuggingFace metadata (if available)
        let has_hf = info.hf_repo.is_some();
        if !has_hf {
            return;
        }

        ui.add_space(10.0);
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
}

fn format_number(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{:.1}K", n as f64 / 1_000.0),
        _ => n.to_string(),
    }
}
