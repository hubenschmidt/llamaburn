//! Text benchmark config view - composes shared widgets

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Widget};
use tracing::{info, warn};

use llamaburn_services::{
    BenchmarkEvent, BenchmarkHistoryEntry, BenchmarkService, BenchmarkType,
    HistoryService, ModelList, OllamaClient, OllamaError, TextBenchmark, TextBenchmarkResult,
};

use crate::panels::benchmark::components::{ModelSelector, TransportControls};

/// Text benchmark configuration view
pub struct ConfigView<'a> {
    text: &'a mut TextBenchmark,
    service: &'a BenchmarkService,
    model_list: &'a mut ModelList,
    benchmark_rx: &'a mut Option<Receiver<BenchmarkEvent>>,
    preload_rx: &'a mut Option<Receiver<Result<(), OllamaError>>>,
    ollama: &'a OllamaClient,
    history: &'a HistoryService,
}

impl<'a> ConfigView<'a> {
    pub fn new(
        text: &'a mut TextBenchmark,
        service: &'a BenchmarkService,
        model_list: &'a mut ModelList,
        benchmark_rx: &'a mut Option<Receiver<BenchmarkEvent>>,
        preload_rx: &'a mut Option<Receiver<Result<(), OllamaError>>>,
        ollama: &'a OllamaClient,
        history: &'a HistoryService,
    ) -> Self {
        Self {
            text,
            service,
            model_list,
            benchmark_rx,
            preload_rx,
            ollama,
            history,
        }
    }

    /// Start a text benchmark
    fn start_benchmark(&mut self) {
        self.text.start(&self.model_list.selected);
        self.text.append_output(&format!(
            "Starting text benchmark: {} iterations, {} warmup, temp={:.1}\n",
            self.text.config.iterations, self.text.config.warmup_runs, self.text.config.temperature
        ));

        let (rx, _cancel_token) = self.service.run_streaming(self.text.config.clone());
        *self.benchmark_rx = Some(rx);
    }

    /// Cancel text benchmark
    fn cancel_benchmark(&mut self) {
        self.text.stop();
        *self.benchmark_rx = None;
    }

    /// Poll and handle benchmark events
    fn poll_events(&mut self) {
        let Some(receiver) = self.benchmark_rx.take() else { return };

        loop {
            match receiver.try_recv() {
                Ok(event) => self.handle_event(event),
                Err(TryRecvError::Empty) => {
                    *self.benchmark_rx = Some(receiver);
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    self.text.stop();
                    break;
                }
            }
        }
    }

    /// Handle a single benchmark event
    fn handle_event(&mut self, event: BenchmarkEvent) {
        match event {
            BenchmarkEvent::Warmup { current, total } => {
                self.text.set_progress(format!("Warmup {}/{}", current, total));
            }
            BenchmarkEvent::Iteration { current, total, prompt: _ } => {
                self.text.set_progress(format!("Iteration {}/{}", current, total));
            }
            BenchmarkEvent::Token { content } => {
                self.text.append_output(&content);
            }
            BenchmarkEvent::IterationComplete { metrics } => {
                self.text.append_output(&format!(
                    "\n[Iteration {}] {:.2} t/s, TTFT: {:.0}ms, Total: {:.0}ms\n",
                    self.text.collected_metrics.len() + 1,
                    metrics.tokens_per_sec,
                    metrics.time_to_first_token_ms,
                    metrics.total_generation_ms
                ));
                self.text.add_metrics(metrics);
            }
            BenchmarkEvent::Done { summary } => {
                let result = TextBenchmarkResult {
                    avg_tps: summary.avg_tps,
                    avg_ttft_ms: summary.avg_ttft_ms,
                    avg_total_ms: summary.avg_total_ms,
                    min_tps: summary.min_tps,
                    max_tps: summary.max_tps,
                    iterations: self.text.config.iterations,
                };

                self.text.append_output(&format!(
                    "\n✅ Complete: {:.2} t/s avg ({:.2}-{:.2})\n",
                    result.avg_tps, result.min_tps, result.max_tps
                ));

                // Save history
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                let entry = BenchmarkHistoryEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    timestamp,
                    benchmark_type: BenchmarkType::Text,
                    model_id: self.text.config.model_id.clone(),
                    config: self.text.config.clone(),
                    summary: summary.clone(),
                    metrics: self.text.collected_metrics.clone(),
                };

                if let Err(e) = self.history.insert(&entry) {
                    warn!("Failed to save benchmark history: {}", e);
                } else {
                    info!("Saved benchmark result to history: {}", entry.id);
                }

                self.text.set_result(result);
                *self.benchmark_rx = None;
                self.text.set_progress(String::new());
            }
            BenchmarkEvent::Cancelled => {
                self.text.append_output("\n⚠️ Benchmark cancelled\n");
                self.text.stop();
                *self.benchmark_rx = None;
                self.text.set_progress(String::new());
            }
            BenchmarkEvent::Error { message } => {
                self.text.append_output(&format!("\n❌ Error: {}\n", message));
                self.text.set_error(Some(message));
                self.text.stop();
                *self.benchmark_rx = None;
                self.text.set_progress(String::new());
            }
        }
    }
}

impl Widget for ConfigView<'_> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui.vertical(|ui| {
            let disabled = self.text.running || self.model_list.loading;

            egui::Grid::new("text_config_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    // Model selector
                    ui.label("Model:");
                    let selector_resp = ModelSelector::new(self.model_list, "text_model_select")
                        .disabled(disabled)
                        .show(ui);

                    if let Some(model_name) = selector_resp.selected {
                        self.model_list.select(model_name.clone());
                        *self.preload_rx = Some(self.ollama.preload_model_async(&model_name));
                        self.model_list.start_preload(&model_name);
                        self.text
                            .append_output(&format!("Loading {} into VRAM...\n", model_name));
                    }
                    if selector_resp.unload_clicked && !self.model_list.selected.is_empty() {
                        let model_name = self.model_list.selected.clone();
                        let _ = self.ollama.unload_model_async(&model_name);
                        self.model_list.selected.clear();
                        self.text
                            .append_output(&format!("Unloading {}...\n", model_name));
                    }
                    ui.end_row();

                    // Iterations
                    ui.label("Iterations:");
                    ui.add_enabled(
                        !disabled,
                        egui::DragValue::new(&mut self.text.config.iterations).range(1..=100),
                    );
                    ui.end_row();

                    // Warmup
                    ui.label("Warmup:");
                    ui.add_enabled(
                        !disabled,
                        egui::DragValue::new(&mut self.text.config.warmup_runs).range(0..=10),
                    );
                    ui.end_row();

                    // Temperature
                    ui.label("Temperature:");
                    ui.add_enabled(
                        !disabled,
                        egui::DragValue::new(&mut self.text.config.temperature)
                            .range(0.0..=2.0)
                            .speed(0.1),
                    );
                    ui.end_row();
                });

            ui.add_space(10.0);

            // Transport controls
            let can_run = !self.text.running
                && !self.model_list.loading
                && !self.model_list.selected.is_empty();

            let transport_resp = TransportControls::new(self.text.running, can_run).show(ui);

            if transport_resp.run_clicked {
                self.start_benchmark();
            }
            if transport_resp.cancel_clicked {
                self.cancel_benchmark();
            }

            // Poll for benchmark events
            self.poll_events();
        });

        response.response
    }
}
