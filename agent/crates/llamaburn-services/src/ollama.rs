use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, error, info, instrument};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Error, Debug)]
pub enum OllamaError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] ureq::Error),
    #[error("JSON parse failed: {0}")]
    Json(#[from] std::io::Error),
    #[error("Connection refused - is Ollama running?")]
    ConnectionRefused,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: u64,
    pub digest: String,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelDetails {
    pub format: String,
    pub family: String,
    pub parameter_size: String,
    pub quantization_level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaShowResponse {
    pub modelfile: String,
    pub parameters: String,
    pub template: String,
    pub details: OllamaModelDetails,
}

#[derive(Serialize)]
struct ShowRequest {
    name: String,
}

#[derive(Serialize)]
struct UnloadRequest {
    model: String,
    keep_alive: i32,
}

pub struct OllamaClient {
    host: String,
}

impl OllamaClient {
    pub fn new(host: impl Into<String>) -> Self {
        Self { host: host.into() }
    }

    pub fn default_host() -> Self {
        Self::new("http://localhost:11434")
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    #[instrument(skip(self), fields(host = %self.host))]
    pub fn list_models(&self) -> Result<Vec<OllamaModel>, OllamaError> {
        let url = format!("{}/api/tags", self.host);
        debug!("Fetching models from Ollama API");

        let response = ureq::get(&url)
            .timeout(REQUEST_TIMEOUT)
            .call()
            .map_err(|e| map_ureq_error(e, "Connection refused - Ollama not running?"))?;

        let tags: TagsResponse = response.into_json()?;
        info!(count = tags.models.len(), "Fetched models from Ollama");
        Ok(tags.models)
    }

    #[instrument(skip(self))]
    pub fn list_model_names(&self) -> Result<Vec<String>, OllamaError> {
        let models = self.list_models()?;
        let names: Vec<String> = models.into_iter().map(|m| m.name).collect();
        debug!(models = ?names, "Model names");
        Ok(names)
    }

    /// Fetch models asynchronously via a channel
    #[instrument(skip(self))]
    pub fn fetch_models_async(&self) -> Receiver<Result<Vec<String>, OllamaError>> {
        info!("Starting async model fetch");
        let (tx, rx) = channel();
        let host = self.host.clone();

        thread::spawn(move || {
            let client = OllamaClient::new(host);
            let result = client.list_model_names();
            let _ = tx.send(result);
        });

        rx
    }

    /// Create a channel pair for fetching models
    pub fn create_model_fetcher(&self) -> (Sender<()>, Receiver<Result<Vec<String>, OllamaError>>) {
        let (request_tx, request_rx) = channel::<()>();
        let (response_tx, response_rx) = channel();
        let host = self.host.clone();

        thread::spawn(move || {
            while request_rx.recv().is_ok() {
                let client = OllamaClient::new(&host);
                let result = client.list_model_names();
                if response_tx.send(result).is_err() {
                    break;
                }
            }
        });

        (request_tx, response_rx)
    }

    /// Get detailed model information
    #[instrument(skip(self), fields(model = %model_id))]
    pub fn show_model(&self, model_id: &str) -> Result<OllamaShowResponse, OllamaError> {
        let url = format!("{}/api/show", self.host);
        debug!("Fetching model details from Ollama");

        let request = ShowRequest {
            name: model_id.to_string(),
        };

        let response = ureq::post(&url)
            .timeout(REQUEST_TIMEOUT)
            .send_json(&request)
            .map_err(|e| map_ureq_error(e, "Connection refused"))?;

        let show_response: OllamaShowResponse = response.into_json()?;
        info!(model = model_id, "Fetched model details");
        Ok(show_response)
    }

    /// Get model details asynchronously via a channel
    #[instrument(skip(self))]
    pub fn show_model_async(&self, model_id: &str) -> Receiver<Result<OllamaShowResponse, OllamaError>> {
        info!(model = model_id, "Starting async model show");
        let (tx, rx) = channel();
        let host = self.host.clone();
        let model = model_id.to_string();

        thread::spawn(move || {
            let client = OllamaClient::new(host);
            let result = client.show_model(&model);
            let _ = tx.send(result);
        });

        rx
    }

    /// Unload a model from VRAM by setting keep_alive to 0
    #[instrument(skip(self), fields(model = %model_id))]
    pub fn unload_model(&self, model_id: &str) -> Result<(), OllamaError> {
        let url = format!("{}/api/generate", self.host);
        info!("Unloading model from VRAM");

        let request = UnloadRequest {
            model: model_id.to_string(),
            keep_alive: 0,
        };

        ureq::post(&url)
            .timeout(REQUEST_TIMEOUT)
            .send_json(&request)
            .map_err(|e| map_ureq_error(e, "Connection refused"))?;

        info!(model = model_id, "Model unloaded");
        Ok(())
    }

    /// Unload model asynchronously
    pub fn unload_model_async(&self, model_id: &str) -> Receiver<Result<(), OllamaError>> {
        info!(model = model_id, "Starting async model unload");
        let (tx, rx) = channel();
        let host = self.host.clone();
        let model = model_id.to_string();

        thread::spawn(move || {
            let client = OllamaClient::new(host);
            let result = client.unload_model(&model);
            let _ = tx.send(result);
        });

        rx
    }
}

impl Default for OllamaClient {
    fn default() -> Self {
        Self::default_host()
    }
}

/// Map ureq errors to OllamaError, detecting connection failures
fn map_ureq_error(e: ureq::Error, context: &str) -> OllamaError {
    let ureq::Error::Transport(ref t) = e else {
        error!("HTTP error: {}", e);
        return OllamaError::Http(e);
    };

    if t.kind() == ureq::ErrorKind::ConnectionFailed {
        error!("{}", context);
        return OllamaError::ConnectionRefused;
    }

    error!("HTTP error: {}", e);
    OllamaError::Http(e)
}
