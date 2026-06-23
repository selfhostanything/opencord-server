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
                "display_name": "Message Test User",
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

async fn create_channel(app: &axum::Router, token: &str, suffix: &str) -> String {
    let org = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            token,
            json!({ "name": format!("Message Org {suffix}") }),
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
            token,
            json!({ "name": format!("Message Space {suffix}") }),
        ))
        .await
        .unwrap();
    assert_eq!(space.status(), StatusCode::CREATED);
    let space_id = response_json(space).await["space"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let channel = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            token,
            json!({ "name": format!("message-channel-{suffix}") }),
        ))
        .await
        .unwrap();
    assert_eq!(channel.status(), StatusCode::CREATED);
    response_json(channel).await["channel"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn channel_member_can_send_list_edit_and_delete_message() {
    let app = test_app();
    let token = register_token(&app, "message-owner@example.com").await;
    let channel_id = create_channel(&app, &token, "owner").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &token,
            json!({ "content": "hello from OpenCord" }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let message_id = body["message"]["id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(message_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(body["message"]["channel_id"], channel_id);
    assert_eq!(body["message"]["content"], "hello from OpenCord");
    assert_eq!(body["message"]["content_format"], "plain");
    assert!(body["message"]["author_user_id"].as_str().is_some());
    assert!(body["message"]["edited_at"].is_null());
    assert!(body["message"]["deleted_at"].is_null());

    let list = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/channels/{channel_id}/messages"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body = response_json(list).await;
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][0]["id"], message_id);

    let updated = app
        .clone()
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/messages/{message_id}"),
            &token,
            json!({ "content": "edited message" }),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let body = response_json(updated).await;
    assert_eq!(body["message"]["content"], "edited message");
    assert!(body["message"]["edited_at"].as_str().is_some());

    let deleted = app
        .clone()
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/messages/{message_id}"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let list_after_delete = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/channels/{channel_id}/messages"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(list_after_delete.status(), StatusCode::OK);
    assert!(
        response_json(list_after_delete).await["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn message_endpoints_require_bearer_auth() {
    let app = test_app();
    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/channels/{}/messages", uuid::Uuid::now_v7()),
            json!({ "content": "No auth message" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn message_create_is_rate_limited_per_user_channel() {
    let app = test_app();
    let token = register_token(&app, "message-rate-limited-owner@example.com").await;
    let channel_id = create_channel(&app, &token, "rate-limited").await;

    for index in 0..5 {
        let created = app
            .clone()
            .oneshot(bearer_request(
                Method::POST,
                &format!("/channels/{channel_id}/messages"),
                &token,
                json!({ "content": format!("limited message {index}") }),
            ))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
    }

    let limited = app
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &token,
            json!({ "content": "limited message blocked" }),
        ))
        .await
        .unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        limited
            .headers()
            .get("x-ratelimit-remaining")
            .unwrap()
            .to_str()
            .unwrap(),
        "0"
    );
    assert!(limited.headers().get(header::RETRY_AFTER).is_some());
    let body = response_json(limited).await;
    assert_eq!(body["error"]["code"], "rate_limited");
}

#[tokio::test]
async fn messages_are_isolated_by_channel_space_membership() {
    let app = test_app();
    let owner_token = register_token(&app, "message-isolated-owner@example.com").await;
    let outsider_token = register_token(&app, "message-isolated-outsider@example.com").await;
    let channel_id = create_channel(&app, &owner_token, "isolated").await;

    let outsider_create = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &outsider_token,
            json!({ "content": "outsider message" }),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_create.status(), StatusCode::NOT_FOUND);

    let owner_create = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &owner_token,
            json!({ "content": "private message" }),
        ))
        .await
        .unwrap();
    assert_eq!(owner_create.status(), StatusCode::CREATED);
    let message_id = response_json(owner_create).await["message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let outsider_list = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/channels/{channel_id}/messages"),
            &outsider_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_list.status(), StatusCode::NOT_FOUND);

    let outsider_edit = app
        .clone()
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/messages/{message_id}"),
            &outsider_token,
            json!({ "content": "stolen edit" }),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_edit.status(), StatusCode::NOT_FOUND);

    let outsider_delete = app
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/messages/{message_id}"),
            &outsider_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_delete.status(), StatusCode::NOT_FOUND);
}
