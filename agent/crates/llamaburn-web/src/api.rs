use gloo_net::http::Request;
use llamaburn_core::model::ModelConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub avg_ttft_ms: f64,
    pub avg_tps: f64,
    pub avg_total_ms: f64,
    pub min_tps: f64,
    pub max_tps: f64,
    pub iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub ollama_host: String,
    pub ollama_connected: bool,
    pub model_count: usize,
}

pub async fn fetch_models() -> Result<Vec<ModelConfig>, String> {
    Request::get("/api/models")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn fetch_status() -> Result<StatusResponse, String> {
    Request::get("/api/status")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
struct ModelRequest {
    model_id: String,
}

pub async fn load_model(model_id: String) -> Result<(), String> {
    let req = ModelRequest { model_id };
    let resp = Request::post("/api/load")
        .json(&req)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Failed to load model: {}", resp.status()))
    }
}

pub async fn unload_model(model_id: String) -> Result<(), String> {
    let req = ModelRequest { model_id };
    let resp = Request::post("/api/unload")
        .json(&req)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Failed to unload model: {}", resp.status()))
    }
}

pub async fn cancel_benchmark() -> Result<(), String> {
    let resp = Request::post("/api/benchmark/cancel")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() || resp.status() == 404 {
        Ok(())
    } else {
        Err(format!("Failed to cancel benchmark: {}", resp.status()))
    }
}
