use std::time::Instant;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;

use crate::domain::ids;
use crate::state::AppState;

pub const REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestId(pub String);

pub async fn request_observability(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = request_id_from_headers(&request).unwrap_or_else(new_request_id);
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started_at = Instant::now();

    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| HeaderValue::from_static("req_invalid")),
    );

    let status = response.status().as_u16();
    state
        .metrics
        .record_http_request(method.as_str().to_owned(), status);
    tracing::info!(
        request_id = request_id.as_str(),
        method = method.as_str(),
        path = path.as_str(),
        status,
        latency_ms = started_at.elapsed().as_millis() as u64,
        "http request completed"
    );

    response
}

fn request_id_from_headers(request: &Request<Body>) -> Option<String> {
    request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(ToOwned::to_owned)
}

fn new_request_id() -> String {
    format!("req_{}", ids::new_uuid_v7())
}
