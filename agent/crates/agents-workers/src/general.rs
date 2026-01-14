use agents_core::{AgentError, ModelConfig, Worker, WorkerResult, WorkerType};
use agents_llm::{LlmClient, LlmStream};
use async_trait::async_trait;
use tracing::info;

use crate::prompts::GENERAL_WORKER_PROMPT;

pub struct GeneralWorker;

impl GeneralWorker {
    pub fn new() -> Self {
        Self
    }

    fn create_client(model: &ModelConfig) -> LlmClient {
        LlmClient::new(&model.model, model.api_base.as_deref())
    }

    pub async fn execute_stream(&self, task_description: &str, model: &ModelConfig) -> Result<LlmStream, AgentError> {
        info!("GeneralWorker: streaming response with model {}", model.name);
        let client = Self::create_client(model);
        client.chat_stream(GENERAL_WORKER_PROMPT, task_description).await
    }

    pub async fn execute_task(
        &self,
        task_description: &str,
        feedback: Option<&str>,
        model: &ModelConfig,
    ) -> Result<WorkerResult, AgentError> {
        info!("GeneralWorker: executing with model {}", model.name);

        let context = feedback
            .map(|fb| format!("{task_description}\n\nPrevious feedback: {fb}"))
            .unwrap_or_else(|| task_description.to_string());

        let client = Self::create_client(model);
        match client.chat(GENERAL_WORKER_PROMPT, &context).await {
            Ok(resp) => Ok(WorkerResult::ok(resp.content)),
            Err(e) => Ok(WorkerResult::err(e)),
        }
    }
}

impl Default for GeneralWorker {
    fn default() -> Self {
        Self::new()
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
        model: &ModelConfig,
    ) -> Result<WorkerResult, AgentError> {
        self.execute_task(task_description, feedback, model).await
    }
}
