use agents_core::{AgentError, Message, OrchestratorDecision};
use agents_llm::LlmClient;
use tracing::info;

use crate::prompts::ORCHESTRATOR_PROMPT;

pub struct Orchestrator {
    client: LlmClient,
}

impl Orchestrator {
    pub fn new(model: &str) -> Self {
        Self {
            client: LlmClient::new(model),
        }
    }

    pub async fn route(
        &self,
        user_input: &str,
        history: &[Message],
    ) -> Result<OrchestratorDecision, AgentError> {
        info!("ORCHESTRATOR: Routing request");

        let history_context = if history.is_empty() {
            String::new()
        } else {
            let recent: Vec<_> = history.iter().rev().take(6).rev().collect();
            recent
                .iter()
                .map(|m| format!("{}: {}", m.role.to_uppercase(), m.content))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let context = format!(
            "Conversation History:\n{history_context}\n\nCurrent User Request: {user_input}\n\nAnalyze this request and determine which worker should handle it."
        );

        let decision = self
            .client
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
