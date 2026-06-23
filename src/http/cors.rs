use std::env;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

const SECURITY_CSP: &str = "default-src 'none'; frame-ancestors 'none'; base-uri 'none'";
const ALLOWED_METHODS: &str = "DELETE, GET, OPTIONS, PATCH, POST, PUT";
const ALLOWED_HEADERS: &str = "Accept, Authorization, Content-Type";

pub async fn browser_cors(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let is_preflight = request.method() == Method::OPTIONS;
    let allowed_origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|origin| allowed_origin_header(&state.config.public_url, origin));

    let mut response = if is_preflight {
        if allowed_origin.is_none() {
            let mut response = StatusCode::FORBIDDEN.into_response();
            apply_security_headers(response.headers_mut(), &state.config.public_url);
            return response;
        }

        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };

    if let Some(origin) = allowed_origin {
        apply_cors_headers(response.headers_mut(), origin);
    }
    apply_security_headers(response.headers_mut(), &state.config.public_url);

    response
}

fn apply_cors_headers(headers: &mut HeaderMap, origin: HeaderValue) {
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(ALLOWED_METHODS),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(ALLOWED_HEADERS),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
}

fn apply_security_headers(headers: &mut HeaderMap, public_url: &str) {
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(SECURITY_CSP),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );

    if public_url
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("https://")
    {
        headers.insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }
}

fn allowed_origin_header(public_url: &str, origin: &HeaderValue) -> Option<HeaderValue> {
    let normalized_origin = origin.to_str().ok().and_then(normalize_origin)?;
    let allowed = allowed_origins(public_url);
    if allowed.iter().any(|allowed| allowed == &normalized_origin) {
        Some(origin.clone())
    } else {
        None
    }
}

fn allowed_origins(public_url: &str) -> Vec<String> {
    let mut allowed = Vec::new();

    if let Some(public_origin) = origin_for_url(public_url) {
        allowed.push(public_origin);
    }

    if let Ok(value) = env::var("OPENCORD_ALLOWED_ORIGINS") {
        for origin in value.split(',').filter_map(normalize_origin) {
            if !allowed.iter().any(|allowed| allowed == &origin) {
                allowed.push(origin);
            }
        }
    }

    allowed
}

fn origin_for_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let (scheme, rest) = trimmed.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }

    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if authority.is_empty() {
        return None;
    }

    normalize_origin(&format!("{}://{}", scheme.to_ascii_lowercase(), authority))
}

fn normalize_origin(origin: &str) -> Option<String> {
    let trimmed = origin.trim().trim_end_matches('/');
    let (scheme, authority) = trimmed.split_once("://")?;
    if authority.is_empty() {
        return None;
    }
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }

    Some(format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        authority.to_ascii_lowercase()
    ))
}
