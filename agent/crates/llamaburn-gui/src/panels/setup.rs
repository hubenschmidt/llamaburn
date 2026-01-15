use std::sync::Arc;

use eframe::egui;
use llamaburn_services::{settings_keys, HistoryService, SettingsService};

pub struct SetupPanel {
    settings_service: SettingsService,
    history_service: Arc<HistoryService>,

    // Form state
    ollama_host: String,
    hf_api_key: String,

    // Status
    save_status: Option<String>,
    reset_confirm: bool,
}

impl SetupPanel {
    pub fn new(history_service: Arc<HistoryService>) -> Self {
        let settings_service = SettingsService::new(history_service.connection());

        let ollama_host = settings_service
            .get(settings_keys::OLLAMA_HOST)
            .ok()
            .flatten()
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        let hf_api_key = settings_service
            .get(settings_keys::HF_API_KEY)
            .ok()
            .flatten()
            .unwrap_or_default();

        Self {
            settings_service,
            history_service,
            ollama_host,
            hf_api_key,
            save_status: None,
            reset_confirm: false,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("Setup")
                .heading()
                .color(egui::Color32::GRAY),
        );
        ui.add_space(10.0);

        self.render_ollama_settings(ui);
        ui.add_space(20.0);

        self.render_huggingface_settings(ui);
        ui.add_space(20.0);

        self.render_database_settings(ui);

        if let Some(status) = &self.save_status {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(status).color(egui::Color32::GREEN));
        }
    }

    fn render_ollama_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Ollama").strong());
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.label("Host:");
            let response = ui.text_edit_singleline(&mut self.ollama_host);
            if response.changed() {
                self.save_status = None;
            }
        });

        ui.add_space(5.0);
        if ui.button("Save Ollama Settings").clicked() {
            self.save_ollama_settings();
        }
    }

    fn render_huggingface_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("HuggingFace").strong());
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.label("API Key:");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.hf_api_key)
                    .password(true)
                    .hint_text("Optional - for higher rate limits"),
            );
            if response.changed() {
                self.save_status = None;
            }
        });

        ui.add_space(5.0);
        if ui.button("Save HuggingFace Settings").clicked() {
            self.save_hf_settings();
        }
    }

    fn render_database_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Database").strong());
        ui.add_space(5.0);

        ui.label(format!(
            "Location: {}",
            self.history_service.db_path().display()
        ));

        ui.add_space(10.0);

        if !self.reset_confirm {
            if ui
                .button(egui::RichText::new("Reset Database").color(egui::Color32::RED))
                .clicked()
            {
                self.reset_confirm = true;
            }
            return;
        }

        ui.horizontal(|ui| {
            ui.label("Are you sure? This will delete all history.");
            if ui.button("Cancel").clicked() {
                self.reset_confirm = false;
            }
            if ui
                .button(egui::RichText::new("Yes, Reset").color(egui::Color32::RED))
                .clicked()
            {
                self.reset_database();
                self.reset_confirm = false;
            }
        });
    }

    fn save_ollama_settings(&mut self) {
        match self
            .settings_service
            .set(settings_keys::OLLAMA_HOST, &self.ollama_host)
        {
            Ok(_) => {
                self.save_status = Some("Ollama settings saved".to_string());
            }
            Err(e) => {
                self.save_status = Some(format!("Failed to save: {}", e));
            }
        }
    }

    fn save_hf_settings(&mut self) {
        if self.hf_api_key.is_empty() {
            if let Err(e) = self.settings_service.delete(settings_keys::HF_API_KEY) {
                self.save_status = Some(format!("Failed to delete: {}", e));
                return;
            }
            self.save_status = Some("HuggingFace API key removed".to_string());
            return;
        }

        match self
            .settings_service
            .set(settings_keys::HF_API_KEY, &self.hf_api_key)
        {
            Ok(_) => {
                self.save_status = Some("HuggingFace settings saved".to_string());
            }
            Err(e) => {
                self.save_status = Some(format!("Failed to save: {}", e));
            }
        }
    }

    fn reset_database(&mut self) {
        match self.history_service.reset_database() {
            Ok(_) => {
                self.save_status = Some("Database reset complete".to_string());
            }
            Err(e) => {
                self.save_status = Some(format!("Failed to reset: {}", e));
            }
        }
    }
}
