use agents_core::{AgentError, Message};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
        ResponseFormat, ResponseFormatJsonObject,
    },
    Client,
};
use serde::de::DeserializeOwned;
use tracing::debug;

pub struct LlmClient {
    client: Client<OpenAIConfig>,
    default_model: String,
}

impl LlmClient {
    pub fn new(model: &str) -> Self {
        Self {
            client: Client::new(),
            default_model: model.to_string(),
        }
    }

    pub fn with_model(model: &str) -> Self {
        Self::new(model)
    }

    pub async fn chat(&self, system_prompt: &str, user_input: &str) -> Result<String, AgentError> {
        self.chat_with_model(system_prompt, user_input, &self.default_model)
            .await
    }

    pub async fn chat_with_model(
        &self,
        system_prompt: &str,
        user_input: &str,
        model: &str,
    ) -> Result<String, AgentError> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(vec![
                ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(system_prompt)
                        .build()
                        .map_err(|e| AgentError::LlmError(e.to_string()))?,
                ),
                ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(user_input)
                        .build()
                        .map_err(|e| AgentError::LlmError(e.to_string()))?,
                ),
            ])
            .build()
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| AgentError::LlmError("No response content".into()))?;

        Ok(content)
    }

    pub async fn chat_with_history(
        &self,
        system_prompt: &str,
        history: &[Message],
        user_input: &str,
    ) -> Result<String, AgentError> {
        let mut messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(system_prompt)
                    .build()
                    .map_err(|e| AgentError::LlmError(e.to_string()))?,
            ),
        ];

        for msg in history {
            let chat_msg = match msg.role.as_str() {
                "user" => ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(&msg.content)
                        .build()
                        .map_err(|e| AgentError::LlmError(e.to_string()))?,
                ),
                "assistant" => ChatCompletionRequestMessage::Assistant(
                    async_openai::types::ChatCompletionRequestAssistantMessageArgs::default()
                        .content(&msg.content)
                        .build()
                        .map_err(|e| AgentError::LlmError(e.to_string()))?,
                ),
                _ => continue,
            };
            messages.push(chat_msg);
        }

        messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_input)
                .build()
                .map_err(|e| AgentError::LlmError(e.to_string()))?,
        ));

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.default_model)
            .messages(messages)
            .build()
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| AgentError::LlmError("No response content".into()))?;

        Ok(content)
    }

    pub async fn structured<T: DeserializeOwned>(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<T, AgentError> {
        self.structured_with_model(system_prompt, user_input, &self.default_model)
            .await
    }

    pub async fn structured_with_model<T: DeserializeOwned>(
        &self,
        system_prompt: &str,
        user_input: &str,
        model: &str,
    ) -> Result<T, AgentError> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .response_format(ResponseFormat::JsonObject(ResponseFormatJsonObject {
                r#type: async_openai::types::ResponseFormatJsonObjectType::JsonObject,
            }))
            .messages(vec![
                ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(system_prompt)
                        .build()
                        .map_err(|e| AgentError::LlmError(e.to_string()))?,
                ),
                ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(user_input)
                        .build()
                        .map_err(|e| AgentError::LlmError(e.to_string()))?,
                ),
            ])
            .build()
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| AgentError::LlmError(e.to_string()))?;

        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| AgentError::LlmError("No response content".into()))?;

        debug!("Structured response: {}", content);

        serde_json::from_str(&content).map_err(|e| {
            AgentError::ParseError(format!("Failed to parse: {} - content: {}", e, content))
        })
    }
}
