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
    pub fn new(model: &str, api_key: String) -> Self {
        Self {
            client: LlmClient::new(model),
            http: reqwest::Client::new(),
            api_key,
        }
    }

    async fn search(&self, query: &str, num_results: u8) -> Result<Vec<SearchResult>, AgentError> {
        if self.api_key.is_empty() {
            return Err(AgentError::ExternalApi("SERPAPI_KEY not configured".into()));
        }

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

        let results = data
            .organic_results
            .unwrap_or_default()
            .into_iter()
            .take(num_results as usize)
            .map(|r| SearchResult {
                title: r.title.unwrap_or_default(),
                link: r.link.unwrap_or_default(),
                snippet: r.snippet.unwrap_or_default(),
            })
            .collect();

        Ok(results)
    }

    fn format_results(results: &[SearchResult]) -> String {
        results
            .iter()
            .enumerate()
            .map(|(i, r)| format!("{}. {}\n   {}\n   {}", i + 1, r.title, r.link, r.snippet))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

struct SearchResult {
    title: String,
    link: String,
    snippet: String,
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
        info!("SEARCH_WORKER: Starting execution");

        let query = parameters
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or(task_description);

        let num_results = parameters
            .get("num_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as u8)
            .unwrap_or(5);

        info!("SEARCH_WORKER: Searching for '{}' ({} results)", query, num_results);

        let search_results = match self.search(query, num_results).await {
            Ok(results) => results,
            Err(e) => {
                return Ok(WorkerResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };

        info!("SEARCH_WORKER: Got {} results", search_results.len());

        let feedback_section = feedback
            .map(|fb| format!("Previous feedback to address: {fb}"))
            .unwrap_or_default();

        let context = format!(
            "Task: {task_description}\n\nSearch Results:\n{}\n\n{feedback_section}\n\nSynthesize these results into a clear, informative response.",
            Self::format_results(&search_results)
        );

        let result = self.client.chat(SEARCH_WORKER_PROMPT, &context).await;

        match result {
            Ok(output) => {
                info!("SEARCH_WORKER: Execution complete");
                Ok(WorkerResult {
                    success: true,
                    output,
                    error: None,
                })
            }
            Err(e) => {
                info!("SEARCH_WORKER: Failed with error: {}", e);
                Ok(WorkerResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                })
            }
        }
    }
}
