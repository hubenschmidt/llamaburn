use eframe::egui;
use llamaburn_services::{GpuMetrics, GpuMonitor};
use std::sync::mpsc::Receiver;

pub struct GpuMonitorPanel {
    metrics: GpuMetrics,
    metrics_rx: Receiver<GpuMetrics>,
}

impl GpuMonitorPanel {
    pub fn new() -> Self {
        let monitor = GpuMonitor::default();
        let metrics_rx = monitor.subscribe();

        Self {
            metrics: GpuMetrics::default(),
            metrics_rx,
        }
    }

    pub fn update(&mut self) {
        while let Ok(metrics) = self.metrics_rx.try_recv() {
            self.metrics = metrics;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.update();

        ui.horizontal(|ui| {
            ui.heading("GPU Monitor");
            let status_color = if self.metrics.connected {
                egui::Color32::GREEN
            } else {
                egui::Color32::RED
            };
            ui.colored_label(status_color, "●");
        });

        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.metrics.raw_output.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            });
    }
}

impl Default for GpuMonitorPanel {
    fn default() -> Self {
        Self::new()
    }
}
