use std::sync::Arc;

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
    let mut user_uuid: Option<String> = None;

    while let Some(Ok(msg)) = receiver.next().await {
        let Message::Text(text) = msg else {
            continue;
        };

        let payload: WsPayload = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(e) => {
                error!("JSON parse error: {}", e);
                continue;
            }
        };

        if let Some(uuid) = &payload.uuid {
            user_uuid = Some(uuid.clone());
        }

        if payload.init {
            info!("Initialized connection for {:?}", user_uuid);
            continue;
        }

        let Some(message) = payload.message else {
            continue;
        };

        let uuid = user_uuid.clone().unwrap_or_else(|| "anonymous".to_string());
        info!("Processing message from {}: {}...", uuid, &message[..message.len().min(50)]);

        let history = state.get_conversation(&uuid);
        state.add_message(&uuid, "user", &message);

        let response = match state.pipeline.process(&message, &history).await {
            Ok(resp) => resp,
            Err(e) => {
                error!("Pipeline error: {}", e);
                "Sorry—there was an error generating the response.".to_string()
            }
        };

        state.add_message(&uuid, "assistant", &response);

        let stream_msg = serde_json::to_string(&WsResponse::stream(&response)).unwrap();
        let end_msg = serde_json::to_string(&WsResponse::end()).unwrap();

        if sender.send(Message::Text(stream_msg.into())).await.is_err() {
            break;
        }
        if sender.send(Message::Text(end_msg.into())).await.is_err() {
            break;
        }
    }

    if let Some(uuid) = user_uuid {
        info!("Connection closed for {}", uuid);
    }
}
