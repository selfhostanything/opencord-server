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
                "display_name": "Compat Gateway Test User",
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
    app: &Router,
    owner_token: &str,
    suffix: &str,
) -> (String, String, String) {
    let org = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            owner_token,
            json!({ "name": format!("Compat Gateway Org {suffix}") }),
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
            json!({ "name": format!("Compat Gateway Space {suffix}") }),
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
            json!({ "name": format!("compat-gateway-channel-{suffix}") }),
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

async fn create_bot(app: &Router, owner_token: &str, organization_id: &str) -> (String, String) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/bot-applications"),
            owner_token,
            json!({
                "name": "Gateway Bot",
                "description": "Exercises Discord-compatible gateway"
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

async fn send_message(app: &Router, token: &str, channel_id: &str, content: &str) -> String {
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
    response_json(response).await["message"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
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
async fn compat_gateway_identify_ready_heartbeat_and_message_create() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, owner_id) = register(&app, "compat-gateway-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "primary").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

    let (mut socket, _) = connect_async(format!("ws://{addr}/api/compat/discord/gateway"))
        .await
        .expect("connect compatibility gateway");

    let hello = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("gateway hello");
    assert_eq!(hello["op"], 10);
    assert_eq!(hello["d"]["heartbeat_interval"], 45000);

    socket
        .send(WsMessage::Text(
            json!({
                "op": 2,
                "d": {
                    "token": bot_token,
                    "intents": 512,
                    "properties": {
                        "os": "test",
                        "browser": "opencord-test",
                        "device": "opencord-test"
                    }
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send identify");

    let ready = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("ready dispatch");
    assert_eq!(ready["op"], 0);
    assert_eq!(ready["t"], "READY");
    assert_eq!(ready["s"], 1);
    assert_eq!(ready["d"]["user"]["id"], bot_user_id);
    assert_eq!(ready["d"]["user"]["username"], "Gateway Bot");
    assert_eq!(ready["d"]["user"]["bot"], true);
    assert!(ready["d"]["session_id"].as_str().is_some());

    socket
        .send(WsMessage::Text(
            json!({ "op": 1, "d": ready["s"] }).to_string().into(),
        ))
        .await
        .expect("send heartbeat");
    let heartbeat_ack = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("heartbeat ack");
    assert_eq!(heartbeat_ack["op"], 11);

    let message_id = send_message(&app, &owner_token, &channel_id, "gateway hello").await;
    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message create dispatch");

    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "MESSAGE_CREATE");
    assert_eq!(event["s"], 2);
    assert_eq!(event["d"]["id"], message_id);
    assert_eq!(event["d"]["channel_id"], channel_id);
    assert_eq!(event["d"]["author"]["id"], owner_id);
    assert_eq!(event["d"]["author"]["bot"], false);
    assert_eq!(event["d"]["content"], "gateway hello");
}

#[tokio::test]
async fn compat_gateway_rejects_invalid_identify_token() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (mut socket, _) = connect_async(format!("ws://{addr}/api/compat/discord/gateway"))
        .await
        .expect("connect compatibility gateway");

    let hello = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("gateway hello");
    assert_eq!(hello["op"], 10);

    socket
        .send(WsMessage::Text(
            json!({
                "op": 2,
                "d": {
                    "token": "ocb_invalid",
                    "intents": 512,
                    "properties": {}
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send invalid identify");

    let invalid_session = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("invalid session event");
    assert_eq!(invalid_session["op"], 9);
    assert_eq!(invalid_session["d"], false);
}
