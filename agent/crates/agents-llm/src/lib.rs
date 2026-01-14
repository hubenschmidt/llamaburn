mod client;
mod ollama;

pub use client::{LlmClient, LlmMetrics, LlmResponse, LlmStream, StreamChunk};
pub use ollama::{discover_models, unload_model, OllamaClient, OllamaMetrics, OllamaMetricsCollector};
