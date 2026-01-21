//! Service container - stateless controllers
//!
//! This is the single entry point for all service access.
//! Services are stateless - they operate on models via &mut references.
//! Models are owned by the application (AppModels in llamaburn-core).

use std::sync::Arc;

use crate::{BenchmarkService, HistoryService, OllamaClient};

/// Central service container - stateless controllers
///
/// Services operate on models via &mut references.
/// Models are owned by AppModels in the GUI layer.
pub struct Services {
    pub benchmark: BenchmarkService,
    pub history: Arc<HistoryService>,
    pub ollama: OllamaClient,
}

impl Services {
    pub fn new() -> Self {
        let history = Arc::new(
            HistoryService::new(None).expect("Failed to initialize history database"),
        );

        Self {
            benchmark: BenchmarkService::new("http://localhost:11434"),
            history,
            ollama: OllamaClient::default(),
        }
    }

    /// Create with custom Ollama host
    pub fn with_host(ollama_host: impl Into<String>) -> Self {
        let host = ollama_host.into();
        let history = Arc::new(
            HistoryService::new(None).expect("Failed to initialize history database"),
        );

        Self {
            benchmark: BenchmarkService::new(&host),
            history,
            ollama: OllamaClient::new(&host),
        }
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ModelList facade methods - GUI calls these instead of touching models directly
// =============================================================================

use llamaburn_core::{AppModels, ModelInfo};

impl Services {
    // --- ModelList Getters ---

    /// Get the name of the model currently being preloaded
    pub fn get_preloading_name(&self, models: &AppModels) -> String {
        models.models.preloading_name.clone()
    }

    /// Get the currently selected model name
    pub fn get_selected_model(&self, models: &AppModels) -> String {
        models.models.selected.clone()
    }

    /// Get list of available model names
    pub fn get_model_names(&self, models: &AppModels) -> Vec<String> {
        models.models.models.clone()
    }

    /// Check if models are available
    pub fn has_models(&self, models: &AppModels) -> bool {
        !models.models.models.is_empty()
    }

    /// Check if models are currently loading
    pub fn is_loading_models(&self, models: &AppModels) -> bool {
        models.models.loading
    }

    /// Check if a model is being preloaded
    pub fn is_preloading(&self, models: &AppModels) -> bool {
        models.models.preloading
    }

    /// Get model info if available
    pub fn get_model_info<'a>(&self, models: &'a AppModels) -> Option<&'a ModelInfo> {
        models.models.model_info.as_ref()
    }

    // --- ModelList Setters ---

    /// Set the loading state
    pub fn set_loading(&self, models: &mut AppModels, loading: bool) {
        models.models.loading = loading;
    }

    /// Clear the selected model
    pub fn clear_selected_model(&self, models: &mut AppModels) {
        models.models.selected.clear();
    }

    /// Clear model info
    pub fn clear_model_info(&self, models: &mut AppModels) {
        models.models.model_info = None;
    }

    // --- ModelList Delegated Methods ---
    // These already exist on ModelList, but we provide pass-through for consistency

    /// Start loading models from Ollama
    pub fn start_loading_models(&self, models: &mut AppModels) {
        models.models.start_loading();
    }

    /// Set the available models
    pub fn set_models(&self, models: &mut AppModels, model_list: Vec<String>) {
        models.models.set_models(model_list);
    }

    /// Select a model
    pub fn select_model(&self, models: &mut AppModels, name: String) {
        models.models.select(name);
    }

    /// Start preloading a model into VRAM
    pub fn start_preload(&self, models: &mut AppModels, name: &str) {
        models.models.start_preload(name);
    }

    /// Finish preloading
    pub fn finish_preload(&self, models: &mut AppModels) {
        models.models.finish_preload();
    }

    /// Set model info
    pub fn set_model_info(&self, models: &mut AppModels, info: Option<ModelInfo>) {
        models.models.set_model_info(info);
    }
}

// =============================================================================
// TextBenchmark facade methods
// =============================================================================

impl Services {
    // --- TextBenchmark Getters ---

    /// Get text benchmark live output
    pub fn get_text_output(&self, models: &AppModels) -> String {
        models.text.live_output.clone()
    }

    /// Get text benchmark progress
    pub fn get_text_progress(&self, models: &AppModels) -> String {
        models.text.progress.clone()
    }

    /// Get text benchmark error
    pub fn get_text_error(&self, models: &AppModels) -> Option<String> {
        models.text.error.clone()
    }

    /// Check if text benchmark is running
    pub fn is_text_running(&self, models: &AppModels) -> bool {
        models.text.running
    }

    /// Get text benchmark result
    pub fn get_text_result(&self, models: &AppModels) -> Option<llamaburn_core::TextBenchmarkResult> {
        models.text.result.clone()
    }

    // --- TextBenchmark Setters ---

    /// Set text benchmark error
    pub fn set_text_error(&self, models: &mut AppModels, error: Option<String>) {
        models.text.error = error;
    }

    /// Append to text benchmark output
    pub fn append_text_output(&self, models: &mut AppModels, text: &str) {
        models.text.append_output(text);
    }

    /// Clear text benchmark output
    pub fn clear_text_output(&self, models: &mut AppModels) {
        models.text.clear_output();
    }

    /// Clear text benchmark state (for tab switching)
    pub fn clear_text_state(&self, models: &mut AppModels) {
        models.text.last_model_for_info.clear();
    }
}

// =============================================================================
// AudioBenchmark facade methods
// =============================================================================

impl Services {
    // --- AudioBenchmark Getters ---

    pub fn get_audio_output(&self, models: &AppModels) -> String {
        models.audio.live_output.clone()
    }

    pub fn get_audio_progress(&self, models: &AppModels) -> String {
        models.audio.progress.clone()
    }

    pub fn get_audio_error(&self, models: &AppModels) -> Option<String> {
        models.audio.error.clone()
    }

    // --- AudioBenchmark Setters ---

    pub fn set_audio_error(&self, models: &mut AppModels, error: Option<String>) {
        models.audio.set_error(error);
    }

    pub fn append_audio_output(&self, models: &mut AppModels, text: &str) {
        models.audio.append_output(text);
    }

    pub fn clear_audio_output(&self, models: &mut AppModels) {
        models.audio.clear_output();
    }

    pub fn set_audio_progress(&self, models: &mut AppModels, progress: String) {
        models.audio.set_progress(progress);
    }
}

// =============================================================================
// CodeBenchmark facade methods
// =============================================================================

impl Services {
    // --- CodeBenchmark Getters ---

    pub fn get_code_output(&self, models: &AppModels) -> String {
        models.code.live_output.clone()
    }

    pub fn get_code_progress(&self, models: &AppModels) -> String {
        models.code.progress.clone()
    }

    pub fn get_code_error(&self, models: &AppModels) -> Option<String> {
        models.code.error.clone()
    }

    // --- CodeBenchmark Setters ---

    pub fn set_code_error(&self, models: &mut AppModels, error: Option<String>) {
        models.code.error = error;
    }

    pub fn set_code_progress(&self, models: &mut AppModels, progress: String) {
        models.code.set_progress(progress);
    }

    pub fn append_code_output(&self, models: &mut AppModels, text: &str) {
        models.code.append_output(text);
    }

    pub fn clear_code_output(&self, models: &mut AppModels) {
        models.code.clear_output();
    }
}
