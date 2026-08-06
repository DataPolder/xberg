//! Issue #1391 — opt-in Prometheus `/metrics` endpoint.
//!
//! xberg never installs an OTel `MeterProvider` in production code:
//! `telemetry::metrics::get_metrics()` binds its instruments to whichever meter provider
//! is global the *first* time anything calls it, via a process-global `OnceLock`, and then
//! never looks again (see that module's doc comment, and `issue_332_telemetry_metrics.rs`
//! for the same hazard). Left alone, that resolves to the OTel no-op meter, and `/metrics`
//! would scrape an empty registry forever.
//!
//! This test calls `xberg::telemetry::init_prometheus()` explicitly, before building the
//! router or driving any extraction, to prove the documented call order actually wires a
//! real, populated registry into `GET /metrics`. It is `#[serial]` because the installed
//! meter provider — and the counters it feeds — are process-global and cumulative;
//! concurrent extractions from another test in this binary could otherwise land inside this
//! test's before/after window.
#![cfg(all(feature = "api", feature = "prometheus"))]

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serial_test::serial;
use tower::ServiceExt;
use xberg::api::{ApiSizeLimits, create_router_with_limits};

/// Body text that has never been extracted on this machine before, so this test's
/// `/extract` call is never served from a warm cache entry left by a previous run.
fn unique_body() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock must be after the unix epoch")
        .as_nanos();
    format!("issue 1391 prometheus metrics fixture, run {nanos}")
}

/// Build a single-file multipart `/extract` request body, matching the shape used by
/// `api_extract_multipart.rs`.
fn multipart_extract_request(body_text: &str) -> Request<Body> {
    let boundary = "X-BOUNDARY";
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"files\"; filename=\"issue-1391.txt\"\r\n\
Content-Type: text/plain\r\n\
\r\n\
{body_text}\r\n\
--{boundary}--\r\n"
    );
    let body_bytes = body.into_bytes();

    Request::builder()
        .method("POST")
        .uri("/extract")
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .header("content-length", body_bytes.len())
        .body(Body::from(body_bytes))
        .expect("failed to build /extract request")
}

/// Find the Prometheus counter family declared by a `# TYPE <name> counter` line whose
/// name contains every one of `name_fragments`, and return the sum of every sample value
/// recorded for it.
///
/// The exact exported name is deliberately NOT hard-coded here: `opentelemetry-prometheus`
/// name-mangles the OTel dotted metric name `xberg.extraction.total` (dots -> underscores)
/// and may or may not append an extra `_total` suffix under the OTel-to-Prometheus
/// compatibility rules — that exact shape needs empirical confirmation against a real
/// scrape, not an assumption baked into this test. Searching by fragment, and printing the
/// full body on failure, makes a wrong guess fail loudly with the actual text instead of
/// silently asserting against the wrong name.
fn sum_counter_family(metrics_body: &str, name_fragments: &[&str]) -> f64 {
    let type_line = metrics_body
        .lines()
        .find(|line| {
            line.starts_with("# TYPE")
                && line.ends_with("counter")
                && name_fragments.iter().all(|fragment| line.contains(fragment))
        })
        .unwrap_or_else(|| {
            panic!(
                "no `# TYPE ... counter` line matching every fragment {name_fragments:?} was found \
                 in the /metrics body; full body follows:\n{metrics_body}"
            )
        });

    // `# TYPE <name> counter` — the name is the second whitespace-separated token.
    let metric_name = type_line
        .split_whitespace()
        .nth(2)
        .unwrap_or_else(|| panic!("malformed TYPE line, could not extract metric name: {type_line:?}"));

    metrics_body
        .lines()
        .filter(|line| !line.starts_with('#') && (line.starts_with(metric_name)))
        .map(|line| {
            let value_str = line
                .rsplit(' ')
                .next()
                .unwrap_or_else(|| panic!("malformed sample line, no value found: {line:?}"));
            value_str
                .parse::<f64>()
                .unwrap_or_else(|_| panic!("sample value was not a valid float: {line:?}"))
        })
        .sum()
}

#[tokio::test]
#[serial]
async fn metrics_endpoint_reports_extraction_total_after_a_real_extraction() {
    // Must run before this binary's first extraction, or `get_metrics()`'s `OnceLock`
    // latches onto the OTel no-op meter and this test can never pass — see the module doc.
    let _registry = xberg::telemetry::init_prometheus();

    let router = create_router_with_limits(xberg::ExtractionConfig::default(), ApiSizeLimits::default());

    let extract_response = router
        .clone()
        .oneshot(multipart_extract_request(&unique_body()))
        .await
        .expect("/extract request must succeed at the transport level");
    assert_eq!(
        extract_response.status(),
        StatusCode::OK,
        "the real extraction driving this test must itself succeed"
    );

    let metrics_response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/metrics")
                .body(Body::empty())
                .expect("failed to build /metrics request"),
        )
        .await
        .expect("/metrics request must succeed at the transport level");

    assert_eq!(metrics_response.status(), StatusCode::OK, "/metrics must return 200");
    let content_type = metrics_response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("/metrics must set a Content-Type header")
        .to_string();
    assert_eq!(
        content_type, "text/plain; version=0.0.4",
        "/metrics must use the Prometheus text exposition format, not JSON"
    );

    let body_bytes = to_bytes(metrics_response.into_body(), 10 * 1024 * 1024)
        .await
        .expect("failed to read /metrics response body");
    let metrics_body = String::from_utf8(body_bytes.to_vec()).expect("/metrics body must be valid UTF-8");

    let extraction_total = sum_counter_family(&metrics_body, &["extraction", "total"]);
    assert!(
        extraction_total > 0.0,
        "expected a non-zero xberg extraction-total counter after a real extraction; \
         full /metrics body follows:\n{metrics_body}"
    );
}
