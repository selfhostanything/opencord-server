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

fn bearer_json_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn register_creates_uuid_v7_user_session_and_me_returns_user() {
    let app = test_app();

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "Ada@Example.COM",
                "display_name": "Ada Lovelace",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    let user_id = body["user"]["id"].as_str().unwrap();
    assert_eq!(uuid::Uuid::parse_str(user_id).unwrap().get_version_num(), 7);
    assert_eq!(body["user"]["email"], "ada@example.com");
    assert_eq!(body["user"]["display_name"], "Ada Lovelace");
    assert!(body["user"].get("password").is_none());

    let token = body["session"]["token"].as_str().unwrap();
    assert!(token.len() >= 32);

    let me = app
        .oneshot(bearer_json_request(Method::GET, "/me", token, json!({})))
        .await
        .unwrap();

    assert_eq!(me.status(), StatusCode::OK);
    let body = response_json(me).await;
    assert_eq!(body["user"]["id"], user_id);
    assert_eq!(body["user"]["email"], "ada@example.com");
}

#[tokio::test]
async fn register_rejects_duplicate_email_case_insensitively() {
    let app = test_app();
    let payload = json!({
        "email": "sam@example.com",
        "display_name": "Sam",
        "password": "correct horse battery staple"
    });

    let first = app
        .clone()
        .oneshot(json_request(Method::POST, "/auth/register", payload))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);

    let duplicate = app
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "SAM@example.com",
                "display_name": "Other Sam",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    let body = response_json(duplicate).await;
    assert_eq!(body["error"]["code"], "email_already_registered");
}

#[tokio::test]
async fn login_logout_and_session_check_enforce_bearer_token() {
    let app = test_app();

    app.clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "grace@example.com",
                "display_name": "Grace Hopper",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    let bad_login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/login",
            json!({
                "email": "grace@example.com",
                "password": "wrong password"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(bad_login.status(), StatusCode::UNAUTHORIZED);

    let login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/login",
            json!({
                "email": "GRACE@example.com",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let token = response_json(login).await["session"]["token"]
        .as_str()
        .unwrap()
        .to_owned();

    let me = app
        .clone()
        .oneshot(bearer_json_request(Method::GET, "/me", &token, json!({})))
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);

    let logout = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            "/auth/logout",
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);

    let me_after_logout = app
        .oneshot(bearer_json_request(Method::GET, "/me", &token, json!({})))
        .await
        .unwrap();
    assert_eq!(me_after_logout.status(), StatusCode::UNAUTHORIZED);
}
