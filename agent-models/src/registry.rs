use std::sync::Arc;

use crate::{
    client::LlmClient,
    error::ModelError,
    providers::{GeminiClient, OpenAICompatibleClient},
};

pub fn resolve_model(model_name: &str) -> Result<Arc<dyn LlmClient>, ModelError> {
    // Parse the model string into provider and model
    let (provider, model) = model_name
        .split_once('/')
        .ok_or_else(|| ModelError::InvalidModelName(model_name.to_string()))?;

    // TODO: This is a hardcoded dispatcher, the registry should be populated from some
    //       configuration files
    match provider {
        "gemini" => {
            let key = std::env::var("GEMINI_API_KEY")
                .map_err(|_| ModelError::MissingApiKey("GEMINI_API_KEY".into()))?;
            Ok(Arc::new(GeminiClient::new(key, model)))
        }

        "groq" => {
            let key = std::env::var("GROQ_API_KEY")
                .map_err(|_| ModelError::MissingApiKey("GROQ_API_KEY".into()))?;
            Ok(Arc::new(OpenAICompatibleClient::new(
                "https://api.groq.com/openai/v1",
                key,
                model,
            )))
        }

        "cloudflare" => {
            let account_id = std::env::var("CLOUDFLARE_ACCOUNT_ID")
                .map_err(|_| ModelError::MissingApiKey("CLOUDFLARE_ACCOUNT_ID".into()))?;

            let api_key = std::env::var("CLOUDFLARE_API_TOKEN")
                .map_err(|_| ModelError::MissingApiKey("CLOUDFLARE_API_TOKEN".into()))?;

            let base_url =
                format!("https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1");

            Ok(Arc::new(OpenAICompatibleClient::new(
                &base_url, api_key, model,
            )))
        }

        other => Err(ModelError::UnsupportedProvider(other.to_string())),
    }
}
