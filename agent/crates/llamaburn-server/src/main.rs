use axum::body::Body;
use axum::http::{Request, Response};
use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        Json,
    },
    routing::{get, post},
    Router,
};
use futures::stream::{self, Stream, StreamExt};
use llamaburn_benchmark::{ollama::OllamaClient, BenchmarkEvent, BenchmarkRunner};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use llamaburn_core::{config::BenchmarkConfig, model::ModelConfig};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

#[derive(Clone)]
struct AppState {
    ollama_host: String,
    benchmark_cancel: Arc<tokio::sync::Mutex<Option<CancellationToken>>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter("info")
        .init();

    let ollama_host =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "./dist".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".to_string());

    let state = AppState {
        ollama_host,
        benchmark_cancel: Arc::new(tokio::sync::Mutex::new(None)),
    };

    let trace_layer = TraceLayer::new_for_http()
        .on_request(|req: &Request<Body>, _span: &tracing::Span| {
            tracing::info!("{} {}", req.method(), req.uri());
        })
        .on_response(
            |res: &Response<Body>, latency: Duration, _span: &tracing::Span| {
                tracing::info!("→ {} ({} ms)", res.status().as_u16(), latency.as_millis());
            },
        );

    let api_routes = Router::new()
        .route("/models", get(get_models))
        .route("/benchmark", get(run_benchmark_stream))
        .route("/benchmark/cancel", post(cancel_benchmark))
        .route("/load", post(load_model))
        .route("/unload", post(unload_model))
        .route("/status", get(get_status))
        .route("/gpu/stream", get(gpu_stream))
        .with_state(state);

    let app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(ServeDir::new(&static_dir))
        .layer(trace_layer)
        .layer(CorsLayer::permissive());

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Server listening on {}", addr);
    tracing::info!("Serving static files from {}", static_dir);
    tracing::info!("HTTP request logging enabled");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_models(State(state): State<AppState>) -> Result<Json<Vec<ModelConfig>>, StatusCode> {
    tracing::info!("GET /api/models called");
    let client = OllamaClient::new(&state.ollama_host);
    match client.list_models().await {
        Ok(models) => Ok(Json(models)),
        Err(e) => {
            tracing::error!("Failed to list models: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Debug, Deserialize)]
struct BenchmarkQuery {
    model: String,
    #[serde(default)]
    iterations: Option<u32>,
    #[serde(default)]
    warmup: Option<u32>,
    #[serde(default)]
    temp: Option<f32>,
}

async fn run_benchmark_stream(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<BenchmarkQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let cancel_token = CancellationToken::new();
    {
        let mut cancel_guard = state.benchmark_cancel.lock().await;
        *cancel_guard = Some(cancel_token.clone());
    }

    let config = BenchmarkConfig {
        benchmark_type: Default::default(),
        model_id: query.model,
        iterations: query.iterations.unwrap_or(5),
        warmup_runs: query.warmup.unwrap_or(2),
        prompt_set: "default".to_string(),
        temperature: query.temp.unwrap_or(0.0),
        max_tokens: None,
        top_p: None,
        top_k: None,
    };

    let prompts = vec![
        "Explain the concept of recursion in programming.".to_string(),
        "What are the benefits of using a database index?".to_string(),
        "Describe the difference between TCP and UDP.".to_string(),
    ];

    let (tx, rx) = mpsc::channel::<BenchmarkEvent>(100);
    let runner = BenchmarkRunner::new(&state.ollama_host);
    let state_clone = state.clone();

    tokio::spawn(async move {
        runner.run_streaming(&config, &prompts, cancel_token, tx).await;
        let mut cancel_guard = state_clone.benchmark_cancel.lock().await;
        *cancel_guard = None;
    });

    let stream = ReceiverStream::new(rx).map(|event| {
        let json = serde_json::to_string(&event).unwrap_or_default();
        Ok(Event::default().data(json))
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn cancel_benchmark(State(state): State<AppState>) -> StatusCode {
    let cancel_guard = state.benchmark_cancel.lock().await;
    if let Some(token) = cancel_guard.as_ref() {
        token.cancel();
        tracing::info!("Benchmark cancelled");
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

#[derive(Serialize)]
struct StatusResponse {
    ollama_host: String,
    ollama_connected: bool,
    model_count: usize,
}

async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let client = OllamaClient::new(&state.ollama_host);
    let (connected, count) = match client.list_models().await {
        Ok(models) => (true, models.len()),
        Err(_) => (false, 0),
    };

    Json(StatusResponse {
        ollama_host: state.ollama_host,
        ollama_connected: connected,
        model_count: count,
    })
}

#[derive(Debug, Deserialize)]
struct UnloadRequest {
    model_id: String,
}

#[derive(Serialize)]
struct UnloadResponse {
    success: bool,
}

async fn unload_model(
    State(state): State<AppState>,
    Json(req): Json<UnloadRequest>,
) -> Result<Json<UnloadResponse>, StatusCode> {
    let client = OllamaClient::new(&state.ollama_host);
    match client.unload(&req.model_id).await {
        Ok(_) => Ok(Json(UnloadResponse { success: true })),
        Err(e) => {
            tracing::error!("Failed to unload model: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Debug, Deserialize)]
struct LoadRequest {
    model_id: String,
}

#[derive(Serialize)]
struct LoadResponse {
    success: bool,
}

async fn load_model(
    State(state): State<AppState>,
    Json(req): Json<LoadRequest>,
) -> Result<Json<LoadResponse>, StatusCode> {
    let client = OllamaClient::new(&state.ollama_host);
    match client.warmup(&req.model_id).await {
        Ok(_) => Ok(Json(LoadResponse { success: true })),
        Err(e) => {
            tracing::error!("Failed to load model: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn gpu_stream() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold((), |_| async {
        let output = get_rocm_smi_output().await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        Some((Ok(Event::default().data(output)), ()))
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}

async fn get_rocm_smi_output() -> String {
    let result = Command::new("rocm-smi")
        .arg("--showuse")
        .arg("--showmemuse")
        .arg("--showtemp")
        .output()
        .await;

    match result {
        Ok(output) => {
            if output.status.success() {
                String::from_utf8_lossy(&output.stdout).to_string()
            } else {
                format!(
                    "rocm-smi error: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
            }
        }
        Err(e) => format!("Failed to run rocm-smi: {}", e),
    }
}
