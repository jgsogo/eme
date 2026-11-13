use async_trait::async_trait;

use crate::client::LlmClient;

use crate::error::ModelError;
use crate::types::{CompletionRequest, ModelResponse, TokenUsage};
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct GeminiClient {
    http: Client,
    api_key: String,
    model: String,
}

impl GeminiClient {
    pub fn new(key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            http: Client::new(),
            api_key: key.into(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentRequest {
    contents: Vec<Content>,

    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,

    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize)]
struct Content {
    role: String,
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct SystemInstruction {
    parts: Vec<Part>,
}

#[derive(Serialize)]
struct Part {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    temperature: Option<f32>,
    max_output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct GenerateResponse {
    candidates: Option<Vec<Candidate>>,
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Deserialize)]
struct Candidate {
    content: CandidateContent,
}

#[derive(Deserialize)]
struct CandidateContent {
    parts: Vec<PartResponse>,
}

#[derive(Deserialize)]
struct PartResponse {
    text: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    prompt_token_count: u32,
    candidates_token_count: u32,
}

#[async_trait]
impl LlmClient for GeminiClient {
    async fn complete(&self, request: CompletionRequest) -> Result<ModelResponse, ModelError> {
        let body = GenerateContentRequest {
            contents: vec![Content {
                role: "user".into(),
                parts: vec![Part {
                    text: request.prompt,
                }],
            }],
            system_instruction: request.system.map(|si| SystemInstruction {
                parts: vec![Part { text: si }],
            }),
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        );

        let response = self
            .http
            .post(url)
            .header("x-goog-api-key", &self.api_key)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ModelError::Provider(format!("HTTP {status}: {body}")));
        }

        let result: GenerateResponse = response.json().await?;

        let candidate = result
            .candidates
            .and_then(|mut candidates| {
                if candidates.is_empty() {
                    None
                } else {
                    Some(candidates.remove(0))
                }
            })
            .ok_or_else(|| ModelError::InvalidResponse("No candidates returned".into()))?;

        let content = candidate
            .content
            .parts
            .into_iter()
            .filter_map(|part| part.text)
            .collect::<Vec<_>>()
            .join("");

        let usage = result.usage_metadata.map(|usage| TokenUsage {
            input_tokens: usage.prompt_token_count,
            output_tokens: usage.candidates_token_count,
        });

        Ok(ModelResponse {
            model: self.model.clone(),
            content,
            usage,
        })
    }
}
