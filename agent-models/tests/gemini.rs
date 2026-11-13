use agent_models::client::LlmClient;
use agent_models::providers::GeminiClient;
use agent_models::types::CompletionRequest;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path},
};

fn request(prompt: &str) -> CompletionRequest {
    CompletionRequest {
        system: None,
        prompt: prompt.to_string(),
        temperature: None,
        max_tokens: None,
    }
}

fn client(server: &MockServer) -> GeminiClient {
    GeminiClient::new_with_base_url(server.uri(), "test-api-key", "test-model")
}

#[tokio::test]
async fn sends_generate_content_request() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1beta/models/test-model:generateContent"))
        .and(header("x-goog-api-key", "test-api-key"))
        .and(header("content-type", "application/json"))
        .and(body_json(json!({
            "systemInstruction": {
                "parts": [
                    {
                        "text": "You are a helpful assistant."
                    }
                ]
            },
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        {
                            "text": "Hello"
                        }
                    ]
                }
            ],
            "generationConfig": {
                "temperature": 0.7,
                "maxOutputTokens": 128
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {
                            "text": "Hello back!"
                        }
                    ]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = client(&server);

    let request = CompletionRequest {
        system: Some("You are a helpful assistant.".into()),
        prompt: "Hello".into(),
        temperature: Some(0.7),
        max_tokens: Some(128),
    };

    let response = client.complete(request).await.unwrap();

    assert_eq!(response.model, "test-model");
    assert_eq!(response.content, "Hello back!");

    let usage = response.usage.unwrap();

    assert_eq!(usage.input_tokens, 10);
    assert_eq!(usage.output_tokens, 5);
}

#[tokio::test]
async fn rejects_empty_candidates() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "candidates": []
        })))
        .mount(&server)
        .await;

    let client = client(&server);
    let error = client.complete(request("Hello")).await.unwrap_err();

    assert!(matches!(
        error,
        agent_models::error::ModelError::InvalidResponse(_)
    ));
}

#[tokio::test]
async fn rejects_missing_candidates() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let client = client(&server);
    let error = client.complete(request("Hello")).await.unwrap_err();

    assert!(matches!(
        error,
        agent_models::error::ModelError::InvalidResponse(_)
    ));
}

#[tokio::test]
async fn returns_provider_error_for_http_failure() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Invalid API key"))
        .mount(&server)
        .await;

    let client = client(&server);
    let error = client.complete(request("Hello")).await.unwrap_err();

    match error {
        agent_models::error::ModelError::Provider(message) => {
            assert!(message.contains("401"));
            assert!(message.contains("Invalid API key"));
        }
        other => panic!("Expected Provider error, got: {other:?}"),
    }
}

#[tokio::test]
async fn rejects_malformed_json_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"candidates": ["#))
        .mount(&server)
        .await;

    let client = client(&server);
    let error = client.complete(request("Hello")).await.unwrap_err();

    assert!(
        matches!(error, agent_models::error::ModelError::Http(_)),
        "Expected HTTP/deserialization error, got: {error:?}"
    );
}
