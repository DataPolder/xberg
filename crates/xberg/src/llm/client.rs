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
///
/// For a `bedrock/`-prefixed model, liter-llm's own provider validation runs
/// here and rejects the request up front — with a message naming the required
/// AWS credentials and how to supply them — when neither explicit
/// [`BedrockConfig`](crate::core::config::BedrockConfig) credentials nor
/// `AWS_ACCESS_KEY_ID` in the environment are available. That upstream message
/// is wrapped with the model that failed to build, so the resulting error names
/// the operation (building the LLM client), the input (the model string), and
/// (via the wrapped source) a concrete suggestion.
pub(crate) fn create_client(config: &LlmConfig) -> crate::Result<DefaultClient> {
    let client_config = build_client_config(config)?;

    DefaultClient::new(client_config, Some(&config.model)).map_err(|e| {
        let msg = format!("Failed to build LLM client for model '{}': {e}", config.model);
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
            bedrock: Some(Box::new(bedrock)),
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

    /// Regression test for https://github.com/xberg-io/xberg/issues/1381
    ///
    /// A `bedrock/`-prefixed model with no explicit `BedrockConfig` credentials
    /// and no `AWS_ACCESS_KEY_ID` in the environment must fail fast at
    /// `create_client` time with a clear, actionable error instead of a bare
    /// "failed to build client" message or a confusing runtime request failure.
    /// `#[serial]` because the test mutates the process-wide `AWS_ACCESS_KEY_ID`
    /// environment variable, matching the save/restore convention used by the
    /// other env-mutating tests in this crate (see
    /// `core::server_config::tests::env_tests`).
    ///
    /// `#[allow(unsafe_code)]` is scoped to this one function rather than the module:
    /// the crate sets `#![deny(unsafe_code)]` (lib.rs:36), and `std::env::set_var` /
    /// `remove_var` are `unsafe` as of edition 2024. `core::server_config::tests::env_tests`
    /// takes the same exemption, but as a file-level `#![allow]` — it can, because it is a
    /// dedicated test file. These tests sit inline in a production module, so a
    /// module-scoped allow would also cover `create_client` itself. ~keep
    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn test_create_client_reports_clear_error_for_unconfigured_bedrock() {
        let original = std::env::var("AWS_ACCESS_KEY_ID").ok();
        // SAFETY: guarded by #[serial_test::serial] — no other test in this
        // process observes AWS_ACCESS_KEY_ID concurrently while this one runs.
        unsafe {
            std::env::remove_var("AWS_ACCESS_KEY_ID");
        }

        let config = bedrock_model_config(BedrockConfig::default());
        let result = create_client(&config);

        // SAFETY: see above.
        unsafe {
            match &original {
                Some(val) => std::env::set_var("AWS_ACCESS_KEY_ID", val),
                None => std::env::remove_var("AWS_ACCESS_KEY_ID"),
            }
        }

        match result {
            Err(crate::XbergError::Validation { message, .. }) => {
                assert!(
                    message.contains("bedrock/anthropic.claude-3-sonnet-20240229-v1:0"),
                    "error should name the model (the input) that failed to build: {message}"
                );
                assert!(
                    message.contains("AWS credentials"),
                    "error should name the root cause (missing AWS credentials): {message}"
                );
                assert!(
                    message.contains("AWS_ACCESS_KEY_ID") && message.contains("AWS_SECRET_ACCESS_KEY"),
                    "error should suggest how to fix it (set explicit config or the AWS env vars): {message}"
                );
            }
            Err(other) => panic!("expected a Validation error, got: {other}"),
            Ok(_) => panic!("expected create_client to reject an unconfigured bedrock model"),
        }
    }

    /// Once Bedrock credentials actually reach liter-llm's own `ClientConfig`
    /// (built by `build_client_config`), that type's `Debug` impl — not xberg's
    /// `LlmConfig`/`BedrockConfig` impls — is the last line of defense against a
    /// credential reaching a log via an accidental `tracing::debug!("{config:?}")`
    /// or panic dump. This proves the credential survives the xberg -> liter-llm
    /// boundary redacted, using real-looking (but fake) values.
    #[test]
    fn test_build_client_config_debug_never_leaks_bedrock_credentials() {
        let config = bedrock_model_config(BedrockConfig {
            region: Some("us-east-1".to_string()),
            access_key_id: Some("AKIAEXAMPLEFAKE".to_string()),
            secret_access_key: Some("example-fake-secret".to_string()),
            session_token: Some("example-fake-token".to_string()),
            ..BedrockConfig::default()
        });

        let client_config = build_client_config(&config).expect("build client config");
        let rendered = format!("{client_config:?}");

        for secret in ["AKIAEXAMPLEFAKE", "example-fake-secret", "example-fake-token"] {
            assert!(
                !rendered.contains(secret),
                "liter-llm ClientConfig Debug leaked {secret}: {rendered}"
            );
        }
        assert!(
            rendered.contains("us-east-1"),
            "non-secret region should stay visible for diagnosability: {rendered}"
        );
    }

    /// A misconfigured request (invalid header) on a model that also carries
    /// live-looking Bedrock credentials must never leak those credentials into
    /// the resulting error — the error's `Display` output is exactly the string
    /// that reaches xberg's logs and callers, so this is the real "does a
    /// credential reach a log" boundary (as opposed to the config-file
    /// persistence `Serialize` impl, which legitimately carries a credential the
    /// caller explicitly set, the same way `api_key` already does).
    #[test]
    fn test_create_client_error_never_leaks_bedrock_credentials() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Bad\r\nInjected".to_string(), "value".to_string());
        let mut config = bedrock_model_config(BedrockConfig {
            region: Some("us-east-1".to_string()),
            access_key_id: Some("AKIAEXAMPLEFAKE".to_string()),
            secret_access_key: Some("example-fake-secret".to_string()),
            session_token: Some("example-fake-token".to_string()),
            ..BedrockConfig::default()
        });
        config.headers = Some(headers);

        // Not `expect_err`: that needs `T: Debug` and liter-llm's `DefaultClient` does not
        // implement it, so the success arm has to be discarded by hand.
        let Err(err) = create_client(&config) else {
            panic!("invalid header must reject the request");
        };
        let rendered = format!("{err}");

        for secret in ["AKIAEXAMPLEFAKE", "example-fake-secret", "example-fake-token"] {
            assert!(
                !rendered.contains(secret),
                "create_client error leaked {secret}: {rendered}"
            );
        }
    }
}
