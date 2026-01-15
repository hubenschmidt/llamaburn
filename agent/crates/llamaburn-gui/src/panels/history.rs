use eframe::egui;
use llamaburn_core::BenchmarkType;
use llamaburn_services::{BenchmarkHistoryEntry, HistoryFilter, HistoryService};
use std::collections::HashSet;
use std::sync::Arc;

pub struct HistoryPanel {
    history_service: Arc<HistoryService>,
    entries: Vec<BenchmarkHistoryEntry>,
    filter_type: Option<BenchmarkType>,
    needs_refresh: bool,
    delete_confirm: Option<String>,
    selected_ids: HashSet<String>,
    show_comparison: bool,
}

impl HistoryPanel {
    pub fn new(history_service: Arc<HistoryService>) -> Self {
        Self {
            history_service,
            entries: Vec::new(),
            filter_type: None,
            needs_refresh: true,
            delete_confirm: None,
            selected_ids: HashSet::new(),
            show_comparison: false,
        }
    }

    fn refresh(&mut self) {
        let filter = HistoryFilter {
            benchmark_type: self.filter_type,
            limit: Some(100),
            ..Default::default()
        };

        match self.history_service.list(filter) {
            Ok(entries) => {
                self.entries = entries;
            }
            Err(e) => {
                tracing::warn!("Failed to load history: {}", e);
            }
        }
        self.needs_refresh = false;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if self.needs_refresh {
            self.refresh();
        }

        ui.label(
            egui::RichText::new("Benchmark History")
                .heading()
                .color(egui::Color32::GRAY),
        );
        ui.add_space(10.0);

        self.render_filters(ui);
        ui.add_space(10.0);

        if self.show_comparison {
            self.render_comparison(ui);
        } else {
            self.render_table(ui);
        }
    }

    fn render_filters(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Filter by type:");

            let selected_label = self
                .filter_type
                .map(|t| t.label())
                .unwrap_or("All");

            egui::ComboBox::from_id_salt("history_type_filter")
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(&mut self.filter_type, None, "All")
                        .changed()
                    {
                        self.needs_refresh = true;
                    }
                    for bt in BenchmarkType::all() {
                        if ui
                            .selectable_value(&mut self.filter_type, Some(*bt), bt.label())
                            .changed()
                        {
                            self.needs_refresh = true;
                        }
                    }
                });

            ui.add_space(20.0);

            if ui.button("Refresh").clicked() {
                self.needs_refresh = true;
            }

            if ui.button("Clear All").clicked() {
                self.delete_confirm = Some("__all__".to_string());
            }

            ui.add_space(20.0);

            let selected_count = self.selected_ids.len();
            let can_compare = selected_count >= 2;

            match self.show_comparison {
                true => {
                    if ui.button("← Back to List").clicked() {
                        self.show_comparison = false;
                    }
                }
                false => {
                    if ui
                        .add_enabled(can_compare, egui::Button::new(format!("Compare ({})", selected_count)))
                        .clicked()
                    {
                        self.show_comparison = true;
                    }

                    if selected_count > 0 && ui.button("Clear Selection").clicked() {
                        self.selected_ids.clear();
                    }
                }
            }
        });

        // Delete confirmation dialog
        if let Some(ref id) = self.delete_confirm.clone() {
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    if id == "__all__" {
                        ui.label("Delete ALL benchmark history?");
                    } else {
                        ui.label(format!("Delete entry {}?", &id[..8.min(id.len())]));
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.delete_confirm = None;
                        }
                        if ui
                            .button(egui::RichText::new("Delete").color(egui::Color32::RED))
                            .clicked()
                        {
                            if id == "__all__" {
                                if let Err(e) = self.history_service.clear_all() {
                                    tracing::warn!("Failed to clear history: {}", e);
                                }
                            } else {
                                if let Err(e) = self.history_service.delete(&id) {
                                    tracing::warn!("Failed to delete entry: {}", e);
                                }
                            }
                            self.delete_confirm = None;
                            self.needs_refresh = true;
                        }
                    });
                });
        }
    }

    fn render_table(&mut self, ui: &mut egui::Ui) {
        if self.entries.is_empty() {
            ui.label("No benchmark history yet. Run some benchmarks!");
            return;
        }

        ui.label(format!("{} entries", self.entries.len()));
        ui.add_space(5.0);

        let mut toggle_id: Option<String> = None;
        let mut delete_id: Option<String> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("history_table")
                    .num_columns(7)
                    .spacing([15.0, 8.0])
                    .striped(true)
                    .show(ui, |ui| {
                        // Header
                        ui.label(egui::RichText::new("").strong()); // Checkbox
                        ui.label(egui::RichText::new("Model").strong());
                        ui.label(egui::RichText::new("Type").strong());
                        ui.label(egui::RichText::new("Avg TPS").strong());
                        ui.label(egui::RichText::new("TTFT").strong());
                        ui.label(egui::RichText::new("Date").strong());
                        ui.label(egui::RichText::new("").strong()); // Actions
                        ui.end_row();

                        // Rows
                        for entry in &self.entries {
                            let is_selected = self.selected_ids.contains(&entry.id);
                            if ui.checkbox(&mut is_selected.clone(), "").clicked() {
                                toggle_id = Some(entry.id.clone());
                            }

                            ui.label(&entry.model_id);
                            ui.label(entry.benchmark_type.label());
                            ui.label(format!("{:.1}", entry.summary.avg_tps));
                            ui.label(format!("{:.0}ms", entry.summary.avg_ttft_ms));
                            ui.label(format_timestamp(entry.timestamp));

                            if ui.small_button("🗑").clicked() {
                                delete_id = Some(entry.id.clone());
                            }
                            ui.end_row();
                        }
                    });
            });

        // Handle toggle outside the borrow
        if let Some(id) = toggle_id {
            if self.selected_ids.contains(&id) {
                self.selected_ids.remove(&id);
            } else {
                self.selected_ids.insert(id);
            }
        }

        if let Some(id) = delete_id {
            self.delete_confirm = Some(id);
        }
    }

    fn render_comparison(&self, ui: &mut egui::Ui) {
        let selected_entries: Vec<&BenchmarkHistoryEntry> = self
            .entries
            .iter()
            .filter(|e| self.selected_ids.contains(&e.id))
            .collect();

        if selected_entries.len() < 2 {
            ui.label("Select at least 2 entries to compare");
            return;
        }

        // Header with model names
        let model_names: Vec<&str> = selected_entries.iter().map(|e| e.model_id.as_str()).collect();
        ui.label(
            egui::RichText::new(format!("Comparison: {}", model_names.join(" vs ")))
                .heading()
                .color(egui::Color32::GRAY),
        );
        ui.add_space(10.0);

        egui::Grid::new("comparison_table")
            .num_columns(selected_entries.len() + 2)
            .spacing([20.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                // Header row
                ui.label(egui::RichText::new("Metric").strong());
                for entry in &selected_entries {
                    ui.label(egui::RichText::new(&entry.model_id).strong());
                }
                ui.label(egui::RichText::new("Best").strong());
                ui.end_row();

                // Avg TPS (higher is better)
                self.render_metric_row(
                    ui, &selected_entries, "Avg TPS",
                    |e| e.summary.avg_tps, |v| format!("{:.1}", v), true,
                );

                // Avg TTFT (lower is better)
                self.render_metric_row(
                    ui, &selected_entries, "Avg TTFT",
                    |e| e.summary.avg_ttft_ms, |v| format!("{:.0}ms", v), false,
                );

                // Min TPS (higher is better)
                self.render_metric_row(
                    ui, &selected_entries, "Min TPS",
                    |e| e.summary.min_tps, |v| format!("{:.1}", v), true,
                );

                // Max TPS (higher is better)
                self.render_metric_row(
                    ui, &selected_entries, "Max TPS",
                    |e| e.summary.max_tps, |v| format!("{:.1}", v), true,
                );

                // Iterations row
                ui.label("Iterations");
                for entry in &selected_entries {
                    ui.label(format!("{}", entry.summary.iterations));
                }
                ui.label("");
                ui.end_row();
            });
    }

    fn render_metric_row<F, G>(
        &self,
        ui: &mut egui::Ui,
        entries: &[&BenchmarkHistoryEntry],
        label: &str,
        get_value: F,
        format_value: G,
        higher_is_better: bool,
    ) where
        F: Fn(&BenchmarkHistoryEntry) -> f64,
        G: Fn(f64) -> String,
    {
        ui.label(label);

        let values: Vec<f64> = entries.iter().map(|e| get_value(e)).collect();
        let best = match higher_is_better {
            true => values.iter().cloned().fold(f64::MIN, f64::max),
            false => values.iter().cloned().fold(f64::MAX, f64::min),
        };

        for entry in entries {
            let value = get_value(entry);
            let is_best = (value - best).abs() < 0.01;
            let text = format_value(value);
            let label_text = match is_best {
                true => egui::RichText::new(text).strong().color(egui::Color32::GREEN),
                false => egui::RichText::new(text),
            };
            ui.label(label_text);
        }

        ui.label(format_value(best));
        ui.end_row();
    }
}

fn format_timestamp(ts: i64) -> String {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let entry_time = UNIX_EPOCH + Duration::from_secs(ts as u64);
    let now = SystemTime::now();

    let Ok(elapsed) = now.duration_since(entry_time) else {
        return "Future".to_string();
    };

    let secs = elapsed.as_secs();

    if secs < 60 {
        return "Just now".to_string();
    }
    if secs < 3600 {
        return format!("{}m ago", secs / 60);
    }
    if secs < 86400 {
        return format!("{}h ago", secs / 3600);
    }
    if secs < 604800 {
        return format!("{}d ago", secs / 86400);
    }

    // Fall back to date
    let days = secs / 86400;
    format!("{}d ago", days)
}
