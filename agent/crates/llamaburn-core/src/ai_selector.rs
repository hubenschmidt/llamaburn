//! Model list model - manages available models from Ollama

use crate::ModelInfo;
use serde::{Deserialize, Serialize};

/// Model list - owns available models and selection state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelList {
    /// Available models from Ollama
    pub models: Vec<String>,
    /// Currently selected model (for text/audio)
    pub selected: String,
    /// Loading models from Ollama
    pub loading: bool,
    /// Preloading a model into VRAM
    pub preloading: bool,
    /// Name of model being preloaded
    pub preloading_name: String,
    /// Model info for selected model
    pub model_info: Option<ModelInfo>,
}

impl ModelList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_models(&mut self, models: Vec<String>) {
        self.models = models;
        self.loading = false;
    }

    pub fn start_loading(&mut self) {
        self.loading = true;
    }

    pub fn select(&mut self, model: String) {
        self.selected = model;
    }

    pub fn start_preload(&mut self, model: &str) {
        self.preloading = true;
        self.preloading_name = model.to_string();
    }

    pub fn finish_preload(&mut self) {
        self.preloading = false;
        self.preloading_name.clear();
    }

    pub fn set_model_info(&mut self, info: Option<ModelInfo>) {
        self.model_info = info;
    }

    pub fn has_models(&self) -> bool {
        !self.models.is_empty()
    }

    pub fn is_selected(&self, model: &str) -> bool {
        self.selected == model
    }
}
