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

async fn response_bytes(response: axum::response::Response) -> Vec<u8> {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body")
        .to_vec()
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
                "display_name": "Attachment Test User",
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
    token: &str,
    suffix: &str,
) -> (String, String, String) {
    let org = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            "/organizations",
            token,
            json!({ "name": format!("Attachment Org {suffix}") }),
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
            token,
            json!({ "name": format!("Attachment Space {suffix}") }),
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
            token,
            json!({ "name": format!("attachment-channel-{suffix}") }),
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

async fn presign_attachment(app: &axum::Router, token: &str, channel_id: &str) -> Value {
    let response = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            "/attachments/presign",
            token,
            json!({
                "channel_id": channel_id,
                "file_name": "diagram.png",
                "content_type": "image/png",
                "size_bytes": 11
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await
}

#[tokio::test]
async fn attachment_presign_is_rate_limited_per_user_channel() {
    let app = test_app();
    let (token, _) = register(&app, "attachment-rate-limited-owner@example.com").await;
    let (_, _, channel_id) = create_space_with_channel(&app, &token, "rate-limited").await;

    for _ in 0..5 {
        presign_attachment(&app, &token, &channel_id).await;
    }

    let limited = app
        .oneshot(bearer_json_request(
            Method::POST,
            "/attachments/presign",
            &token,
            json!({
                "channel_id": channel_id,
                "file_name": "blocked.png",
                "content_type": "image/png",
                "size_bytes": 11
            }),
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
async fn attachment_upload_is_limited_to_original_uploader() {
    let app = test_app();
    let (owner_token, _) = register(&app, "attachment-upload-owner@example.com").await;
    let (member_token, member_id) = register(&app, "attachment-upload-member@example.com").await;
    let (_, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "upload-owner").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let presigned = presign_attachment(&app, &owner_token, &channel_id).await;
    let attachment_id = presigned["attachment"]["id"].as_str().unwrap();

    let member_upload = app
        .clone()
        .oneshot(bearer_bytes_request(
            Method::PUT,
            &format!("/attachments/{attachment_id}/content"),
            &member_token,
            "image/png",
            b"hello image".to_vec(),
        ))
        .await
        .unwrap();

    assert_eq!(member_upload.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn member_uploads_attachment_and_links_it_to_message() {
    let app = test_app();
    let (token, user_id) = register(&app, "attachment-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &token, "owner").await;

    let presigned = presign_attachment(&app, &token, &channel_id).await;
    let attachment = &presigned["attachment"];
    let attachment_id = attachment["id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(attachment_id)
            .unwrap()
            .get_version_num(),
        7
    );
    assert_eq!(attachment["organization_id"], organization_id);
    assert_eq!(attachment["space_id"], space_id);
    assert_eq!(attachment["channel_id"], channel_id);
    assert_eq!(attachment["uploader_user_id"], user_id);
    assert_eq!(attachment["file_name"], "diagram.png");
    assert_eq!(attachment["content_type"], "image/png");
    assert_eq!(attachment["size_bytes"], 11);
    assert_eq!(attachment["status"], "pending");
    assert_eq!(attachment["message_id"], Value::Null);
    assert_eq!(presigned["upload"]["method"], "PUT");
    assert_eq!(
        presigned["upload"]["url"],
        format!("https://chat.example.com/attachments/{attachment_id}/content")
    );

    let upload = app
        .clone()
        .oneshot(bearer_bytes_request(
            Method::PUT,
            &format!("/attachments/{attachment_id}/content"),
            &token,
            "image/png",
            b"hello image".to_vec(),
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::OK);
    let uploaded = response_json(upload).await;
    assert_eq!(uploaded["attachment"]["status"], "uploaded");

    let message = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &token,
            json!({
                "content": "here is the diagram",
                "attachment_ids": [attachment_id]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(message.status(), StatusCode::CREATED);
    let body = response_json(message).await;
    let message_id = body["message"]["id"].as_str().unwrap();
    assert_eq!(body["message"]["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["message"]["attachments"][0]["download_url"],
        format!("https://chat.example.com/attachments/{attachment_id}/content")
    );
    assert_eq!(body["message"]["attachments"][0]["message_id"], message_id);

    let list = app
        .clone()
        .oneshot(bearer_json_request(
            Method::GET,
            &format!("/channels/{channel_id}/messages"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = response_json(list).await;
    assert_eq!(
        list_body["messages"][0]["attachments"][0]["file_name"],
        "diagram.png"
    );

    let download = app
        .clone()
        .oneshot(bearer_json_request(
            Method::GET,
            &format!("/attachments/{attachment_id}/content"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(
        download.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/png"
    );
    assert_eq!(response_bytes(download).await, b"hello image");
}

#[tokio::test]
async fn attachments_require_uploaded_same_channel_files() {
    let app = test_app();
    let (token, _) = register(&app, "attachment-validation@example.com").await;
    let (_, _, first_channel_id) = create_space_with_channel(&app, &token, "first").await;
    let (_, _, second_channel_id) = create_space_with_channel(&app, &token, "second").await;

    let pending = presign_attachment(&app, &token, &first_channel_id).await;
    let pending_id = pending["attachment"]["id"].as_str().unwrap();

    let rejected_pending = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            &format!("/channels/{first_channel_id}/messages"),
            &token,
            json!({
                "content": "not uploaded yet",
                "attachment_ids": [pending_id]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(rejected_pending.status(), StatusCode::BAD_REQUEST);

    let upload = app
        .clone()
        .oneshot(bearer_bytes_request(
            Method::PUT,
            &format!("/attachments/{pending_id}/content"),
            &token,
            "image/png",
            b"hello image".to_vec(),
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::OK);

    let rejected_channel = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            &format!("/channels/{second_channel_id}/messages"),
            &token,
            json!({
                "content": "wrong channel",
                "attachment_ids": [pending_id]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(rejected_channel.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn attachment_content_is_protected_by_channel_visibility() {
    let app = test_app();
    let (owner_token, _) = register(&app, "attachment-private-owner@example.com").await;
    let (outsider_token, _) = register(&app, "attachment-private-outsider@example.com").await;
    let (_, _, channel_id) = create_space_with_channel(&app, &owner_token, "private").await;

    let presigned = presign_attachment(&app, &owner_token, &channel_id).await;
    let attachment_id = presigned["attachment"]["id"].as_str().unwrap();

    let upload = app
        .clone()
        .oneshot(bearer_bytes_request(
            Method::PUT,
            &format!("/attachments/{attachment_id}/content"),
            &owner_token,
            "image/png",
            b"hello image".to_vec(),
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::OK);

    let outsider_download = app
        .clone()
        .oneshot(bearer_json_request(
            Method::GET,
            &format!("/attachments/{attachment_id}/content"),
            &outsider_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_download.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn presign_rejects_missing_auth_and_oversize_files() {
    let app = test_app();
    let (token, _) = register(&app, "attachment-oversize@example.com").await;
    let (_, _, channel_id) = create_space_with_channel(&app, &token, "oversize").await;

    let unauthenticated = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/attachments/presign",
            json!({
                "channel_id": channel_id,
                "file_name": "diagram.png",
                "content_type": "image/png",
                "size_bytes": 11
            }),
        ))
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let oversize = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            "/attachments/presign",
            &token,
            json!({
                "channel_id": channel_id,
                "file_name": "huge.mov",
                "content_type": "video/quicktime",
                "size_bytes": 10_485_761
            }),
        ))
        .await
        .unwrap();
    assert_eq!(oversize.status(), StatusCode::BAD_REQUEST);
}
