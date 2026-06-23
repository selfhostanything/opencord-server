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

fn bot_multipart_request(
    method: Method,
    uri: &str,
    token: &str,
    boundary: &str,
    body: Vec<u8>,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header(header::AUTHORIZATION, format!("Bot {token}"))
        .body(Body::from(body))
        .unwrap()
}

fn multipart_message_body(
    boundary: &str,
    payload: Value,
    filename: &str,
    content_type: &str,
    file_bytes: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n{}\r\n",
            payload
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
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

async fn create_bot_with_application(
    app: &Router,
    owner_token: &str,
    organization_id: &str,
) -> (String, String, String) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/bot-applications"),
            owner_token,
            json!({
                "name": "Gateway Command Bot",
                "description": "Exercises Discord-compatible interaction gateway"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    (
        body["bot_application"]["id"].as_str().unwrap().to_owned(),
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

async fn invite_bot_to_space(
    app: &Router,
    owner_token: &str,
    organization_id: &str,
    application_id: &str,
    space_id: &str,
) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!(
                "/organizations/{organization_id}/bot-applications/{application_id}/spaces/{space_id}/invite"
            ),
            owner_token,
            json!({ "role": "member" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn create_role(app: &Router, owner_token: &str, space_id: &str, name: &str) -> String {
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

async fn create_uploaded_attachment(app: &Router, token: &str, channel_id: &str) -> String {
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

async fn send_message_with_attachment(
    app: &Router,
    token: &str,
    channel_id: &str,
    attachment_id: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            token,
            json!({
                "content": "diagram attached",
                "attachment_ids": [attachment_id]
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["message"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn send_compat_embed_message(
    app: &Router,
    bot_token: &str,
    channel_id: &str,
    embed: Value,
) -> String {
    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            bot_token,
            json!({
                "content": "",
                "embeds": [embed],
                "allowed_mentions": {
                    "parse": []
                }
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn send_compat_text_message(app: &Router, bot_token: &str, channel_id: &str) -> String {
    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            bot_token,
            json!({
                "content": "compat gateway text",
                "tts": false
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn send_compat_mention_message(
    app: &Router,
    bot_token: &str,
    channel_id: &str,
    mentioned_user_id: &str,
    suppressed_user_id: &str,
    role_id: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            bot_token,
            json!({
                "content": format!("ping <@{mentioned_user_id}> <@{suppressed_user_id}> <@&{role_id}> @everyone"),
                "allowed_mentions": {
                    "parse": ["everyone"],
                    "users": [mentioned_user_id],
                    "roles": [role_id]
                }
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn send_compat_component_message(
    app: &Router,
    bot_token: &str,
    channel_id: &str,
    component: Value,
) -> String {
    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            bot_token,
            json!({
                "content": "",
                "components": [component],
                "allowed_mentions": {
                    "parse": []
                }
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn send_compat_multipart_message(
    app: &Router,
    bot_token: &str,
    channel_id: &str,
) -> (String, String) {
    let boundary = "opencord-compat-gateway-upload-boundary";
    let response = app
        .clone()
        .oneshot(bot_multipart_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            bot_token,
            boundary,
            multipart_message_body(
                boundary,
                json!({
                    "content": "",
                    "allowed_mentions": {
                        "parse": []
                    }
                }),
                "gateway-report.txt",
                "text/plain",
                b"gateway report",
            ),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    (
        body["id"].as_str().unwrap().to_owned(),
        body["attachments"][0]["id"].as_str().unwrap().to_owned(),
    )
}

async fn send_compat_reply_message(
    app: &Router,
    bot_token: &str,
    channel_id: &str,
    base_message_id: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            bot_token,
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

    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create_command(
    app: &Router,
    bot_token: &str,
    application_id: &str,
    space_id: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/applications/{application_id}/guilds/{space_id}/commands"
            ),
            bot_token,
            json!({
                "name": "deploy",
                "description": "Deploy a release",
                "type": 1,
                "options": [
                    {
                        "type": 3,
                        "name": "version",
                        "description": "Release version",
                        "required": true
                    }
                ]
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create_interaction(
    app: &Router,
    token: &str,
    channel_id: &str,
    command_id: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/command-interactions"),
            token,
            json!({
                "command_id": command_id,
                "options": [
                    {
                        "name": "version",
                        "value": "1.2.3"
                    }
                ]
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["interaction"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create_component_interaction(
    app: &Router,
    token: &str,
    channel_id: &str,
    message_id: &str,
    custom_id: &str,
) -> (String, String) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/component-interactions"),
            token,
            json!({
                "message_id": message_id,
                "custom_id": custom_id
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    let interaction = &body["interaction"];
    assert_eq!(interaction["type"], 3);
    assert_eq!(interaction["message_id"], message_id);
    assert_eq!(interaction["custom_id"], custom_id);
    assert_eq!(interaction["component_type"], 2);

    (
        interaction["id"].as_str().unwrap().to_owned(),
        interaction["token"].as_str().unwrap().to_owned(),
    )
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
async fn compat_gateway_message_create_includes_components() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-component-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "component").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");

    let action_row = json!({
        "type": 1,
        "components": [
            {
                "type": 2,
                "style": 1,
                "label": "Deploy",
                "custom_id": "deploy:prod"
            }
        ]
    });
    let message_id =
        send_compat_component_message(&app, &bot_token, &channel_id, action_row.clone()).await;
    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message create dispatch");

    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "MESSAGE_CREATE");
    assert_eq!(event["d"]["id"], message_id);
    assert_eq!(event["d"]["author"]["id"], bot_user_id);
    assert_eq!(event["d"]["author"]["bot"], true);
    assert_eq!(event["d"]["content"], "");
    assert_eq!(event["d"]["components"], json!([action_row]));
}

#[tokio::test]
async fn compat_gateway_message_create_includes_allowed_mentions() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-mention-owner@example.com").await;
    let (_, mentioned_user_id) = register(&app, "compat-gateway-mentioned-user@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "mention").await;
    let role_id = create_role(&app, &owner_token, &space_id, "Release Watchers").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &mentioned_user_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");

    let message_id = send_compat_mention_message(
        &app,
        &bot_token,
        &channel_id,
        &mentioned_user_id,
        &bot_user_id,
        &role_id,
    )
    .await;
    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message create dispatch");

    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "MESSAGE_CREATE");
    assert_eq!(event["d"]["id"], message_id);
    assert_eq!(event["d"]["mention_everyone"], true);
    assert_eq!(event["d"]["mentions"].as_array().unwrap().len(), 1);
    assert_eq!(event["d"]["mentions"][0]["id"], mentioned_user_id);
    assert_eq!(
        event["d"]["mentions"][0]["username"],
        "Compat Gateway Test User"
    );
    assert_eq!(event["d"]["mentions"][0]["bot"], false);
    assert_eq!(event["d"]["mention_roles"], json!([role_id]));
}

#[tokio::test]
async fn compat_gateway_message_create_includes_reply_reference() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, owner_id) = register(&app, "compat-gateway-reply-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "reply").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");

    let base_message_id = send_message(&app, &owner_token, &channel_id, "base message").await;
    let base_event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("base message create dispatch");
    assert_eq!(base_event["t"], "MESSAGE_CREATE");

    let reply_message_id =
        send_compat_reply_message(&app, &bot_token, &channel_id, &base_message_id).await;
    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("reply message create dispatch");

    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "MESSAGE_CREATE");
    assert_eq!(event["d"]["id"], reply_message_id);
    assert_eq!(event["d"]["author"]["id"], bot_user_id);
    assert_eq!(event["d"]["author"]["bot"], true);
    assert_eq!(event["d"]["content"], "reply message");
    assert_eq!(
        event["d"]["message_reference"]["message_id"],
        base_message_id
    );
    assert_eq!(event["d"]["message_reference"]["channel_id"], channel_id);
    assert_eq!(event["d"]["referenced_message"]["id"], base_message_id);
    assert_eq!(event["d"]["referenced_message"]["channel_id"], channel_id);
    assert_eq!(event["d"]["referenced_message"]["author"]["id"], owner_id);
    assert_eq!(event["d"]["referenced_message"]["author"]["bot"], false);
    assert_eq!(event["d"]["referenced_message"]["content"], "base message");
    assert_eq!(event["d"]["referenced_message"]["type"], 0);
}

#[tokio::test]
async fn compat_gateway_message_create_includes_native_attachments() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, owner_id) =
        register(&app, "compat-gateway-attachment-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "attachment").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");

    let attachment_id = create_uploaded_attachment(&app, &owner_token, &channel_id).await;
    let message_id =
        send_message_with_attachment(&app, &owner_token, &channel_id, &attachment_id).await;
    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message create dispatch");

    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "MESSAGE_CREATE");
    assert_eq!(event["d"]["id"], message_id);
    assert_eq!(event["d"]["author"]["id"], owner_id);
    assert_eq!(event["d"]["author"]["bot"], false);
    assert_eq!(event["d"]["content"], "diagram attached");
    assert_eq!(event["d"]["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(event["d"]["attachments"][0]["id"], attachment_id);
    assert_eq!(event["d"]["attachments"][0]["filename"], "diagram.png");
    assert_eq!(event["d"]["attachments"][0]["content_type"], "image/png");
    assert_eq!(event["d"]["attachments"][0]["size"], 11);
    assert_eq!(
        event["d"]["attachments"][0]["url"],
        format!("https://chat.example.com/attachments/{attachment_id}/content")
    );
    assert_eq!(
        event["d"]["attachments"][0]["proxy_url"],
        format!("https://chat.example.com/attachments/{attachment_id}/content")
    );
}

#[tokio::test]
async fn compat_gateway_message_create_includes_multipart_attachments() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-multipart-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "multipart").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");

    let (message_id, attachment_id) =
        send_compat_multipart_message(&app, &bot_token, &channel_id).await;
    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message create dispatch");

    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "MESSAGE_CREATE");
    assert_eq!(event["d"]["id"], message_id);
    assert_eq!(event["d"]["author"]["id"], bot_user_id);
    assert_eq!(event["d"]["author"]["bot"], true);
    assert_eq!(event["d"]["content"], "");
    assert_eq!(event["d"]["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(event["d"]["attachments"][0]["id"], attachment_id);
    assert_eq!(
        event["d"]["attachments"][0]["filename"],
        "gateway-report.txt"
    );
    assert_eq!(event["d"]["attachments"][0]["content_type"], "text/plain");
    assert_eq!(event["d"]["attachments"][0]["size"], 14);
    assert_eq!(
        event["d"]["attachments"][0]["url"],
        format!("https://chat.example.com/attachments/{attachment_id}/content")
    );
}

#[tokio::test]
async fn compat_gateway_message_create_includes_basic_embeds() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-embed-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "embed").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");

    let embed = json!({
        "title": "Deploy ready",
        "description": "Release 1.2.3 passed checks",
        "color": 5793266
    });
    let message_id = send_compat_embed_message(&app, &bot_token, &channel_id, embed.clone()).await;
    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message create dispatch");

    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "MESSAGE_CREATE");
    assert_eq!(event["d"]["id"], message_id);
    assert_eq!(event["d"]["author"]["id"], bot_user_id);
    assert_eq!(event["d"]["author"]["bot"], true);
    assert_eq!(event["d"]["content"], "");
    assert_eq!(event["d"]["embeds"], json!([embed]));
}

#[tokio::test]
async fn compat_gateway_dispatches_guild_create_when_bot_invited_to_space() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-guild-create-owner@example.com").await;
    let (organization_id, space_id, _) =
        create_space_with_channel(&app, &owner_token, "guild-create").await;
    let (application_id, bot_token, _) =
        create_bot_with_application(&app, &owner_token, &organization_id).await;

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
                    "token": bot_token,
                    "intents": 1,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");
    assert_eq!(ready["d"]["guilds"].as_array().map(Vec::len), Some(0));

    invite_bot_to_space(
        &app,
        &owner_token,
        &organization_id,
        &application_id,
        &space_id,
    )
    .await;

    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("guild create dispatch");
    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "GUILD_CREATE");
    assert_eq!(event["s"], 2);
    assert_eq!(event["d"]["id"], space_id);
    assert_eq!(event["d"]["name"], "Compat Gateway Space guild-create");
    assert_eq!(event["d"]["unavailable"], false);
}

#[tokio::test]
async fn compat_gateway_ready_includes_visible_guilds() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-ready-guilds-owner@example.com").await;
    let (organization_id, space_id, _) =
        create_space_with_channel(&app, &owner_token, "ready-guilds").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["d"]["guilds"].as_array().map(Vec::len), Some(1));
    assert_eq!(ready["d"]["guilds"][0]["id"], space_id);
    assert_eq!(
        ready["d"]["guilds"][0]["name"],
        "Compat Gateway Space ready-guilds"
    );
    assert_eq!(ready["d"]["guilds"][0]["unavailable"], false);
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
async fn compat_gateway_dispatches_message_delete_for_deleted_compat_message() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-delete-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "delete").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");
    assert_eq!(ready["s"], 1);

    let message_id = send_compat_text_message(&app, &bot_token, &channel_id).await;
    let create_event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message create dispatch");
    assert_eq!(create_event["t"], "MESSAGE_CREATE");
    assert_eq!(create_event["s"], 2);
    assert_eq!(create_event["d"]["id"], message_id);

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

    let delete_event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message delete dispatch");
    assert_eq!(delete_event["op"], 0);
    assert_eq!(delete_event["t"], "MESSAGE_DELETE");
    assert_eq!(delete_event["s"], 3);
    assert_eq!(delete_event["d"]["id"], message_id);
    assert_eq!(delete_event["d"]["channel_id"], channel_id);
    assert_eq!(delete_event["d"]["guild_id"], space_id);
}

#[tokio::test]
async fn compat_gateway_dispatches_message_update_for_edited_compat_message() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-update-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "update").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");
    assert_eq!(ready["s"], 1);

    let message_id = send_compat_text_message(&app, &bot_token, &channel_id).await;
    let create_event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message create dispatch");
    assert_eq!(create_event["t"], "MESSAGE_CREATE");
    assert_eq!(create_event["s"], 2);
    assert_eq!(create_event["d"]["id"], message_id);

    let edited = app
        .clone()
        .oneshot(bot_request(
            Method::PATCH,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages/{message_id}"),
            &bot_token,
            json!({
                "content": "compat gateway text edited",
                "allowed_mentions": {
                    "parse": []
                }
            }),
        ))
        .await
        .unwrap();
    assert_eq!(edited.status(), StatusCode::OK);
    assert_eq!(
        response_json(edited).await["content"],
        "compat gateway text edited"
    );

    let update_event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("message update dispatch");
    assert_eq!(update_event["op"], 0);
    assert_eq!(update_event["t"], "MESSAGE_UPDATE");
    assert_eq!(update_event["s"], 3);
    assert_eq!(update_event["d"]["id"], message_id);
    assert_eq!(update_event["d"]["channel_id"], channel_id);
    assert_eq!(update_event["d"]["author"]["id"], bot_user_id);
    assert_eq!(update_event["d"]["author"]["bot"], true);
    assert_eq!(update_event["d"]["content"], "compat gateway text edited");
    assert!(update_event["d"]["edited_timestamp"].as_str().is_some());
}

#[tokio::test]
async fn compat_gateway_dispatches_channel_create_and_update() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-channel-owner@example.com").await;
    let (organization_id, space_id, _) =
        create_space_with_channel(&app, &owner_token, "channel-events").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");
    assert_eq!(ready["s"], 1);

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            &owner_token,
            json!({
                "name": "Live Events",
                "topic": "channel event stream"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await["channel"].clone();
    let channel_id = created["id"].as_str().unwrap();

    let create_event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("channel create dispatch");
    assert_eq!(create_event["op"], 0);
    assert_eq!(create_event["t"], "CHANNEL_CREATE");
    assert_eq!(create_event["s"], 2);
    assert_eq!(create_event["d"]["id"], channel_id);
    assert_eq!(create_event["d"]["guild_id"], space_id);
    assert_eq!(create_event["d"]["name"], "Live Events");
    assert_eq!(create_event["d"]["type"], 0);
    assert_eq!(create_event["d"]["position"], 0);
    assert_eq!(create_event["d"]["topic"], "channel event stream");
    assert_eq!(create_event["d"]["nsfw"], false);

    let updated = app
        .clone()
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/channels/{channel_id}"),
            &owner_token,
            json!({
                "name": "Live Events Edited",
                "topic": "renamed channel event stream",
                "position": 7
            }),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);

    let update_event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("channel update dispatch");
    assert_eq!(update_event["op"], 0);
    assert_eq!(update_event["t"], "CHANNEL_UPDATE");
    assert_eq!(update_event["s"], 3);
    assert_eq!(update_event["d"]["id"], channel_id);
    assert_eq!(update_event["d"]["guild_id"], space_id);
    assert_eq!(update_event["d"]["name"], "Live Events Edited");
    assert_eq!(update_event["d"]["type"], 0);
    assert_eq!(update_event["d"]["position"], 7);
    assert_eq!(update_event["d"]["topic"], "renamed channel event stream");
}

#[tokio::test]
async fn compat_gateway_dispatches_guild_member_add() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-member-owner@example.com").await;
    let (_, member_user_id) = register(&app, "compat-gateway-member-added@example.com").await;
    let (organization_id, space_id, _) =
        create_space_with_channel(&app, &owner_token, "member-events").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");
    assert_eq!(ready["s"], 1);

    add_space_member(&app, &owner_token, &space_id, &member_user_id).await;

    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("guild member add dispatch");
    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "GUILD_MEMBER_ADD");
    assert_eq!(event["s"], 2);
    assert_eq!(event["d"]["guild_id"], space_id);
    assert_eq!(event["d"]["user"]["id"], member_user_id);
    assert_eq!(event["d"]["user"]["username"], "Compat Gateway Test User");
    assert_eq!(event["d"]["user"]["bot"], false);
    assert!(event["d"]["roles"].as_array().unwrap().is_empty());
    assert_eq!(event["d"]["deaf"], false);
    assert_eq!(event["d"]["mute"], false);
    assert_eq!(event["d"]["pending"], false);
    assert!(event["d"]["joined_at"].as_str().is_some());
}

#[tokio::test]
async fn compat_gateway_resumes_existing_session_and_sequence() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, owner_id) = register(&app, "compat-gateway-resume-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "resume").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                    "token": bot_token,
                    "intents": 512,
                    "properties": {}
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
    assert_eq!(ready["t"], "READY");
    assert_eq!(ready["s"], 1);
    let session_id = ready["d"]["session_id"].as_str().unwrap().to_owned();

    let first_message_id = send_message(&app, &owner_token, &channel_id, "before resume").await;
    let first_event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("first message create dispatch");
    assert_eq!(first_event["t"], "MESSAGE_CREATE");
    assert_eq!(first_event["s"], 2);
    assert_eq!(first_event["d"]["id"], first_message_id);

    socket
        .close(None)
        .await
        .expect("close first gateway socket");

    let (mut resumed_socket, _) = connect_async(format!("ws://{addr}/api/compat/discord/gateway"))
        .await
        .expect("connect resumed compatibility gateway");
    let hello = timeout(Duration::from_secs(2), next_json(&mut resumed_socket))
        .await
        .expect("resumed gateway hello");
    assert_eq!(hello["op"], 10);

    resumed_socket
        .send(WsMessage::Text(
            json!({
                "op": 6,
                "d": {
                    "token": bot_token,
                    "session_id": session_id,
                    "seq": 2
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send resume");
    let resumed = timeout(Duration::from_secs(2), next_json(&mut resumed_socket))
        .await
        .expect("resumed dispatch");
    assert_eq!(resumed["op"], 0);
    assert_eq!(resumed["t"], "RESUMED");
    assert_eq!(resumed["s"], 3);
    assert_eq!(resumed["d"]["session_id"], session_id);

    let second_message_id = send_message(&app, &owner_token, &channel_id, "after resume").await;
    let second_event = timeout(Duration::from_secs(2), next_json(&mut resumed_socket))
        .await
        .expect("second message create dispatch");
    assert_eq!(second_event["op"], 0);
    assert_eq!(second_event["t"], "MESSAGE_CREATE");
    assert_eq!(second_event["s"], 4);
    assert_eq!(second_event["d"]["id"], second_message_id);
    assert_eq!(second_event["d"]["author"]["id"], owner_id);
    assert_eq!(second_event["d"]["content"], "after resume");
}

#[tokio::test]
async fn compat_gateway_dispatches_interaction_create_for_visible_command() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, owner_id) = register(&app, "compat-gateway-command-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "command").await;
    let (application_id, bot_token, bot_user_id) =
        create_bot_with_application(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
    assert_eq!(ready["t"], "READY");

    let command_id = create_command(&app, &bot_token, &application_id, &space_id).await;
    let interaction_id = create_interaction(&app, &owner_token, &channel_id, &command_id).await;
    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("interaction create dispatch");

    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "INTERACTION_CREATE");
    assert_eq!(event["s"], 2);
    assert_eq!(event["d"]["id"], interaction_id);
    assert_eq!(event["d"]["application_id"], application_id);
    assert_eq!(event["d"]["guild_id"], space_id);
    assert_eq!(event["d"]["channel_id"], channel_id);
    assert_eq!(event["d"]["member"]["user"]["id"], owner_id);
    assert!(event["d"]["token"].as_str().unwrap().starts_with("oci_"));
    assert_eq!(event["d"]["data"]["id"], command_id);
    assert_eq!(event["d"]["data"]["name"], "deploy");
    assert_eq!(event["d"]["data"]["type"], 1);
    assert_eq!(event["d"]["data"]["options"][0]["name"], "version");
    assert_eq!(event["d"]["data"]["options"][0]["value"], "1.2.3");
}

#[tokio::test]
async fn compat_gateway_dispatches_component_interaction_and_callback_response() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, owner_id) =
        register(&app, "compat-gateway-component-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "component-interaction").await;
    let (application_id, bot_token, bot_user_id) =
        create_bot_with_application(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

    let component = json!({
        "type": 1,
        "components": [
            {
                "type": 2,
                "style": 1,
                "label": "Approve",
                "custom_id": "deploy:approve"
            }
        ]
    });
    let message_id =
        send_compat_component_message(&app, &bot_token, &channel_id, component.clone()).await;

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
    assert_eq!(ready["t"], "READY");

    let (interaction_id, interaction_token) = create_component_interaction(
        &app,
        &owner_token,
        &channel_id,
        &message_id,
        "deploy:approve",
    )
    .await;
    let event = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("component interaction dispatch");

    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], "INTERACTION_CREATE");
    assert_eq!(event["s"], 2);
    assert_eq!(event["d"]["id"], interaction_id);
    assert_eq!(event["d"]["application_id"], application_id);
    assert_eq!(event["d"]["type"], 3);
    assert_eq!(event["d"]["guild_id"], space_id);
    assert_eq!(event["d"]["channel_id"], channel_id);
    assert_eq!(event["d"]["member"]["user"]["id"], owner_id);
    assert_eq!(event["d"]["token"], interaction_token);
    assert_eq!(event["d"]["data"]["custom_id"], "deploy:approve");
    assert_eq!(event["d"]["data"]["component_type"], 2);
    assert_eq!(event["d"]["message"]["id"], message_id);
    assert_eq!(event["d"]["message"]["components"], json!([component]));

    let callback = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/interactions/{interaction_id}/{interaction_token}/callback"
            ),
            json!({
                "type": 4,
                "data": {
                    "content": "Approved deployment"
                }
            }),
        ))
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::NO_CONTENT);

    let messages = app
        .clone()
        .oneshot(bot_request(
            Method::GET,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(messages.status(), StatusCode::OK);
    let messages = response_json(messages).await;
    let response_message = messages
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["content"] == "Approved deployment")
        .expect("callback response message");
    assert_eq!(response_message["author"]["id"], bot_user_id);
    assert_eq!(response_message["author"]["bot"], true);
}

#[tokio::test]
async fn compat_gateway_rejects_unknown_resume_session() {
    let app = test_app();
    let addr = serve_app(app.clone()).await;
    let (owner_token, _) = register(&app, "compat-gateway-resume-invalid-owner@example.com").await;
    let (organization_id, space_id, _) =
        create_space_with_channel(&app, &owner_token, "resume-invalid").await;
    let (bot_token, bot_user_id) = create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

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
                "op": 6,
                "d": {
                    "token": bot_token,
                    "session_id": "gw_missing",
                    "seq": 0
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send invalid resume");
    let invalid_session = timeout(Duration::from_secs(2), next_json(&mut socket))
        .await
        .expect("invalid session event");
    assert_eq!(invalid_session["op"], 9);
    assert_eq!(invalid_session["d"], false);
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
