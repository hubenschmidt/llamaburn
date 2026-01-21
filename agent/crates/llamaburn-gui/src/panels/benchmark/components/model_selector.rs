//! Model selector widget - single model dropdown with load/unload

use eframe::egui::{self, Widget};
use llamaburn_services::ModelList;

/// Response from the model selector widget
#[derive(Default)]
pub struct ModelSelectorResponse {
    /// Model was selected (changed)
    pub selected: Option<String>,
    /// Unload button was clicked
    pub unload_clicked: bool,
}

/// Model selector widget - dropdown with preload spinner and unload button
pub struct ModelSelector<'a> {
    model_list: &'a ModelList,
    id: &'a str,
    disabled: bool,
}

impl<'a> ModelSelector<'a> {
    pub fn new(model_list: &'a ModelList, id: &'a str) -> Self {
        Self {
            model_list,
            id,
            disabled: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Render and return response (for handling selection/unload)
    pub fn show(self, ui: &mut egui::Ui) -> ModelSelectorResponse {
        let mut response = ModelSelectorResponse::default();

        ui.horizontal(|ui| {
            ui.add_enabled_ui(!self.disabled, |ui| {
                let selected_text = self.display_text();

                egui::ComboBox::from_id_salt(self.id)
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for model in &self.model_list.models {
                            let is_selected = self.model_list.selected == *model;
                            if ui.selectable_label(is_selected, model).clicked() {
                                response.selected = Some(model.clone());
                            }
                        }
                    });
            });

            if self.model_list.loading || self.model_list.preloading {
                ui.spinner();
            }

            let can_unload = !self.model_list.selected.is_empty() && !self.disabled;
            if ui.add_enabled(can_unload, egui::Button::new("Unload")).clicked() {
                response.unload_clicked = true;
            }
        });

        response
    }

    fn display_text(&self) -> &str {
        if self.model_list.loading {
            return "Loading models...";
        }
        if self.model_list.models.is_empty() {
            return "No models found";
        }
        if self.model_list.selected.is_empty() {
            return "Select model...";
        }
        &self.model_list.selected
    }
}

// Also keep the function-based API for backwards compatibility during migration
pub fn render_model_selector(
    ui: &mut egui::Ui,
    id: &str,
    models: &[String],
    selected_model: &str,
    loading_models: bool,
    preloading: bool,
    disabled: bool,
) -> ModelSelectorResponse {
    let mut response = ModelSelectorResponse::default();

    ui.horizontal(|ui| {
        ui.add_enabled_ui(!disabled, |ui| {
            let selected_text = select_display_text(loading_models, models.is_empty(), selected_model.is_empty(), selected_model);

            egui::ComboBox::from_id_salt(id)
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for model in models {
                        let is_selected = selected_model == model;
                        if ui.selectable_label(is_selected, model).clicked() {
                            response.selected = Some(model.clone());
                        }
                    }
                });
        });

        if loading_models || preloading {
            ui.spinner();
        }

        let can_unload = !selected_model.is_empty() && !disabled;
        if ui.add_enabled(can_unload, egui::Button::new("Unload")).clicked() {
            response.unload_clicked = true;
        }
    });

    response
}

fn select_display_text<'a>(loading: bool, empty: bool, none_selected: bool, selected: &'a str) -> &'a str {
    if loading {
        return "Loading models...";
    }
    if empty {
        return "No models found";
    }
    if none_selected {
        return "Select model...";
    }
    selected
}
