use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use opencord_server::config::AppConfig;
use opencord_server::routes::api_router;
use serde_json::{Value, json};
use tower::ServiceExt;

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

fn json_request(method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn bearer_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn register(app: &axum::Router, email: &str) -> String {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Push Token User",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["session"]["token"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn push_token_registration_is_authenticated_idempotent_and_masked() {
    let app = test_app();
    let session_token = register(&app, "push-owner@example.com").await;
    let device_token = "ExponentPushToken[abcdefghijklmnopqrstuvwxyz123456]";

    let create = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/push-tokens",
            &session_token,
            json!({
                "platform": "ios",
                "token": device_token,
                "device_name": "Ada iPhone"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let body = response_json(create).await;
    let push_token = &body["push_token"];
    let push_token_id = push_token["id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(push_token_id)
            .unwrap()
            .get_version_num(),
        7
    );
    assert_eq!(push_token["platform"], "ios");
    assert_eq!(push_token["token_last_four"], "456]");
    assert_eq!(push_token["device_name"], "Ada iPhone");
    assert!(push_token["created_at"].as_str().is_some());
    assert!(push_token["updated_at"].as_str().is_some());
    assert!(push_token.get("token").is_none());

    let duplicate = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/push-tokens",
            &session_token,
            json!({
                "platform": "ios",
                "token": device_token,
                "device_name": "Ada iPhone 16"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::CREATED);
    let duplicate_body = response_json(duplicate).await;
    assert_eq!(duplicate_body["push_token"]["id"], push_token_id);
    assert_eq!(duplicate_body["push_token"]["device_name"], "Ada iPhone 16");

    let list = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            "/push-tokens",
            &session_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body = response_json(list).await;
    let tokens = body["push_tokens"].as_array().unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0]["id"], push_token_id);
    assert!(tokens[0].get("token").is_none());
}

#[tokio::test]
async fn push_tokens_require_bearer_auth_and_validate_input() {
    let app = test_app();
    let session_token = register(&app, "push-validation@example.com").await;

    let unauthenticated = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/push-tokens",
            json!({
                "platform": "android",
                "token": "ExponentPushToken[abcdefghijklmnopqrstuvwxyz123456]"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let invalid_platform = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/push-tokens",
            &session_token,
            json!({
                "platform": "watchos",
                "token": "ExponentPushToken[abcdefghijklmnopqrstuvwxyz123456]"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(invalid_platform.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(invalid_platform).await["error"]["code"],
        "invalid_request"
    );

    let invalid_token = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/push-tokens",
            &session_token,
            json!({
                "platform": "android",
                "token": "short"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(invalid_token.status(), StatusCode::BAD_REQUEST);

    let empty_list = app
        .oneshot(bearer_request(
            Method::GET,
            "/push-tokens",
            &session_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(empty_list.status(), StatusCode::OK);
    assert!(
        response_json(empty_list).await["push_tokens"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
