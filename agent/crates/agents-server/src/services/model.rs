use std::sync::Arc;

use agents_core::ModelConfig;
use agents_llm::unload_model;
use agents_pipeline::StreamResponse;
use futures::StreamExt;
use tracing::info;

use crate::error::AppError;
use crate::state::AppState;

pub async fn warmup(
    state: &Arc<AppState>,
    model_id: &str,
    previous_model_id: Option<&str>,
) -> Result<ModelConfig, AppError> {
    unload_previous(state, previous_model_id).await;

    let model = state.get_model(model_id);
    info!("Warming up model: {}", model.name);

    let result = state
        .pipeline
        .process_stream("hi", &[], &model)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let StreamResponse::Stream(mut stream) = result else {
        return Ok(model);
    };

    while stream.next().await.is_some() {}

    info!("Model {} ready", model.name);
    Ok(model)
}

pub async fn unload(state: &Arc<AppState>, model_id: &str) -> Result<(), AppError> {
    let model = state.get_model(model_id);

    let Some(api_base) = &model.api_base else {
        return Ok(()); // Not a local model, nothing to unload
    };

    let ollama_host = api_base.trim_end_matches("/v1");
    info!("Unloading model: {}", model.name);

    unload_model(ollama_host, &model.model)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(())
}

async fn unload_previous(state: &Arc<AppState>, previous_model_id: Option<&str>) {
    let Some(prev_id) = previous_model_id else {
        return;
    };

    if let Err(e) = unload(state, prev_id).await {
        info!("Note: Could not unload model (may already be unloaded): {:?}", e);
    }
}
