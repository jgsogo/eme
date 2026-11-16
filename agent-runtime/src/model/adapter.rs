use std::sync::Arc;

use super::AgentModel;
use crate::{ModelRequest, ModelTurn};
use agent_models::{
    ConversationClient, ConversationMessage, ConversationRequest, ConversationResponse, ModelError,
    ToolCall, ToolDefinition,
};
use async_trait::async_trait;

/// Adapts an agent-models conversation client to the runtime interface.
pub struct ModelAdapter {
    client: Arc<dyn ConversationClient>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

impl ModelAdapter {
    pub fn new(client: Arc<dyn ConversationClient>) -> Self {
        Self {
            client,
            temperature: Some(0.7),
            max_tokens: Some(1024),
        }
    }

    pub fn with_temperature(mut self, temperature: Option<f32>) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: Option<u32>) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

#[async_trait]
impl AgentModel for ModelAdapter {
    async fn complete(&self, request: ModelRequest) -> Result<ModelTurn, ModelError> {
        let request = ConversationRequest {
            system: request.system_prompt,
            messages: request
                .messages
                .into_iter()
                .map(|message| match message {
                    crate::Message::User { content } => ConversationMessage::User { content },
                    crate::Message::Assistant {
                        content,
                        tool_calls,
                    } => ConversationMessage::Assistant {
                        content,
                        tool_calls: tool_calls
                            .into_iter()
                            .map(|call| ToolCall {
                                id: call.id,
                                name: call.name,
                                arguments: call.arguments,
                                provider_metadata: call.provider_metadata,
                            })
                            .collect(),
                    },
                    crate::Message::Tool {
                        tool_call_id,
                        name,
                        content,
                        is_error,
                        provider_metadata,
                    } => ConversationMessage::Tool {
                        tool_call_id,
                        name,
                        content,
                        is_error,
                        provider_metadata,
                    },
                })
                .collect(),
            tools: request
                .tools
                .into_iter()
                .map(|tool| ToolDefinition {
                    name: tool.name,
                    description: tool.description,
                    parameters: tool.parameters,
                })
                .collect(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
        };

        let response: ConversationResponse = self.client.complete_conversation(request).await?;

        Ok(ModelTurn {
            content: response.content,
            tool_calls: response
                .tool_calls
                .into_iter()
                .map(|call| crate::ToolCall {
                    id: call.id,
                    name: call.name,
                    arguments: call.arguments,
                    provider_metadata: call.provider_metadata,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::{Message, ModelRequest, ToolCall, ToolDefinition};

    struct FakeClient {
        received: Arc<Mutex<Option<ConversationRequest>>>,
    }

    #[async_trait]
    impl ConversationClient for FakeClient {
        async fn complete_conversation(
            &self,
            request: ConversationRequest,
        ) -> Result<ConversationResponse, ModelError> {
            *self.received.lock().unwrap() = Some(request);

            Ok(ConversationResponse {
                model: "fake-model".into(),
                content: Some("Done".into()),
                tool_calls: vec![ToolCall {
                    id: "call-1".into(),
                    name: "calculator".into(),
                    arguments: json!({"expression": "1 + 1"}),
                    provider_metadata: None,
                }],
                usage: None,
            })
        }
    }

    #[tokio::test]
    async fn adapts_runtime_requests_and_model_responses() {
        let received = Arc::new(Mutex::new(None));
        let client = Arc::new(FakeClient {
            received: received.clone(),
        });

        let adapter = ModelAdapter::new(client)
            .with_temperature(Some(0.2))
            .with_max_tokens(Some(2048));

        let request = ModelRequest {
            system_prompt: Some("You are helpful.".into()),
            messages: vec![
                Message::User {
                    content: "Calculate 1 + 1".into(),
                },
                Message::Assistant {
                    content: None,
                    tool_calls: vec![agent_models::ToolCall {
                        id: "call-previous".into(),
                        name: "calculator".into(),
                        arguments: json!({"expression": "2 + 2"}),
                        provider_metadata: None,
                    }],
                },
                Message::Tool {
                    tool_call_id: "call-previous".into(),
                    name: "calculator".into(),
                    content: "4".into(),
                    is_error: false,
                    provider_metadata: None,
                },
            ],
            tools: vec![ToolDefinition {
                name: "calculator".into(),
                description: "Evaluates arithmetic expressions.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "expression": {"type": "string"}
                    },
                    "required": ["expression"]
                }),
            }],
        };

        let turn = adapter.complete(request).await.unwrap();

        assert_eq!(turn.content.as_deref(), Some("Done"));
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].id, "call-1");
        assert_eq!(turn.tool_calls[0].name, "calculator");
        assert_eq!(turn.tool_calls[0].arguments, json!({"expression": "1 + 1"}));

        let received = received.lock().unwrap();
        let received = received.as_ref().unwrap();

        assert_eq!(received.system.as_deref(), Some("You are helpful."));
        assert_eq!(received.temperature, Some(0.2));
        assert_eq!(received.max_tokens, Some(2048));
        assert_eq!(received.messages.len(), 3);
        assert_eq!(received.tools.len(), 1);

        assert!(matches!(
            &received.messages[0],
            ConversationMessage::User { content } if content == "Calculate 1 + 1"
        ));

        assert!(matches!(
            &received.messages[1],
            ConversationMessage::Assistant { content: None, tool_calls }
                if tool_calls.len() == 1
                    && tool_calls[0].id == "call-previous"
        ));

        assert!(matches!(
            &received.messages[2],
            ConversationMessage::Tool {
                tool_call_id,
                name,
                content,
                is_error: false,
                provider_metadata: None,
            } if tool_call_id == "call-previous"
                && name == "calculator"
                && content == "4"
        ));

        assert_eq!(received.tools[0].name, "calculator");
    }
}
