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

async fn register_token(app: &axum::Router, email: &str) -> String {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Channel Test User",
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

async fn create_organization(app: &axum::Router, token: &str, name: &str) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            token,
            json!({ "name": name }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["organization"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create_space(
    app: &axum::Router,
    token: &str,
    organization_id: &str,
    name: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            token,
            json!({ "name": name }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["space"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn space_member_can_create_list_and_update_text_channel() {
    let app = test_app();
    let token = register_token(&app, "channel-owner@example.com").await;
    let organization_id = create_organization(&app, &token, "Channel Parent").await;
    let space_id = create_space(&app, &token, &organization_id, "Channel Space").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            &token,
            json!({
                "name": "General Chat",
                "topic": "team-wide discussion"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let channel_id = body["channel"]["id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(channel_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(body["channel"]["organization_id"], organization_id);
    assert_eq!(body["channel"]["space_id"], space_id);
    assert_eq!(body["channel"]["kind"], "text");
    assert_eq!(body["channel"]["name"], "General Chat");
    assert_eq!(body["channel"]["slug"], "general-chat");
    assert_eq!(body["channel"]["topic"], "team-wide discussion");
    assert_eq!(body["channel"]["is_private"], false);

    let list = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/spaces/{space_id}/channels"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(list.status(), StatusCode::OK);
    let body = response_json(list).await;
    assert_eq!(body["channels"].as_array().unwrap().len(), 1);
    assert_eq!(body["channels"][0]["id"], channel_id);

    let updated = app
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/channels/{channel_id}"),
            &token,
            json!({
                "name": "Announcements",
                "topic": "read-only updates later",
                "position": 10,
                "is_private": true
            }),
        ))
        .await
        .unwrap();

    assert_eq!(updated.status(), StatusCode::OK);
    let body = response_json(updated).await;
    assert_eq!(body["channel"]["id"], channel_id);
    assert_eq!(body["channel"]["name"], "Announcements");
    assert_eq!(body["channel"]["slug"], "announcements");
    assert_eq!(body["channel"]["topic"], "read-only updates later");
    assert_eq!(body["channel"]["position"], 10);
    assert_eq!(body["channel"]["is_private"], true);
}

#[tokio::test]
async fn channel_endpoints_require_bearer_auth() {
    let app = test_app();
    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/spaces/{}/channels", uuid::Uuid::now_v7()),
            json!({ "name": "No Auth Channel" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn channels_are_isolated_by_space_membership() {
    let app = test_app();
    let owner_token = register_token(&app, "channel-isolated-owner@example.com").await;
    let outsider_token = register_token(&app, "channel-isolated-outsider@example.com").await;
    let organization_id = create_organization(&app, &owner_token, "Channel Isolation Parent").await;
    let space_id = create_space(
        &app,
        &owner_token,
        &organization_id,
        "Private Channel Space",
    )
    .await;

    let outsider_create = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            &outsider_token,
            json!({ "name": "Outsider Channel" }),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_create.status(), StatusCode::NOT_FOUND);

    let owner_create = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            &owner_token,
            json!({ "name": "Private Channel" }),
        ))
        .await
        .unwrap();
    assert_eq!(owner_create.status(), StatusCode::CREATED);
    let channel_id = response_json(owner_create).await["channel"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let outsider_list = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/spaces/{space_id}/channels"),
            &outsider_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_list.status(), StatusCode::NOT_FOUND);

    let outsider_update = app
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/channels/{channel_id}"),
            &outsider_token,
            json!({ "name": "Stolen Channel" }),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_update.status(), StatusCode::NOT_FOUND);
}
