use agents_core::{AgentError, Message, ModelConfig, OrchestratorDecision};
use agents_llm::LlmClient;
use tracing::info;

use crate::prompts::ORCHESTRATOR_PROMPT;

pub struct Orchestrator;

impl Orchestrator {
    pub fn new() -> Self {
        Self
    }

    fn create_client(model: &ModelConfig) -> LlmClient {
        LlmClient::new(&model.model, model.api_base.as_deref())
    }

    pub async fn route(
        &self,
        user_input: &str,
        history: &[Message],
        model: &ModelConfig,
    ) -> Result<OrchestratorDecision, AgentError> {
        info!("ORCHESTRATOR: Routing request with model {}", model.name);

        let history_context = if history.is_empty() {
            String::new()
        } else {
            let recent: Vec<_> = history.iter().rev().take(6).rev().collect();
            recent
                .iter()
                .map(|m| format!("{:?}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let context = format!(
            "Conversation History:\n{history_context}\n\nCurrent User Request: {user_input}\n\nAnalyze this request and determine which worker should handle it."
        );

        let client = Self::create_client(model);
        let (decision, _metrics) = client
            .structured::<OrchestratorDecision>(ORCHESTRATOR_PROMPT, &context)
            .await?;

        info!(
            "ORCHESTRATOR: Routing to {:?} - {}",
            decision.worker_type,
            &decision.task_description[..decision.task_description.len().min(80)]
        );

        Ok(decision)
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}
