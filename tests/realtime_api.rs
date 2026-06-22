use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use futures_util::{SinkExt, StreamExt};
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
                "display_name": "Realtime Test User",
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

async fn create_channel(app: &Router, token: &str, suffix: &str) -> String {
    let org = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            token,
            json!({ "name": format!("Realtime Org {suffix}") }),
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
            json!({ "name": format!("Realtime Space {suffix}") }),
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
            json!({ "name": format!("realtime-channel-{suffix}") }),
        ))
        .await
        .unwrap();
    assert_eq!(channel.status(), StatusCode::CREATED);
    response_json(channel).await["channel"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn send_message(app: &Router, token: &str, channel_id: &str, content: &str) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            token,
            json!({ "content": content }),
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
async fn websocket_receives_message_created_for_visible_channel() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (token, _) = register(&app, "realtime-owner@example.com").await;
    let channel_id = create_channel(&app, &token, "message").await;
    let (mut socket, _) = connect_async(format!("ws://{addr}/ws?token={token}"))
        .await
        .expect("connect websocket");

    send_message(&app, &token, &channel_id, "hello realtime").await;

    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message.created event");

    assert_eq!(event["type"], "message.created");
    assert_eq!(event["scope"]["channel_id"], channel_id);
    assert_eq!(event["data"]["message"]["content"], "hello realtime");
}

#[tokio::test]
async fn websocket_filters_message_events_by_channel_permission() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "realtime-filter-owner@example.com").await;
    let (outsider_token, _) = register(&app, "realtime-filter-outsider@example.com").await;
    let channel_id = create_channel(&app, &owner_token, "filter").await;
    let (mut outsider_socket, _) = connect_async(format!("ws://{addr}/ws?token={outsider_token}"))
        .await
        .expect("connect outsider websocket");

    send_message(&app, &owner_token, &channel_id, "private realtime").await;

    let event = timeout(Duration::from_millis(300), next_json(&mut outsider_socket)).await;
    assert!(event.is_err(), "outsider should not receive channel event");
}

#[tokio::test]
async fn websocket_broadcasts_typing_started_events() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (token, user_id) = register(&app, "realtime-typing-owner@example.com").await;
    let channel_id = create_channel(&app, &token, "typing").await;
    let (mut socket, _) = connect_async(format!("ws://{addr}/ws?token={token}"))
        .await
        .expect("connect websocket");

    socket
        .send(WsMessage::Text(
            json!({
                "type": "typing.start",
                "channel_id": channel_id
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send typing start");

    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("typing.started event");

    assert_eq!(event["type"], "typing.started");
    assert_eq!(event["scope"]["channel_id"], channel_id);
    assert_eq!(event["data"]["user_id"], user_id);
}
