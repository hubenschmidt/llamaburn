use agents_core::{AgentError, ModelConfig, Worker, WorkerResult, WorkerType};
use agents_llm::{LlmClient, LlmStream};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::prompts::SEARCH_WORKER_PROMPT;

#[derive(Debug, Serialize)]
struct SerperRequest {
    q: String,
    num: u8,
}

#[derive(Debug, Deserialize)]
struct SerperResponse {
    organic: Option<Vec<OrganicResult>>,
}

#[derive(Debug, Deserialize)]
struct OrganicResult {
    title: Option<String>,
    link: Option<String>,
    snippet: Option<String>,
}

pub struct SearchWorker {
    http: reqwest::Client,
    api_key: String,
}

impl SearchWorker {
    pub fn new(api_key: String) -> Result<Self, AgentError> {
        if api_key.is_empty() {
            return Err(AgentError::ExternalApi("SERPER_API_KEY not configured".into()));
        }
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
        })
    }

    fn create_client(model: &ModelConfig) -> LlmClient {
        LlmClient::new(&model.model, model.api_base.as_deref())
    }

    async fn search(&self, query: &str, num_results: u8) -> Result<Vec<OrganicResult>, AgentError> {
        info!("SearchWorker: Calling Serper for query: {}", query);

        let request_body = SerperRequest {
            q: query.to_string(),
            num: num_results,
        };

        let response = self
            .http
            .post("https://google.serper.dev/search")
            .header("X-API-KEY", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| AgentError::ExternalApi(e.to_string()))?;

        let status = response.status();
        let text = response.text().await.map_err(|e| AgentError::ExternalApi(e.to_string()))?;

        info!("SearchWorker: Serper status={}, response_len={}", status, text.len());

        let data: SerperResponse = serde_json::from_str(&text)
            .map_err(|e| AgentError::ExternalApi(format!("Parse error: {} - body: {}", e, &text[..text.len().min(500)])))?;

        let results = data.organic.unwrap_or_default();
        info!("SearchWorker: Got {} results", results.len());

        Ok(results)
    }

    fn format_results(results: &[OrganicResult]) -> String {
        results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "{}. {}\n   {}\n   {}",
                    i + 1,
                    r.title.as_deref().unwrap_or(""),
                    r.link.as_deref().unwrap_or(""),
                    r.snippet.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub async fn execute_stream(
        &self,
        task_description: &str,
        parameters: &serde_json::Value,
        model: &ModelConfig,
    ) -> Result<LlmStream, AgentError> {
        info!("SearchWorker: streaming response with model {}", model.name);

        let query = parameters
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or(task_description);

        let num_results = parameters
            .get("num_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as u8)
            .unwrap_or(5);

        let search_results = self.search(query, num_results).await?;

        let formatted = Self::format_results(&search_results);
        info!("SearchWorker: Formatted results:\n{}", &formatted[..formatted.len().min(500)]);

        let context = format!(
            "Task: {task_description}\n\nSearch Results:\n{formatted}\n\nSynthesize these results into a clear response."
        );

        let client = Self::create_client(model);
        client.chat_stream(SEARCH_WORKER_PROMPT, &context).await
    }
}

#[async_trait]
impl Worker for SearchWorker {
    fn worker_type(&self) -> WorkerType {
        WorkerType::Search
    }

    async fn execute(
        &self,
        task_description: &str,
        parameters: &serde_json::Value,
        feedback: Option<&str>,
        model: &ModelConfig,
    ) -> Result<WorkerResult, AgentError> {
        info!("SearchWorker: executing with model {}", model.name);

        let query = parameters
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or(task_description);

        let num_results = parameters
            .get("num_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as u8)
            .unwrap_or(5);

        let search_results = match self.search(query, num_results).await {
            Ok(results) => results,
            Err(e) => return Ok(WorkerResult::err(e)),
        };

        let feedback_section = feedback
            .map(|fb| format!("\n\nPrevious feedback: {fb}"))
            .unwrap_or_default();

        let context = format!(
            "Task: {task_description}\n\nSearch Results:\n{}{feedback_section}\n\nSynthesize these results into a clear response.",
            Self::format_results(&search_results)
        );

        let client = Self::create_client(model);
        match client.chat(SEARCH_WORKER_PROMPT, &context).await {
            Ok(resp) => Ok(WorkerResult::ok(resp.content)),
            Err(e) => Ok(WorkerResult::err(e)),
        }
    }
}
