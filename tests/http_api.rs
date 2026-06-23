use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use opencord_server::config::AppConfig;
use opencord_server::http::request_id::REQUEST_ID_HEADER;
use opencord_server::routes::api_router;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

fn test_app() -> axum::Router {
    api_router(AppConfig {
        version: "test-version".to_owned(),
        public_url: "https://chat.example.com".to_owned(),
    })
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    serde_json::from_slice(&bytes).expect("response should be json")
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    String::from_utf8(bytes.to_vec()).expect("response should be utf-8")
}

#[tokio::test]
async fn healthz_returns_status_and_version() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], "test-version");
}

#[tokio::test]
async fn api_responses_include_security_headers() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let headers = response.headers();
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["referrer-policy"], "no-referrer");
    assert_eq!(headers["x-frame-options"], "DENY");
    assert_eq!(
        headers["content-security-policy"],
        "default-src 'none'; frame-ancestors 'none'; base-uri 'none'"
    );
    assert_eq!(
        headers["strict-transport-security"],
        "max-age=31536000; includeSubDomains"
    );
}

#[tokio::test]
async fn api_responses_include_request_id_and_http_metrics() {
    let app = test_app();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let generated_request_id = response
        .headers()
        .get(REQUEST_ID_HEADER)
        .expect("response includes x-request-id")
        .to_str()
        .expect("request ID is valid header text");
    let uuid = Uuid::parse_str(
        generated_request_id
            .strip_prefix("req_")
            .expect("generated request IDs use req_ prefix"),
    )
    .expect("generated request ID suffix is a UUID");
    assert_eq!(uuid.get_version_num(), 7);

    let metrics = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response_text(metrics).await;
    assert!(body.contains("opencord_http_requests_total{method=\"GET\",status=\"200\"} 1"));
}

#[tokio::test]
async fn api_responses_preserve_client_request_id() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .header(REQUEST_ID_HEADER, "client-request-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[REQUEST_ID_HEADER], "client-request-1");
}

#[tokio::test]
async fn discovery_endpoints_return_basic_metadata() {
    let well_known = test_app()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/.well-known/opencord")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(well_known.status(), StatusCode::OK);
    let body = response_json(well_known).await;
    assert_eq!(body["server"], "opencord");
    assert_eq!(body["version"], "test-version");
    assert_eq!(body["api_base_url"], "https://chat.example.com/api");
    assert_eq!(body["realtime_url"], "wss://chat.example.com/ws");

    let version = test_app()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/version")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response_json(version).await["version"], "test-version");

    let capabilities = test_app()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/capabilities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response_json(capabilities).await;
    assert!(
        body["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|capability| capability == "uuidv7")
    );
    assert!(
        body["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|capability| capability == "attachments")
    );
    assert!(
        body["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|capability| capability == "audit")
    );
    assert!(
        body["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|capability| capability == "push_tokens")
    );
}

#[tokio::test]
async fn cors_preflight_supports_browser_clients() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/healthz")
                .header(header::ORIGIN, "https://chat.example.com")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        "https://chat.example.com"
    );
    assert_eq!(response.headers()[header::VARY], "Origin");
    assert!(
        response.headers()[header::ACCESS_CONTROL_ALLOW_METHODS]
            .to_str()
            .unwrap()
            .contains("POST")
    );
    assert!(
        response.headers()[header::ACCESS_CONTROL_ALLOW_METHODS]
            .to_str()
            .unwrap()
            .contains("PUT")
    );
}

#[tokio::test]
async fn cors_preflight_rejects_unconfigured_origins() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/healthz")
                .header(header::ORIGIN, "https://untrusted.example")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}
