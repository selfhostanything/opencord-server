use std::env;

use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaControlConfig {
    pub provider: String,
    pub livekit_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub token_ttl_seconds: i64,
    pub region: String,
}

impl MediaControlConfig {
    pub fn from_env() -> Self {
        Self {
            provider: "livekit".to_owned(),
            livekit_url: trim_url(
                env::var("OPENCORD_LIVEKIT_URL")
                    .unwrap_or_else(|_| "ws://localhost:7880".to_owned()),
            ),
            livekit_api_key: non_empty_env("OPENCORD_LIVEKIT_API_KEY", "devkey"),
            livekit_api_secret: non_empty_env("OPENCORD_LIVEKIT_API_SECRET", "secret"),
            token_ttl_seconds: env::var("OPENCORD_MEDIA_TOKEN_TTL_SECONDS")
                .ok()
                .and_then(|value| value.parse::<i64>().ok())
                .map(|seconds| seconds.clamp(60, 3600))
                .unwrap_or(600),
            region: non_empty_env("OPENCORD_MEDIA_REGION", "local"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaRoomType {
    VoiceChannel,
}

impl MediaRoomType {
    pub fn parse(value: &str) -> Result<Self, MediaError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "voice_channel" => Ok(Self::VoiceChannel),
            _ => Err(MediaError::InvalidInput(
                "media room_type must be voice_channel",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::VoiceChannel => "voice_channel",
        }
    }

    fn room_prefix(self) -> &'static str {
        match self {
            Self::VoiceChannel => "voice",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaTokenGrants {
    pub can_publish_audio: bool,
    pub can_publish_video: bool,
    pub can_publish_screen: bool,
    pub can_subscribe: bool,
}

impl MediaTokenGrants {
    pub fn validate(self) -> Result<(), MediaError> {
        if self.can_publish_audio
            || self.can_publish_video
            || self.can_publish_screen
            || self.can_subscribe
        {
            Ok(())
        } else {
            Err(MediaError::InvalidInput(
                "media token must allow subscribing or at least one publish source",
            ))
        }
    }

    fn can_publish(self) -> bool {
        self.can_publish_audio || self.can_publish_video || self.can_publish_screen
    }

    fn publish_sources(self) -> Vec<&'static str> {
        let mut sources = Vec::new();
        if self.can_publish_audio {
            sources.push("microphone");
        }
        if self.can_publish_video {
            sources.push("camera");
        }
        if self.can_publish_screen {
            sources.push("screen_share");
            sources.push("screen_share_audio");
        }

        sources
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueMediaRoomToken {
    pub room_type: MediaRoomType,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub channel_id: Uuid,
    pub participant_user_id: Uuid,
    pub grants: MediaTokenGrants,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaRoomToken {
    pub provider: String,
    pub server_url: String,
    pub region: String,
    pub room_type: MediaRoomType,
    pub room_name: String,
    pub organization_id: Uuid,
    pub space_id: Uuid,
    pub channel_id: Uuid,
    pub participant_identity: String,
    pub participant_token: String,
    pub expires_at: DateTime<Utc>,
    pub grants: MediaTokenGrants,
}

#[derive(Debug)]
pub enum MediaError {
    InvalidInput(&'static str),
    SigningFailed,
}

impl MediaError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) => StatusCode::BAD_REQUEST,
            Self::SigningFailed => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_request",
            Self::SigningFailed => "media_token_signing_failed",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidInput(message) => message,
            Self::SigningFailed => "media token could not be signed",
        }
    }
}

#[derive(Clone)]
pub struct MediaControlService {
    config: MediaControlConfig,
}

impl MediaControlService {
    pub fn from_env() -> Self {
        Self::new(MediaControlConfig::from_env())
    }

    pub fn new(config: MediaControlConfig) -> Self {
        Self { config }
    }

    pub fn issue_room_token(
        &self,
        request: IssueMediaRoomToken,
    ) -> Result<MediaRoomToken, MediaError> {
        request.grants.validate()?;

        let issued_at = Utc::now();
        let expires_at = issued_at + Duration::seconds(self.config.token_ttl_seconds);
        let room_name = room_name(request.room_type, request.channel_id);
        let participant_identity = request.participant_user_id.to_string();
        let participant_token = self.sign_livekit_token(
            &room_name,
            &participant_identity,
            &request,
            issued_at,
            expires_at,
        )?;

        Ok(MediaRoomToken {
            provider: self.config.provider.clone(),
            server_url: self.config.livekit_url.clone(),
            region: self.config.region.clone(),
            room_type: request.room_type,
            room_name,
            organization_id: request.organization_id,
            space_id: request.space_id,
            channel_id: request.channel_id,
            participant_identity,
            participant_token,
            expires_at,
            grants: request.grants,
        })
    }

    fn sign_livekit_token(
        &self,
        room_name: &str,
        participant_identity: &str,
        request: &IssueMediaRoomToken,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<String, MediaError> {
        let mut video = json!({
            "room": room_name,
            "roomJoin": true,
            "canPublish": request.grants.can_publish(),
            "canPublishData": true,
            "canSubscribe": request.grants.can_subscribe,
        });

        if request.grants.can_publish() {
            video["canPublishSources"] = json!(request.grants.publish_sources());
        }

        let payload = json!({
            "iss": self.config.livekit_api_key,
            "sub": participant_identity,
            "nbf": issued_at.timestamp(),
            "exp": expires_at.timestamp(),
            "video": video,
            "metadata": "",
            "attributes": {
                "opencord.organization_id": request.organization_id.to_string(),
                "opencord.space_id": request.space_id.to_string(),
                "opencord.channel_id": request.channel_id.to_string(),
                "opencord.room_type": request.room_type.as_str(),
            },
        });

        sign_jwt(
            json!({ "alg": "HS256", "typ": "JWT" }),
            payload,
            &self.config.livekit_api_secret,
        )
    }
}

fn room_name(room_type: MediaRoomType, channel_id: Uuid) -> String {
    format!(
        "opencord_{}_{}",
        room_type.room_prefix(),
        channel_id.simple()
    )
}

fn sign_jwt(header: Value, payload: Value, secret: &str) -> Result<String, MediaError> {
    let header = encode_json(header)?;
    let payload = encode_json(payload)?;
    let signing_input = format!("{header}.{payload}");
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| MediaError::SigningFailed)?;
    mac.update(signing_input.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    Ok(format!("{signing_input}.{signature}"))
}

fn encode_json(value: Value) -> Result<String, MediaError> {
    let bytes = serde_json::to_vec(&value).map_err(|_| MediaError::SigningFailed)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn trim_url(url: String) -> String {
    let trimmed = url.trim().trim_end_matches('/').to_owned();
    if trimmed.is_empty() {
        "ws://localhost:7880".to_owned()
    } else {
        trimmed
    }
}

fn non_empty_env(name: &str, fallback: &str) -> String {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}
