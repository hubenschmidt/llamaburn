use std::pin::Pin;

use agents_core::{AgentError, Message, MessageRole};
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::StreamChunk;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OllamaMetrics {
    #[serde(default)]
    pub total_duration: u64,
    #[serde(default)]
    pub load_duration: u64,
    #[serde(default)]
    pub prompt_eval_count: u32,
    #[serde(default)]
    pub prompt_eval_duration: u64,
    #[serde(default)]
    pub eval_count: u32,
    #[serde(default)]
    pub eval_duration: u64,
}

impl OllamaMetrics {
    pub fn tokens_per_sec(&self) -> f64 {
        if self.eval_duration == 0 {
            return 0.0;
        }
        (self.eval_count as f64) / (self.eval_duration as f64 / 1_000_000_000.0)
    }

    pub fn total_duration_ms(&self) -> u64 {
        self.total_duration / 1_000_000
    }

    pub fn load_duration_ms(&self) -> u64 {
        self.load_duration / 1_000_000
    }

    pub fn prompt_eval_ms(&self) -> u64 {
        self.prompt_eval_duration / 1_000_000
    }

    pub fn eval_ms(&self) -> u64 {
        self.eval_duration / 1_000_000
    }
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: Option<OllamaResponseMessage>,
    done: bool,
    #[serde(flatten)]
    metrics: OllamaMetrics,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

pub struct OllamaClient {
    client: Client,
    api_base: String,
    model: String,
}

impl OllamaClient {
    pub fn new(model: &str, api_base: &str) -> Self {
        let base = api_base
            .trim_end_matches('/')
            .replace("/v1", "");

        Self {
            client: Client::new(),
            api_base: base,
            model: model.to_string(),
        }
    }

    fn build_messages(system_prompt: &str, history: &[Message], user_input: &str) -> Vec<OllamaMessage> {
        let mut messages = vec![OllamaMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
        }];

        for msg in history {
            messages.push(OllamaMessage {
                role: match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                }
                .to_string(),
                content: msg.content.clone(),
            });
        }

        messages.push(OllamaMessage {
            role: "user".to_string(),
            content: user_input.to_string(),
        });

        messages
    }

    pub async fn chat_with_metrics(
        &self,
        system_prompt: &str,
        history: &[Message],
        user_input: &str,
    ) -> Result<(String, OllamaMetrics), AgentError> {
        let url = format!("{}/api/chat", self.api_base);

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages: Self::build_messages(system_prompt, history, user_input),
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let resp: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let content = resp
            .message
            .map(|m| m.content)
            .unwrap_or_default();

        info!(
            "Ollama: {}ms total, {:.1} tok/s, {} eval tokens",
            resp.metrics.total_duration_ms(),
            resp.metrics.tokens_per_sec(),
            resp.metrics.eval_count
        );

        Ok((content, resp.metrics))
    }

    pub async fn chat_stream_with_metrics(
        &self,
        system_prompt: &str,
        history: &[Message],
        user_input: &str,
    ) -> Result<(Pin<Box<dyn Stream<Item = Result<StreamChunk, AgentError>> + Send>>, OllamaMetricsCollector), AgentError>
    {
        use futures::StreamExt;

        let url = format!("{}/api/chat", self.api_base);

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages: Self::build_messages(system_prompt, history, user_input),
            stream: true,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let metrics_collector = OllamaMetricsCollector::new();
        let collector_clone = metrics_collector.clone();

        let stream = response.bytes_stream();

        let mapped = stream.filter_map(move |result| {
            let collector = collector_clone.clone();
            async move {
                match result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        for line in text.lines() {
                            let line = line.trim();
                            if line.is_empty() {
                                continue;
                            }

                            if let Ok(resp) = serde_json::from_str::<OllamaChatResponse>(line) {
                                if resp.done {
                                    collector.set_metrics(resp.metrics);
                                    return Some(Ok(StreamChunk::Usage {
                                        input_tokens: collector.get_metrics().prompt_eval_count,
                                        output_tokens: collector.get_metrics().eval_count,
                                    }));
                                }

                                if let Some(msg) = resp.message {
                                    if !msg.content.is_empty() {
                                        return Some(Ok(StreamChunk::Content(msg.content)));
                                    }
                                }
                            }
                        }
                        None
                    }
                    Err(e) => Some(Err(AgentError::LlmError(e.to_string()))),
                }
            }
        });

        Ok((Box::pin(mapped), metrics_collector))
    }
}

#[derive(Clone)]
pub struct OllamaMetricsCollector {
    metrics: std::sync::Arc<std::sync::Mutex<OllamaMetrics>>,
}

impl OllamaMetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: std::sync::Arc::new(std::sync::Mutex::new(OllamaMetrics::default())),
        }
    }

    pub fn set_metrics(&self, metrics: OllamaMetrics) {
        if let Ok(mut m) = self.metrics.lock() {
            *m = metrics;
        }
    }

    pub fn get_metrics(&self) -> OllamaMetrics {
        self.metrics.lock().map(|m| m.clone()).unwrap_or_default()
    }
}

impl Default for OllamaMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
