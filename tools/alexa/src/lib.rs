use agent_runtime::Tool;
use async_trait::async_trait;
use serde_json::Value;

pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    async fn execute(&self, arguments: Value) -> Result<String, String> {
        let text = arguments
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| "Missing required argument: text".to_string())?;

        Ok(text.to_owned())
    }
}