mod dto;
mod error;
mod handlers;
mod services;
mod state;
mod ws;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use axum::body::Body;
use axum::http::{Request, Response};
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .compact()
        .init();

    let state = Arc::new(AppState::new());

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|req: &Request<Body>| {
            tracing::info_span!(
                "request",
                method = %req.method(),
                uri = %req.uri(),
                version = ?req.version(),
            )
        })
        .on_response(|res: &Response<Body>, latency: Duration, _span: &tracing::Span| {
            info!(
                latency = %format!("{} ms", latency.as_millis()),
                status = %res.status().as_u16(),
                "finished processing request"
            );
        });

    let logged_routes = Router::new()
        .route("/ws", get(ws::ws_handler))
        .route("/wake", post(handlers::model::wake))
        .layer(trace_layer);

    let app = Router::new()
        .merge(logged_routes)
        .route("/health", get(handlers::health))
        .layer(cors)
        .with_state(state);

    let addr = "0.0.0.0:8000";
    info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
