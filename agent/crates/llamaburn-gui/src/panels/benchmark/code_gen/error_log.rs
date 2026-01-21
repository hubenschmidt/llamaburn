//! Error log UI components for code benchmark panel

use eframe::egui;
use llamaburn_services::Language;

use super::CodeGenBenchmarkPanel;

/// Entry in the error log for debugging
#[derive(Debug, Clone)]
pub struct ErrorLogEntry {
    pub timestamp: std::time::Instant,
    pub model_id: String,
    pub language: Language,
    pub temperature: f32,
    pub max_tokens: u32,
    pub problem_id: String,
    pub test_num: u32,
    pub test_input: String,
    pub expected: String,
    pub actual: String,
    pub error: Option<String>,
}

impl CodeGenBenchmarkPanel {
    /// Render collapsible log panels at bottom of screen.
    /// Does not return actions - log loading is handled internally.
    pub fn render_error_log(&mut self, ui: &mut egui::Ui) {
        let mut load_entry: Option<ErrorLogEntry> = None;
        let mut clear_failures = false;
        let mut clear_errors = false;

        // Render Test Failure Log (LLM code failures - expected benchmark results)
        let failure_count = self.test_failure_log.len();
        if failure_count > 0 {
            ui.add_space(5.0);
            let header_text = format!("Test Failures ({} failures)", failure_count);
            let header = egui::CollapsingHeader::new(
                egui::RichText::new(&header_text).color(egui::Color32::from_rgb(200, 200, 100)),
            )
            .default_open(self.test_failure_log_expanded)
            .show(ui, |ui| {
                self.test_failure_log_expanded = true;
                if Self::render_log_entries(ui, &self.test_failure_log, &mut load_entry) {
                    clear_failures = true;
                }
            });
            if !header.fully_open() {
                self.test_failure_log_expanded = false;
            }
        }

        // Render Error Log (harness errors - need fixing)
        let error_count = self.error_log.len();
        if error_count > 0 {
            ui.add_space(5.0);
            let header_text = format!("Harness Errors ({} errors)", error_count);
            let header = egui::CollapsingHeader::new(
                egui::RichText::new(&header_text).color(egui::Color32::from_rgb(255, 100, 100)),
            )
            .default_open(self.error_log_expanded)
            .show(ui, |ui| {
                self.error_log_expanded = true;
                if Self::render_log_entries(ui, &self.error_log, &mut load_entry) {
                    clear_errors = true;
                }
            });
            if !header.fully_open() {
                self.error_log_expanded = false;
            }
        }

        // Handle clear buttons (deferred to avoid borrow issues)
        if clear_failures {
            self.test_failure_log.clear();
        }
        if clear_errors {
            self.error_log.clear();
        }

        // Handle Load button click
        if let Some(entry) = load_entry {
            self.load_from_history(
                entry.model_id,
                entry.language,
                entry.temperature,
                Some(entry.max_tokens),
                vec![entry.problem_id],
            );
        }
    }

    /// Helper to render log entries (used by both error log and test failure log)
    /// Returns true if Clear button was clicked
    fn render_log_entries(
        ui: &mut egui::Ui,
        entries: &[ErrorLogEntry],
        load_entry: &mut Option<ErrorLogEntry>,
    ) -> bool {
        let mut clear_clicked = false;
        ui.horizontal(|ui| {
            if ui.small_button("Clear").clicked() {
                clear_clicked = true;
            }
            if ui.small_button("Copy All").clicked() {
                let text = entries
                    .iter()
                    .map(|e| {
                        let err_str = e.error.as_deref().unwrap_or("");
                        format!(
                            "[{} | {} | T={:.1} | {}tok] {} - Test #{}\n  Input: {}\n  Expected: {}\n  Actual: {}\n  Error: {}",
                            e.model_id, e.language.label(), e.temperature, e.max_tokens,
                            e.problem_id, e.test_num, e.test_input,
                            e.expected, e.actual, err_str
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                ui.ctx().copy_text(text);
            }
        });

        ui.add_space(5.0);

        egui::ScrollArea::vertical()
            .max_height(250.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for entry in entries.iter().rev().take(50) {
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::vec2(8.0, 4.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&entry.problem_id).strong());
                                ui.label(format!("Test #{}", entry.test_num));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button("Load")
                                            .on_hover_text("Load this config into benchmark runner")
                                            .clicked()
                                        {
                                            *load_entry = Some(entry.clone());
                                        }
                                    },
                                );
                            });

                            let config_text = format!(
                                "{} | {} | T={:.1} | {}tok",
                                entry.model_id,
                                entry.language.label(),
                                entry.temperature,
                                entry.max_tokens
                            );
                            ui.label(egui::RichText::new(config_text).weak().small());

                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Input:").small());
                                let input_display = match entry.test_input.len() > 60 {
                                    true => format!("{}...", &entry.test_input[..60]),
                                    false => entry.test_input.clone(),
                                };
                                ui.label(egui::RichText::new(input_display).monospace().small());
                            });

                            ui.horizontal(|ui| {
                                ui.label("Expected:");
                                ui.label(egui::RichText::new(&entry.expected).monospace());
                            });

                            ui.horizontal(|ui| {
                                ui.label("Actual:");
                                let actual_text = match entry.actual.is_empty() {
                                    true => "(empty)",
                                    false => &entry.actual,
                                };
                                ui.label(
                                    egui::RichText::new(actual_text)
                                        .monospace()
                                        .color(egui::Color32::from_rgb(255, 100, 100)),
                                );
                            });

                            if let Some(err) = &entry.error {
                                ui.label(
                                    egui::RichText::new(err)
                                        .small()
                                        .color(egui::Color32::from_rgb(255, 150, 150)),
                                );
                            }
                        });
                    ui.add_space(2.0);
                }
            });
        clear_clicked
    }
}
