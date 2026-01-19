use eframe::egui;
use llamaburn_core::{BenchmarkType, Language};
use llamaburn_services::{AudioHistoryEntry, BenchmarkHistoryEntry, CodeHistoryEntry, HistoryFilter, HistoryService};
use std::collections::HashSet;
use std::sync::Arc;

/// Request to load benchmark params from history
#[derive(Clone)]
pub struct LoadCodeBenchmarkRequest {
    pub model_id: String,
    pub language: Language,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub problem_ids: Vec<String>,
}

/// Unified history entry for display
#[derive(Clone)]
pub enum HistoryEntry {
    Text(BenchmarkHistoryEntry),
    Audio(AudioHistoryEntry),
    Code(CodeHistoryEntry),
}

impl HistoryEntry {
    pub fn id(&self) -> &str {
        match self {
            HistoryEntry::Text(e) => &e.id,
            HistoryEntry::Audio(e) => &e.id,
            HistoryEntry::Code(e) => &e.id,
        }
    }

    pub fn timestamp(&self) -> i64 {
        match self {
            HistoryEntry::Text(e) => e.timestamp,
            HistoryEntry::Audio(e) => e.timestamp,
            HistoryEntry::Code(e) => e.timestamp,
        }
    }

    pub fn model_id(&self) -> &str {
        match self {
            HistoryEntry::Text(e) => &e.model_id,
            HistoryEntry::Audio(e) => &e.model_id,
            HistoryEntry::Code(e) => &e.model_id,
        }
    }

    pub fn benchmark_type(&self) -> BenchmarkType {
        match self {
            HistoryEntry::Text(e) => e.benchmark_type,
            HistoryEntry::Audio(e) => e.benchmark_type,
            HistoryEntry::Code(e) => e.benchmark_type,
        }
    }

    pub fn metric_1(&self) -> String {
        match self {
            HistoryEntry::Text(e) => format!("{:.1}", e.summary.avg_tps),
            HistoryEntry::Audio(e) => format!("{:.3}x", e.summary.avg_rtf),
            HistoryEntry::Code(e) => format!("{:.1}%", e.summary.pass_rate * 100.0),
        }
    }

    pub fn metric_1_label(&self) -> &'static str {
        match self {
            HistoryEntry::Text(_) => "TPS",
            HistoryEntry::Audio(_) => "RTF",
            HistoryEntry::Code(_) => "Pass",
        }
    }

    pub fn metric_2(&self) -> String {
        match self {
            HistoryEntry::Text(e) => format!("{:.0}ms", e.summary.avg_ttft_ms),
            HistoryEntry::Audio(e) => format!("{:.0}ms", e.summary.avg_processing_ms),
            HistoryEntry::Code(e) => format!("{:.1}", e.summary.avg_tps),
        }
    }

    pub fn metric_2_label(&self) -> &'static str {
        match self {
            HistoryEntry::Text(_) => "TTFT",
            HistoryEntry::Audio(_) => "Time",
            HistoryEntry::Code(_) => "TPS",
        }
    }

    pub fn metric_3(&self) -> String {
        match self {
            HistoryEntry::Text(e) => format!("{}", e.summary.iterations),
            HistoryEntry::Audio(e) => format!("{}", e.summary.iterations),
            HistoryEntry::Code(e) => format!("{:.0}ms", e.summary.avg_execution_time_ms),
        }
    }

    pub fn metric_3_label(&self) -> &'static str {
        match self {
            HistoryEntry::Text(_) => "Runs",
            HistoryEntry::Audio(_) => "Runs",
            HistoryEntry::Code(_) => "Exec",
        }
    }

    pub fn metric_4(&self) -> String {
        match self {
            HistoryEntry::Text(e) => format!("{:.1}/{:.1}", e.summary.min_tps, e.summary.max_tps),
            HistoryEntry::Audio(e) => format!("{:.3}/{:.3}", e.summary.min_rtf, e.summary.max_rtf),
            HistoryEntry::Code(e) => format!(
                "E:{}/{} M:{}/{} H:{}/{}",
                e.summary.easy_solved, e.summary.easy_total,
                e.summary.medium_solved, e.summary.medium_total,
                e.summary.hard_solved, e.summary.hard_total
            ),
        }
    }

    pub fn metric_4_label(&self) -> &'static str {
        match self {
            HistoryEntry::Text(_) => "Min/Max",
            HistoryEntry::Audio(_) => "Min/Max",
            HistoryEntry::Code(_) => "By Diff",
        }
    }

    pub fn session_display(&self) -> String {
        let HistoryEntry::Code(e) = self else {
            return "—".to_string();
        };
        let Some(ref session_id) = e.session_id else {
            return "—".to_string();
        };
        session_id.chars().take(6).collect()
    }

    /// Code benchmark params: language, temperature, max_tokens
    pub fn code_params(&self) -> String {
        let HistoryEntry::Code(e) = self else {
            return "—".to_string();
        };
        let tokens = e.config.max_tokens.map(|t| t.to_string()).unwrap_or("—".to_string());
        format!("{} T={:.1} {}tok", e.language.label(), e.config.temperature, tokens)
    }
}

pub struct HistoryPanel {
    history_service: Arc<HistoryService>,
    entries: Vec<HistoryEntry>,
    filter_type: Option<BenchmarkType>,
    needs_refresh: bool,
    delete_confirm: Option<String>,
    selected_ids: HashSet<String>,
    show_comparison: bool,
    pub load_request: Option<LoadCodeBenchmarkRequest>,
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
            load_request: None,
        }
    }

    /// Take the pending load request (clears it)
    pub fn take_load_request(&mut self) -> Option<LoadCodeBenchmarkRequest> {
        self.load_request.take()
    }

    fn refresh(&mut self) {
        let limit = Some(100);
        let mut entries: Vec<HistoryEntry> = Vec::new();

        // Load based on filter
        let load_text = self.filter_type.is_none() || self.filter_type == Some(BenchmarkType::Text);
        let load_audio = self.filter_type.is_none() || self.filter_type == Some(BenchmarkType::Audio);
        let load_code = self.filter_type.is_none() || self.filter_type == Some(BenchmarkType::Code);

        if load_text {
            let filter = HistoryFilter {
                benchmark_type: Some(BenchmarkType::Text),
                limit,
                ..Default::default()
            };
            if let Ok(text_entries) = self.history_service.list(filter) {
                entries.extend(text_entries.into_iter().map(HistoryEntry::Text));
            }
        }

        if load_audio {
            if let Ok(audio_entries) = self.history_service.list_audio(limit) {
                entries.extend(audio_entries.into_iter().map(HistoryEntry::Audio));
            }
        }

        if load_code {
            if let Ok(code_entries) = self.history_service.list_code(limit) {
                entries.extend(code_entries.into_iter().map(HistoryEntry::Code));
            }
        }

        // Sort by timestamp descending
        entries.sort_by(|a, b| b.timestamp().cmp(&a.timestamp()));

        // Limit total entries
        entries.truncate(100);

        self.entries = entries;
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
            return;
        }
        self.render_table(ui);
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

                    if selected_count > 0 && ui.button(format!("Delete Selected ({})", selected_count)).clicked() {
                        self.delete_confirm = Some("__selected__".to_string());
                    }

                    if !self.entries.is_empty() && ui.button("Export CSV").clicked() {
                        self.export_csv();
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
                    let msg = match id.as_str() {
                        "__all__" => "Delete ALL benchmark history?".to_string(),
                        "__selected__" => format!("Delete {} selected entries?", self.selected_ids.len()),
                        _ => format!("Delete entry {}?", &id[..8.min(id.len())]),
                    };
                    ui.label(msg);
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.delete_confirm = None;
                        }
                        if ui
                            .button(egui::RichText::new("Delete").color(egui::Color32::RED))
                            .clicked()
                        {
                            match id.as_str() {
                                "__all__" => {
                                    if let Err(e) = self.history_service.clear_all() {
                                        tracing::warn!("Failed to delete all: {}", e);
                                    }
                                }
                                "__selected__" => {
                                    for entry_id in &self.selected_ids.clone() {
                                        if let Err(e) = self.history_service.delete(entry_id) {
                                            tracing::warn!("Failed to delete {}: {}", entry_id, e);
                                        }
                                    }
                                    self.selected_ids.clear();
                                }
                                _ => {
                                    if let Err(e) = self.history_service.delete(&id) {
                                        tracing::warn!("Failed to delete: {}", e);
                                    }
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
        let mut load_entry: Option<LoadCodeBenchmarkRequest> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("history_table")
                    .num_columns(16)
                    .spacing([10.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        // Header
                        ui.label(egui::RichText::new("").strong());
                        ui.label(egui::RichText::new("Model").strong());
                        ui.label(egui::RichText::new("Type").strong());
                        ui.label(egui::RichText::new("Params").strong());
                        ui.label(egui::RichText::new("TPS").strong());
                        ui.label(egui::RichText::new("Test Pass").strong());
                        ui.label(egui::RichText::new("TTFT").strong());
                        ui.label(egui::RichText::new("RTF").strong());
                        ui.label(egui::RichText::new("Runs").strong());
                        ui.label(egui::RichText::new("Exec").strong());
                        ui.label(egui::RichText::new("Detail").strong());
                        ui.label(egui::RichText::new("Session").strong());
                        ui.label(egui::RichText::new("Date").strong());
                        ui.label(egui::RichText::new("").strong()); // Load
                        ui.label(egui::RichText::new("").strong()); // Delete
                        ui.label(egui::RichText::new("").strong()); // padding
                        ui.end_row();

                        // Rows
                        for entry in &self.entries {
                            let entry_id = entry.id().to_string();
                            let mut is_selected = self.selected_ids.contains(&entry_id);
                            if ui.checkbox(&mut is_selected, "").changed() {
                                toggle_id = Some(entry_id.clone());
                            }

                            ui.label(entry.model_id());
                            ui.label(entry.benchmark_type().label());
                            ui.label(entry.code_params());

                            // Separate columns per metric type
                            let (tps, pass, ttft, rtf, runs, exec, detail) = match entry {
                                HistoryEntry::Text(e) => (
                                    format!("{:.1}", e.summary.avg_tps),
                                    String::new(),
                                    format!("{:.0}ms", e.summary.avg_ttft_ms),
                                    String::new(),
                                    e.summary.iterations.to_string(),
                                    String::new(),
                                    format!("{:.1}/{:.1}", e.summary.min_tps, e.summary.max_tps),
                                ),
                                HistoryEntry::Audio(e) => (
                                    String::new(),
                                    String::new(),
                                    String::new(),
                                    format!("{:.3}x", e.summary.avg_rtf),
                                    e.summary.iterations.to_string(),
                                    String::new(),
                                    format!("{:.3}/{:.3}", e.summary.min_rtf, e.summary.max_rtf),
                                ),
                                HistoryEntry::Code(e) => (
                                    format!("{:.1}", e.summary.avg_tps),
                                    format!("{:.1}%", e.summary.pass_rate * 100.0),
                                    String::new(),
                                    String::new(),
                                    String::new(),
                                    format!("{:.0}ms", e.summary.avg_execution_time_ms),
                                    format!("E:{}/{} M:{}/{} H:{}/{}",
                                        e.summary.easy_solved, e.summary.easy_total,
                                        e.summary.medium_solved, e.summary.medium_total,
                                        e.summary.hard_solved, e.summary.hard_total),
                                ),
                            };
                            ui.label(tps);
                            ui.label(pass);
                            ui.label(ttft);
                            ui.label(rtf);
                            ui.label(runs);
                            ui.label(exec);
                            ui.label(detail);
                            ui.label(entry.session_display());
                            ui.label(format_timestamp(entry.timestamp()));

                            // Load button (only for Code entries)
                            if let HistoryEntry::Code(code_entry) = entry {
                                if ui.small_button("📋").on_hover_text("Load params").clicked() {
                                    load_entry = Some(LoadCodeBenchmarkRequest {
                                        model_id: code_entry.model_id.clone(),
                                        language: code_entry.language,
                                        temperature: code_entry.config.temperature,
                                        max_tokens: code_entry.config.max_tokens,
                                        problem_ids: code_entry.config.problem_ids.clone(),
                                    });
                                }
                            } else {
                                ui.label("");
                            }

                            if ui.small_button("🗑").clicked() {
                                delete_id = Some(entry_id);
                            }
                            ui.label(""); // padding
                            ui.end_row();
                        }
                    });
            });

        // Handle toggle outside the borrow
        if let Some(id) = toggle_id {
            let was_present = self.selected_ids.remove(&id);
            if !was_present {
                self.selected_ids.insert(id);
            }
        }

        if let Some(id) = delete_id {
            self.delete_confirm = Some(id);
        }

        if load_entry.is_some() {
            self.load_request = load_entry;
        }
    }

    fn render_comparison(&self, ui: &mut egui::Ui) {
        // Only compare Text entries for now
        let selected_entries: Vec<&BenchmarkHistoryEntry> = self
            .entries
            .iter()
            .filter(|e| self.selected_ids.contains(e.id()))
            .filter_map(|e| match e {
                HistoryEntry::Text(entry) => Some(entry),
                _ => None,
            })
            .collect();

        if selected_entries.len() < 2 {
            ui.label("Select at least 2 Text benchmark entries to compare");
            ui.label("(Audio and Code comparison not yet supported)");
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
        let (init, fold_fn): (f64, fn(f64, f64) -> f64) = match higher_is_better {
            true => (f64::MIN, f64::max),
            false => (f64::MAX, f64::min),
        };
        let best = values.iter().cloned().fold(init, fold_fn);

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

    fn export_csv(&self) {
        let entries = self.entries.clone();
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("Export History")
                .add_filter("CSV Files", &["csv"])
                .set_file_name("benchmark_history.csv")
                .save_file();
            let Some(path) = path else { return };

            let mut csv = String::from("Timestamp,Model,Type,Params,TPS,Test Pass,TTFT,RTF,Runs,ExecTime,Detail,Session\n");
            for entry in &entries {
                let (tps, pass, ttft, rtf, runs, exec, detail) = match &entry {
                    HistoryEntry::Text(e) => (
                        format!("{:.1}", e.summary.avg_tps),
                        String::new(),
                        format!("{:.0}", e.summary.avg_ttft_ms),
                        String::new(),
                        e.summary.iterations.to_string(),
                        String::new(),
                        format!("{:.1}/{:.1}", e.summary.min_tps, e.summary.max_tps),
                    ),
                    HistoryEntry::Audio(e) => (
                        String::new(),
                        String::new(),
                        String::new(),
                        format!("{:.3}", e.summary.avg_rtf),
                        e.summary.iterations.to_string(),
                        String::new(),
                        format!("{:.3}/{:.3}", e.summary.min_rtf, e.summary.max_rtf),
                    ),
                    HistoryEntry::Code(e) => (
                        format!("{:.1}", e.summary.avg_tps),
                        format!("{:.1}", e.summary.pass_rate * 100.0),
                        String::new(),
                        String::new(),
                        String::new(),
                        format!("{:.0}", e.summary.avg_execution_time_ms),
                        format!("E:{}/{} M:{}/{} H:{}/{}",
                            e.summary.easy_solved, e.summary.easy_total,
                            e.summary.medium_solved, e.summary.medium_total,
                            e.summary.hard_solved, e.summary.hard_total),
                    ),
                };
                let row = format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                    entry.timestamp(),
                    entry.model_id(),
                    format!("{:?}", entry.benchmark_type()),
                    entry.code_params().replace(',', ";"),
                    tps, pass, ttft, rtf, runs, exec, detail,
                    entry.session_display(),
                );
                csv.push_str(&row);
            }

            if let Err(e) = std::fs::write(&path, &csv) {
                tracing::error!("Failed to export history CSV: {}", e);
            }
        });
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
