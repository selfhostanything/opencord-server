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
                "display_name": "Retention Test User",
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

async fn create_space(app: &axum::Router, owner_token: &str, suffix: &str) -> (String, String) {
    let org = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            owner_token,
            json!({ "name": format!("Retention Org {suffix}") }),
        ))
        .await
        .unwrap();
    assert_eq!(org.status(), StatusCode::CREATED);
    let organization_id = response_json(org).await["organization"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let space = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            owner_token,
            json!({ "name": format!("Retention Space {suffix}") }),
        ))
        .await
        .unwrap();
    assert_eq!(space.status(), StatusCode::CREATED);
    let space_id = response_json(space).await["space"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    (organization_id, space_id)
}

async fn add_space_member(app: &axum::Router, owner_token: &str, space_id: &str, user_id: &str) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/members"),
            owner_token,
            json!({
                "user_id": user_id,
                "role": "member"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn organization_admin_can_configure_retention_policy() {
    let app = test_app();
    let (owner_token, _) = register(&app, "retention-owner@example.com").await;
    let (member_token, member_id) = register(&app, "retention-member@example.com").await;
    let (organization_id, space_id) = create_space(&app, &owner_token, "policy").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let configured = app
        .clone()
        .oneshot(bearer_request(
            Method::PUT,
            &format!("/organizations/{organization_id}/retention-policy"),
            &owner_token,
            json!({
                "messages_retain_days": 365,
                "files_retain_days": 90,
                "audit_logs_retain_days": 730,
                "deleted_message_purge_days": 30
            }),
        ))
        .await
        .unwrap();
    assert_eq!(configured.status(), StatusCode::OK);
    let body = response_json(configured).await;
    assert_eq!(body["policy"]["organization_id"], organization_id);
    assert_eq!(body["policy"]["messages_retain_days"], 365);
    assert_eq!(body["policy"]["files_retain_days"], 90);
    assert_eq!(body["policy"]["audit_logs_retain_days"], 730);
    assert_eq!(body["policy"]["deleted_message_purge_days"], 30);

    let fetched = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/retention-policy"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    let body = response_json(fetched).await;
    assert_eq!(body["policy"]["messages_retain_days"], 365);

    let forbidden = app
        .oneshot(bearer_request(
            Method::PUT,
            &format!("/organizations/{organization_id}/retention-policy"),
            &member_token,
            json!({
                "messages_retain_days": 1,
                "files_retain_days": 1,
                "audit_logs_retain_days": 1,
                "deleted_message_purge_days": 1
            }),
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}
