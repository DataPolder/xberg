//! OpenWebUI compatibility handlers.
//!
//! Provides endpoints compatible with OpenWebUI's Content Extraction Engine:
//!
//! - `PUT /process` — "External" engine: raw binary body, returns `{page_content, metadata}`
//! - `POST /v1/convert/file` — "Docling" engine: multipart form-data, returns `{document: {md_content}, status}`

use axum::{Json, body::Bytes, extract::State, http::HeaderMap};
use tower::Service;

use crate::service::ExtractionRequest;

use super::{
    error::{ApiError, MultipartApi},
    types::{ApiState, DoclingCompatDocument, DoclingCompatResponse, OpenWebDocumentMetadata, OpenWebDocumentResponse},
};

/// OpenWebUI "External" engine handler.
///
/// PUT /process
///
/// Accepts raw binary file content in the request body.
/// Uses `Content-Type` header for MIME type and `X-Filename` header for the filename.
///
/// Returns a JSON document matching OpenWebUI's external document loader contract.
#[utoipa::path(
    put,
    path = "/process",
    tag = "openweb",
    request_body(content_type = "application/octet-stream", content = Vec<u8>),
    responses(
        (status = 200, description = "Document extracted", body = OpenWebDocumentResponse),
        (status = 400, description = "Bad request", body = crate::api::types::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::api::types::ErrorResponse),
    )
)]
#[cfg_attr(
    feature = "otel",
    tracing::instrument(name = "api.openweb_process", skip(state, headers, body))
)]
pub(crate) async fn openweb_external_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<OpenWebDocumentResponse>, ApiError> {
    if body.is_empty() {
        return Err(ApiError::validation(crate::error::XbergError::validation(
            "Empty request body — upload a file as the raw request body",
        )));
    }

    let mime_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).trim())
        .unwrap_or(crate::core::mime::OCTET_STREAM_MIME_TYPE)
        .to_string();

    let filename = headers
        .get("X-Filename")
        .and_then(|v| v.to_str().ok())
        .map(|v| urlencoding::decode(v).unwrap_or_else(|_| v.into()).into_owned())
        .unwrap_or_else(|| "unknown".to_string());

    let mime_type = if mime_type == crate::core::mime::OCTET_STREAM_MIME_TYPE {
        crate::core::mime::detect_mime_type(&filename, false).unwrap_or(mime_type)
    } else {
        mime_type
    };

    let mut config = (*state.default_config).clone();
    config.output_format = crate::core::config::OutputFormat::Markdown;

    let request = ExtractionRequest::bytes(body.to_vec(), mime_type, config);
    let mut svc = state
        .extraction_service
        .lock()
        .expect("extraction service lock poisoned")
        .clone();
    let result = svc.call(request).await?;

    Ok(Json(OpenWebDocumentResponse {
        page_content: result.content,
        metadata: OpenWebDocumentMetadata { source: filename },
    }))
}

/// OpenWebUI "Docling" engine handler (docling-serve compatible).
///
/// POST /v1/convert/file
///
/// Accepts multipart form-data with a `files` field containing the document.
/// Returns a JSON response matching docling-serve's `/v1/convert/file` contract.
///
/// OpenWebUI reads only `document.md_content` from the response.
#[utoipa::path(
    post,
    path = "/v1/convert/file",
    tag = "openweb",
    request_body(content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Document converted", body = DoclingCompatResponse),
        (status = 400, description = "Bad request", body = crate::api::types::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::api::types::ErrorResponse),
    )
)]
#[cfg_attr(
    feature = "otel",
    tracing::instrument(name = "api.openweb_docling", skip(state, multipart))
)]
pub(crate) async fn openweb_docling_handler(
    State(state): State<ApiState>,
    MultipartApi(mut multipart): MultipartApi,
) -> Result<Json<DoclingCompatResponse>, ApiError> {
    let mut file_data: Option<(Vec<u8>, String)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::validation(crate::error::XbergError::validation(e.to_string())))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "files" || field_name == "file" {
            let file_name = field.file_name().map(|s| s.to_string());
            let content_type = field.content_type().map(|s| s.to_string());
            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::validation(crate::error::XbergError::validation(e.to_string())))?;

            let mut mime_type = content_type.unwrap_or_else(|| crate::core::mime::OCTET_STREAM_MIME_TYPE.to_string());

            if mime_type == crate::core::mime::OCTET_STREAM_MIME_TYPE
                && let Some(ref name) = file_name
                && let Ok(detected) = crate::core::mime::detect_mime_type(name, false)
            {
                mime_type = detected;
            }

            file_data = Some((data.to_vec(), mime_type));
            break;
        }
    }

    let (data, mime_type) = file_data.ok_or_else(|| {
        ApiError::validation(crate::error::XbergError::validation(
            "No file provided. Upload a file with field name 'files'.",
        ))
    })?;

    let mut config = (*state.default_config).clone();
    config.output_format = crate::core::config::OutputFormat::Markdown;

    let request = ExtractionRequest::bytes(data, mime_type, config);
    let mut svc = state
        .extraction_service
        .lock()
        .expect("extraction service lock poisoned")
        .clone();
    let result = svc.call(request).await?;

    Ok(Json(DoclingCompatResponse {
        document: DoclingCompatDocument {
            md_content: result.content,
        },
        status: "success".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::{post, put},
    };
    use tower::ServiceExt;

    use super::*;

    /// Tiny real fixture (39 bytes) from the shared `test_documents` corpus: plain text,
    /// no OCR/network required, extracts to non-empty Markdown quickly.
    fn fixture_bytes() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test_documents/text/plain.txt");
        std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()))
    }

    fn test_router() -> Router {
        let extraction_service = crate::service::ExtractionServiceBuilder::new().build();
        let state = ApiState {
            default_config: std::sync::Arc::new(crate::ExtractionConfig::default()),
            extraction_service: std::sync::Arc::new(std::sync::Mutex::new(extraction_service)),
            #[cfg(feature = "api")]
            job_store: std::sync::Arc::new(crate::api::jobs::JobStore::new()),
        };

        Router::new()
            .route("/process", put(openweb_external_handler))
            .route("/v1/convert/file", post(openweb_docling_handler))
            .with_state(state)
    }

    #[tokio::test]
    async fn openweb_process_returns_markdown_and_source() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/process")
                    .header("content-type", "text/plain")
                    .header("X-Filename", "plain.txt")
                    .body(Body::from(fixture_bytes()))
                    .expect("valid request"),
            )
            .await
            .expect("handler responded");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes readable");
        let parsed: OpenWebDocumentResponse =
            serde_json::from_slice(&bytes).expect("response parses as OpenWebDocumentResponse");
        assert!(!parsed.page_content.is_empty(), "page_content must be non-empty");
        assert_eq!(parsed.metadata.source, "plain.txt");
    }

    #[tokio::test]
    async fn openweb_process_rejects_empty_body() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/process")
                    .header("content-type", "text/plain")
                    .header("X-Filename", "empty.txt")
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("handler responded");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn openweb_process_url_decodes_filename_header() {
        let app = test_router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/process")
                    .header("content-type", "text/plain")
                    .header("X-Filename", "my%20file.txt")
                    .body(Body::from(fixture_bytes()))
                    .expect("valid request"),
            )
            .await
            .expect("handler responded");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes readable");
        let parsed: OpenWebDocumentResponse =
            serde_json::from_slice(&bytes).expect("response parses as OpenWebDocumentResponse");
        assert_eq!(parsed.metadata.source, "my file.txt");
    }

    #[tokio::test]
    async fn openweb_docling_convert_returns_md_content() {
        let app = test_router();
        let boundary = "testboundary123";

        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"plain.txt\"\r\nContent-Type: text/plain\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(&fixture_bytes());
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/convert/file")
                    .header("content-type", format!("multipart/form-data; boundary={boundary}"))
                    .body(Body::from(body))
                    .expect("valid request"),
            )
            .await
            .expect("handler responded");

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes readable");
        let parsed: DoclingCompatResponse =
            serde_json::from_slice(&bytes).expect("response parses as DoclingCompatResponse");
        assert!(!parsed.document.md_content.is_empty(), "md_content must be non-empty");
        assert_eq!(parsed.status, "success");
    }

    #[tokio::test]
    async fn openweb_docling_rejects_missing_file() {
        let app = test_router();
        let boundary = "testboundary";
        let body = format!("--{boundary}--\r\n");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/convert/file")
                    .header("content-type", format!("multipart/form-data; boundary={boundary}"))
                    .body(Body::from(body))
                    .expect("valid request"),
            )
            .await
            .expect("handler responded");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
