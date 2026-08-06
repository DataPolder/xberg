//! LLM client factory — converts xberg's LlmConfig to a liter-llm DefaultClient.

use std::time::Duration;

use liter_llm::client::{ClientConfig, ClientConfigBuilder, DefaultClient};

use crate::core::config::LlmConfig;

/// Translate xberg's [`LlmConfig`] into a liter-llm [`ClientConfig`].
///
/// `model`, `temperature`, and `max_tokens` are request-time parameters, not
/// client-level settings; they are deliberately carried on [`LlmConfig`] without
/// being mapped here, matching liter-llm's own `LlmConfig::into_client_builder`.
///
/// Split out of [`create_client`] so the mapping is observable in tests without
/// constructing a live HTTP client.
fn build_client_config(config: &LlmConfig) -> crate::Result<ClientConfig> {
    let api_key = config.api_key.as_deref().unwrap_or_default();
    let mut builder = ClientConfigBuilder::new(api_key);

    if let Some(ref base_url) = config.base_url {
        let sanitized = base_url.trim_end_matches('/');
        builder = builder.base_url(sanitized.to_string());
    }
    if let Some(timeout) = config.timeout_secs {
        builder = builder.timeout(Duration::from_secs(timeout));
    }
    if let Some(max_retries) = config.max_retries {
        builder = builder.max_retries(max_retries);
    }
    if let Some(load_env) = config.load_env {
        builder = builder.load_env(load_env);
    }
    if let Some(ref headers) = config.headers {
        for (key, value) in headers {
            builder = builder.header(key.as_str(), value.as_str()).map_err(|e| {
                let msg = format!("Invalid LLM header '{key}': {e}");
                crate::XbergError::Validation {
                    message: msg,
                    source: Some(Box::new(e)),
                }
            })?;
        }
    }

    // Bedrock: only fields the caller actually set are forwarded, so anything left
    // unset keeps liter-llm's env-var fallback (`AWS_REGION`, `AWS_ACCESS_KEY_ID`,
    // ...) and the default AWS credential chain. Credentials are applied when
    // either half is present, matching liter-llm's own `into_client_builder`.
    if let Some(ref bedrock) = config.bedrock {
        if let Some(ref region) = bedrock.region {
            builder = builder.bedrock_region(region.clone());
        }
        if let Some(ref prefix) = bedrock.cross_region_prefix {
            builder = builder.bedrock_cross_region_prefix(prefix.clone());
        }
        if bedrock.access_key_id.is_some() || bedrock.secret_access_key.is_some() {
            builder = builder.bedrock_credentials(
                bedrock.access_key_id.clone().unwrap_or_default(),
                bedrock.secret_access_key.clone().unwrap_or_default(),
                bedrock.session_token.clone(),
            );
        }
    }

    Ok(builder.build())
}

/// Create a liter-llm [`DefaultClient`] from xberg's [`LlmConfig`].
///
/// The `model` field from the config is passed as a model hint so that
/// liter-llm can resolve the correct provider automatically.
///
/// When `api_key` is `None`, liter-llm falls back to the provider's standard
/// environment variable (e.g., `OPENAI_API_KEY`).
pub(crate) fn create_client(config: &LlmConfig) -> crate::Result<DefaultClient> {
    let client_config = build_client_config(config)?;

    DefaultClient::new(client_config, Some(&config.model)).map_err(|e| {
        let msg = format!("Failed to build LLM client: {e}");
        crate::XbergError::Validation {
            message: msg,
            source: Some(Box::new(e)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{BedrockConfig, LlmConfig};

    #[cfg(feature = "api")]
    #[tokio::test]
    async fn test_client_path_normalization_with_base_url() {
        use axum::{Router, routing::post};
        use liter_llm::LlmClient;
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        let app = Router::new().fallback(post(
            move |_method: axum::http::Method, uri: axum::http::Uri, headers: axum::http::HeaderMap| async move {
                assert_eq!(uri.path(), "/v1/chat/completions");

                let auth = headers
                    .get("authorization")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("none")
                    .to_string();
                let _ = tx.send(auth);

                axum::response::Json(serde_json::json!({
                    "id": "test",
                    "object": "chat.completion",
                    "created": 12345,
                    "model": "test",
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": "{\"foo\": \"bar\"}" },
                        "finish_reason": "stop"
                    }]
                }))
            },
        ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let base_url = format!("http://{}/v1/", addr);
        let config = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            api_key: Some("test-api-key".to_string()),
            base_url: Some(base_url),
            ..LlmConfig::default()
        };

        let client = create_client(&config).unwrap();

        let request = liter_llm::ChatCompletionRequest {
            model: config.model.clone(),
            messages: vec![liter_llm::Message::User(liter_llm::UserMessage {
                content: liter_llm::UserContent::Text("test".to_string()),
                ..Default::default()
            })],
            ..Default::default()
        };

        let _ = client.chat(request).await.expect("Request failed");

        let auth_header = tokio::time::timeout(tokio::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("Timeout waiting for header")
            .expect("No header received");

        assert_eq!(auth_header, "Bearer test-api-key");
    }

    #[test]
    fn test_create_client_sanitizes_base_url() {
        let config = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            api_key: Some("test-key".to_string()),
            base_url: Some("https://api.openai.com/v1/".to_string()),
            ..LlmConfig::default()
        };

        let _ = create_client(&config).unwrap();
    }

    #[test]
    fn test_create_client_applies_load_env_and_valid_headers() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Gateway-Key".to_string(), "secret123".to_string());
        let config = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            api_key: Some("test-key".to_string()),
            load_env: Some(true),
            headers: Some(headers),
            ..LlmConfig::default()
        };

        assert!(
            create_client(&config).is_ok(),
            "valid load_env + headers should build a client"
        );
    }

    #[test]
    fn test_create_client_rejects_invalid_header() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Bad\r\nInjected".to_string(), "value".to_string());
        let config = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            api_key: Some("test-key".to_string()),
            headers: Some(headers),
            ..LlmConfig::default()
        };

        match create_client(&config) {
            Err(crate::XbergError::Validation { message, .. }) => {
                assert!(message.contains("Invalid LLM header"), "unexpected message: {message}");
            }
            Err(other) => panic!("expected a Validation error, got: {other}"),
            Ok(_) => panic!("expected create_client to reject the invalid header"),
        }
    }

    fn bedrock_model_config(bedrock: BedrockConfig) -> LlmConfig {
        LlmConfig {
            model: "bedrock/anthropic.claude-3-sonnet-20240229-v1:0".to_string(),
            bedrock: Some(bedrock),
            ..LlmConfig::default()
        }
    }

    /// Regression test for https://github.com/xberg-io/xberg/issues/1381
    ///
    /// Region and cross-region prefix must reach the liter-llm client config —
    /// before this they were unreachable because `LlmConfig` had no `bedrock` field.
    #[test]
    fn test_build_client_config_maps_bedrock_region_and_cross_region_prefix() {
        let config = bedrock_model_config(BedrockConfig {
            region: Some("eu-central-1".to_string()),
            cross_region_prefix: Some("eu".to_string()),
            ..BedrockConfig::default()
        });

        let client_config = build_client_config(&config).expect("build client config");

        assert_eq!(client_config.bedrock_region.as_deref(), Some("eu-central-1"));
        assert_eq!(client_config.bedrock_cross_region_prefix.as_deref(), Some("eu"));
        assert_eq!(client_config.bedrock_access_key_id, None);
        assert_eq!(client_config.bedrock_secret_access_key, None);
        assert_eq!(client_config.bedrock_session_token, None);
    }

    /// Explicit static credentials must reach the client config verbatim.
    #[test]
    fn test_build_client_config_maps_bedrock_credentials() {
        let config = bedrock_model_config(BedrockConfig {
            region: Some("us-east-1".to_string()),
            access_key_id: Some("AKIAEXAMPLE".to_string()),
            secret_access_key: Some("example-secret".to_string()),
            session_token: Some("example-token".to_string()),
            ..BedrockConfig::default()
        });

        let client_config = build_client_config(&config).expect("build client config");

        assert_eq!(client_config.bedrock_region.as_deref(), Some("us-east-1"));
        assert_eq!(client_config.bedrock_access_key_id.as_deref(), Some("AKIAEXAMPLE"));
        assert_eq!(
            client_config.bedrock_secret_access_key.as_deref(),
            Some("example-secret")
        );
        assert_eq!(client_config.bedrock_session_token.as_deref(), Some("example-token"));
    }

    /// Session token is optional: long-lived key pairs must map without one.
    #[test]
    fn test_build_client_config_maps_bedrock_credentials_without_session_token() {
        let config = bedrock_model_config(BedrockConfig {
            access_key_id: Some("AKIAEXAMPLE".to_string()),
            secret_access_key: Some("example-secret".to_string()),
            ..BedrockConfig::default()
        });

        let client_config = build_client_config(&config).expect("build client config");

        assert_eq!(client_config.bedrock_access_key_id.as_deref(), Some("AKIAEXAMPLE"));
        assert_eq!(
            client_config.bedrock_secret_access_key.as_deref(),
            Some("example-secret")
        );
        assert_eq!(client_config.bedrock_session_token, None);
    }

    /// With no `bedrock` block, every Bedrock slot must stay `None` so liter-llm
    /// resolves region and credentials from the AWS environment / default chain.
    #[test]
    fn test_build_client_config_leaves_bedrock_unset_when_absent() {
        let config = LlmConfig {
            model: "openai/gpt-4o".to_string(),
            api_key: Some("test-key".to_string()),
            ..LlmConfig::default()
        };

        let client_config = build_client_config(&config).expect("build client config");

        assert_eq!(client_config.bedrock_region, None);
        assert_eq!(client_config.bedrock_cross_region_prefix, None);
        assert_eq!(client_config.bedrock_access_key_id, None);
        assert_eq!(client_config.bedrock_secret_access_key, None);
        assert_eq!(client_config.bedrock_session_token, None);
    }
}
