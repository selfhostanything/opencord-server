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
                "display_name": "Media Test User",
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
            json!({ "name": format!("Media Org {suffix}") }),
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
            json!({ "name": format!("Media Space {suffix}") }),
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
            json!({ "name": format!("media-channel-{suffix}") }),
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

async fn create_role(
    app: &axum::Router,
    owner_token: &str,
    space_id: &str,
    permissions: Value,
) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/roles"),
            owner_token,
            json!({
                "name": "Voice Members",
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

#[tokio::test]
async fn media_control_issues_room_scoped_livekit_token_for_authorized_member() {
    let app = test_app();
    let (owner_token, _) = register(&app, "media-owner@example.com").await;
    let (member_token, member_id) = register(&app, "media-member@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "scoped-token").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;
    let role_id = create_role(
        &app,
        &owner_token,
        &space_id,
        json!(["CONNECT_VOICE", "SPEAK"]),
    )
    .await;
    assign_role(&app, &owner_token, &space_id, &role_id, &member_id).await;

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/media/rooms/token",
            &member_token,
            json!({
                "room_type": "voice_channel",
                "organization_id": organization_id,
                "space_id": space_id,
                "channel_id": channel_id,
                "can_publish_audio": true,
                "can_publish_video": false,
                "can_publish_screen": false,
                "can_subscribe": true
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    let media = &body["media"];
    assert_eq!(media["provider"], "livekit");
    assert_eq!(media["server_url"], "ws://localhost:7880");
    assert_eq!(media["region"], "local");
    assert_eq!(media["room_type"], "voice_channel");
    assert_eq!(media["organization_id"], organization_id);
    assert_eq!(media["space_id"], space_id);
    assert_eq!(media["channel_id"], channel_id);
    assert_eq!(media["participant_identity"], member_id);
    assert_eq!(media["grants"]["can_publish_audio"], true);
    assert_eq!(media["grants"]["can_publish_video"], false);
    assert_eq!(media["grants"]["can_publish_screen"], false);
    assert_eq!(media["grants"]["can_subscribe"], true);

    let payload = decode_jwt_payload(media["participant_token"].as_str().unwrap());
    assert_eq!(payload["iss"], "devkey");
    assert_eq!(payload["sub"], member_id);
    assert_eq!(payload["video"]["room"], media["room_name"]);
    assert_eq!(payload["video"]["roomJoin"], true);
    assert_eq!(payload["video"]["canPublish"], true);
    assert_eq!(payload["video"]["canSubscribe"], true);
    assert_eq!(payload["video"]["canPublishData"], true);
    assert_eq!(payload["video"]["canPublishSources"], json!(["microphone"]));
    assert!(payload["exp"].as_i64().unwrap() > payload["nbf"].as_i64().unwrap());
    assert_eq!(
        payload["attributes"]["opencord.organization_id"],
        organization_id
    );
    assert_eq!(payload["attributes"]["opencord.space_id"], space_id);
    assert_eq!(payload["attributes"]["opencord.channel_id"], channel_id);
    assert_eq!(payload["attributes"]["opencord.room_type"], "voice_channel");
}

#[tokio::test]
async fn media_control_requires_bearer_auth() {
    let response = test_app()
        .oneshot(json_request(
            Method::POST,
            "/media/rooms/token",
            json!({
                "room_type": "voice_channel",
                "organization_id": uuid::Uuid::now_v7().to_string(),
                "space_id": uuid::Uuid::now_v7().to_string(),
                "channel_id": uuid::Uuid::now_v7().to_string()
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn media_control_denies_member_without_voice_permissions() {
    let app = test_app();
    let (owner_token, _) = register(&app, "media-owner-denied@example.com").await;
    let (member_token, member_id) = register(&app, "media-member-denied@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "denied").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/media/rooms/token",
            &member_token,
            json!({
                "room_type": "voice_channel",
                "organization_id": organization_id,
                "space_id": space_id,
                "channel_id": channel_id,
                "can_publish_audio": true,
                "can_publish_video": false,
                "can_publish_screen": false,
                "can_subscribe": true
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

fn decode_jwt_payload(token: &str) -> Value {
    let payload = token
        .split('.')
        .nth(1)
        .expect("token should have a payload segment");
    let bytes = decode_base64_url(payload);
    serde_json::from_slice(&bytes).expect("JWT payload should be JSON")
}

fn decode_base64_url(input: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;

    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            b'=' => continue,
            _ => panic!("invalid base64url byte"),
        } as u32;

        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
    }

    output
}
