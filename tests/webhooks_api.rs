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
                "display_name": "Webhook Test User",
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
    kind: &str,
) -> (String, String, String) {
    let org = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            owner_token,
            json!({ "name": format!("Webhook Org {suffix}") }),
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
            json!({ "name": format!("Webhook Space {suffix}") }),
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
            json!({
                "name": format!("webhook-channel-{suffix}"),
                "kind": kind
            }),
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
async fn channel_manager_can_create_and_execute_incoming_webhook() {
    let app = test_app();
    let (owner_token, owner_id) = register(&app, "webhook-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "execute", "text").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks"),
            &owner_token,
            json!({ "name": "Deploy Hook" }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let webhook = &body["webhook"];
    let webhook_id = webhook["id"].as_str().unwrap();
    let raw_token = webhook["token"].as_str().unwrap();
    let bot_user_id = webhook["bot_user_id"].as_str().unwrap();
    assert_eq!(webhook["organization_id"], organization_id);
    assert_eq!(webhook["space_id"], space_id);
    assert_eq!(webhook["channel_id"], channel_id);
    assert_eq!(webhook["created_by_user_id"], owner_id);
    assert_eq!(webhook["name"], "Deploy Hook");
    assert_eq!(webhook["status"], "active");
    assert_eq!(
        webhook["token_last_four"],
        &raw_token[raw_token.len() - 4..]
    );
    assert_eq!(
        webhook["execute_url"],
        format!("https://chat.example.com/api/webhooks/{webhook_id}/{raw_token}")
    );
    assert_eq!(
        uuid::Uuid::parse_str(webhook_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(
        uuid::Uuid::parse_str(bot_user_id)
            .unwrap()
            .get_version_num(),
        7
    );
    assert!(raw_token.starts_with("ocw_"));

    let embed = json!({
        "title": "Production deploy",
        "description": "Build 2026.06.23 shipped",
        "color": 5763719
    });
    let executed = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/webhooks/{webhook_id}/{raw_token}"),
            json!({
                "content": "deployment shipped",
                "embeds": [embed.clone()],
                "username": "Deploy Bot",
                "avatar_url": "https://chat.example.com/assets/deploy-bot.png"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(executed.status(), StatusCode::CREATED);
    let body = response_json(executed).await;
    assert_eq!(body["message"]["organization_id"], organization_id);
    assert_eq!(body["message"]["space_id"], space_id);
    assert_eq!(body["message"]["channel_id"], channel_id);
    assert_eq!(body["message"]["author_user_id"], bot_user_id);
    assert_eq!(body["message"]["content"], "deployment shipped");
    assert_eq!(body["message"]["embeds"], json!([embed]));
    assert_eq!(body["message"]["webhook_username"], "Deploy Bot");
    assert_eq!(
        body["message"]["webhook_avatar_url"],
        "https://chat.example.com/assets/deploy-bot.png"
    );
    assert!(
        body["message"]["attachments"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let listed = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/channels/{channel_id}/messages"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][0]["author_user_id"], bot_user_id);
    assert_eq!(body["messages"][0]["embeds"], json!([embed]));
    assert_eq!(body["messages"][0]["webhook_username"], "Deploy Bot");
    assert_eq!(
        body["messages"][0]["webhook_avatar_url"],
        "https://chat.example.com/assets/deploy-bot.png"
    );
}

#[tokio::test]
async fn channel_manager_can_list_rotate_and_delete_incoming_webhook() {
    let app = test_app();
    let (owner_token, _) = register(&app, "webhook-manage-owner@example.com").await;
    let (_, _, channel_id) = create_space_with_channel(&app, &owner_token, "manage", "text").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks"),
            &owner_token,
            json!({ "name": "Release Hook" }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let webhook_id = body["webhook"]["id"].as_str().unwrap().to_owned();
    let first_token = body["webhook"]["token"].as_str().unwrap().to_owned();

    let listed = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/channels/{channel_id}/webhooks"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    let webhooks = body["webhooks"].as_array().unwrap();
    assert_eq!(webhooks.len(), 1);
    assert_eq!(webhooks[0]["id"], webhook_id);
    assert_eq!(webhooks[0]["channel_id"], channel_id);
    assert_eq!(webhooks[0]["name"], "Release Hook");
    assert_eq!(webhooks[0]["status"], "active");
    assert_eq!(
        webhooks[0]["token_last_four"],
        &first_token[first_token.len() - 4..]
    );
    assert!(webhooks[0].get("token").is_none());
    assert!(webhooks[0].get("execute_url").is_none());

    let rotated = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks/{webhook_id}/token/rotate"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(rotated.status(), StatusCode::CREATED);
    let body = response_json(rotated).await;
    let second_token = body["webhook"]["token"].as_str().unwrap().to_owned();
    assert!(second_token.starts_with("ocw_"));
    assert_ne!(second_token, first_token);
    assert_eq!(
        body["webhook"]["token_last_four"],
        &second_token[second_token.len() - 4..]
    );
    assert_eq!(
        body["webhook"]["execute_url"],
        format!("https://chat.example.com/api/webhooks/{webhook_id}/{second_token}")
    );

    let rejected_old_token = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/webhooks/{webhook_id}/{first_token}"),
            json!({ "content": "must not post" }),
        ))
        .await
        .unwrap();
    assert_eq!(rejected_old_token.status(), StatusCode::NOT_FOUND);

    let accepted_new_token = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/webhooks/{webhook_id}/{second_token}"),
            json!({ "content": "second token works" }),
        ))
        .await
        .unwrap();
    assert_eq!(accepted_new_token.status(), StatusCode::CREATED);

    let deleted = app
        .clone()
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/channels/{channel_id}/webhooks/{webhook_id}"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let rejected_deleted = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/webhooks/{webhook_id}/{second_token}"),
            json!({ "content": "must not post" }),
        ))
        .await
        .unwrap();
    assert_eq!(rejected_deleted.status(), StatusCode::NOT_FOUND);

    let listed = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/channels/{channel_id}/webhooks"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    assert!(body["webhooks"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn webhook_management_writes_audit_events_without_raw_tokens() {
    let app = test_app();
    let (owner_token, owner_id) = register(&app, "webhook-audit-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "audit", "text").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks"),
            &owner_token,
            json!({ "name": "Audit Hook" }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let webhook_id = body["webhook"]["id"].as_str().unwrap().to_owned();
    let first_token = body["webhook"]["token"].as_str().unwrap().to_owned();
    let first_last_four = body["webhook"]["token_last_four"]
        .as_str()
        .unwrap()
        .to_owned();

    let rotated = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks/{webhook_id}/token/rotate"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(rotated.status(), StatusCode::CREATED);
    let body = response_json(rotated).await;
    let second_token = body["webhook"]["token"].as_str().unwrap().to_owned();
    let second_last_four = body["webhook"]["token_last_four"]
        .as_str()
        .unwrap()
        .to_owned();

    let deleted = app
        .clone()
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/channels/{channel_id}/webhooks/{webhook_id}"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let audit = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/spaces/{space_id}/audit-events"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(audit.status(), StatusCode::OK);
    let body = response_json(audit).await;
    let events = body["audit_events"].as_array().unwrap();
    assert_eq!(events.len(), 3);
    let actions = events
        .iter()
        .map(|event| event["action"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        actions,
        vec![
            "webhook.created",
            "webhook.token_rotated",
            "webhook.deleted"
        ]
    );

    let created_event = &events[0];
    assert_eq!(created_event["organization_id"], organization_id);
    assert_eq!(created_event["space_id"], space_id);
    assert_eq!(created_event["actor_user_id"], owner_id);
    assert_eq!(created_event["target_type"], "incoming_webhook");
    assert_eq!(created_event["target_id"], webhook_id);
    assert_eq!(created_event["metadata"]["channel_id"], channel_id);
    assert_eq!(created_event["metadata"]["name"], "Audit Hook");
    assert_eq!(
        created_event["metadata"]["token_last_four"],
        first_last_four
    );

    let rotated_event = &events[1];
    assert_eq!(rotated_event["metadata"]["channel_id"], channel_id);
    assert_eq!(
        rotated_event["metadata"]["token_last_four"],
        second_last_four
    );

    let deleted_event = &events[2];
    assert_eq!(deleted_event["metadata"]["channel_id"], channel_id);
    assert_eq!(deleted_event["metadata"]["name"], "Audit Hook");

    let serialized_events = serde_json::to_string(events).unwrap();
    assert!(!serialized_events.contains(&first_token));
    assert!(!serialized_events.contains(&second_token));
}

#[tokio::test]
async fn webhook_create_requires_manage_channels() {
    let app = test_app();
    let (owner_token, _) = register(&app, "webhook-permission-owner@example.com").await;
    let (member_token, member_id) = register(&app, "webhook-permission-member@example.com").await;
    let (_, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "permission", "text").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks"),
            &member_token,
            json!({ "name": "Member Hook" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn webhook_management_requires_manage_channels() {
    let app = test_app();
    let (owner_token, _) = register(&app, "webhook-manage-permission-owner@example.com").await;
    let (member_token, member_id) =
        register(&app, "webhook-manage-permission-member@example.com").await;
    let (_, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "manage-permission", "text").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks"),
            &owner_token,
            json!({ "name": "Owner Hook" }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let webhook_id = body["webhook"]["id"].as_str().unwrap();

    let listed = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/channels/{channel_id}/webhooks"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::FORBIDDEN);

    let rotated = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks/{webhook_id}/token/rotate"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(rotated.status(), StatusCode::FORBIDDEN);

    let deleted = app
        .clone()
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/channels/{channel_id}/webhooks/{webhook_id}"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn webhook_execution_rejects_invalid_token() {
    let app = test_app();
    let (owner_token, _) = register(&app, "webhook-token-owner@example.com").await;
    let (_, _, channel_id) = create_space_with_channel(&app, &owner_token, "token", "text").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks"),
            &owner_token,
            json!({ "name": "Token Hook" }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let webhook_id = body["webhook"]["id"].as_str().unwrap();

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/webhooks/{webhook_id}/ocw_wrongtoken"),
            json!({ "content": "must not post" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn webhook_execution_is_rate_limited_per_webhook_url() {
    let app = test_app();
    let (owner_token, _) = register(&app, "webhook-rate-limit-owner@example.com").await;
    let (_, _, channel_id) =
        create_space_with_channel(&app, &owner_token, "rate-limit", "text").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks"),
            &owner_token,
            json!({ "name": "Rate Limit Hook" }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let webhook_id = body["webhook"]["id"].as_str().unwrap().to_owned();
    let raw_token = body["webhook"]["token"].as_str().unwrap().to_owned();
    let execute_path = format!("/api/webhooks/{webhook_id}/{raw_token}");

    for remaining in (0..5).rev() {
        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                &execute_path,
                json!({ "content": format!("allowed {remaining}") }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.headers()["x-ratelimit-limit"].to_str().unwrap(),
            "5"
        );
        assert_eq!(
            response.headers()["x-ratelimit-remaining"]
                .to_str()
                .unwrap(),
            remaining.to_string()
        );
        assert_eq!(
            response.headers()["x-ratelimit-bucket"].to_str().unwrap(),
            format!("webhook:{webhook_id}")
        );
        assert!(response.headers().contains_key("x-ratelimit-reset"));
    }

    let limited = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &execute_path,
            json!({ "content": "must be limited" }),
        ))
        .await
        .unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        limited.headers()["x-ratelimit-limit"].to_str().unwrap(),
        "5"
    );
    assert_eq!(
        limited.headers()["x-ratelimit-remaining"].to_str().unwrap(),
        "0"
    );
    assert_eq!(
        limited.headers()["x-ratelimit-bucket"].to_str().unwrap(),
        format!("webhook:{webhook_id}")
    );
    assert!(limited.headers().contains_key("x-ratelimit-reset"));
    assert!(limited.headers().contains_key("retry-after"));
    let body = response_json(limited).await;
    assert_eq!(body["error"]["code"], "rate_limited");
}

#[tokio::test]
async fn webhooks_are_limited_to_text_channels() {
    let app = test_app();
    let (owner_token, _) = register(&app, "webhook-voice-owner@example.com").await;
    let (_, _, channel_id) = create_space_with_channel(&app, &owner_token, "voice", "voice").await;

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/webhooks"),
            &owner_token,
            json!({ "name": "Voice Hook" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
