use agents_core::{AgentError, EvaluatorResult};
use agents_llm::LlmClient;
use tracing::info;

use crate::prompts::EVALUATOR_PROMPT;

pub struct Evaluator {
    client: LlmClient,
}

impl Evaluator {
    pub fn new(model: &str) -> Self {
        Self {
            client: LlmClient::new(model),
        }
    }

    pub async fn evaluate(
        &self,
        worker_output: &str,
        task_description: &str,
        success_criteria: &str,
    ) -> Result<EvaluatorResult, AgentError> {
        info!("EVALUATOR: Starting evaluation");

        let context = format!(
            "Task Description: {task_description}\n\nSuccess Criteria: {success_criteria}\n\nWorker Output:\n{worker_output}\n\nEvaluate this output against the success criteria and provide your assessment."
        );

        let result = self
            .client
            .structured::<EvaluatorResult>(EVALUATOR_PROMPT, &context)
            .await?;

        let status = if result.passed { "PASS" } else { "FAIL" };
        info!("EVALUATOR: Result = {} (score: {}/100)", status, result.score);

        Ok(result)
    }
}
