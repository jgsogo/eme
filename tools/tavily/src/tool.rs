#[path = "options/mod.rs"]
mod options;
use options::*;

use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::Value;

use crate::client::{SearchRequest, TavilyClient};

/// Searches the web using Tavily.
pub struct WebSearch;

fn default_max_results() -> u8 {
    5
}

fn default_search_depth() -> SearchDepth {
    SearchDepth::Basic
}

impl SearchDepth {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Advanced => "advanced",
            Self::Fast => "fast",
            Self::UltraFast => "ultra-fast",
        }
    }
}

impl SearchTopic {
    fn as_str(&self) -> &'static str {
        match self {
            Self::General => "general",
            Self::News => "news",
            Self::Finance => "finance",
        }
    }
}

impl TimeRange {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }
}

#[async_trait]
impl Tool for WebSearch {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let arguments: Arguments =
            serde_json::from_value(arguments).map_err(|error| ToolError::InvalidArguments {
                tool_name: "web_search".into(),
                errors: vec![error.to_string()],
            })?;

        let query = arguments.query.trim();
        if query.is_empty() {
            return Err(ToolError::InvalidArguments {
                tool_name: "web_search".into(),
                errors: vec!["'query' must not be empty".into()],
            });
        }

        let api_key = std::env::var("TAVILY_API_KEY")
            .map_err(|_| ToolError::Configuration("TAVILY_API_KEY env var not set".into()))?;

        let request = SearchRequest {
            query: query.to_owned(),
            search_depth: arguments.search_depth.as_str().to_owned(),
            topic: arguments.topic.as_str().to_owned(),
            max_results: arguments.max_results,
            time_range: arguments.time_range.map(|value| value.as_str().to_owned()),
            include_domains: arguments.include_domains,
            exclude_domains: arguments.exclude_domains,
        };

        let client =
            TavilyClient::new(api_key).map_err(|error| ToolError::Execution(error.to_string()))?;
        let response = client
            .search(&request)
            .await
            .map_err(|error| ToolError::Execution(error.to_string()))?;

        serde_json::to_string_pretty(&response)
            .map_err(|error| ToolError::Execution(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults_and_optional_filters() {
        let arguments: Arguments = serde_json::from_value(serde_json::json!({
            "query": "  Rust agents  ",
            "include_domains": ["rust-lang.org"]
        }))
        .unwrap();

        assert_eq!(arguments.max_results, 5);
        assert_eq!(arguments.search_depth.as_str(), "basic");
        assert_eq!(arguments.topic.as_str(), "general");
        assert_eq!(arguments.include_domains.unwrap(), ["rust-lang.org"]);
    }

    #[test]
    fn rejects_invalid_search_depth() {
        let error = serde_json::from_value::<Arguments>(serde_json::json!({
            "query": "Rust",
            "search_depth": "deepest"
        }))
        .unwrap_err();

        assert!(error.to_string().contains("unknown variant"));
    }
}
