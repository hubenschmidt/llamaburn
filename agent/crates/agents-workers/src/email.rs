use agents_core::{AgentError, ModelConfig, Worker, WorkerResult, WorkerType};
use agents_llm::{LlmClient, LlmStream};
use async_trait::async_trait;
use serde::Serialize;
use tracing::info;

use crate::prompts::EMAIL_WORKER_PROMPT;

#[derive(Serialize)]
struct SendGridMail {
    personalizations: Vec<Personalization>,
    from: EmailAddress,
    subject: String,
    content: Vec<Content>,
}

#[derive(Serialize)]
struct Personalization {
    to: Vec<EmailAddress>,
}

#[derive(Serialize)]
struct EmailAddress {
    email: String,
}

#[derive(Serialize)]
struct Content {
    r#type: String,
    value: String,
}

pub struct EmailWorker {
    http: reqwest::Client,
    api_key: String,
    from_email: String,
}

impl EmailWorker {
    pub fn new(api_key: String, from_email: String) -> Result<Self, AgentError> {
        if api_key.is_empty() {
            return Err(AgentError::ExternalApi("SENDGRID_API_KEY not configured".into()));
        }
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
            from_email,
        })
    }

    fn create_client(model: &ModelConfig) -> LlmClient {
        LlmClient::new(&model.model, model.api_base.as_deref())
    }

    async fn send_email(&self, to: &str, subject: &str, body: &str) -> Result<u16, AgentError> {
        let mail = SendGridMail {
            personalizations: vec![Personalization {
                to: vec![EmailAddress { email: to.to_string() }],
            }],
            from: EmailAddress { email: self.from_email.clone() },
            subject: subject.to_string(),
            content: vec![Content {
                r#type: "text/plain".to_string(),
                value: body.to_string(),
            }],
        };

        let response = self
            .http
            .post("https://api.sendgrid.com/v3/mail/send")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&mail)
            .send()
            .await
            .map_err(|e| AgentError::ExternalApi(e.to_string()))?;

        let status = response.status().as_u16();

        if status >= 400 {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::ExternalApi(format!("SendGrid error ({}): {}", status, error_text)));
        }

        Ok(status)
    }

    pub async fn compose_stream(
        &self,
        task_description: &str,
        parameters: &serde_json::Value,
        model: &ModelConfig,
    ) -> Result<Option<LlmStream>, AgentError> {
        let body_param = parameters.get("body").and_then(|v| v.as_str()).unwrap_or("");

        if !body_param.is_empty() {
            return Ok(None);
        }

        info!("EmailWorker: streaming body composition with model {}", model.name);

        let to = parameters.get("to").and_then(|v| v.as_str()).unwrap_or("");
        let subject = parameters.get("subject").and_then(|v| v.as_str()).unwrap_or("");

        let context = format!(
            "Task: {task_description}\n\nTo: {to}\nSubject: {subject}\n\nCompose the email content."
        );

        let client = Self::create_client(model);
        let stream = client.chat_stream(EMAIL_WORKER_PROMPT, &context).await?;
        Ok(Some(stream))
    }

    pub async fn send(&self, to: &str, subject: &str, body: &str) -> Result<String, AgentError> {
        let status = self.send_email(to, subject, body).await?;
        Ok(format!("Email sent to {}\nSubject: {}\nStatus: {}", to, subject, status))
    }
}

#[async_trait]
impl Worker for EmailWorker {
    fn worker_type(&self) -> WorkerType {
        WorkerType::Email
    }

    async fn execute(
        &self,
        task_description: &str,
        parameters: &serde_json::Value,
        feedback: Option<&str>,
        model: &ModelConfig,
    ) -> Result<WorkerResult, AgentError> {
        info!("EmailWorker: executing with model {}", model.name);

        let to = parameters.get("to").and_then(|v| v.as_str()).unwrap_or("");
        let subject = parameters.get("subject").and_then(|v| v.as_str()).unwrap_or("");
        let body_param = parameters.get("body").and_then(|v| v.as_str()).unwrap_or("");

        let body = if body_param.is_empty() {
            let feedback_section = feedback
                .map(|fb| format!("\n\nPrevious feedback: {fb}"))
                .unwrap_or_default();

            let context = format!(
                "Task: {task_description}\n\nTo: {to}\nSubject: {subject}{feedback_section}\n\nCompose the email content."
            );

            let client = Self::create_client(model);
            match client.chat(EMAIL_WORKER_PROMPT, &context).await {
                Ok(resp) => resp.content,
                Err(e) => return Ok(WorkerResult::err(e)),
            }
        } else {
            body_param.to_string()
        };

        match self.send_email(to, subject, &body).await {
            Ok(status) => Ok(WorkerResult::ok(format!(
                "Email sent to {}\nSubject: {}\nStatus: {}",
                to, subject, status
            ))),
            Err(e) => Ok(WorkerResult::err(e)),
        }
    }
}
