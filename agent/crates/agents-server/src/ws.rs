use std::sync::Arc;
use std::time::Instant;

use agents_core::MessageRole;
use agents_llm::{OllamaClient, OllamaMetrics, StreamChunk};
use agents_pipeline::StreamResponse;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use tracing::{error, info};

use crate::dto::{InitResponse, WsMetadata, WsPayload, WsResponse};
use crate::state::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut uuid = String::new();

    while let Some(Ok(msg)) = receiver.next().await {
        let Message::Text(text) = msg else { continue };

        let payload: WsPayload = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                error!("JSON parse error: {}", e);
                continue;
            }
        };

        info!("WS payload: {:?}", payload);

        if payload.init {
            uuid = payload.uuid.unwrap_or_else(|| "anonymous".to_string());
            info!("Connection initialized: {}", uuid);

            let init_resp = InitResponse {
                models: state.models.clone(),
            };
            let init_msg = serde_json::to_string(&init_resp).expect("serialize");
            if sender.send(Message::Text(init_msg.into())).await.is_err() {
                break;
            }
            continue;
        }

        let Some(message) = payload.message else {
            continue;
        };

        let model_id = payload.model_id.as_deref().unwrap_or("");
        let model = state.get_model(model_id);

        let preview_len = message.len().min(50);
        info!(
            "Message from {} (model: {}): {}...",
            uuid,
            model.name,
            &message[..preview_len]
        );

        let history = state.get_conversation(&uuid);
        state.add_message(&uuid, MessageRole::User, &message);

        let start = Instant::now();

        let use_ollama_native = payload.verbose && model.api_base.is_some();

        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut ollama_metrics: Option<OllamaMetrics> = None;

        let full_response = if use_ollama_native {
            let api_base = model.api_base.as_ref().unwrap();
            let client = OllamaClient::new(&model.model, api_base);

            info!("Using native Ollama API for verbose metrics");

            let result = client
                .chat_stream_with_metrics("You are a helpful assistant.", &history, &message)
                .await;

            match result {
                Ok((mut stream, metrics_collector)) => {
                    let mut accumulated = String::new();
                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(StreamChunk::Content(chunk)) => {
                                accumulated.push_str(&chunk);
                                let msg = serde_json::to_string(&WsResponse::stream(&chunk))
                                    .expect("serialize");
                                if sender.send(Message::Text(msg.into())).await.is_err() {
                                    break;
                                }
                            }
                            Ok(StreamChunk::Usage {
                                input_tokens: i,
                                output_tokens: o,
                            }) => {
                                input_tokens = i;
                                output_tokens = o;
                            }
                            Err(e) => {
                                error!("Stream error: {}", e);
                                break;
                            }
                        }
                    }
                    ollama_metrics = Some(metrics_collector.get_metrics());
                    accumulated
                }
                Err(e) => {
                    error!("Ollama error: {}", e);
                    let error_msg = "Sorry—there was an error generating the response.";
                    let msg =
                        serde_json::to_string(&WsResponse::stream(error_msg)).expect("serialize");
                    let _ = sender.send(Message::Text(msg.into())).await;
                    error_msg.to_string()
                }
            }
        } else {
            let stream_result = state
                .pipeline
                .process_stream(&message, &history, &model)
                .await;

            match stream_result {
                Ok(StreamResponse::Stream(mut stream)) => {
                    let mut accumulated = String::new();
                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(StreamChunk::Content(chunk)) => {
                                accumulated.push_str(&chunk);
                                let msg = serde_json::to_string(&WsResponse::stream(&chunk))
                                    .expect("serialize");
                                if sender.send(Message::Text(msg.into())).await.is_err() {
                                    break;
                                }
                            }
                            Ok(StreamChunk::Usage {
                                input_tokens: i,
                                output_tokens: o,
                            }) => {
                                input_tokens = i;
                                output_tokens = o;
                            }
                            Err(e) => {
                                error!("Stream error: {}", e);
                                break;
                            }
                        }
                    }
                    accumulated
                }
                Ok(StreamResponse::Complete(response)) => {
                    let msg =
                        serde_json::to_string(&WsResponse::stream(&response)).expect("serialize");
                    if sender.send(Message::Text(msg.into())).await.is_err() {
                        continue;
                    }
                    response
                }
                Err(e) => {
                    error!("Pipeline error: {}", e);
                    let error_msg = "Sorry—there was an error generating the response.";
                    let msg =
                        serde_json::to_string(&WsResponse::stream(error_msg)).expect("serialize");
                    let _ = sender.send(Message::Text(msg.into())).await;
                    error_msg.to_string()
                }
            }
        };

        let elapsed_ms = start.elapsed().as_millis() as u64;

        state.add_message(&uuid, MessageRole::Assistant, &full_response);

        let metadata = match ollama_metrics {
            Some(m) => {
                info!(
                    "Ollama metrics: {:.1} tok/s, {} tokens, {}ms total",
                    m.tokens_per_sec(),
                    m.eval_count,
                    m.total_duration_ms()
                );
                WsMetadata {
                    input_tokens: m.prompt_eval_count,
                    output_tokens: m.eval_count,
                    elapsed_ms,
                    load_duration_ms: Some(m.load_duration_ms()),
                    prompt_eval_ms: Some(m.prompt_eval_ms()),
                    eval_ms: Some(m.eval_ms()),
                    tokens_per_sec: Some(m.tokens_per_sec()),
                }
            }
            None => WsMetadata {
                input_tokens,
                output_tokens,
                elapsed_ms,
                ..Default::default()
            },
        };

        info!("Sending metadata: {}", metadata);
        let end_msg =
            serde_json::to_string(&WsResponse::end_with_metadata(metadata)).expect("serialize");
        if sender.send(Message::Text(end_msg.into())).await.is_err() {
            break;
        }
    }

    info!("Connection closed: {}", uuid);
}
