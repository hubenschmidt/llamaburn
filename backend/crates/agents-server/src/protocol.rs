use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct WsPayload {
    pub uuid: Option<String>,
    pub message: Option<String>,
    #[serde(default)]
    pub init: bool,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum WsResponse {
    Stream { on_chat_model_stream: String },
    End { on_chat_model_end: bool },
}

impl WsResponse {
    pub fn stream(content: &str) -> Self {
        Self::Stream {
            on_chat_model_stream: content.to_string(),
        }
    }

    pub fn end() -> Self {
        Self::End {
            on_chat_model_end: true,
        }
    }
}
