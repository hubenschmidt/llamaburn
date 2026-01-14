use agents_core::{AgentError, EvaluatorResult, ModelConfig};
use agents_llm::LlmClient;
use tracing::info;

use crate::prompts::EVALUATOR_PROMPT;

pub struct Evaluator;

impl Evaluator {
    pub fn new() -> Self {
        Self
    }

    fn create_client(model: &ModelConfig) -> LlmClient {
        LlmClient::new(&model.model, model.api_base.as_deref())
    }

    pub async fn evaluate(
        &self,
        worker_output: &str,
        task_description: &str,
        success_criteria: &str,
        model: &ModelConfig,
    ) -> Result<EvaluatorResult, AgentError> {
        info!("EVALUATOR: Starting evaluation with model {}", model.name);

        let context = format!(
            "Task Description: {task_description}\n\nSuccess Criteria: {success_criteria}\n\nWorker Output:\n{worker_output}\n\nEvaluate this output against the success criteria and provide your assessment."
        );

        let client = Self::create_client(model);
        let (result, _metrics) = client
            .structured::<EvaluatorResult>(EVALUATOR_PROMPT, &context)
            .await?;

        let status = if result.passed { "PASS" } else { "FAIL" };
        info!("EVALUATOR: Result = {} (score: {}/100)", status, result.score);

        Ok(result)
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}
