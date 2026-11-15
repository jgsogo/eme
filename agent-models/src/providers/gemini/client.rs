mod dto;
use self::dto::*;

use async_trait::async_trait;

use crate::client::ConversationClient;
use reqwest::Client;
use crate::error::ModelError;
use crate::types::TokenUsage;
use crate::types::{ConversationMessage, ConversationRequest, ConversationResponse, ToolCall};

pub struct GeminiClient {
    http: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl GeminiClient {
    pub fn new(key: impl Into<String>, model: impl Into<String>) -> Self {
        Self::new_with_base_url("https://generativelanguage.googleapis.com", key, model)
    }

    pub fn new_with_base_url(
        base_url: impl Into<String>,
        key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into(),
            api_key: key.into(),
            model: model.into(),
        }
    }
}































#[async_trait]
impl ConversationClient for GeminiClient {
    async fn complete_conversation(
        &self,
        request: ConversationRequest,
    ) -> Result<ConversationResponse, ModelError> {
        let contents = request
            .messages
            .into_iter()
            .map(|message| match message {
                ConversationMessage::User { content } => Content {
                    role: "user".into(),
                    parts: vec![Part {
                        text: Some(content),
                        function_call: None,
                        function_response: None,
                        thought_signature: None,
                    }],
                },

                ConversationMessage::Assistant {
                    content,
                    tool_calls,
                } => {
                    let mut parts = Vec::new();

                    if let Some(content) = content {
                        parts.push(Part {
                            text: Some(content),
                            function_call: None,
                            function_response: None,
                            thought_signature: None,
                        });
                    }

                    for call in tool_calls {
                        let metadata = call.provider_metadata.as_ref();

                        let thought_signature = metadata
                            .and_then(|metadata| metadata.get("thought_signature"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned);

                        let provider_call_id = metadata
                            .and_then(|metadata| metadata.get("provider_call_id"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned);

                        parts.push(Part {
                            text: None,
                            function_call: Some(FunctionCall {
                                name: call.name,
                                args: call.arguments,
                                id: provider_call_id,
                            }),
                            function_response: None,
                            thought_signature,
                        });
                    }

                    Content {
                        role: "model".into(),
                        parts,
                    }
                }

                ConversationMessage::Tool {
                    tool_call_id: _,
                    name,
                    content,
                    is_error,
                    provider_metadata,
                } => {
                    let provider_call_id = provider_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("provider_call_id"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned);

                    Content {
                        role: "user".into(),
                        parts: vec![Part {
                            text: None,
                            function_call: None,
                            function_response: Some(FunctionResponse {
                                name,
                                response: serde_json::json!({
                                    "result": content,
                                    "is_error": is_error,
                                }),
                                id: provider_call_id,
                            }),
                            thought_signature: None,
                        }],
                    }
                }
            })
            .collect();

        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(vec![GeminiTool {
                function_declarations: request
                    .tools
                    .into_iter()
                    .map(|tool| FunctionDeclaration {
                        name: tool.name,
                        description: tool.description,
                        parameters: normalize_gemini_schema(&tool.parameters),
                    })
                    .collect(),
            }])
        };

        let body = GenerateContentRequest {
            contents,
            system_instruction: request.system.map(|system| SystemInstruction {
                parts: vec![Part {
                    text: Some(system),
                    function_call: None,
                    function_response: None,
                    thought_signature: None,
                }],
            }),
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
            tools,
        };

        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, self.model
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

        let mut content = String::new();
        let mut tool_calls = Vec::new();

        for (index, part) in candidate.content.parts.into_iter().enumerate() {
            if let Some(text) = part.text {
                content.push_str(&text);
            }

            if let Some(call) = part.function_call {
                let provider_call_id = call.id;

                let id = provider_call_id
                    .clone()
                    .unwrap_or_else(|| format!("{}-{index}", call.name));

                let mut metadata = serde_json::Map::new();

                if let Some(signature) = part.thought_signature {
                    metadata.insert(
                        "thought_signature".into(),
                        serde_json::Value::String(signature),
                    );
                }

                if let Some(provider_call_id) = provider_call_id {
                    metadata.insert(
                        "provider_call_id".into(),
                        serde_json::Value::String(provider_call_id),
                    );
                }

                let provider_metadata = if metadata.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Object(metadata))
                };

                tool_calls.push(ToolCall {
                    id,
                    name: call.name,
                    arguments: call.args,
                    provider_metadata,
                });
            }
        }

        let usage = result.usage_metadata.map(|usage| TokenUsage {
            input_tokens: usage.prompt_token_count,
            output_tokens: usage.candidates_token_count,
        });

        Ok(ConversationResponse {
            model: self.model.clone(),
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls,
            usage,
        })
    }
}

fn normalize_gemini_schema(schema: &Value) -> Value {
    fn remove_unsupported_fields(value: &mut Value) {
        match value {
            Value::Object(object) => {
                object.remove("additionalProperties");

                for value in object.values_mut() {
                    remove_unsupported_fields(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    remove_unsupported_fields(value);
                }
            }
            _ => {}
        }
    }

    let mut schema = schema.clone();
    remove_unsupported_fields(&mut schema);
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolDefinition;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[tokio::test]
    async fn preserves_function_call_id_and_thought_signature() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{
                            "functionCall": {
                                "id": "gemini-call-123",
                                "name": "calculator",
                                "args": {"expression": "1 + 1"}
                            },
                            "thoughtSignature": "signature-abc"
                        }]
                    }
                }],
                "usageMetadata": {
                    "promptTokenCount": 10,
                    "candidatesTokenCount": 5
                }
            })))
            .mount(&server)
            .await;

        let client =
            GeminiClient::new_with_base_url(server.uri(), "test-api-key", "gemini-2.5-flash");

        let response = client
            .complete_conversation(ConversationRequest {
                system: None,
                messages: vec![ConversationMessage::User {
                    content: "Calculate 1 + 1".into(),
                }],
                tools: vec![ToolDefinition {
                    name: "calculator".into(),
                    description: "Evaluates arithmetic expressions".into(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "expression": {"type": "string"}
                        },
                        "required": ["expression"]
                    }),
                }],
                temperature: Some(0.2),
                max_tokens: Some(1024),
            })
            .await
            .unwrap();

        assert_eq!(response.tool_calls.len(), 1);

        let call = &response.tool_calls[0];

        assert_eq!(call.id, "gemini-call-123");
        assert_eq!(call.name, "calculator");
        assert_eq!(call.arguments, json!({"expression": "1 + 1"}));
        assert_eq!(
            call.provider_metadata.as_ref().unwrap()["thought_signature"],
            "signature-abc"
        );
        assert_eq!(
            call.provider_metadata.as_ref().unwrap()["provider_call_id"],
            "gemini-call-123"
        );
    }

    #[tokio::test]
    async fn sends_original_function_call_metadata_in_follow_up() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1beta/models/gemini-2.5-flash:generateContent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{"text": "The result is 2."}]
                    }
                }]
            })))
            .mount(&server)
            .await;

        let client =
            GeminiClient::new_with_base_url(server.uri(), "test-api-key", "gemini-2.5-flash");

        let metadata = json!({
            "thought_signature": "signature-abc",
            "provider_call_id": "gemini-call-123"
        });

        let _response = client
            .complete_conversation(ConversationRequest {
                system: None,
                messages: vec![
                    ConversationMessage::User {
                        content: "Calculate 1 + 1".into(),
                    },
                    ConversationMessage::Assistant {
                        content: None,
                        tool_calls: vec![ToolCall {
                            id: "runtime-call-456".into(),
                            name: "calculator".into(),
                            arguments: json!({"expression": "1 + 1"}),
                            provider_metadata: Some(metadata.clone()),
                        }],
                    },
                    ConversationMessage::Tool {
                        tool_call_id: "runtime-call-456".into(),
                        name: "calculator".into(),
                        content: "2".into(),
                        is_error: false,
                        provider_metadata: Some(metadata),
                    },
                ],
                tools: vec![],
                temperature: None,
                max_tokens: None,
            })
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();

        let assistant_parts = body["contents"][1]["parts"].as_array().unwrap();
        let tool_parts = body["contents"][2]["parts"].as_array().unwrap();

        assert_eq!(assistant_parts[0]["functionCall"]["id"], "gemini-call-123");
        assert_eq!(assistant_parts[0]["thoughtSignature"], "signature-abc");

        assert_eq!(tool_parts[0]["functionResponse"]["id"], "gemini-call-123");
        assert_eq!(tool_parts[0]["functionResponse"]["name"], "calculator");
    }

    #[test]
    fn normalizes_function_schema_for_gemini() {
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Message to send"
                },
                "options": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "urgent": {
                            "type": "boolean"
                        }
                    }
                }
            },
            "required": ["text"]
        });

        let normalized = normalize_gemini_schema(&schema);

        assert!(normalized.get("additionalProperties").is_none());
        assert!(
            normalized["properties"]["options"]
                .get("additionalProperties")
                .is_none()
        );

        assert_eq!(normalized["required"], serde_json::json!(["text"]));
        assert_eq!(normalized["properties"]["text"]["type"], "string");

        // The original schema remains intact.
        assert_eq!(schema["additionalProperties"], false);
    }
}
