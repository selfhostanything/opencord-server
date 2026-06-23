use axum::extract::State;
use axum::extract::ws::{Message as WebSocketMessage, WebSocket, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::domain::bot::AuthenticatedBot;
use crate::domain::ids;
use crate::domain::permission::Permission;
use crate::domain::realtime::RealtimeEvent;
use crate::models::compat::{CompatMessageResponse, CompatUserResponse};
use crate::state::AppState;

const OP_DISPATCH: i32 = 0;
const OP_HEARTBEAT: i32 = 1;
const OP_IDENTIFY: i32 = 2;
const OP_RESUME: i32 = 6;
const OP_INVALID_SESSION: i32 = 9;
const OP_HELLO: i32 = 10;
const OP_HEARTBEAT_ACK: i32 = 11;
const HEARTBEAT_INTERVAL_MS: u64 = 45_000;

#[derive(Debug, Deserialize)]
struct GatewayMessage {
    op: i32,
    d: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct IdentifyPayload {
    token: String,
    intents: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ResumePayload {
    token: String,
    session_id: String,
    seq: Option<i64>,
}

pub async fn gateway(
    State(state): State<AppState>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, std::convert::Infallible> {
    Ok(upgrade
        .on_upgrade(move |socket| handle_gateway_socket(state, socket))
        .into_response())
}

async fn handle_gateway_socket(state: AppState, mut socket: WebSocket) {
    if send_json(
        &mut socket,
        &json!({
            "op": OP_HELLO,
            "d": {
                "heartbeat_interval": HEARTBEAT_INTERVAL_MS
            }
        }),
    )
    .await
    .is_err()
    {
        return;
    }

    let mut events = state.realtime.subscribe();
    let mut identified_bot: Option<AuthenticatedBot> = None;
    let mut active_session_id: Option<String> = None;
    let mut sequence: i64 = 0;

    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(Ok(message)) = message else {
                    break;
                };

                if !handle_client_message(
                    &state,
                    &mut socket,
                    message,
                    &mut identified_bot,
                    &mut active_session_id,
                    &mut sequence
                ).await {
                    break;
                }
            }
            event = events.recv() => {
                let Ok(event) = event else {
                    continue;
                };
                let Some(bot) = identified_bot.as_ref() else {
                    continue;
                };

                if event.event_type == "message.created"
                    && can_bot_receive_event(&state, bot, &event).await
                {
                    let Some(message) = compat_message_from_event(&event, bot) else {
                        continue;
                    };
                    sequence += 1;
                    if send_dispatch(&mut socket, "MESSAGE_CREATE", sequence, message).await.is_err() {
                        break;
                    }
                    update_active_session_sequence(&state, active_session_id.as_deref(), sequence);
                } else if event.event_type == "interaction.created"
                    && can_bot_receive_event(&state, bot, &event).await
                {
                    let Some(interaction) = compat_interaction_from_event(&event, bot) else {
                        continue;
                    };
                    sequence += 1;
                    if send_dispatch(&mut socket, "INTERACTION_CREATE", sequence, interaction).await.is_err() {
                        break;
                    }
                    update_active_session_sequence(&state, active_session_id.as_deref(), sequence);
                }
            }
        }
    }
}

async fn handle_client_message(
    state: &AppState,
    socket: &mut WebSocket,
    message: WebSocketMessage,
    identified_bot: &mut Option<AuthenticatedBot>,
    active_session_id: &mut Option<String>,
    sequence: &mut i64,
) -> bool {
    match message {
        WebSocketMessage::Text(text) => {
            let Ok(message) = serde_json::from_str::<GatewayMessage>(&text) else {
                let _ = send_invalid_session(socket).await;
                return true;
            };

            match message.op {
                OP_HEARTBEAT => send_json(socket, &json!({ "op": OP_HEARTBEAT_ACK }))
                    .await
                    .is_ok(),
                OP_IDENTIFY => {
                    identify_bot(
                        state,
                        socket,
                        message.d,
                        identified_bot,
                        active_session_id,
                        sequence,
                    )
                    .await
                }
                OP_RESUME => {
                    resume_bot(
                        state,
                        socket,
                        message.d,
                        identified_bot,
                        active_session_id,
                        sequence,
                    )
                    .await
                }
                _ => {
                    let _ = send_invalid_session(socket).await;
                    true
                }
            }
        }
        WebSocketMessage::Ping(payload) => {
            socket.send(WebSocketMessage::Pong(payload)).await.is_ok()
        }
        WebSocketMessage::Pong(_) => true,
        WebSocketMessage::Close(_) => false,
        WebSocketMessage::Binary(_) => {
            let _ = send_invalid_session(socket).await;
            true
        }
    }
}

async fn identify_bot(
    state: &AppState,
    socket: &mut WebSocket,
    payload: Option<Value>,
    identified_bot: &mut Option<AuthenticatedBot>,
    active_session_id: &mut Option<String>,
    sequence: &mut i64,
) -> bool {
    let Some(payload) = payload else {
        let _ = send_invalid_session(socket).await;
        return true;
    };
    let Ok(payload) = serde_json::from_value::<IdentifyPayload>(payload) else {
        let _ = send_invalid_session(socket).await;
        return true;
    };

    let Ok(bot) = state.bots.authenticate_token(&payload.token).await else {
        let _ = send_invalid_session(socket).await;
        return false;
    };

    *sequence += 1;
    let session_id = format!("gw_{}", ids::new_uuid_v7());
    let ready = json!({
        "v": 10,
        "session_id": session_id.clone(),
        "resume_gateway_url": "/api/compat/discord/gateway",
        "user": {
            "id": bot.bot_user_id.to_string(),
            "username": bot.name.clone(),
            "bot": true
        },
        "guilds": [],
        "application": {
            "id": bot.application_id.to_string()
        },
        "intents": payload.intents.unwrap_or_default()
    });

    state
        .compat_gateway_sessions
        .create(session_id.clone(), &bot, *sequence);
    *identified_bot = Some(bot);
    *active_session_id = Some(session_id);

    send_dispatch(socket, "READY", *sequence, ready)
        .await
        .is_ok()
}

async fn resume_bot(
    state: &AppState,
    socket: &mut WebSocket,
    payload: Option<Value>,
    identified_bot: &mut Option<AuthenticatedBot>,
    active_session_id: &mut Option<String>,
    sequence: &mut i64,
) -> bool {
    let Some(payload) = payload else {
        let _ = send_invalid_session(socket).await;
        return true;
    };
    let Ok(payload) = serde_json::from_value::<ResumePayload>(payload) else {
        let _ = send_invalid_session(socket).await;
        return true;
    };

    let Ok(bot) = state.bots.authenticate_token(&payload.token).await else {
        let _ = send_invalid_session(socket).await;
        return false;
    };

    let Some(session) = state.compat_gateway_sessions.resume(
        &payload.session_id,
        &bot,
        payload.seq.unwrap_or_default(),
    ) else {
        let _ = send_invalid_session(socket).await;
        return true;
    };

    *sequence = session.sequence + 1;
    state
        .compat_gateway_sessions
        .update_sequence(&session.session_id, *sequence);
    *identified_bot = Some(bot);
    *active_session_id = Some(session.session_id.clone());

    send_dispatch(
        socket,
        "RESUMED",
        *sequence,
        json!({
            "session_id": session.session_id
        }),
    )
    .await
    .is_ok()
}

fn update_active_session_sequence(state: &AppState, session_id: Option<&str>, sequence: i64) {
    if let Some(session_id) = session_id {
        state
            .compat_gateway_sessions
            .update_sequence(session_id, sequence);
    }
}

async fn can_bot_receive_event(
    state: &AppState,
    bot: &AuthenticatedBot,
    event: &RealtimeEvent,
) -> bool {
    let Some(channel_id) = event.scope.channel_id.as_deref() else {
        return true;
    };
    let Ok(channel_id) = Uuid::parse_str(channel_id) else {
        return false;
    };
    let Ok(channel) = state.channels.get(channel_id).await else {
        return false;
    };
    if channel.organization_id != bot.organization_id {
        return false;
    }
    let Ok(space) = state
        .spaces
        .get_for_user(bot.bot_user_id, channel.space_id)
        .await
    else {
        return false;
    };

    state
        .permissions
        .can_in_channel(bot.bot_user_id, &space, &channel, Permission::ViewChannel)
        .await
        .unwrap_or(false)
}

fn compat_message_from_event(
    event: &RealtimeEvent,
    current_bot: &AuthenticatedBot,
) -> Option<CompatMessageResponse> {
    let message = event.data.get("message")?;
    compat_message_from_value(message, current_bot)
}

fn compat_message_from_value(
    message: &Value,
    current_bot: &AuthenticatedBot,
) -> Option<CompatMessageResponse> {
    let author_user_id = message.get("author_user_id")?.as_str()?.to_owned();
    let author_is_current_bot = author_user_id == current_bot.bot_user_id.to_string();
    let embeds = message
        .get("embeds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let components = message
        .get("components")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let attachments = compat_attachments_from_event(message.get("attachments"));
    let message_reference = compat_message_reference_from_event(message);
    let referenced_message = message
        .get("referenced_message")
        .and_then(|referenced_message| compat_message_from_value(referenced_message, current_bot))
        .map(Box::new);
    let mentions = compat_mentions_from_event(message.get("mentions"));
    let mention_roles = compat_mention_roles_from_event(message.get("mention_roles"));

    Some(CompatMessageResponse {
        id: message.get("id")?.as_str()?.to_owned(),
        channel_id: message.get("channel_id")?.as_str()?.to_owned(),
        author: CompatUserResponse {
            id: author_user_id,
            username: if author_is_current_bot {
                current_bot.name.clone()
            } else {
                "OpenCord User".to_owned()
            },
            bot: author_is_current_bot,
        },
        content: message.get("content")?.as_str()?.to_owned(),
        timestamp: message.get("created_at")?.as_str()?.to_owned(),
        edited_timestamp: message
            .get("edited_at")
            .and_then(|value| value.as_str())
            .map(str::to_owned),
        tts: false,
        mention_everyone: message
            .get("mention_everyone")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        mentions,
        mention_roles,
        attachments,
        embeds,
        components,
        message_reference,
        referenced_message,
        pinned: false,
        kind: 0,
    })
}

fn compat_message_reference_from_event(
    message: &Value,
) -> Option<crate::models::compat::CompatMessageReferenceResponse> {
    let reply_to_message_id = message.get("reply_to_message_id")?.as_str()?;
    Some(crate::models::compat::CompatMessageReferenceResponse {
        message_id: reply_to_message_id.to_owned(),
        channel_id: message.get("channel_id")?.as_str()?.to_owned(),
        guild_id: message
            .get("space_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn compat_attachments_from_event(attachments: Option<&Value>) -> Vec<Value> {
    attachments
        .and_then(Value::as_array)
        .map(|attachments| {
            attachments
                .iter()
                .filter_map(compat_attachment_from_event)
                .collect()
        })
        .unwrap_or_default()
}

fn compat_attachment_from_event(attachment: &Value) -> Option<Value> {
    let id = attachment.get("id")?.as_str()?;
    let filename = attachment.get("file_name")?.as_str()?;
    let size = attachment.get("size_bytes")?.as_i64()?;
    let url = attachment.get("download_url")?.as_str()?;
    let mut value = json!({
        "id": id,
        "filename": filename,
        "size": size,
        "url": url,
        "proxy_url": url
    });

    if let Some(content_type) = attachment.get("content_type").and_then(Value::as_str)
        && let Some(object) = value.as_object_mut()
    {
        object.insert(
            "content_type".to_owned(),
            Value::String(content_type.to_owned()),
        );
    }

    Some(value)
}

fn compat_mentions_from_event(mentions: Option<&Value>) -> Vec<CompatUserResponse> {
    mentions
        .and_then(Value::as_array)
        .map(|mentions| {
            mentions
                .iter()
                .filter_map(compat_mention_from_event)
                .collect()
        })
        .unwrap_or_default()
}

fn compat_mention_from_event(mention: &Value) -> Option<CompatUserResponse> {
    Some(CompatUserResponse {
        id: mention.get("id")?.as_str()?.to_owned(),
        username: mention.get("username")?.as_str()?.to_owned(),
        bot: mention.get("bot").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn compat_mention_roles_from_event(mention_roles: Option<&Value>) -> Vec<String> {
    mention_roles
        .and_then(Value::as_array)
        .map(|mention_roles| {
            mention_roles
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn compat_interaction_from_event(
    event: &RealtimeEvent,
    current_bot: &AuthenticatedBot,
) -> Option<Value> {
    let interaction = event.data.get("interaction")?;
    let application_id = interaction.get("application_id")?.as_str()?;
    if application_id != current_bot.application_id.to_string() {
        return None;
    }
    let command = event.data.get("command")?;

    Some(json!({
        "id": interaction.get("id")?.as_str()?,
        "application_id": application_id,
        "type": 2,
        "token": interaction.get("token")?.as_str()?,
        "guild_id": interaction.get("space_id")?.as_str()?,
        "channel_id": interaction.get("channel_id")?.as_str()?,
        "member": {
            "user": {
                "id": interaction.get("invoking_user_id")?.as_str()?
            }
        },
        "data": {
            "id": command.get("id")?.as_str()?,
            "name": command.get("name")?.as_str()?,
            "type": command.get("type")?.as_i64().unwrap_or(1),
            "options": interaction
                .get("options")
                .cloned()
                .unwrap_or_else(|| json!([]))
        }
    }))
}

async fn send_dispatch<T: serde::Serialize>(
    socket: &mut WebSocket,
    event_type: &str,
    sequence: i64,
    data: T,
) -> Result<(), ()> {
    send_json(
        socket,
        &json!({
            "op": OP_DISPATCH,
            "t": event_type,
            "s": sequence,
            "d": data
        }),
    )
    .await
}

async fn send_invalid_session(socket: &mut WebSocket) -> Result<(), ()> {
    send_json(
        socket,
        &json!({
            "op": OP_INVALID_SESSION,
            "d": false
        }),
    )
    .await
}

async fn send_json<T: serde::Serialize>(socket: &mut WebSocket, value: &T) -> Result<(), ()> {
    let text = serde_json::to_string(value).map_err(|_| ())?;
    socket
        .send(WebSocketMessage::Text(text.into()))
        .await
        .map_err(|_| ())
}
