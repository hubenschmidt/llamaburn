use agents_core::{AgentError, Message, OrchestratorDecision, WorkerType};
use agents_workers::WorkerRegistry;
use tracing::info;

use crate::{Evaluator, Frontline, Orchestrator};

const MAX_RETRIES: usize = 3;

pub struct PipelineRunner {
    frontline: Frontline,
    orchestrator: Orchestrator,
    evaluator: Evaluator,
    workers: WorkerRegistry,
}

impl PipelineRunner {
    pub fn new(
        frontline: Frontline,
        orchestrator: Orchestrator,
        evaluator: Evaluator,
        workers: WorkerRegistry,
    ) -> Self {
        Self {
            frontline,
            orchestrator,
            evaluator,
            workers,
        }
    }

    pub async fn process(
        &self,
        user_input: &str,
        history: &[Message],
    ) -> Result<String, AgentError> {
        let (should_route, response) = self.frontline.process(user_input, history).await?;

        if !should_route {
            return Ok(response);
        }

        let decision = self.orchestrator.route(user_input, history).await?;

        info!(
            "ORCHESTRATOR: Routing to {:?}",
            decision.worker_type
        );

        if decision.worker_type == WorkerType::None {
            return Ok(format!(
                "I'm unable to help with that request. {}",
                decision.task_description
            ));
        }

        self.execute_with_evaluation(decision).await
    }

    async fn execute_with_evaluation(
        &self,
        decision: OrchestratorDecision,
    ) -> Result<String, AgentError> {
        let mut feedback: Option<String> = None;

        for attempt in 0..MAX_RETRIES {
            info!("ORCHESTRATOR: Attempt {}/{}", attempt + 1, MAX_RETRIES);

            let worker_result = self
                .workers
                .execute(
                    decision.worker_type,
                    &decision.task_description,
                    &decision.parameters,
                    feedback.as_deref(),
                )
                .await?;

            if !worker_result.success {
                let error = worker_result.error.unwrap_or_else(|| "Unknown error".into());
                info!("WORKER: Failed with error: {}", error);
                return Ok(format!("Error: {}", error));
            }

            info!("WORKER: Completed successfully");

            let eval_result = self
                .evaluator
                .evaluate(
                    &worker_result.output,
                    &decision.task_description,
                    &decision.success_criteria,
                )
                .await?;

            if eval_result.passed {
                info!("EVALUATOR: Passed (score: {}/100)", eval_result.score);
                return Ok(worker_result.output);
            }

            info!(
                "EVALUATOR: Failed (score: {}/100) - {}",
                eval_result.score,
                &eval_result.feedback[..eval_result.feedback.len().min(80)]
            );

            feedback = Some(format!(
                "{}\n\nSuggestions: {}",
                eval_result.feedback, eval_result.suggestions
            ));

            if attempt == MAX_RETRIES - 1 {
                info!("ORCHESTRATOR: Max retries reached, returning partial result");
                return Ok(format!(
                    "{}\n\n[Note: Response may not fully meet quality criteria after {} attempts. Evaluator feedback: {}]",
                    worker_result.output, MAX_RETRIES, eval_result.feedback
                ));
            }
        }

        Err(AgentError::MaxRetriesExceeded)
    }
}
