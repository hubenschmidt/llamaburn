use agents_core::{AgentError, Message, MessageRole};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs, CreateChatCompletionResponse, ResponseFormat,
    },
    Client,
};
use serde::de::DeserializeOwned;
use tracing::debug;

fn llm_err(e: impl ToString) -> AgentError {
    AgentError::LlmError(e.to_string())
}

fn extract_content(response: CreateChatCompletionResponse) -> Result<String, AgentError> {
    response
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or_else(|| AgentError::LlmError("No response content".into()))
}

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

    pub async fn chat(&self, system_prompt: &str, user_input: &str) -> Result<String, AgentError> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.default_model)
            .messages(vec![
                ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(system_prompt)
                        .build()
                        .map_err(llm_err)?,
                ),
                ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(user_input)
                        .build()
                        .map_err(llm_err)?,
                ),
            ])
            .build()
            .map_err(llm_err)?;

        let response = self.client.chat().create(request).await.map_err(llm_err)?;
        extract_content(response)
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
                    .map_err(llm_err)?,
            ),
        ];

        for msg in history {
            let chat_msg = match msg.role {
                MessageRole::User => ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(msg.content.clone())
                        .build()
                        .map_err(llm_err)?,
                ),
                MessageRole::Assistant => ChatCompletionRequestMessage::Assistant(
                    ChatCompletionRequestAssistantMessageArgs::default()
                        .content(msg.content.clone())
                        .build()
                        .map_err(llm_err)?,
                ),
            };
            messages.push(chat_msg);
        }

        messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_input)
                .build()
                .map_err(llm_err)?,
        ));

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.default_model)
            .messages(messages)
            .build()
            .map_err(llm_err)?;

        let response = self.client.chat().create(request).await.map_err(llm_err)?;
        extract_content(response)
    }

    pub async fn structured<T: DeserializeOwned>(
        &self,
        system_prompt: &str,
        user_input: &str,
    ) -> Result<T, AgentError> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.default_model)
            .response_format(ResponseFormat::JsonObject)
            .messages(vec![
                ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(system_prompt)
                        .build()
                        .map_err(llm_err)?,
                ),
                ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(user_input)
                        .build()
                        .map_err(llm_err)?,
                ),
            ])
            .build()
            .map_err(llm_err)?;

        let response = self.client.chat().create(request).await.map_err(llm_err)?;
        let content = extract_content(response)?;

        debug!("Structured response: {}", content);

        serde_json::from_str(&content).map_err(|e| {
            AgentError::ParseError(format!("Failed to parse: {} - content: {}", e, content))
        })
    }
}
