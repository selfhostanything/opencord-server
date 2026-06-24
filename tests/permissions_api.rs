use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use opencord_server::config::AppConfig;
use opencord_server::routes::{api_router, api_router_with_state};
use opencord_server::state::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

fn test_app() -> axum::Router {
    api_router(AppConfig {
        version: "test-version".to_owned(),
        public_url: "https://chat.example.com".to_owned(),
    })
}

fn test_app_with_state() -> (axum::Router, AppState) {
    let state = AppState::in_memory(AppConfig {
        version: "test-version".to_owned(),
        public_url: "https://chat.example.com".to_owned(),
    });
    (api_router_with_state(state.clone()), state)
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
                "display_name": "Permission Test User",
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
            json!({ "name": format!("Permission Org {suffix}") }),
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
            json!({ "name": format!("Permission Space {suffix}") }),
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
            json!({ "name": format!("permission-channel-{suffix}") }),
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

async fn create_voice_channel(
    app: &axum::Router,
    owner_token: &str,
    space_id: &str,
    suffix: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            owner_token,
            json!({
                "name": format!("permission-voice-{suffix}"),
                "kind": "voice"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["channel"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
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

async fn create_role(
    app: &axum::Router,
    owner_token: &str,
    space_id: &str,
    name: &str,
    permissions: Value,
) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/roles"),
            owner_token,
            json!({
                "name": name,
                "permissions": permissions
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

async fn assign_role(
    app: &axum::Router,
    owner_token: &str,
    space_id: &str,
    role_id: &str,
    user_id: &str,
) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/roles/{role_id}/assignments"),
            owner_token,
            json!({ "user_id": user_id }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn set_member_override(
    app: &axum::Router,
    owner_token: &str,
    channel_id: &str,
    user_id: &str,
    allow: Value,
    deny: Value,
) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/permission-overrides"),
            owner_token,
            json!({
                "target_kind": "member",
                "target_id": user_id,
                "allow": allow,
                "deny": deny
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn voice_permission_revocation_publishes_media_boundary_event() {
    let (app, state) = test_app_with_state();
    let (owner_token, _) = register(&app, "permission-media-owner@example.com").await;
    let (member_token, member_id) = register(&app, "permission-media-member@example.com").await;
    let (_, space_id, _) = create_space_with_channel(&app, &owner_token, "media-revoke").await;
    let channel_id = create_voice_channel(&app, &owner_token, &space_id, "media-revoke").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;
    let voice_role = create_role(
        &app,
        &owner_token,
        &space_id,
        "Voice Members",
        json!(["CONNECT_VOICE", "SPEAK", "SHARE_SCREEN"]),
    )
    .await;
    assign_role(&app, &owner_token, &space_id, &voice_role, &member_id).await;

    let joined = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/voice/channels/{channel_id}/join"),
            &member_token,
            json!({ "self_mute": false, "self_deaf": false }),
        ))
        .await
        .unwrap();
    assert_eq!(joined.status(), StatusCode::CREATED);

    let mut events = state.realtime.subscribe();
    set_member_override(
        &app,
        &owner_token,
        &channel_id,
        &member_id,
        json!([]),
        json!(["CONNECT_VOICE", "SPEAK", "SHARE_SCREEN"]),
    )
    .await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
        .await
        .expect("media permission event")
        .expect("realtime event");
    assert_eq!(event.event_type, "media.permission_revoked");
    assert_eq!(event.scope.channel_id.as_deref(), Some(channel_id.as_str()));
    assert_eq!(event.data["target_kind"], "member");
    assert_eq!(event.data["target_id"], member_id);
    assert_eq!(event.data["action"], "disconnect");
    assert_eq!(event.data["grants"]["can_publish_audio"], false);
    assert_eq!(event.data["grants"]["can_publish_screen"], false);
    assert_eq!(event.data["grants"]["can_subscribe"], true);
}

#[tokio::test]
async fn voice_publish_permission_revocation_keeps_subscription_grant() {
    let (app, state) = test_app_with_state();
    let (owner_token, _) = register(&app, "permission-media-owner-screen@example.com").await;
    let (_, member_id) = register(&app, "permission-media-member-screen@example.com").await;
    let (_, space_id, _) =
        create_space_with_channel(&app, &owner_token, "media-screen-revoke").await;
    let channel_id =
        create_voice_channel(&app, &owner_token, &space_id, "media-screen-revoke").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let mut events = state.realtime.subscribe();
    set_member_override(
        &app,
        &owner_token,
        &channel_id,
        &member_id,
        json!([]),
        json!(["SHARE_SCREEN"]),
    )
    .await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
        .await
        .expect("media permission event")
        .expect("realtime event");
    assert_eq!(event.event_type, "media.permission_revoked");
    assert_eq!(event.data["action"], "restrict_publish");
    assert_eq!(event.data["revoked"]["share_screen"], true);
    assert_eq!(event.data["grants"]["can_publish_audio"], true);
    assert_eq!(event.data["grants"]["can_publish_screen"], false);
    assert_eq!(event.data["grants"]["can_subscribe"], true);
}

#[tokio::test]
async fn member_can_view_and_send_but_cannot_manage_channels() {
    let app = test_app();
    let (owner_token, _) = register(&app, "permission-owner@example.com").await;
    let (member_token, member_id) = register(&app, "permission-member@example.com").await;
    let (_, space_id, channel_id) = create_space_with_channel(&app, &owner_token, "member").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let list = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/spaces/{space_id}/channels"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    assert_eq!(
        response_json(list).await["channels"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let message = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &member_token,
            json!({ "content": "member can send" }),
        ))
        .await
        .unwrap();
    assert_eq!(message.status(), StatusCode::CREATED);

    let patch_channel = app
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/channels/{channel_id}"),
            &member_token,
            json!({ "topic": "member should not manage channels" }),
        ))
        .await
        .unwrap();
    assert_eq!(patch_channel.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn owner_can_remove_space_member_and_reinvite() {
    let app = test_app();
    let (owner_token, _) = register(&app, "permission-owner-remove@example.com").await;
    let (member_token, member_id) = register(&app, "permission-member-remove@example.com").await;
    let (_, space_id, _) = create_space_with_channel(&app, &owner_token, "remove-member").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let removed = app
        .clone()
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/spaces/{space_id}/members/{member_id}"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);

    let member_list = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/spaces/{space_id}/channels"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(member_list.status(), StatusCode::NOT_FOUND);

    let duplicate_remove = app
        .clone()
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/spaces/{space_id}/members/{member_id}"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(duplicate_remove.status(), StatusCode::NOT_FOUND);

    add_space_member(&app, &owner_token, &space_id, &member_id).await;
    let relisted = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/spaces/{space_id}/channels"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(relisted.status(), StatusCode::OK);
}

#[tokio::test]
async fn space_member_remove_requires_manage_space() {
    let app = test_app();
    let (owner_token, owner_id) =
        register(&app, "permission-owner-remove-denied@example.com").await;
    let (member_token, member_id) =
        register(&app, "permission-member-remove-denied@example.com").await;
    let (_, space_id, _) = create_space_with_channel(&app, &owner_token, "remove-denied").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let denied = app
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/spaces/{space_id}/members/{owner_id}"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn overrides_and_roles_control_send_and_manage_messages() {
    let app = test_app();
    let (owner_token, _) = register(&app, "permission-owner-override@example.com").await;
    let (member_token, member_id) = register(&app, "permission-denied-member@example.com").await;
    let (moderator_token, moderator_id) = register(&app, "permission-moderator@example.com").await;
    let (_, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "overrides").await;

    add_space_member(&app, &owner_token, &space_id, &member_id).await;
    add_space_member(&app, &owner_token, &space_id, &moderator_id).await;

    let moderator_role = create_role(
        &app,
        &owner_token,
        &space_id,
        "Moderators",
        json!(["MANAGE_MESSAGES"]),
    )
    .await;
    assign_role(
        &app,
        &owner_token,
        &space_id,
        &moderator_role,
        &moderator_id,
    )
    .await;
    set_member_override(
        &app,
        &owner_token,
        &channel_id,
        &member_id,
        json!([]),
        json!(["SEND_MESSAGES"]),
    )
    .await;

    let denied_send = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &member_token,
            json!({ "content": "blocked by channel override" }),
        ))
        .await
        .unwrap();
    assert_eq!(denied_send.status(), StatusCode::FORBIDDEN);

    let owner_message = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            &owner_token,
            json!({ "content": "moderation target" }),
        ))
        .await
        .unwrap();
    assert_eq!(owner_message.status(), StatusCode::CREATED);
    let message_id = response_json(owner_message).await["message"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let member_delete = app
        .clone()
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/messages/{message_id}"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(member_delete.status(), StatusCode::FORBIDDEN);

    let moderator_delete = app
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/messages/{message_id}"),
            &moderator_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(moderator_delete.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn manage_permissions_endpoints_require_manage_permissions() {
    let app = test_app();
    let (owner_token, _) = register(&app, "permission-owner-manage@example.com").await;
    let (member_token, member_id) = register(&app, "permission-member-manage@example.com").await;
    let (_, space_id, channel_id) = create_space_with_channel(&app, &owner_token, "manage").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let member_create_role = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/roles"),
            &member_token,
            json!({
                "name": "Denied role",
                "permissions": ["MANAGE_MESSAGES"]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(member_create_role.status(), StatusCode::FORBIDDEN);

    let member_override = app
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/permission-overrides"),
            &member_token,
            json!({
                "target_kind": "member",
                "target_id": member_id,
                "allow": ["MANAGE_MESSAGES"],
                "deny": []
            }),
        ))
        .await
        .unwrap();
    assert_eq!(member_override.status(), StatusCode::FORBIDDEN);
}
