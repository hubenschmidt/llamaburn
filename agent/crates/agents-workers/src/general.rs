use agents_core::{AgentError, Worker, WorkerResult, WorkerType};
use agents_llm::LlmClient;
use async_trait::async_trait;
use tracing::info;

use crate::prompts::GENERAL_WORKER_PROMPT;

pub struct GeneralWorker {
    client: LlmClient,
}

impl GeneralWorker {
    pub fn new(model: &str) -> Self {
        Self {
            client: LlmClient::new(model),
        }
    }
}

#[async_trait]
impl Worker for GeneralWorker {
    fn worker_type(&self) -> WorkerType {
        WorkerType::General
    }

    async fn execute(
        &self,
        task_description: &str,
        _parameters: &serde_json::Value,
        feedback: Option<&str>,
    ) -> Result<WorkerResult, AgentError> {
        info!("GeneralWorker: executing");

        let context = feedback
            .map(|fb| format!("{task_description}\n\nPrevious feedback: {fb}"))
            .unwrap_or_else(|| task_description.to_string());

        match self.client.chat(GENERAL_WORKER_PROMPT, &context).await {
            Ok(output) => Ok(WorkerResult::ok(output)),
            Err(e) => Ok(WorkerResult::err(e)),
        }
    }
}
