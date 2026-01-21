//! Transport controls widget - Run/Cancel/spinner for benchmarks

use eframe::egui;

/// Response from transport controls
#[derive(Default)]
pub struct TransportResponse {
    /// Run button was clicked
    pub run_clicked: bool,
    /// Cancel button was clicked
    pub cancel_clicked: bool,
}

/// Transport controls widget - Run/Cancel buttons with spinner
pub struct TransportControls {
    running: bool,
    can_run: bool,
    run_label: &'static str,
    cancel_label: &'static str,
}

impl TransportControls {
    pub fn new(running: bool, can_run: bool) -> Self {
        Self {
            running,
            can_run,
            run_label: "Run Benchmark",
            cancel_label: "Cancel",
        }
    }

    pub fn run_label(mut self, label: &'static str) -> Self {
        self.run_label = label;
        self
    }

    pub fn cancel_label(mut self, label: &'static str) -> Self {
        self.cancel_label = label;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> TransportResponse {
        let mut response = TransportResponse::default();

        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.can_run, egui::Button::new(self.run_label))
                .clicked()
            {
                response.run_clicked = true;
            }

            if self.running {
                if ui.button(self.cancel_label).clicked() {
                    response.cancel_clicked = true;
                }
                ui.spinner();
            }
        });

        response
    }
}
