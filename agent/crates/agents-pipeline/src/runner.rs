use agents_core::{AgentError, Message, ModelConfig, OrchestratorDecision, WorkerType};
use agents_llm::LlmStream;
use agents_workers::{EmailWorker, GeneralWorker, SearchWorker, WorkerRegistry};
use tracing::info;

use crate::{Evaluator, Frontline, Orchestrator};

const MAX_RETRIES: usize = 3;

pub enum StreamResponse {
    Complete(String),
    Stream(LlmStream),
}

pub struct PipelineRunner {
    frontline: Frontline,
    orchestrator: Orchestrator,
    evaluator: Evaluator,
    workers: WorkerRegistry,
    general_worker: GeneralWorker,
    search_worker: Option<SearchWorker>,
    email_worker: Option<EmailWorker>,
}

impl PipelineRunner {
    pub fn new(
        frontline: Frontline,
        orchestrator: Orchestrator,
        evaluator: Evaluator,
        workers: WorkerRegistry,
        general_worker: GeneralWorker,
        search_worker: Option<SearchWorker>,
        email_worker: Option<EmailWorker>,
    ) -> Self {
        Self {
            frontline,
            orchestrator,
            evaluator,
            workers,
            general_worker,
            search_worker,
            email_worker,
        }
    }

    pub async fn process(
        &self,
        user_input: &str,
        history: &[Message],
        use_evaluator: bool,
        model: &ModelConfig,
    ) -> Result<String, AgentError> {
        let (should_route, response) = self.frontline.process(user_input, history, model).await?;

        if !should_route {
            return Ok(response);
        }

        let decision = self.orchestrator.route(user_input, history, model).await?;

        info!(
            "ORCHESTRATOR: Routing to {:?}",
            decision.worker_type
        );

        if !use_evaluator {
            return self.execute_without_evaluation(decision, model).await;
        }

        self.execute_with_evaluation(decision, model).await
    }

    pub async fn process_stream(
        &self,
        user_input: &str,
        history: &[Message],
        model: &ModelConfig,
    ) -> Result<StreamResponse, AgentError> {
        let frontline_stream = self.frontline.process_stream(user_input, history, model).await?;
        if let Some(stream) = frontline_stream {
            return Ok(StreamResponse::Stream(stream));
        }

        let decision = self.orchestrator.route(user_input, history, model).await?;
        info!("ORCHESTRATOR (stream): Routing to {:?}", decision.worker_type);

        self.execute_worker_stream(decision, model).await
    }

    async fn execute_worker_stream(
        &self,
        decision: OrchestratorDecision,
        model: &ModelConfig,
    ) -> Result<StreamResponse, AgentError> {
        match decision.worker_type {
            WorkerType::General => {
                let stream = self.general_worker.execute_stream(&decision.task_description, model).await?;
                Ok(StreamResponse::Stream(stream))
            }
            WorkerType::Search => {
                let Some(ref worker) = self.search_worker else {
                    return Ok(StreamResponse::Complete("Search worker not configured".into()));
                };
                let stream = worker.execute_stream(&decision.task_description, &decision.parameters, model).await?;
                Ok(StreamResponse::Stream(stream))
            }
            WorkerType::Email => {
                let Some(ref _worker) = self.email_worker else {
                    return Ok(StreamResponse::Complete("Email worker not configured".into()));
                };
                let result = self.execute_without_evaluation(decision, model).await?;
                Ok(StreamResponse::Complete(result))
            }
        }
    }

    async fn execute_without_evaluation(
        &self,
        decision: OrchestratorDecision,
        model: &ModelConfig,
    ) -> Result<String, AgentError> {
        let worker_result = self
            .workers
            .execute(
                decision.worker_type,
                &decision.task_description,
                &decision.parameters,
                None,
                model,
            )
            .await?;

        if !worker_result.success {
            let error = worker_result.error.unwrap_or_else(|| "Unknown error".into());
            return Ok(format!("Error: {}", error));
        }

        Ok(worker_result.output)
    }

    async fn execute_with_evaluation(
        &self,
        decision: OrchestratorDecision,
        model: &ModelConfig,
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
                    model,
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
                    model,
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
