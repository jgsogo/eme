use agent_runtime::Tool;
use async_trait::async_trait;
use serde_json::Value;

pub struct AlexaTool;

#[async_trait]
impl Tool for AlexaTool {
    async fn execute(&self, arguments: Value) -> Result<String, String> {
        // TODO: Implement Alexa integration.
        let _ = arguments;

        Ok(String::new())
    }
}