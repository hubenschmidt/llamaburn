use std::sync::Arc;

use axum::{extract::State, Json};

use crate::dto::{WakeRequest, WakeResponse};
use crate::error::AppError;
use crate::services;
use crate::state::AppState;

pub async fn wake(
    State(state): State<Arc<AppState>>,
    Json(req): Json<WakeRequest>,
) -> Result<Json<WakeResponse>, AppError> {
    let model = services::model::warmup(&state, &req.model_id).await?;
    Ok(Json(WakeResponse {
        success: true,
        model: model.name,
    }))
}
