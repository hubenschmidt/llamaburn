use std::env;
use std::sync::Arc;

use agents_core::{Message, MessageRole, ModelConfig};
use agents_pipeline::{Evaluator, Frontline, Orchestrator, PipelineRunner};
use agents_workers::{EmailWorker, GeneralWorker, SearchWorker, WorkerRegistry};
use dashmap::DashMap;
use tracing::warn;

fn default_models() -> Vec<ModelConfig> {
    vec![
        ModelConfig {
            id: "openai-gpt4o".into(),
            name: "GPT-4o (OpenAI)".into(),
            model: "gpt-4o".into(),
            api_base: None,
        },
        ModelConfig {
            id: "ollama-nemotron".into(),
            name: "Nemotron 30B (Local)".into(),
            model: "hf.co/unsloth/Nemotron-3-Nano-30B-A3B-GGUF:Q4_1".into(),
            api_base: Some("http://host.docker.internal:11434/v1".into()),
        },
    ]
}

pub struct AppState {
    pub pipeline: PipelineRunner,
    pub conversations: DashMap<String, Vec<Message>>,
    pub models: Vec<ModelConfig>,
}

impl AppState {
    pub fn new() -> Self {
        let models = default_models();

        let frontline = Frontline::new();
        let orchestrator = Orchestrator::new();
        let evaluator = Evaluator::new();

        let serper_key = env::var("SERPER_API_KEY").unwrap_or_default();
        let sendgrid_key = env::var("SENDGRID_API_KEY").unwrap_or_default();
        let from_email =
            env::var("SENDGRID_FROM_EMAIL").unwrap_or_else(|_| "noreply@example.com".to_string());

        let general_worker = GeneralWorker::new();
        let search_worker = SearchWorker::new(serper_key.clone()).ok();
        let email_worker = EmailWorker::new(sendgrid_key.clone(), from_email.clone()).ok();

        let mut workers = WorkerRegistry::new();
        workers.register(Arc::new(GeneralWorker::new()));

        if let Ok(w) = SearchWorker::new(serper_key) {
            workers.register(Arc::new(w));
        } else {
            warn!("SearchWorker disabled: SERPER_API_KEY not configured");
        }

        if let Ok(w) = EmailWorker::new(sendgrid_key, from_email) {
            workers.register(Arc::new(w));
        } else {
            warn!("EmailWorker disabled: SENDGRID_API_KEY not configured");
        }

        let pipeline = PipelineRunner::new(
            frontline,
            orchestrator,
            evaluator,
            workers,
            general_worker,
            search_worker,
            email_worker,
        );

        Self {
            pipeline,
            conversations: DashMap::new(),
            models,
        }
    }

    pub fn get_model(&self, model_id: &str) -> ModelConfig {
        self.models
            .iter()
            .find(|m| m.id == model_id)
            .cloned()
            .unwrap_or_else(|| self.models[0].clone())
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
