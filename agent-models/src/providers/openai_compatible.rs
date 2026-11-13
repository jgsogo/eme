use crate::client::LlmClient;
use crate::error::ModelError;
use crate::types::{CompletionRequest, ModelResponse, TokenUsage};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OpenAICompatibleClient {
    http: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAICompatibleClient {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<ApiMessage>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<Choice>,
    usage: Option<ApiUsage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ApiMessageResponse,
}

#[derive(Deserialize)]
struct ApiMessageResponse {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[async_trait]
impl LlmClient for OpenAICompatibleClient {
    async fn complete(&self, request: CompletionRequest) -> Result<ModelResponse, ModelError> {
        let mut messages = Vec::new();

        if let Some(system) = request.system {
            messages.push(ApiMessage {
                role: "system".into(),
                content: system,
            });
        }

        messages.push(ApiMessage {
            role: "user".into(),
            content: request.prompt,
        });

        let body = ApiRequest {
            model: self.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        let response = self
            .http
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();

            return Err(ModelError::Provider(format!("HTTP {status}: {body}")));
        }

        let result: ApiResponse = response.json().await?;

        let choice =
            result.choices.into_iter().next().ok_or_else(|| {
                ModelError::InvalidResponse("No completion choices returned".into())
            })?;

        let content = choice
            .message
            .content
            .ok_or_else(|| ModelError::InvalidResponse("Completion has no text content".into()))?;

        let usage = result.usage.map(|usage| TokenUsage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
        });

        Ok(ModelResponse {
            model: self.model.clone(),
            content,
            usage,
        })
    }
}
