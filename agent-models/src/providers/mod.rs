//! Concrete provider clients and their private wire-format implementations.

mod gemini;
mod openai_compatible;

pub use gemini::GeminiClient;
pub use openai_compatible::OpenAICompatibleClient;
