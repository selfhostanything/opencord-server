#![allow(dead_code)]

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
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

pub type TestWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug)]
pub struct TestUser {
    pub token: String,
    pub user_id: String,
}

#[derive(Debug)]
pub struct TestSpace {
    pub organization_id: String,
    pub space_id: String,
    pub channel_id: String,
}

#[derive(Debug)]
pub struct TestBotApplication {
    pub application_id: String,
    pub bot_token: String,
    pub bot_user_id: String,
}

#[derive(Clone)]
pub struct CompatHarness {
    app: Router,
}

impl CompatHarness {
    pub fn new() -> Self {
        Self {
            app: api_router_with_state(AppState::in_memory(AppConfig {
                version: "test-version".to_owned(),
                public_url: "https://chat.example.com".to_owned(),
            })),
        }
    }

    pub async fn json(
        &self,
        method: Method,
        uri: &str,
        body: Value,
    ) -> (StatusCode, Option<Value>) {
        self.send(json_request(method, uri, body)).await
    }

    pub async fn bearer_json(
        &self,
        method: Method,
        uri: &str,
        token: &str,
        body: Value,
    ) -> (StatusCode, Option<Value>) {
        self.send(bearer_request(method, uri, token, body)).await
    }

    pub async fn bot_json(
        &self,
        method: Method,
        uri: &str,
        token: &str,
        body: Value,
    ) -> (StatusCode, Option<Value>) {
        self.send(bot_request(method, uri, token, body)).await
    }

    pub async fn bot_json_with_headers(
        &self,
        method: Method,
        uri: &str,
        token: &str,
        body: Value,
    ) -> (StatusCode, HeaderMap, Option<Value>) {
        self.send_with_headers(bot_request(method, uri, token, body))
            .await
    }

    pub async fn register(&self, email: &str) -> TestUser {
        let (status, body) = self
            .json(
                Method::POST,
                "/auth/register",
                json!({
                    "email": email,
                    "display_name": "Compat Contract User",
                    "password": "correct horse battery staple"
                }),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED);
        let body = body.expect("register response");
        TestUser {
            token: body["session"]["token"].as_str().unwrap().to_owned(),
            user_id: body["user"]["id"].as_str().unwrap().to_owned(),
        }
    }

    pub async fn create_space_with_channel(&self, owner_token: &str, suffix: &str) -> TestSpace {
        let (org_status, org_body) = self
            .bearer_json(
                Method::POST,
                "/organizations",
                owner_token,
                json!({ "name": format!("Compat Contract Org {suffix}") }),
            )
            .await;
        assert_eq!(org_status, StatusCode::CREATED);
        let organization_id = org_body.expect("organization response")["organization"]["id"]
            .as_str()
            .unwrap()
            .to_owned();

        let (space_status, space_body) = self
            .bearer_json(
                Method::POST,
                &format!("/organizations/{organization_id}/spaces"),
                owner_token,
                json!({ "name": format!("Compat Contract Space {suffix}") }),
            )
            .await;
        assert_eq!(space_status, StatusCode::CREATED);
        let space_id = space_body.expect("space response")["space"]["id"]
            .as_str()
            .unwrap()
            .to_owned();

        let (channel_status, channel_body) = self
            .bearer_json(
                Method::POST,
                &format!("/spaces/{space_id}/channels"),
                owner_token,
                json!({ "name": format!("compat-contract-channel-{suffix}") }),
            )
            .await;
        assert_eq!(channel_status, StatusCode::CREATED);
        let channel_id = channel_body.expect("channel response")["channel"]["id"]
            .as_str()
            .unwrap()
            .to_owned();

        TestSpace {
            organization_id,
            space_id,
            channel_id,
        }
    }

    pub async fn create_bot_application(
        &self,
        owner_token: &str,
        organization_id: &str,
        name: &str,
    ) -> TestBotApplication {
        let (status, body) = self
            .bearer_json(
                Method::POST,
                &format!("/organizations/{organization_id}/bot-applications"),
                owner_token,
                json!({
                    "name": name,
                    "description": "Exercises Discord-compatible contract tests"
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        let body = body.expect("bot application response");

        TestBotApplication {
            application_id: body["bot_application"]["id"].as_str().unwrap().to_owned(),
            bot_token: body["bot_token"]["token"].as_str().unwrap().to_owned(),
            bot_user_id: body["bot_application"]["bot_user_id"]
                .as_str()
                .unwrap()
                .to_owned(),
        }
    }

    pub async fn add_space_member(
        &self,
        owner_token: &str,
        space_id: &str,
        user_id: &str,
        role: &str,
    ) {
        let (status, _) = self
            .bearer_json(
                Method::POST,
                &format!("/spaces/{space_id}/members"),
                owner_token,
                json!({
                    "user_id": user_id,
                    "role": role
                }),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED);
    }

    pub async fn connect_compat_gateway(&self) -> TestWebSocket {
        let addr = self.serve().await;
        let (socket, _) = connect_async(format!("ws://{addr}/api/compat/discord/gateway"))
            .await
            .expect("connect compatibility gateway");
        socket
    }

    pub async fn next_gateway_json(&self, socket: &mut TestWebSocket) -> Value {
        while let Some(message) = timeout(Duration::from_secs(2), socket.next())
            .await
            .expect("gateway event timeout")
        {
            let message = message.expect("websocket message");
            if let WsMessage::Text(text) = message {
                return serde_json::from_str(&text).expect("websocket json event");
            }
        }

        panic!("websocket closed before event")
    }

    pub async fn identify_compat_gateway(
        &self,
        socket: &mut TestWebSocket,
        bot_token: &str,
    ) -> Value {
        socket
            .send(WsMessage::Text(
                json!({
                    "op": 2,
                    "d": {
                        "token": bot_token,
                        "intents": 512,
                        "properties": {
                            "os": "test",
                            "browser": "opencord-contract",
                            "device": "opencord-contract"
                        }
                    }
                })
                .to_string()
                .into(),
            ))
            .await
            .expect("send gateway identify");

        self.next_gateway_json(socket).await
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, Option<Value>) {
        let (status, _, body) = self.send_with_headers(request).await;
        (status, body)
    }

    async fn send_with_headers(
        &self,
        request: Request<Body>,
    ) -> (StatusCode, HeaderMap, Option<Value>) {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");

        if bytes.is_empty() {
            return (status, headers, None);
        }

        (
            status,
            headers,
            Some(serde_json::from_slice(&bytes).expect("response should be json")),
        )
    }

    async fn serve(&self) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test websocket listener");
        let addr = listener.local_addr().expect("read local addr");
        let app = self.app.clone();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve test websocket app");
        });

        addr
    }
}

pub fn assert_uuid_v7_string(value: &str) {
    assert_eq!(uuid::Uuid::parse_str(value).unwrap().get_version_num(), 7);
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

fn bot_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bot {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}
