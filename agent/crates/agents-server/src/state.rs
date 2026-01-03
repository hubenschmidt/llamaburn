use std::env;
use std::sync::Arc;

use agents_core::{Message, MessageRole};
use agents_pipeline::{Evaluator, Frontline, Orchestrator, PipelineRunner};
use agents_workers::{EmailWorker, GeneralWorker, SearchWorker, WorkerRegistry};
use dashmap::DashMap;
use tracing::warn;

pub struct AppState {
    pub pipeline: PipelineRunner,
    pub conversations: DashMap<String, Vec<Message>>,
}

impl AppState {
    pub fn new() -> Self {
        let main_model =
            env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-5.2-chat-latest".to_string());
        let worker_model = env::var("WORKER_MODEL").unwrap_or_else(|_| "gpt-5.1".to_string());

        let frontline = Frontline::new(&main_model);
        let orchestrator = Orchestrator::new(&main_model);
        let evaluator = Evaluator::new(&worker_model);

        let serpapi_key = env::var("SERPAPI_KEY").unwrap_or_default();
        let sendgrid_key = env::var("SENDGRID_API_KEY").unwrap_or_default();
        let from_email =
            env::var("SENDGRID_FROM_EMAIL").unwrap_or_else(|_| "noreply@example.com".to_string());

        let mut workers = WorkerRegistry::new();
        workers.register(Arc::new(GeneralWorker::new(&worker_model)));

        if let Ok(w) = SearchWorker::new(&worker_model, serpapi_key) {
            workers.register(Arc::new(w));
        } else {
            warn!("SearchWorker disabled: SERPAPI_KEY not configured");
        }

        if let Ok(w) = EmailWorker::new(&worker_model, sendgrid_key, from_email) {
            workers.register(Arc::new(w));
        } else {
            warn!("EmailWorker disabled: SENDGRID_API_KEY not configured");
        }

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

    pub fn add_message(&self, uuid: &str, role: MessageRole, content: &str) {
        self.conversations
            .entry(uuid.to_string())
            .or_default()
            .push(Message {
                role,
                content: content.to_string(),
            });
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
