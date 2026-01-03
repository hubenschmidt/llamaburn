use agents_core::{AgentError, Worker, WorkerResult, WorkerType};
use agents_llm::LlmClient;
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
    client: LlmClient,
    http: reqwest::Client,
    api_key: String,
    from_email: String,
}

impl EmailWorker {
    pub fn new(model: &str, api_key: String, from_email: String) -> Self {
        Self {
            client: LlmClient::new(model),
            http: reqwest::Client::new(),
            api_key,
            from_email,
        }
    }

    async fn send_email(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<u16, AgentError> {
        if self.api_key.is_empty() {
            return Err(AgentError::ExternalApi("SENDGRID_API_KEY not configured".into()));
        }

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
            return Err(AgentError::ExternalApi(format!(
                "SendGrid error ({}): {}",
                status, error_text
            )));
        }

        Ok(status)
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
    ) -> Result<WorkerResult, AgentError> {
        info!("EMAIL_WORKER: Starting execution");

        let to = parameters
            .get("to")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let subject = parameters
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let body_param = parameters
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        info!("EMAIL_WORKER: To: {}", to);

        let feedback_section = feedback
            .map(|fb| format!("Previous feedback to address: {fb}"))
            .unwrap_or_default();

        let context = format!(
            "Task: {task_description}\n\nParameters provided:\n- To: {to}\n- Subject: {subject}\n- Body: {body_param}\n\n{feedback_section}\n\nCompose the email content."
        );

        let composed_body = match self.client.chat(EMAIL_WORKER_PROMPT, &context).await {
            Ok(output) => output,
            Err(e) => {
                return Ok(WorkerResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                });
            }
        };

        let final_body = if body_param.is_empty() {
            &composed_body
        } else {
            body_param
        };

        info!("EMAIL_WORKER: Sending to {}", to);

        match self.send_email(to, subject, final_body).await {
            Ok(status) => {
                info!("EMAIL_WORKER: Sent successfully (status: {})", status);
                Ok(WorkerResult {
                    success: true,
                    output: format!(
                        "Email sent successfully to {}\nSubject: {}\nStatus: {}",
                        to, subject, status
                    ),
                    error: None,
                })
            }
            Err(e) => {
                info!("EMAIL_WORKER: Send failed: {}", e);
                Ok(WorkerResult {
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                })
            }
        }
    }
}
