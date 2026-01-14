use std::sync::Arc;

use agents_core::ModelConfig;
use agents_pipeline::StreamResponse;
use futures::StreamExt;
use tracing::info;

use crate::error::AppError;
use crate::state::AppState;

pub async fn warmup(state: &Arc<AppState>, model_id: &str) -> Result<ModelConfig, AppError> {
    let model = state.get_model(model_id);
    info!("Warming up model: {}", model.name);

    let result = state
        .pipeline
        .process_stream("hi", &[], &model)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if let StreamResponse::Stream(mut stream) = result {
        while stream.next().await.is_some() {}
    }

    info!("Model {} ready", model.name);
    Ok(model)
}
