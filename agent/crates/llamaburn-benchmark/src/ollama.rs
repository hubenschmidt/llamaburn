use llamaburn_core::{LlamaBurnError, ModelConfig, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct OllamaClient {
    host: String,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    size: u64,
    #[serde(default)]
    details: Option<ModelDetails>,
}

#[derive(Debug, Deserialize)]
struct ModelDetails {
    parameter_size: Option<String>,
    quantization_level: Option<String>,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<ChatOptions>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub message: ResponseMessage,
    #[serde(default)]
    pub eval_count: Option<u64>,
    #[serde(default)]
    pub eval_duration: Option<u64>,
    #[serde(default)]
    pub load_duration: Option<u64>,
    #[serde(default)]
    pub prompt_eval_duration: Option<u64>,
    #[serde(default)]
    pub prompt_eval_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: String,
}

impl OllamaClient {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn list_models(&self) -> Result<Vec<ModelConfig>> {
        let url = format!("{}/api/tags", self.host);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| LlamaBurnError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(LlamaBurnError::OllamaError(format!(
                "Failed to list models: {}",
                resp.status()
            )));
        }

        let tags: TagsResponse = resp
            .json()
            .await
            .map_err(|e| LlamaBurnError::Http(e.to_string()))?;

        let models = tags
            .models
            .into_iter()
            .map(|m| {
                let quantization = m
                    .details
                    .as_ref()
                    .and_then(|d| d.quantization_level.clone());
                let display_name = match &quantization {
                    Some(q) => format!("{}:{} (Local)", m.name, q),
                    None => format!("{} (Local)", m.name),
                };
                ModelConfig {
                    id: m.name.clone(),
                    name: display_name,
                    model: m.name,
                    api_base: Some(self.host.clone()),
                    quantization,
                }
            })
            .collect();

        Ok(models)
    }

    pub async fn chat(
        &self,
        model: &str,
        prompt: &str,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<ChatResponse> {
        let url = format!("{}/api/chat", self.host);

        let options = if temperature.is_some() || max_tokens.is_some() {
            Some(ChatOptions {
                temperature,
                num_predict: max_tokens,
            })
        } else {
            None
        };

        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            stream: false,
            options,
        };

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| LlamaBurnError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(LlamaBurnError::OllamaError(format!(
                "Chat failed: {} - {}",
                status, body
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| LlamaBurnError::Http(e.to_string()))?;

        let chat_resp: ChatResponse = serde_json::from_str(&body).map_err(|e| {
            LlamaBurnError::Http(format!(
                "Failed to parse response: {} - Body: {}",
                e,
                &body[..body.len().min(500)]
            ))
        })?;

        Ok(chat_resp)
    }

    pub async fn warmup(&self, model: &str) -> Result<()> {
        tracing::info!("Warming up model: {}", model);
        self.chat(model, "hi", Some(0.0), Some(1)).await?;
        Ok(())
    }

    pub async fn unload(&self, model: &str) -> Result<()> {
        tracing::info!("Unloading model: {}", model);
        let url = format!("{}/api/chat", self.host);

        let request = serde_json::json!({
            "model": model,
            "messages": [],
            "keep_alive": 0
        });

        self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| LlamaBurnError::Http(e.to_string()))?;

        Ok(())
    }
}
