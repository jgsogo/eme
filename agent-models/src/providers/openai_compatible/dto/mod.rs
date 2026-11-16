//! OpenAI-compatible wire-format data structures used only by the provider adapter.

mod api_conversation_message;
mod api_conversation_request;
mod api_function_call;
mod api_function_call_response;
mod api_function_definition;
mod api_message_response;
mod api_response;
mod api_tool;
mod api_tool_call;
mod api_tool_call_response;
mod api_usage;
mod choice;

pub(super) use api_conversation_message::ApiConversationMessage;
pub(super) use api_conversation_request::ApiConversationRequest;
pub(super) use api_function_call::ApiFunctionCall;
pub(super) use api_function_call_response::ApiFunctionCallResponse;
pub(super) use api_function_definition::ApiFunctionDefinition;
pub(super) use api_message_response::ApiMessageResponse;
pub(super) use api_response::ApiResponse;
pub(super) use api_tool::ApiTool;
pub(super) use api_tool_call::ApiToolCall;
pub(super) use api_tool_call_response::ApiToolCallResponse;
pub(super) use api_usage::ApiUsage;
pub(super) use choice::Choice;
