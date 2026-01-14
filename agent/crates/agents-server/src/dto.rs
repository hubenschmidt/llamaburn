use std::fmt;

use agents_core::ModelConfig;
use serde::{Deserialize, Serialize};

// === HTTP DTOs ===

#[derive(Debug, Deserialize)]
pub struct WakeRequest {
    pub model_id: String,
}

#[derive(Debug, Serialize)]
pub struct WakeResponse {
    pub success: bool,
    pub model: String,
}

// === WebSocket DTOs ===

#[derive(Debug, Deserialize)]
pub struct WsPayload {
    pub uuid: Option<String>,
    pub message: Option<String>,
    pub model_id: Option<String>,
    #[serde(default)]
    pub init: bool,
    #[serde(default)]
    pub verbose: bool,
}

#[derive(Debug, Serialize)]
pub struct InitResponse {
    pub models: Vec<ModelConfig>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct WsMetadata {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_per_sec: Option<f64>,
}

impl fmt::Display for WsMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms, {}/{} tokens", self.elapsed_ms, self.input_tokens, self.output_tokens)?;
        if let Some(tps) = self.tokens_per_sec {
            write!(f, ", {:.1} tok/s", tps)?;
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum WsResponse {
    Stream { on_chat_model_stream: String },
    End { on_chat_model_end: bool, metadata: Option<WsMetadata> },
}

impl WsResponse {
    pub fn stream(content: &str) -> Self {
        Self::Stream {
            on_chat_model_stream: content.to_string(),
        }
    }

    pub fn end_with_metadata(metadata: WsMetadata) -> Self {
        Self::End {
            on_chat_model_end: true,
            metadata: Some(metadata),
        }
    }
}
