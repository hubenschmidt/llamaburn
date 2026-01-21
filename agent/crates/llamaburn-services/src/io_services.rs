//! I/O services container - no facade methods, just I/O
//!
//! This replaces the old Services struct which had ~280 lines of facade methods.
//! GUI accesses models directly; this struct only holds I/O services.

use std::sync::Arc;

use crate::{BenchmarkService, HistoryService, OllamaClient};

/// I/O services container - database, HTTP, async runners only
pub struct IoServices {
    pub benchmark: BenchmarkService,
    pub history: Arc<HistoryService>,
    pub ollama: OllamaClient,
}

impl IoServices {
    pub fn new() -> Self {
        let history = Arc::new(
            HistoryService::new(None).expect("Failed to initialize history database"),
        );

        Self {
            benchmark: BenchmarkService::new("http://localhost:11434"),
            history,
            ollama: OllamaClient::default(),
        }
    }

    /// Create with custom Ollama host
    pub fn with_host(ollama_host: impl Into<String>) -> Self {
        let host = ollama_host.into();
        let history = Arc::new(
            HistoryService::new(None).expect("Failed to initialize history database"),
        );

        Self {
            benchmark: BenchmarkService::new(&host),
            history,
            ollama: OllamaClient::new(&host),
        }
    }
}

impl Default for IoServices {
    fn default() -> Self {
        Self::new()
    }
}
