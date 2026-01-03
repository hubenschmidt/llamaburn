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
        info!("GENERAL_WORKER: Starting execution");

        let context = match feedback {
            Some(fb) => format!("{task_description}\n\nPrevious feedback to address: {fb}"),
            None => task_description.to_string(),
        };

        let result = self.client.chat(GENERAL_WORKER_PROMPT, &context).await;

        match result {
            Ok(output) => {
                info!("GENERAL_WORKER: Execution complete");
                Ok(WorkerResult {
                    success: true,
                    output,
                    error: None,
                })
            }
            Err(e) => {
                info!("GENERAL_WORKER: Failed with error: {}", e);
                Ok(WorkerResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                })
            }
        }
    }
}
