use axum::extract::ws::{Message as WebSocketMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::domain::auth::{AuthError, AuthUser};
use crate::domain::permission::Permission;
use crate::domain::realtime::RealtimeEvent;
use crate::http::session::bearer_token;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct WebSocketAuthQuery {
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClientMessage {
    #[serde(rename = "type")]
    message_type: String,
    channel_id: Option<String>,
}

pub async fn websocket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WebSocketAuthQuery>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, RealtimeApiError> {
    let user = match query.token.as_deref() {
        Some(token) => state.auth.user_for_token(token).await?,
        None => state.auth.user_for_token(bearer_token(&headers)?).await?,
    };

    Ok(upgrade
        .on_upgrade(move |socket| handle_socket(state, user, socket))
        .into_response())
}

async fn handle_socket(state: AppState, user: AuthUser, mut socket: WebSocket) {
    let mut events = state.realtime.subscribe();

    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(Ok(message)) = message else {
                    break;
                };

                if !handle_client_message(&state, &user, &mut socket, message).await {
                    break;
                }
            }
            event = events.recv() => {
                let Ok(event) = event else {
                    continue;
                };

                if can_receive_event(&state, &user, &event).await
                    && send_json(&mut socket, &event).await.is_err()
                {
                    break;
                }
            }
        }
    }
}

async fn handle_client_message(
    state: &AppState,
    user: &AuthUser,
    socket: &mut WebSocket,
    message: WebSocketMessage,
) -> bool {
    match message {
        WebSocketMessage::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
            Ok(message) => handle_client_action(state, user, socket, message).await,
            Err(_) => {
                let _ = send_error(socket, "invalid_payload").await;
                true
            }
        },
        WebSocketMessage::Ping(payload) => {
            socket.send(WebSocketMessage::Pong(payload)).await.is_ok()
        }
        WebSocketMessage::Pong(_) => true,
        WebSocketMessage::Close(_) => false,
        WebSocketMessage::Binary(_) => {
            let _ = send_error(socket, "invalid_payload").await;
            true
        }
    }
}

async fn handle_client_action(
    state: &AppState,
    user: &AuthUser,
    socket: &mut WebSocket,
    message: ClientMessage,
) -> bool {
    match message.message_type.as_str() {
        "ping" => send_json(socket, &json!({ "type": "pong" })).await.is_ok(),
        "typing.start" => {
            publish_typing(state, user, socket, message.channel_id, "typing.started").await
        }
        "typing.stop" => {
            publish_typing(state, user, socket, message.channel_id, "typing.stopped").await
        }
        _ => {
            let _ = send_error(socket, "invalid_payload").await;
            true
        }
    }
}

async fn publish_typing(
    state: &AppState,
    user: &AuthUser,
    socket: &mut WebSocket,
    channel_id: Option<String>,
    event_type: &str,
) -> bool {
    let Some(channel_id) = channel_id else {
        let _ = send_error(socket, "invalid_payload").await;
        return true;
    };
    let Ok(channel_id) = Uuid::parse_str(&channel_id) else {
        let _ = send_error(socket, "invalid_payload").await;
        return true;
    };
    let Ok(channel) = state.channels.get(channel_id).await else {
        let _ = send_error(socket, "forbidden").await;
        return true;
    };
    let Ok(space) = state.spaces.get_for_user(user.id, channel.space_id).await else {
        let _ = send_error(socket, "forbidden").await;
        return true;
    };
    let Ok(()) = state
        .permissions
        .require_channel(user.id, &space, &channel, Permission::SendMessages)
        .await
    else {
        let _ = send_error(socket, "forbidden").await;
        return true;
    };

    state.realtime.publish(RealtimeEvent::channel(
        event_type,
        channel.organization_id,
        channel.space_id,
        channel.id,
        json!({ "user_id": user.id.to_string() }),
    ));

    true
}

async fn can_receive_event(state: &AppState, user: &AuthUser, event: &RealtimeEvent) -> bool {
    let Some(channel_id) = event.scope.channel_id.as_deref() else {
        return true;
    };
    let Ok(channel_id) = Uuid::parse_str(channel_id) else {
        return false;
    };
    let Ok(channel) = state.channels.get(channel_id).await else {
        return false;
    };
    let Ok(space) = state.spaces.get_for_user(user.id, channel.space_id).await else {
        return false;
    };

    state
        .permissions
        .can_in_channel(user.id, &space, &channel, Permission::ViewChannel)
        .await
        .unwrap_or(false)
}

async fn send_json<T: serde::Serialize>(socket: &mut WebSocket, value: &T) -> Result<(), ()> {
    let text = serde_json::to_string(value).map_err(|_| ())?;
    socket
        .send(WebSocketMessage::Text(text.into()))
        .await
        .map_err(|_| ())
}

async fn send_error(socket: &mut WebSocket, code: &'static str) -> Result<(), ()> {
    send_json(
        socket,
        &json!({
            "type": "error",
            "error": {
                "code": code
            }
        }),
    )
    .await
}

#[derive(Debug)]
pub struct RealtimeApiError(AuthError);

impl From<AuthError> for RealtimeApiError {
    fn from(error: AuthError) -> Self {
        Self(error)
    }
}

impl IntoResponse for RealtimeApiError {
    fn into_response(self) -> Response {
        self.0.status_code().into_response()
    }
}
