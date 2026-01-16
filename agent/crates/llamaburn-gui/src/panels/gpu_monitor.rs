use eframe::egui;
use llamaburn_services::{GpuMetrics, GpuMonitor};
use std::sync::mpsc::Receiver;

pub struct GpuMonitorPanel {
    metrics: GpuMetrics,
    metrics_rx: Receiver<GpuMetrics>,
    expanded: bool,
}

impl GpuMonitorPanel {
    pub fn new() -> Self {
        let monitor = GpuMonitor::default();
        let metrics_rx = monitor.subscribe();

        Self {
            metrics: GpuMetrics::default(),
            metrics_rx,
            expanded: true,
        }
    }

    pub fn update(&mut self) {
        while let Ok(metrics) = self.metrics_rx.try_recv() {
            self.metrics = metrics;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.update();

        let status_indicator = match self.metrics.connected {
            true => "🟢",
            false => "🔴",
        };
        let header_text = format!("📊 GPU Monitor {}", status_indicator);

        let header = egui::CollapsingHeader::new(
            egui::RichText::new(header_text).strong(),
        )
        .default_open(self.expanded)
        .show(ui, |ui| {
            self.expanded = true;

            egui::ScrollArea::vertical()
                .max_height(300.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.metrics.raw_output.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                });
        });

        if !header.fully_open() {
            self.expanded = false;
        }
    }
}

impl Default for GpuMonitorPanel {
    fn default() -> Self {
        Self::new()
    }
}
