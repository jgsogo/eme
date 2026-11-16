//! Gemini wire-format data structures used only by the provider adapter.

mod candidate;
mod candidate_content;
mod content;
mod function_call;
mod function_declaration;
mod function_response;
mod gemini_tool;
mod generate_content_request;
mod generate_response;
mod generation_config;
mod part;
mod part_response;
mod response_function_call;
mod system_instruction;
mod usage_metadata;

pub(super) use candidate::Candidate;
pub(super) use candidate_content::CandidateContent;
pub(super) use content::Content;
pub(super) use function_call::FunctionCall;
pub(super) use function_declaration::FunctionDeclaration;
pub(super) use function_response::FunctionResponse;
pub(super) use gemini_tool::GeminiTool;
pub(super) use generate_content_request::GenerateContentRequest;
pub(super) use generate_response::GenerateResponse;
pub(super) use generation_config::GenerationConfig;
pub(super) use part::Part;
pub(super) use part_response::PartResponse;
pub(super) use response_function_call::ResponseFunctionCall;
pub(super) use system_instruction::SystemInstruction;
pub(super) use usage_metadata::UsageMetadata;
