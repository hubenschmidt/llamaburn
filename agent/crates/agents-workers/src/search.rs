use agents_core::{AgentError, Worker, WorkerResult, WorkerType};
use agents_llm::LlmClient;
use async_trait::async_trait;
use serde::Deserialize;
use tracing::info;

use crate::prompts::SEARCH_WORKER_PROMPT;

#[derive(Debug, Deserialize)]
struct SerpApiResponse {
    organic_results: Option<Vec<OrganicResult>>,
}

#[derive(Debug, Deserialize)]
struct OrganicResult {
    title: Option<String>,
    link: Option<String>,
    snippet: Option<String>,
}

pub struct SearchWorker {
    client: LlmClient,
    http: reqwest::Client,
    api_key: String,
}

impl SearchWorker {
    pub fn new(model: &str, api_key: String) -> Result<Self, AgentError> {
        if api_key.is_empty() {
            return Err(AgentError::ExternalApi("SERPAPI_KEY not configured".into()));
        }
        Ok(Self {
            client: LlmClient::new(model),
            http: reqwest::Client::new(),
            api_key,
        })
    }

    async fn search(&self, query: &str, num_results: u8) -> Result<Vec<OrganicResult>, AgentError> {
        let url = format!(
            "https://serpapi.com/search.json?q={}&api_key={}&num={}",
            urlencoding::encode(query),
            self.api_key,
            num_results
        );

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::ExternalApi(e.to_string()))?;

        let data: SerpApiResponse = response
            .json()
            .await
            .map_err(|e| AgentError::ExternalApi(e.to_string()))?;

        Ok(data.organic_results.unwrap_or_default())
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
    ) -> Result<WorkerResult, AgentError> {
        info!("SearchWorker: executing");

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

        match self.client.chat(SEARCH_WORKER_PROMPT, &context).await {
            Ok(output) => Ok(WorkerResult::ok(output)),
            Err(e) => Ok(WorkerResult::err(e)),
        }
    }
}
