//! Text benchmark config view - composes shared widgets

use std::sync::mpsc::Receiver;

use eframe::egui::{self, Widget};

use llamaburn_services::{ModelList, TextBenchmark};
use llamaburn_services::{BenchmarkEvent, BenchmarkService, HistoryService, OllamaClient, OllamaError};

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
}

impl Widget for ConfigView<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
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
                *self.benchmark_rx = self.service.start_text_benchmark(self.text, self.model_list);
            }
            if transport_resp.cancel_clicked {
                BenchmarkService::cancel_text_benchmark(self.text);
                *self.benchmark_rx = None;
            }

            // Poll for benchmark events
            self.service
                .poll_text_benchmark(self.text, self.benchmark_rx, self.history);
        });

        response.response
    }
}
