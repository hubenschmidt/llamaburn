use eframe::egui;

/// Response from the model selector widget
#[derive(Default)]
pub struct ModelSelectorResponse {
    /// Model was selected (changed)
    pub selected: Option<String>,
    /// Unload button was clicked
    pub unload_clicked: bool,
}

/// Render a model selector combo box with preload spinner and unload button.
///
/// Returns which action was taken (if any).
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
