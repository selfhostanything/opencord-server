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

fn bot_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bot {token}"))
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
                "display_name": "Compat Test User",
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
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            owner_token,
            json!({ "name": format!("Compat Org {suffix}") }),
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
            json!({ "name": format!("Compat Space {suffix}") }),
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
            owner_token,
            json!({ "name": format!("compat-channel-{suffix}") }),
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

async fn create_bot(
    app: &axum::Router,
    owner_token: &str,
    organization_id: &str,
) -> (String, String) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/bot-applications"),
            owner_token,
            json!({
                "name": "Compat Bot",
                "description": "Exercises Discord-compatible message routes"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    (
        body["bot_token"]["token"].as_str().unwrap().to_owned(),
        body["bot_application"]["bot_user_id"]
            .as_str()
            .unwrap()
            .to_owned(),
    )
}

async fn add_space_member(
    app: &axum::Router,
    owner_token: &str,
    space_id: &str,
    user_id: &str,
    role: &str,
) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/members"),
            owner_token,
            json!({
                "user_id": user_id,
                "role": role
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn create_role(app: &axum::Router, owner_token: &str, space_id: &str, name: &str) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/roles"),
            owner_token,
            json!({
                "name": name,
                "permissions": ["VIEW_CHANNEL"]
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["role"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create_uploaded_attachment(app: &axum::Router, token: &str, channel_id: &str) -> String {
    let presigned = app
        .clone()
        .oneshot(bearer_request(
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
            "image/png",
            b"hello image".to_vec(),
        ))
        .await
        .unwrap();
    assert_eq!(upload.status(), StatusCode::OK);

    attachment_id
}

#[tokio::test]
async fn bot_can_expand_allowed_mentions_through_compat_routes() {
    let app = test_app();
    let (owner_token, _) = register(&app, "compat-mention-owner@example.com").await;
    let (_, mentioned_user_id) = register(&app, "compat-mentioned-user@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "mentions").await;
    let role_id = create_role(&app, &owner_token, &space_id, "Release Watchers").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &mentioned_user_id, "member").await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id, "member").await;

    let created = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({
                "content": format!("ping <@{mentioned_user_id}> <@{bot_user_id}> <@&{role_id}> @everyone"),
                "allowed_mentions": {
                    "parse": ["everyone"],
                    "users": [mentioned_user_id],
                    "roles": [role_id]
                }
            }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::OK);
    let body = response_json(created).await;
    let message_id = body["id"].as_str().unwrap().to_owned();
    assert_eq!(body["mention_everyone"], true);
    assert_eq!(body["mentions"].as_array().unwrap().len(), 1);
    assert_eq!(body["mentions"][0]["id"], mentioned_user_id);
    assert_eq!(body["mentions"][0]["username"], "Compat Test User");
    assert_eq!(body["mentions"][0]["bot"], false);
    assert_eq!(body["mention_roles"], json!([role_id]));

    let listed = app
        .clone()
        .oneshot(bot_request(
            Method::GET,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["id"], message_id);
    assert_eq!(body[0]["mention_everyone"], true);
    assert_eq!(body[0]["mentions"].as_array().unwrap().len(), 1);
    assert_eq!(body[0]["mentions"][0]["id"], mentioned_user_id);
    assert_eq!(body[0]["mention_roles"], json!([role_id]));

    let edited = app
        .clone()
        .oneshot(bot_request(
            Method::PATCH,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages/{message_id}"),
            &bot_token,
            json!({
                "content": format!("role only <@{mentioned_user_id}> <@&{role_id}>"),
                "allowed_mentions": {
                    "parse": ["roles"]
                }
            }),
        ))
        .await
        .unwrap();

    assert_eq!(edited.status(), StatusCode::OK);
    let body = response_json(edited).await;
    assert_eq!(body["mention_everyone"], false);
    assert!(body["mentions"].as_array().unwrap().is_empty());
    assert_eq!(body["mention_roles"], json!([role_id]));
}

#[tokio::test]
async fn bot_can_send_list_edit_and_delete_messages_through_compat_routes() {
    let app = test_app();
    let (owner_token, _) = register(&app, "compat-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "messages").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id, "member").await;

    let created = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({
                "content": "compat hello",
                "tts": false
            }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::OK);
    let body = response_json(created).await;
    let message_id = body["id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(message_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(body["channel_id"], channel_id);
    assert_eq!(body["author"]["id"], bot_user_id);
    assert_eq!(body["author"]["username"], "Compat Bot");
    assert_eq!(body["author"]["bot"], true);
    assert_eq!(body["content"], "compat hello");
    assert_eq!(body["type"], 0);
    assert_eq!(body["tts"], false);
    assert_eq!(body["pinned"], false);
    assert!(body["embeds"].as_array().unwrap().is_empty());
    assert!(body["attachments"].as_array().unwrap().is_empty());

    let listed = app
        .clone()
        .oneshot(bot_request(
            Method::GET,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["id"], message_id);

    let edited = app
        .clone()
        .oneshot(bot_request(
            Method::PATCH,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages/{message_id}"),
            &bot_token,
            json!({ "content": "compat edited" }),
        ))
        .await
        .unwrap();
    assert_eq!(edited.status(), StatusCode::OK);
    let body = response_json(edited).await;
    assert_eq!(body["content"], "compat edited");
    assert!(body["edited_timestamp"].as_str().is_some());

    let deleted = app
        .clone()
        .oneshot(bot_request(
            Method::DELETE,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages/{message_id}"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let listed_after_delete = app
        .clone()
        .oneshot(bot_request(
            Method::GET,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed_after_delete.status(), StatusCode::OK);
    assert!(
        response_json(listed_after_delete)
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn bot_can_create_and_list_reply_messages_through_compat_routes() {
    let app = test_app();
    let (owner_token, _) = register(&app, "compat-reply-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "replies").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id, "member").await;

    let base = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({ "content": "base message" }),
        ))
        .await
        .unwrap();
    assert_eq!(base.status(), StatusCode::OK);
    let base_message_id = response_json(base).await["id"].as_str().unwrap().to_owned();

    let reply = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({
                "content": "reply message",
                "message_reference": {
                    "message_id": base_message_id,
                    "channel_id": channel_id
                }
            }),
        ))
        .await
        .unwrap();
    assert_eq!(reply.status(), StatusCode::OK);
    let body = response_json(reply).await;
    let reply_message_id = body["id"].as_str().unwrap();
    assert_eq!(body["content"], "reply message");
    assert_eq!(body["message_reference"]["message_id"], base_message_id);
    assert_eq!(body["message_reference"]["channel_id"], channel_id);
    assert_eq!(body["referenced_message"]["id"], base_message_id);
    assert_eq!(body["referenced_message"]["channel_id"], channel_id);
    assert_eq!(body["referenced_message"]["author"]["id"], bot_user_id);
    assert_eq!(body["referenced_message"]["author"]["bot"], true);
    assert_eq!(body["referenced_message"]["content"], "base message");
    assert_eq!(body["referenced_message"]["type"], 0);

    let listed = app
        .clone()
        .oneshot(bot_request(
            Method::GET,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert_eq!(body[1]["id"], reply_message_id);
    assert_eq!(body[1]["message_reference"]["message_id"], base_message_id);
    assert_eq!(body[1]["message_reference"]["channel_id"], channel_id);
    assert_eq!(body[1]["referenced_message"]["id"], base_message_id);
    assert_eq!(body[1]["referenced_message"]["content"], "base message");
}

#[tokio::test]
async fn bot_can_list_native_message_attachments_through_compat_routes() {
    let app = test_app();
    let (owner_token, owner_id) = register(&app, "compat-attachment-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "attachments").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id, "member").await;

    let attachment_id = create_uploaded_attachment(&app, &owner_token, &channel_id).await;
    let message = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &owner_token,
            json!({
                "content": "diagram attached",
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

    let listed = app
        .clone()
        .oneshot(bot_request(
            Method::GET,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["id"], message_id);
    assert_eq!(body[0]["author"]["id"], owner_id);
    assert_eq!(body[0]["content"], "diagram attached");
    assert_eq!(body[0]["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(body[0]["attachments"][0]["id"], attachment_id);
    assert_eq!(body[0]["attachments"][0]["filename"], "diagram.png");
    assert_eq!(body[0]["attachments"][0]["content_type"], "image/png");
    assert_eq!(body[0]["attachments"][0]["size"], 11);
    assert_eq!(
        body[0]["attachments"][0]["url"],
        format!("https://chat.example.com/attachments/{attachment_id}/content")
    );
    assert_eq!(
        body[0]["attachments"][0]["proxy_url"],
        format!("https://chat.example.com/attachments/{attachment_id}/content")
    );
}

#[tokio::test]
async fn bot_can_send_and_list_basic_embeds_through_compat_routes() {
    let app = test_app();
    let (owner_token, _) = register(&app, "compat-embed-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "embeds").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id, "member").await;

    let embed = json!({
        "title": "Deploy ready",
        "description": "Release 1.2.3 passed checks",
        "url": "https://chat.example.com/releases/1.2.3",
        "color": 5793266,
        "fields": [
            {
                "name": "Environment",
                "value": "production",
                "inline": true
            }
        ]
    });
    let created = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({
                "content": "",
                "embeds": [embed.clone()],
                "allowed_mentions": {
                    "parse": []
                }
            }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::OK);
    let body = response_json(created).await;
    let message_id = body["id"].as_str().unwrap();
    assert_eq!(body["author"]["id"], bot_user_id);
    assert_eq!(body["content"], "");
    assert_eq!(body["embeds"], json!([embed]));
    assert!(body["mentions"].as_array().unwrap().is_empty());
    assert!(body["mention_roles"].as_array().unwrap().is_empty());

    let listed = app
        .clone()
        .oneshot(bot_request(
            Method::GET,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["id"], message_id);
    assert_eq!(body[0]["embeds"], json!([embed]));
}

#[tokio::test]
async fn compat_messages_require_valid_bot_token() {
    let app = test_app();

    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/channels/{}/messages",
                uuid::Uuid::now_v7()
            ),
            "ocb_not-valid",
            json!({ "content": "nope" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn compat_bot_must_be_space_member_to_send_messages() {
    let app = test_app();
    let (owner_token, _) = register(&app, "compat-member-owner@example.com").await;
    let (organization_id, _, channel_id) =
        create_space_with_channel(&app, &owner_token, "not-member").await;
    let (bot_token, _) = create_bot(&app, &owner_token, &organization_id).await;

    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({ "content": "bot has no space membership" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn compat_bot_cannot_edit_other_users_message_without_manage_messages() {
    let app = test_app();
    let (owner_token, _) = register(&app, "compat-edit-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "edit-other").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id, "member").await;

    let human_message = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &owner_token,
            json!({ "content": "human owned" }),
        ))
        .await
        .unwrap();
    assert_eq!(human_message.status(), StatusCode::CREATED);
    let message_id = response_json(human_message).await["message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let response = app
        .clone()
        .oneshot(bot_request(
            Method::PATCH,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages/{message_id}"),
            &bot_token,
            json!({ "content": "bot edit attempt" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
