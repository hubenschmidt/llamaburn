use thiserror::Error;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("LLM request failed: {0}")]
    LlmError(String),

    #[error("Failed to parse structured output: {0}")]
    ParseError(String),

    #[error("Worker execution failed: {0}")]
    WorkerFailed(String),

    #[error("External API error: {0}")]
    ExternalApi(String),

    #[error("Max retries exceeded")]
    MaxRetriesExceeded,

    #[error("Unknown worker type: {0}")]
    UnknownWorker(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),
}

impl From<serde_json::Error> for AgentError {
    fn from(err: serde_json::Error) -> Self {
        AgentError::ParseError(err.to_string())
    }
}
