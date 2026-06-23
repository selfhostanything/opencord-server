use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use futures_util::StreamExt;
use opencord_server::config::AppConfig;
use opencord_server::routes::api_router_with_state;
use opencord_server::state::AppState;
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use tower::ServiceExt;

type TestWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn test_app() -> Router {
    api_router_with_state(AppState::in_memory(AppConfig {
        version: "test-version".to_owned(),
        public_url: "https://chat.example.com".to_owned(),
    }))
}

async fn serve_app(app: Router) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test websocket listener");
    let addr = listener.local_addr().expect("read local addr");

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve test websocket app");
    });

    addr
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    serde_json::from_slice(&bytes).expect("response should be json")
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    String::from_utf8(bytes.to_vec()).expect("response should be utf-8")
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

async fn register(app: &Router, email: &str) -> (String, String) {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Voice Test User",
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

async fn create_space_and_channel(
    app: &Router,
    token: &str,
    suffix: &str,
    kind: &str,
) -> (String, String, String) {
    let org = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            token,
            json!({ "name": format!("Voice Org {suffix}") }),
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
            json!({ "name": format!("Voice Space {suffix}") }),
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
            json!({
                "kind": kind,
                "name": format!("voice-channel-{suffix}")
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

async fn add_space_member(app: &Router, owner_token: &str, space_id: &str, user_id: &str) {
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

async fn next_json(socket: &mut TestWebSocket) -> Value {
    while let Some(message) = socket.next().await {
        let message = message.expect("websocket message");
        if let WsMessage::Text(text) = message {
            return serde_json::from_str(&text).expect("websocket json event");
        }
    }

    panic!("websocket closed before event")
}

#[tokio::test]
async fn owner_joins_voice_channel_and_gateway_emits_participant_joined() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (token, user_id) = register(&app, "voice-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_and_channel(&app, &token, "join", "voice").await;
    let (mut socket, _) = connect_async(format!("ws://{addr}/ws?token={token}"))
        .await
        .expect("connect websocket");

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/voice/channels/{channel_id}/join"),
            &token,
            json!({
                "self_mute": false,
                "self_deaf": false
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    assert_eq!(body["voice"]["channel_id"], channel_id);
    assert_eq!(body["voice"]["user_id"], user_id);
    assert_eq!(body["voice"]["self_mute"], false);
    assert_eq!(body["voice"]["self_deaf"], false);
    assert_eq!(body["media"]["room_type"], "voice_channel");
    assert_eq!(body["media"]["organization_id"], organization_id);
    assert_eq!(body["media"]["space_id"], space_id);
    assert_eq!(body["media"]["channel_id"], channel_id);
    assert_eq!(body["media"]["participant_identity"], user_id);
    assert_eq!(body["media"]["grants"]["can_publish_audio"], true);
    assert_eq!(body["media"]["grants"]["can_subscribe"], true);

    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("voice.participant_joined event");

    assert_eq!(event["type"], "voice.participant_joined");
    assert_eq!(event["organization_id"], organization_id);
    assert_eq!(event["scope"]["space_id"], space_id);
    assert_eq!(event["scope"]["channel_id"], channel_id);
    assert_eq!(event["data"]["participant"]["user_id"], user_id);
    assert_eq!(event["data"]["participant"]["channel_id"], channel_id);
    assert_eq!(event["data"]["participant"]["self_mute"], false);
    assert_eq!(event["data"]["participant"]["self_deaf"], false);
    assert_eq!(
        event["data"]["media"]["room_name"],
        body["media"]["room_name"]
    );
    assert!(event["data"]["media"]["participant_token"].is_null());
}

#[tokio::test]
async fn join_voice_channel_requires_voice_permissions() {
    let app = test_app();
    let (owner_token, _) = register(&app, "voice-permission-owner@example.com").await;
    let (member_token, member_id) = register(&app, "voice-permission-member@example.com").await;
    let (_, space_id, channel_id) =
        create_space_and_channel(&app, &owner_token, "permission", "voice").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/voice/channels/{channel_id}/join"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn join_voice_channel_rejects_text_channels() {
    let app = test_app();
    let (token, _) = register(&app, "voice-text-owner@example.com").await;
    let (_, _, channel_id) = create_space_and_channel(&app, &token, "text", "text").await;

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/voice/channels/{channel_id}/join"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(response).await["error"]["message"],
        "channel must be a voice channel"
    );
}

#[tokio::test]
async fn voice_join_updates_media_metrics() {
    let app = test_app();
    let (owner_token, _) = register(&app, "voice-metrics-owner@example.com").await;
    let (member_token, member_id) = register(&app, "voice-metrics-member@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_and_channel(&app, &owner_token, "metrics", "voice").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let joined = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/voice/channels/{channel_id}/join"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(joined.status(), StatusCode::CREATED);

    let rejected = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/voice/channels/{channel_id}/join"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

    let metrics = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(metrics.status(), StatusCode::OK);
    let body = response_text(metrics).await;
    assert!(body.contains("opencord_media_voice_join_success_total 1"));
    assert!(
        body.contains(r#"opencord_media_voice_join_failures_total{reason="permission_denied"} 1"#)
    );
    assert!(body.contains(&format!(
        r#"opencord_media_voice_participants_current{{organization_id="{organization_id}",space_id="{space_id}",channel_id="{channel_id}"}} 1"#
    )));
}
