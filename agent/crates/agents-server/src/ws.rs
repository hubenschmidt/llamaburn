use std::sync::Arc;

use agents_core::MessageRole;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use tracing::{error, info};

use crate::protocol::{WsPayload, WsResponse};
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

        if payload.init {
            uuid = payload.uuid.unwrap_or_else(|| "anonymous".to_string());
            info!("Connection initialized: {}", uuid);
            continue;
        }

        let Some(message) = payload.message else { continue };

        info!("Message from {}: {}...", uuid, &message[..message.len().min(50)]);

        let history = state.get_conversation(&uuid);
        state.add_message(&uuid, MessageRole::User, &message);

        let response = match state.pipeline.process(&message, &history).await {
            Ok(resp) => resp,
            Err(e) => {
                error!("Pipeline error: {}", e);
                "Sorry—there was an error generating the response.".to_string()
            }
        };

        state.add_message(&uuid, MessageRole::Assistant, &response);

        let stream_msg = serde_json::to_string(&WsResponse::stream(&response)).expect("serialize");
        let end_msg = serde_json::to_string(&WsResponse::end()).expect("serialize");

        if sender.send(Message::Text(stream_msg.into())).await.is_err() {
            break;
        }
        if sender.send(Message::Text(end_msg.into())).await.is_err() {
            break;
        }
    }

    info!("Connection closed: {}", uuid);
}
