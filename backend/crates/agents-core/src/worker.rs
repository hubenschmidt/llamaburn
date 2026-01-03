use async_trait::async_trait;

use crate::{AgentError, WorkerResult, WorkerType};

#[async_trait]
pub trait Worker: Send + Sync {
    fn worker_type(&self) -> WorkerType;

    async fn execute(
        &self,
        task_description: &str,
        parameters: &serde_json::Value,
        feedback: Option<&str>,
    ) -> Result<WorkerResult, AgentError>;
}
