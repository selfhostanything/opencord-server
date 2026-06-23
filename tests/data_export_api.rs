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

fn bearer_bytes_request(
    method: Method,
    uri: &str,
    token: &str,
    content_type: &str,
    body: impl Into<Vec<u8>>,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.into()))
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
                "display_name": "Data Export Test User",
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

async fn create_space_with_channel(
    app: &axum::Router,
    owner_token: &str,
    suffix: &str,
) -> (String, String, String) {
    let org = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            "/organizations",
            owner_token,
            json!({ "name": format!("Data Export Org {suffix}") }),
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
        .oneshot(bearer_json_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            owner_token,
            json!({ "name": format!("Data Export Space {suffix}") }),
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
        .oneshot(bearer_json_request(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            owner_token,
            json!({ "name": format!("data-export-channel-{suffix}") }),
        ))
        .await
        .unwrap();
    assert_eq!(channel.status(), StatusCode::CREATED);
    let channel_id = response_json(channel).await["channel"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    (organization_id, space_id, channel_id)
}

async fn add_space_member(app: &axum::Router, owner_token: &str, space_id: &str, user_id: &str) {
    let response = app
        .clone()
        .oneshot(bearer_json_request(
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

async fn create_linked_attachment_message(
    app: &axum::Router,
    token: &str,
    channel_id: &str,
) -> (String, String) {
    let presigned = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            "/attachments/presign",
            token,
            json!({
                "channel_id": channel_id,
                "file_name": "export-plan.txt",
                "content_type": "text/plain",
                "size_bytes": 11
            }),
        ))
        .await
        .unwrap();
    assert_eq!(presigned.status(), StatusCode::CREATED);
    let attachment_id = response_json(presigned).await["attachment"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let upload = app
        .clone()
        .oneshot(bearer_bytes_request(
            Method::PUT,
            &format!("/attachments/{attachment_id}/content"),
            token,
            "text/plain",
            b"hello world".to_vec(),
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::OK);

    let message = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            token,
            json!({
                "content": "export this message",
                "attachment_ids": [attachment_id]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(message.status(), StatusCode::CREATED);
    let message_id = response_json(message).await["message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    (message_id, attachment_id)
}

#[tokio::test]
async fn organization_admin_can_export_messages_and_file_manifest() {
    let app = test_app();
    let (owner_token, _) = register(&app, "data-export-owner@example.com").await;
    let (member_token, member_id) = register(&app, "data-export-member@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "primary").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;
    let (message_id, attachment_id) =
        create_linked_attachment_message(&app, &owner_token, &channel_id).await;

    let exported = app
        .clone()
        .oneshot(bearer_json_request(
            Method::GET,
            &format!(
                "/organizations/{organization_id}/data-export?from=2020-01-01T00:00:00Z&to=2030-01-01T00:00:00Z"
            ),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(exported.status(), StatusCode::OK);
    let body = response_json(exported).await;
    assert_eq!(body["export"]["organization_id"], organization_id);
    assert_eq!(body["export"]["format"], "json");
    assert_eq!(body["export"]["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["export"]["files"].as_array().unwrap().len(), 1);

    let message = &body["export"]["messages"][0];
    assert_eq!(message["id"], message_id);
    assert_eq!(message["organization_id"], organization_id);
    assert_eq!(message["channel_id"], channel_id);
    assert_eq!(message["content"], "export this message");
    assert!(message["created_at"].as_str().is_some());

    let file = &body["export"]["files"][0];
    assert_eq!(file["id"], attachment_id);
    assert_eq!(file["message_id"], message_id);
    assert_eq!(file["file_name"], "export-plan.txt");
    assert_eq!(file["content_type"], "text/plain");
    assert_eq!(file["size_bytes"], 11);
    assert_eq!(file["status"], "linked");
    assert_eq!(
        file["download_url"],
        format!("https://chat.example.com/attachments/{attachment_id}/content")
    );

    let forbidden = app
        .oneshot(bearer_json_request(
            Method::GET,
            &format!(
                "/organizations/{organization_id}/data-export?from=2020-01-01T00:00:00Z&to=2030-01-01T00:00:00Z"
            ),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}
