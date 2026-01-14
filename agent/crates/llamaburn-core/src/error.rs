use thiserror::Error;

#[derive(Error, Debug)]
pub enum LlamaBurnError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Ollama error: {0}")]
    OllamaError(String),

    #[error("Benchmark failed: {0}")]
    BenchmarkFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, LlamaBurnError>;
