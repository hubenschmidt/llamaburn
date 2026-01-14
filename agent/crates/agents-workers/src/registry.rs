use std::collections::HashMap;
use std::sync::Arc;

use agents_core::{AgentError, ModelConfig, Worker, WorkerResult, WorkerType};

pub struct WorkerRegistry {
    workers: HashMap<WorkerType, Arc<dyn Worker>>,
}

impl WorkerRegistry {
    pub fn new() -> Self {
        Self {
            workers: HashMap::new(),
        }
    }

    pub fn register(&mut self, worker: Arc<dyn Worker>) {
        self.workers.insert(worker.worker_type(), worker);
    }

    pub async fn execute(
        &self,
        worker_type: WorkerType,
        task_description: &str,
        parameters: &serde_json::Value,
        feedback: Option<&str>,
        model: &ModelConfig,
    ) -> Result<WorkerResult, AgentError> {
        let worker = self
            .workers
            .get(&worker_type)
            .ok_or_else(|| AgentError::UnknownWorker(format!("{:?}", worker_type)))?;

        worker.execute(task_description, parameters, feedback, model).await
    }
}

impl Default for WorkerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
