use agents_core::{AgentError, FrontlineDecision, Message};
use agents_llm::LlmClient;
use tracing::info;

use crate::prompts::FRONTLINE_PROMPT;

pub struct Frontline {
    client: LlmClient,
}

impl Frontline {
    pub fn new(model: &str) -> Self {
        Self {
            client: LlmClient::new(model),
        }
    }

    pub async fn process(
        &self,
        user_input: &str,
        history: &[Message],
    ) -> Result<(bool, String), AgentError> {
        info!("FRONTLINE: Processing request");

        let history_context = if history.is_empty() {
            String::new()
        } else {
            let recent: Vec<_> = history.iter().rev().take(4).rev().collect();
            recent
                .iter()
                .map(|m| format!("{}: {}", m.role.to_uppercase(), m.content))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let context = format!(
            "Recent conversation:\n{history_context}\n\nCurrent user message: {user_input}\n\nDecide whether to handle this directly or route to the orchestrator."
        );

        let response = self.client.structured::<FrontlineDecision>(FRONTLINE_PROMPT, &context).await?;

        if response.should_route {
            info!("FRONTLINE: Routing to orchestrator ({})", response.response);
            return Ok((true, response.response));
        }

        info!("FRONTLINE: Handled directly");
        Ok((false, response.response))
    }
}
