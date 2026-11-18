use super::dto::*;

use crate::client::ConversationClient;

use crate::error::ModelError;
use crate::types::TokenUsage;
use crate::types::{ConversationMessage, ConversationRequest, ConversationResponse, ToolCall};
use async_trait::async_trait;
use reqwest::Client;

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

#[async_trait]
impl ConversationClient for OpenAICompatibleClient {
    async fn complete_conversation(
        &self,
        request: ConversationRequest,
    ) -> Result<ConversationResponse, ModelError> {
        let mut messages = Vec::new();

        if let Some(system) = request.system {
            messages.push(ApiConversationMessage {
                role: "system".into(),
                content: Some(system),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }

        for message in request.messages {
            let message = match message {
                ConversationMessage::User { content } => ApiConversationMessage {
                    role: "user".into(),
                    content: Some(content),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },

                ConversationMessage::Assistant {
                    content,
                    tool_calls,
                } => {
                    let tool_calls = tool_calls
                        .into_iter()
                        .map(|call| ApiToolCall {
                            id: call.id,
                            kind: "function".into(),
                            function: ApiFunctionCall {
                                name: call.name,
                                arguments: call.arguments.to_string(),
                            },
                        })
                        .collect::<Vec<_>>();

                    ApiConversationMessage {
                        role: "assistant".into(),
                        content,
                        name: None,
                        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
                        tool_call_id: None,
                    }
                }

                ConversationMessage::Tool {
                    tool_call_id,
                    name,
                    content,
                    ..
                } => ApiConversationMessage {
                    role: "tool".into(),
                    content: Some(content),
                    name: Some(name),
                    tool_calls: None,
                    tool_call_id: Some(tool_call_id),
                },
            };

            messages.push(message);
        }

        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(
                request
                    .tools
                    .into_iter()
                    .map(|tool| ApiTool {
                        kind: "function".into(),
                        function: ApiFunctionDefinition {
                            name: tool.name,
                            description: tool.description,
                            parameters: tool.parameters,
                        },
                    })
                    .collect(),
            )
        };

        let body = ApiConversationRequest {
            model: self.model.clone(),
            messages,
            tools,
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
            return Err(ModelError::ProviderResponse {
                status: status.as_u16(),
                body,
            });
        }

        let result: ApiResponse = response.json().await?;

        let choice =
            result.choices.into_iter().next().ok_or_else(|| {
                ModelError::InvalidResponse("No completion choices returned".into())
            })?;

        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|call| {
                let arguments =
                    serde_json::from_str(&call.function.arguments).map_err(|error| {
                        ModelError::InvalidResponse(format!(
                            "Invalid tool arguments for '{}': {error}",
                            call.function.name
                        ))
                    })?;

                Ok(ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments,
                    provider_metadata: None,
                })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;

        let usage = result.usage.map(|usage| TokenUsage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
        });

        Ok(ConversationResponse {
            model: self.model.clone(),
            content: choice.message.content,
            tool_calls,
            usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::types::ToolDefinition;

    use super::*;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    #[tokio::test]
    async fn completes_conversation_with_text_response() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Hello!"
                    }
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 5
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = OpenAICompatibleClient::new(server.uri(), "test-key", "test-model");

        let response = client
            .complete_conversation(ConversationRequest {
                system: Some("Be helpful.".into()),
                messages: vec![ConversationMessage::User {
                    content: "Hello".into(),
                }],
                tools: vec![],
                temperature: Some(0.2),
                max_tokens: Some(100),
            })
            .await
            .unwrap();

        assert_eq!(response.model, "test-model");
        assert_eq!(response.content.as_deref(), Some("Hello!"));
        assert!(response.tool_calls.is_empty());

        let usage = response.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 5);
    }

    #[tokio::test]
    async fn parses_tool_calls_from_response() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_123",
                            "type": "function",
                            "function": {
                                "name": "calculator",
                                "arguments": "{\"expression\":\"1 + 1\"}"
                            }
                        }]
                    }
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = OpenAICompatibleClient::new(server.uri(), "test-key", "test-model");

        let response = client
            .complete_conversation(ConversationRequest {
                system: None,
                messages: vec![ConversationMessage::User {
                    content: "Calculate 1 + 1".into(),
                }],
                tools: vec![ToolDefinition {
                    name: "calculator".into(),
                    description: "Evaluate an arithmetic expression".into(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "expression": { "type": "string" }
                        },
                        "required": ["expression"]
                    }),
                }],
                temperature: None,
                max_tokens: None,
            })
            .await
            .unwrap();

        assert_eq!(response.content, None);
        assert_eq!(response.tool_calls.len(), 1);

        let call = &response.tool_calls[0];
        assert_eq!(call.id, "call_123");
        assert_eq!(call.name, "calculator");
        assert_eq!(call.arguments, json!({ "expression": "1 + 1" }));

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);

        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();

        assert_eq!(body["model"], "test-model");

        assert_eq!(
            body["messages"][0],
            json!({
                "role": "user",
                "content": "Calculate 1 + 1"
            })
        );

        assert_eq!(
            body["tools"][0],
            json!({
                "type": "function",
                "function": {
                    "name": "calculator",
                    "description": "Evaluate an arithmetic expression",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "expression": { "type": "string" }
                        },
                        "required": ["expression"]
                    }
                }
            })
        );
    }

    #[tokio::test]
    async fn serializes_tool_call_follow_up() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "The result is 2."
                    }
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = OpenAICompatibleClient::new(server.uri(), "test-key", "test-model");

        client
            .complete_conversation(ConversationRequest {
                system: None,
                messages: vec![
                    ConversationMessage::User {
                        content: "Calculate 1 + 1".into(),
                    },
                    ConversationMessage::Assistant {
                        content: None,
                        tool_calls: vec![ToolCall {
                            id: "call_123".into(),
                            name: "calculator".into(),
                            arguments: json!({ "expression": "1 + 1" }),
                            provider_metadata: None,
                        }],
                    },
                    ConversationMessage::Tool {
                        tool_call_id: "call_123".into(),
                        name: "calculator".into(),
                        content: "2".into(),
                        is_error: false,
                        provider_metadata: None,
                    },
                ],
                tools: vec![],
                temperature: None,
                max_tokens: None,
            })
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);

        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();

        assert_eq!(
            body["messages"][1],
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_123",
                    "type": "function",
                    "function": {
                        "name": "calculator",
                        "arguments": "{\"expression\":\"1 + 1\"}"
                    }
                }]
            })
        );

        assert_eq!(
            body["messages"][2],
            json!({
                "role": "tool",
                "content": "2",
                "name": "calculator",
                "tool_call_id": "call_123"
            })
        );
    }

    #[tokio::test]
    async fn rejects_invalid_tool_call_arguments() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_123",
                            "type": "function",
                            "function": {
                                "name": "calculator",
                                "arguments": "{invalid json"
                            }
                        }]
                    }
                }]
            })))
            .mount(&server)
            .await;

        let client = OpenAICompatibleClient::new(server.uri(), "test-key", "test-model");

        let error = client
            .complete_conversation(ConversationRequest {
                system: None,
                messages: vec![ConversationMessage::User {
                    content: "Calculate 1 + 1".into(),
                }],
                tools: vec![],
                temperature: None,
                max_tokens: None,
            })
            .await
            .unwrap_err();

        assert!(matches!(error, ModelError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn returns_provider_error_for_http_failure() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "error": {
                    "message": "Invalid API key"
                }
            })))
            .mount(&server)
            .await;

        let client = OpenAICompatibleClient::new(server.uri(), "invalid-key", "test-model");

        let error = client
            .complete_conversation(ConversationRequest {
                system: None,
                messages: vec![ConversationMessage::User {
                    content: "Hello".into(),
                }],
                tools: vec![],
                temperature: None,
                max_tokens: None,
            })
            .await
            .unwrap_err();

        match error {
            ModelError::ProviderResponse { status, body } => {
                assert_eq!(status, 401);
                assert!(body.contains("Invalid API key"));
            }
            other => panic!("Expected provider error, got: {other:?}"),
        }
    }
}
