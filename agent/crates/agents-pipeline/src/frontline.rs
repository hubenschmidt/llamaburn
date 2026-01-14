use agents_core::{AgentError, FrontlineDecision, Message, ModelConfig};
use agents_llm::{LlmClient, LlmStream};
use serde::Deserialize;
use tracing::info;

use crate::prompts::{FRONTLINE_DECISION_PROMPT, FRONTLINE_PROMPT, FRONTLINE_RESPONSE_PROMPT};

#[derive(Deserialize)]
struct QuickDecision {
    should_route: bool,
}

pub struct Frontline;

impl Frontline {
    pub fn new() -> Self {
        Self
    }

    fn create_client(model: &ModelConfig) -> LlmClient {
        LlmClient::new(&model.model, model.api_base.as_deref())
    }

    fn build_history_context(history: &[Message]) -> String {
        if history.is_empty() {
            return String::new();
        }
        let recent: Vec<_> = history.iter().rev().take(4).rev().collect();
        recent
            .iter()
            .map(|m| format!("{:?}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub async fn process(
        &self,
        user_input: &str,
        history: &[Message],
        model: &ModelConfig,
    ) -> Result<(bool, String), AgentError> {
        info!("FRONTLINE: Processing request with model {}", model.name);

        let history_context = Self::build_history_context(history);
        let context = format!(
            "Recent conversation:\n{history_context}\n\nCurrent user message: {user_input}\n\nDecide whether to handle this directly or route to the orchestrator."
        );

        let client = Self::create_client(model);
        let (response, _metrics) = client.structured::<FrontlineDecision>(FRONTLINE_PROMPT, &context).await?;

        if response.should_route {
            info!("FRONTLINE: Routing to orchestrator ({})", response.response);
            return Ok((true, response.response));
        }

        info!("FRONTLINE: Handled directly");
        Ok((false, response.response))
    }

    pub async fn process_stream(
        &self,
        user_input: &str,
        history: &[Message],
        model: &ModelConfig,
    ) -> Result<Option<LlmStream>, AgentError> {
        info!("FRONTLINE: Processing request (streaming) with model {}", model.name);

        let history_context = Self::build_history_context(history);
        let context = format!("Recent conversation:\n{history_context}\n\nUser: {user_input}");

        let client = Self::create_client(model);
        let (decision, _metrics) = client
            .structured::<QuickDecision>(FRONTLINE_DECISION_PROMPT, &context)
            .await?;

        if decision.should_route {
            info!("FRONTLINE: Routing to orchestrator");
            return Ok(None);
        }

        info!("FRONTLINE: Streaming direct response");
        let stream = client.chat_stream(FRONTLINE_RESPONSE_PROMPT, &context).await?;
        Ok(Some(stream))
    }
}

impl Default for Frontline {
    fn default() -> Self {
        Self::new()
    }
}
