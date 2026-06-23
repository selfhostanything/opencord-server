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

async fn register(app: &axum::Router, email: &str) -> (String, String) {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Bot Test User",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    (
        body["session"]["token"].as_str().unwrap().to_owned(),
        body["user"]["id"].as_str().unwrap().to_owned(),
    )
}

async fn create_organization(app: &axum::Router, token: &str) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            token,
            json!({ "name": "Bot Org" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["organization"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn organization_admin_can_create_bot_application_user_and_token() {
    let app = test_app();
    let (owner_token, owner_id) = register(&app, "bot-owner@example.com").await;
    let organization_id = create_organization(&app, &owner_token).await;

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/bot-applications"),
            &owner_token,
            json!({
                "name": "Release Helper",
                "description": "Posts release notes and deployment summaries"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    let application = &body["bot_application"];
    let token = &body["bot_token"];
    assert_eq!(application["organization_id"], organization_id);
    assert_eq!(application["created_by_user_id"], owner_id);
    assert_eq!(application["name"], "Release Helper");
    assert_eq!(
        application["description"],
        "Posts release notes and deployment summaries"
    );
    assert_eq!(application["status"], "active");
    assert_eq!(
        uuid::Uuid::parse_str(application["id"].as_str().unwrap())
            .unwrap()
            .get_version_num(),
        7
    );
    assert_eq!(
        uuid::Uuid::parse_str(application["bot_user_id"].as_str().unwrap())
            .unwrap()
            .get_version_num(),
        7
    );
    assert_eq!(token["application_id"], application["id"]);
    assert_eq!(
        uuid::Uuid::parse_str(token["id"].as_str().unwrap())
            .unwrap()
            .get_version_num(),
        7
    );
    let raw_token = token["token"].as_str().unwrap();
    let token_last_four = token["token_last_four"].as_str().unwrap();
    assert!(raw_token.starts_with("ocb_"));
    assert_eq!(&raw_token[raw_token.len() - 4..], token_last_four);
}

#[tokio::test]
async fn bot_application_create_requires_organization_admin() {
    let app = test_app();
    let (owner_token, _) = register(&app, "bot-private-owner@example.com").await;
    let (outsider_token, _) = register(&app, "bot-private-outsider@example.com").await;
    let organization_id = create_organization(&app, &owner_token).await;

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/bot-applications"),
            &outsider_token,
            json!({
                "name": "Unauthorized Bot",
                "description": "Should not be created"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
