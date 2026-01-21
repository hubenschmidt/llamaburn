//! Configuration UI for code benchmark panel

use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use llamaburn_services::{Difficulty, Language};
use llamaburn_services::{BatchStatus, Preset};

use super::util::{format_temp_label, format_tokens_label, MAX_TOKENS_BUCKETS, TEMPERATURE_BUCKETS};
use super::{CodeGenAction, CodeGenBenchmarkPanel, CodeGenRenderContext};
use crate::panels::benchmark::components::{multi_select_dropdown, toggle_selection};

impl CodeGenBenchmarkPanel {
    /// Render the config UI. Returns actions for parent to process.
    pub fn render_config(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &CodeGenRenderContext,
    ) -> Vec<CodeGenAction> {
        let mut actions = Vec::new();

        let disabled = self.running || ctx.model_list.loading;
        let queue_running = !self.combo_queue.is_empty() || self.current_combo.is_some();
        let interactive = !disabled && !queue_running;

        // Show incomplete sessions banner if any exist
        actions.extend(self.render_incomplete_sessions_banner(ui, interactive));

        // Preset row
        actions.extend(self.render_preset_row(ui, interactive));
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(5.0);

        // Models dropdown with multi-select
        multi_select_dropdown(
            ui,
            "models_popup",
            "Models",
            &ctx.model_list.models,
            &mut self.selected_models,
            |m| m.clone(),
            interactive,
            280.0,
        );

        ui.add_space(3.0);

        // Languages dropdown
        let all_langs = Language::all().to_vec();
        multi_select_dropdown(
            ui,
            "langs_popup",
            "Languages",
            &all_langs,
            &mut self.selected_languages,
            |l| l.label().to_string(),
            interactive,
            200.0,
        );

        ui.add_space(3.0);

        // Temperature dropdown with custom input
        self.render_temperature_dropdown(ui, interactive);

        ui.add_space(3.0);

        // Max tokens dropdown
        self.render_max_tokens_dropdown(ui, interactive);

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(5.0);

        // Input section - problem set selection
        self.render_problem_selection(ui, disabled);

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(5.0);

        // Execution section
        ui.label(egui::RichText::new("Execution").strong());
        ui.add_space(5.0);

        let running = self.running || queue_running;

        // Show running controls (Pause/Cancel) when running
        if running {
            if let Some(pause_action) = self.render_running_controls(ui) {
                actions.push(pause_action);
            }
            return actions;
        }

        // Progress indicator (when not running but has completed some)
        self.render_progress_indicator(ui);

        // Run button and options
        actions.extend(self.render_run_button(ui));

        actions
    }

    /// Render banner for incomplete/paused batch sessions
    fn render_incomplete_sessions_banner(
        &mut self,
        ui: &mut egui::Ui,
        interactive: bool,
    ) -> Vec<CodeGenAction> {
        if self.pending_resume_batches.is_empty() {
            return vec![];
        }

        let mut actions = Vec::new();
        let batches = self.pending_resume_batches.clone();
        let mut resume_idx: Option<usize> = None;
        let mut discard_idx: Option<usize> = None;

        egui::Frame::group(ui.style())
            .inner_margin(egui::vec2(8.0, 6.0))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("Incomplete Sessions ({})", batches.len()))
                        .strong(),
                );
                ui.add_space(4.0);

                for (idx, batch) in batches.iter().enumerate() {
                    ui.separator();
                    ui.add_space(2.0);

                    let status_text = match batch.status {
                        BatchStatus::Paused => "Paused",
                        BatchStatus::Running => "Interrupted",
                        BatchStatus::Completed => "Completed",
                    };
                    let progress_text = format!(
                        "{} - {}/{} complete",
                        status_text, batch.queue_completed, batch.queue_total
                    );
                    ui.label(progress_text);

                    let config_text = format!(
                        "{} models x {} langs x {} temps x {} tokens",
                        batch.selected_models.len(),
                        batch.selected_languages.len(),
                        batch.selected_temperatures.len(),
                        batch.selected_max_tokens.len(),
                    );
                    ui.label(egui::RichText::new(config_text).small().weak());

                    if let Some(ref error) = batch.error_message {
                        ui.label(
                            egui::RichText::new(format!("Error: {}", error))
                                .small()
                                .color(egui::Color32::RED),
                        );
                    }

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(interactive, egui::Button::new("Resume"))
                            .clicked()
                        {
                            resume_idx = Some(idx);
                        }
                        if ui
                            .add_enabled(interactive, egui::Button::new("Discard"))
                            .clicked()
                        {
                            discard_idx = Some(idx);
                        }
                    });
                }
            });

        ui.add_space(8.0);

        // Handle resume action
        if let Some(idx) = resume_idx {
            actions.extend(self.resume_batch(idx));
        }

        // Handle discard action
        if let Some(idx) = discard_idx {
            if let Some(batch) = self.pending_resume_batches.get(idx) {
                actions.push(CodeGenAction::DeleteBatch(batch.session_id.clone()));
            }
            self.pending_resume_batches.remove(idx);
        }

        actions
    }

    /// Resume a paused batch. Returns actions.
    fn resume_batch(&mut self, idx: usize) -> Vec<CodeGenAction> {
        let Some(batch) = self.pending_resume_batches.get(idx).cloned() else {
            return vec![];
        };

        let mut actions = Vec::new();

        // Restore state from batch
        self.restore_from_batch(&batch);

        // Update status to Running
        let mut updated_batch = batch.clone();
        updated_batch.status = BatchStatus::Running;
        updated_batch.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        updated_batch.error_message = None;
        updated_batch.failed_combo = None;
        actions.push(CodeGenAction::UpdateBatch(updated_batch));

        // Remove from pending list
        self.pending_resume_batches.remove(idx);

        // Start execution
        actions.push(CodeGenAction::AppendOutput(format!(
            "=== Resuming Batch {} ===\n{}/{} combinations remaining\n",
            batch.session_id,
            batch.queue_total - batch.queue_completed,
            batch.queue_total
        )));
        actions.push(CodeGenAction::AdvanceToNextCombo);

        actions
    }

    /// Render preset dropdown and save button
    fn render_preset_row(&mut self, ui: &mut egui::Ui, interactive: bool) -> Vec<CodeGenAction> {
        let mut actions = Vec::new();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Preset").strong());
            ui.add_space(10.0);

            let current_preset_name = self
                .active_preset_id
                .as_ref()
                .and_then(|id| self.presets.iter().find(|p| &p.id == id))
                .map(|p| p.name.as_str())
                .unwrap_or("None");

            let presets_popup_id = ui.make_persistent_id("presets_popup");
            let presets_btn = ui.add_enabled(
                interactive,
                egui::Button::new(current_preset_name).min_size(egui::vec2(180.0, 0.0)),
            );

            if presets_btn.clicked() {
                ui.memory_mut(|mem| mem.toggle_popup(presets_popup_id));
            }

            egui::popup_below_widget(
                ui,
                presets_popup_id,
                &presets_btn,
                egui::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(180.0);

                    if ui
                        .selectable_label(self.active_preset_id.is_none(), "None")
                        .clicked()
                    {
                        self.active_preset_id = None;
                        ui.memory_mut(|mem| mem.close_popup());
                    }

                    ui.separator();

                    let presets = self.presets.clone();
                    for preset in &presets {
                        let is_active = self.active_preset_id.as_ref() == Some(&preset.id);
                        if ui.selectable_label(is_active, &preset.name).clicked() {
                            self.load_preset(preset);
                            ui.memory_mut(|mem| mem.close_popup());
                        }
                    }

                    if let Some(active_id) = &self.active_preset_id.clone() {
                        ui.separator();
                        if ui.small_button("Delete Selected").clicked() {
                            actions.push(CodeGenAction::DeletePreset(active_id.clone()));
                            self.presets.retain(|p| &p.id != active_id);
                            self.active_preset_id = None;
                            ui.memory_mut(|mem| mem.close_popup());
                        }
                    }
                },
            );

            ui.add_space(10.0);

            if ui
                .add_enabled(interactive, egui::Button::new("Save Preset"))
                .clicked()
            {
                self.show_save_preset_modal = true;
                self.preset_name_input.clear();
            }
        });

        // Save preset modal
        if self.show_save_preset_modal {
            if let Some(preset_action) = self.render_save_preset_modal(ui) {
                actions.push(preset_action);
            }
        }

        actions
    }

    /// Render save preset modal. Returns action if preset saved.
    fn render_save_preset_modal(&mut self, ui: &mut egui::Ui) -> Option<CodeGenAction> {
        let mut action = None;

        egui::Window::new("Save Preset")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label("Enter preset name:");
                ui.text_edit_singleline(&mut self.preset_name_input);
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.show_save_preset_modal = false;
                    }

                    let name = self.preset_name_input.trim();
                    let can_save = !name.is_empty()
                        && self.selected_models.len() == 1
                        && self.selected_languages.len() == 1
                        && self.selected_temperatures.len() == 1
                        && self.selected_max_tokens.len() == 1;

                    if ui
                        .add_enabled(can_save, egui::Button::new("Save"))
                        .clicked()
                    {
                        let preset = self.build_preset(name.to_string());
                        self.active_preset_id = Some(preset.id.clone());
                        self.presets.push(preset.clone());
                        action = Some(CodeGenAction::InsertPreset(preset));
                        self.show_save_preset_modal = false;
                    }
                });

                // Validation messages
                if self.selected_models.len() != 1 {
                    ui.label(
                        egui::RichText::new("Select exactly 1 model")
                            .small()
                            .color(egui::Color32::YELLOW),
                    );
                }
                if self.selected_languages.len() != 1 {
                    ui.label(
                        egui::RichText::new("Select exactly 1 language")
                            .small()
                            .color(egui::Color32::YELLOW),
                    );
                }
                if self.selected_temperatures.len() != 1 {
                    ui.label(
                        egui::RichText::new("Select exactly 1 temperature")
                            .small()
                            .color(egui::Color32::YELLOW),
                    );
                }
                if self.selected_max_tokens.len() != 1 {
                    ui.label(
                        egui::RichText::new("Select exactly 1 max tokens")
                            .small()
                            .color(egui::Color32::YELLOW),
                    );
                }
            });

        action
    }

    /// Build a preset from current config
    fn build_preset(&self, name: String) -> Preset {
        Preset {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            model_id: self.selected_models.first().cloned().unwrap_or_default(),
            language: self
                .selected_languages
                .first()
                .copied()
                .unwrap_or(Language::Python),
            temperature: self.selected_temperatures.first().copied().unwrap_or(0.0),
            max_tokens: self.selected_max_tokens.first().copied(),
            problem_ids: self.selected_problem_ids.clone(),
        }
    }

    /// Render temperature dropdown
    fn render_temperature_dropdown(&mut self, ui: &mut egui::Ui, interactive: bool) {
        let temp_label = format_temp_label(&self.selected_temperatures);
        let temp_popup_id = ui.make_persistent_id("temp_popup");
        let temp_btn = ui.add_enabled(
            interactive,
            egui::Button::new(&temp_label).min_size(egui::vec2(290.0, 0.0)),
        );
        if temp_btn.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(temp_popup_id));
        }
        egui::popup_below_widget(
            ui,
            temp_popup_id,
            &temp_btn,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(220.0);
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        self.selected_temperatures = TEMPERATURE_BUCKETS.to_vec();
                    }
                    if ui.small_button("Clear").clicked() {
                        self.selected_temperatures.clear();
                    }
                });
                ui.separator();
                for temp in TEMPERATURE_BUCKETS {
                    let mut selected = self.selected_temperatures.contains(temp);
                    if ui
                        .checkbox(&mut selected, format!("{:.1}", temp))
                        .changed()
                    {
                        toggle_selection(&mut self.selected_temperatures, *temp, selected);
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Custom:");
                    ui.add(
                        egui::DragValue::new(&mut self.custom_temperature)
                            .range(0.0..=2.0)
                            .speed(0.05),
                    );
                    if ui.small_button("Add").clicked() {
                        let val = self.custom_temperature;
                        if !self.selected_temperatures.contains(&val) {
                            self.selected_temperatures.push(val);
                            self.selected_temperatures
                                .sort_by(|a, b| a.partial_cmp(b).expect("NaN in temperatures"));
                        }
                    }
                });
            },
        );
    }

    /// Render max tokens dropdown
    fn render_max_tokens_dropdown(&mut self, ui: &mut egui::Ui, interactive: bool) {
        let tokens_label = format_tokens_label(&self.selected_max_tokens);
        let tokens_popup_id = ui.make_persistent_id("tokens_popup");
        let tokens_btn = ui.add_enabled(
            interactive,
            egui::Button::new(&tokens_label).min_size(egui::vec2(290.0, 0.0)),
        );
        if tokens_btn.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(tokens_popup_id));
        }
        egui::popup_below_widget(
            ui,
            tokens_popup_id,
            &tokens_btn,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(180.0);
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        self.selected_max_tokens = MAX_TOKENS_BUCKETS.to_vec();
                    }
                    if ui.small_button("Clear").clicked() {
                        self.selected_max_tokens.clear();
                    }
                });
                ui.separator();
                for tokens in MAX_TOKENS_BUCKETS {
                    let mut selected = self.selected_max_tokens.contains(tokens);
                    if ui.checkbox(&mut selected, format!("{}", tokens)).changed() {
                        toggle_selection(&mut self.selected_max_tokens, *tokens, selected);
                    }
                }
            },
        );
    }

    /// Render problem set selection
    fn render_problem_selection(&mut self, ui: &mut egui::Ui, disabled: bool) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Input").strong());
            ui.add_space(10.0);
            ui.add_enabled_ui(!disabled, |ui| {
                let current_set_name = self
                    .problem_sets
                    .get(self.selected_problem_set_idx)
                    .map(|ps| ps.name.as_str())
                    .unwrap_or("None");
                egui::ComboBox::from_id_salt("problem_set_select")
                    .selected_text(current_set_name)
                    .show_ui(ui, |ui| {
                        for (idx, ps) in self.problem_sets.iter().enumerate() {
                            if ui
                                .selectable_label(self.selected_problem_set_idx == idx, &ps.name)
                                .clicked()
                            {
                                self.selected_problem_set_idx = idx;
                            }
                        }
                    });
            });
        });

        let current_set_ids: Vec<String> = self
            .current_problems()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let total_problems: usize = self
            .problem_sets
            .iter()
            .map(|ps| ps.problems.len())
            .sum();

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!disabled, egui::Button::new("Select All (Set)"))
                .clicked()
            {
                for id in current_set_ids {
                    if !self.selected_problem_ids.contains(&id) {
                        self.selected_problem_ids.push(id);
                    }
                }
            }
            if ui
                .add_enabled(!disabled, egui::Button::new("Clear All"))
                .clicked()
            {
                self.selected_problem_ids.clear();
            }
            ui.label(format!(
                "{}/{} total",
                self.selected_problem_ids.len(),
                total_problems
            ));
        });
        ui.add_space(5.0);

        let problems = self.current_problems().to_vec();
        let problems_height = (ui.available_height() - 50.0).max(80.0);
        egui::ScrollArea::vertical()
            .max_height(problems_height)
            .show(ui, |ui| {
                for problem in &problems {
                    let is_selected = self.selected_problem_ids.contains(&problem.id);
                    let difficulty_color = match problem.difficulty {
                        Difficulty::Easy => egui::Color32::GREEN,
                        Difficulty::Medium => egui::Color32::YELLOW,
                        Difficulty::Hard => egui::Color32::RED,
                    };

                    ui.horizontal(|ui| {
                        let mut selected = is_selected;
                        if ui
                            .add_enabled(!disabled, egui::Checkbox::new(&mut selected, ""))
                            .changed()
                        {
                            if selected {
                                self.selected_problem_ids.push(problem.id.clone());
                            } else {
                                self.selected_problem_ids.retain(|id| id != &problem.id);
                            }
                        }

                        ui.colored_label(
                            difficulty_color,
                            format!("[{}]", problem.difficulty.label()),
                        );
                        ui.label(&problem.title);
                    });
                }
            });
    }

    /// Render running controls. Returns Some action if Pause clicked.
    fn render_running_controls(&self, ui: &mut egui::Ui) -> Option<CodeGenAction> {
        let mut pause_action = None;

        // Progress bar
        let completed = self.queue_completed;
        let total = self.queue_total;
        if total > 0 {
            let progress = (completed as f32 + 0.5) / total as f32;
            ui.horizontal(|ui| {
                ui.add(egui::ProgressBar::new(progress.min(1.0)).show_percentage());
                ui.label(format!("{} of {} executed", completed, total));
            });
        }

        ui.horizontal(|ui| {
            if ui.button("Pause").clicked() {
                pause_action = Some(CodeGenAction::AppendOutput(String::new())); // Signal pause
            }
            if ui.button("Cancel").clicked() {
                // Cancel handled by returning specific marker
            }
            ui.spinner();

            let eta_label = self.calculate_eta_label();
            if !eta_label.is_empty() {
                ui.label(eta_label);
            }

            let combo_label = self
                .current_combo
                .as_ref()
                .map(|combo| {
                    format!(
                        "{} | {} | T={:.1} | {}tok",
                        combo.model,
                        combo.language.label(),
                        combo.temperature,
                        combo.max_tokens.unwrap_or(2048)
                    )
                })
                .unwrap_or_default();
            ui.label(combo_label);
        });

        pause_action
    }

    /// Calculate ETA label based on average combo duration
    fn calculate_eta_label(&self) -> String {
        let durations = &self.combo_durations_ms;
        if durations.is_empty() {
            return String::new();
        }

        let avg_ms: u64 = durations.iter().sum::<u64>() / durations.len() as u64;
        let remaining = self
            .queue_total
            .saturating_sub(self.queue_completed + 1);
        let eta_ms = avg_ms * remaining as u64;

        super::util::format_duration_eta(eta_ms)
    }

    /// Render progress indicator
    fn render_progress_indicator(&self, ui: &mut egui::Ui) {
        let completed = self.queue_completed;
        let total = self.queue_total;
        if total == 0 {
            return;
        }

        let progress = completed as f32 / total as f32;
        ui.horizontal(|ui| {
            ui.add(egui::ProgressBar::new(progress).show_percentage());
            ui.label(format!("{} of {} executed", completed, total));
        });
        ui.add_space(5.0);
    }

    /// Render run button and options. Returns actions.
    fn render_run_button(&mut self, ui: &mut egui::Ui) -> Vec<CodeGenAction> {
        let mut actions = Vec::new();

        ui.horizontal(|ui| {
            let combo_count = self.combination_count();
            let has_selections = !self.selected_models.is_empty()
                && !self.selected_languages.is_empty()
                && !self.selected_temperatures.is_empty()
                && !self.selected_max_tokens.is_empty()
                && !self.selected_problem_ids.is_empty();

            let button_label = format!(
                "Run {} combo{} ({} x {} x {} x {})",
                combo_count,
                if combo_count == 1 { "" } else { "s" },
                self.selected_models.len(),
                self.selected_languages.len(),
                self.selected_temperatures.len(),
                self.selected_max_tokens.len()
            );

            let green = egui::Color32::from_rgb(34, 139, 34);
            let button =
                egui::Button::new(egui::RichText::new(button_label).color(egui::Color32::WHITE))
                    .fill(green);

            if ui.add_enabled(has_selections, button).clicked() {
                // Return start action - parent will call start_matrix
                actions.extend(self.start_matrix());
            }
        });

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.auto_run_tests, "Run Tests");
            ui.checkbox(&mut self.skip_on_error, "Skip on Error")
                .on_hover_text("Skip failed combos and continue (for unattended runs)");
        });

        actions
    }

    /// Set presets (called after loading from history service)
    pub fn set_presets(&mut self, presets: Vec<Preset>) {
        self.presets = presets;
    }
}
