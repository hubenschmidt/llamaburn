use std::env;
use std::sync::Arc;

use agents_core::Message;
use agents_pipeline::{Evaluator, Frontline, Orchestrator, PipelineRunner};
use agents_workers::{EmailWorker, GeneralWorker, SearchWorker, WorkerRegistry};
use dashmap::DashMap;

pub struct AppState {
    pub pipeline: PipelineRunner,
    pub conversations: DashMap<String, Vec<Message>>,
}

impl AppState {
    pub fn new() -> Self {
        let main_model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());
        let worker_model = env::var("WORKER_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

        let frontline = Frontline::new(&main_model);
        let orchestrator = Orchestrator::new(&main_model);
        let evaluator = Evaluator::new(&worker_model);

        let serpapi_key = env::var("SERPAPI_KEY").unwrap_or_default();
        let sendgrid_key = env::var("SENDGRID_API_KEY").unwrap_or_default();
        let from_email = env::var("SENDGRID_FROM_EMAIL").unwrap_or_else(|_| "noreply@example.com".to_string());

        let mut workers = WorkerRegistry::new();
        workers.register(Arc::new(GeneralWorker::new(&worker_model)));
        workers.register(Arc::new(SearchWorker::new(&worker_model, serpapi_key)));
        workers.register(Arc::new(EmailWorker::new(&worker_model, sendgrid_key, from_email)));

        let pipeline = PipelineRunner::new(frontline, orchestrator, evaluator, workers);

        Self {
            pipeline,
            conversations: DashMap::new(),
        }
    }

    pub fn get_conversation(&self, uuid: &str) -> Vec<Message> {
        self.conversations
            .get(uuid)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    pub fn add_message(&self, uuid: &str, role: &str, content: &str) {
        self.conversations
            .entry(uuid.to_string())
            .or_default()
            .push(Message {
                role: role.to_string(),
                content: content.to_string(),
            });
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
