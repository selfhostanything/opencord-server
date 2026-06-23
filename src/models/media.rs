use serde::{Deserialize, Serialize};

use crate::domain::media::{MediaRoomToken, MediaTokenGrants};

#[derive(Debug, Deserialize)]
pub struct CreateMediaRoomTokenRequest {
    pub room_type: String,
    pub organization_id: String,
    pub space_id: String,
    pub channel_id: String,
    pub can_publish_audio: Option<bool>,
    pub can_publish_video: Option<bool>,
    pub can_publish_screen: Option<bool>,
    pub can_subscribe: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MediaTokenGrantsResponse {
    pub can_publish_audio: bool,
    pub can_publish_video: bool,
    pub can_publish_screen: bool,
    pub can_subscribe: bool,
}

#[derive(Debug, Serialize)]
pub struct MediaRoomTokenResponse {
    pub provider: String,
    pub server_url: String,
    pub region: String,
    pub room_type: &'static str,
    pub room_name: String,
    pub organization_id: String,
    pub space_id: String,
    pub channel_id: String,
    pub participant_identity: String,
    pub participant_token: String,
    pub expires_at: String,
    pub grants: MediaTokenGrantsResponse,
}

#[derive(Debug, Serialize)]
pub struct MediaRoomTokenResourceResponse {
    pub media: MediaRoomTokenResponse,
}

impl CreateMediaRoomTokenRequest {
    pub fn grants(&self) -> MediaTokenGrants {
        MediaTokenGrants {
            can_publish_audio: self.can_publish_audio.unwrap_or(false),
            can_publish_video: self.can_publish_video.unwrap_or(false),
            can_publish_screen: self.can_publish_screen.unwrap_or(false),
            can_subscribe: self.can_subscribe.unwrap_or(true),
        }
    }
}

impl From<MediaTokenGrants> for MediaTokenGrantsResponse {
    fn from(grants: MediaTokenGrants) -> Self {
        Self {
            can_publish_audio: grants.can_publish_audio,
            can_publish_video: grants.can_publish_video,
            can_publish_screen: grants.can_publish_screen,
            can_subscribe: grants.can_subscribe,
        }
    }
}

impl From<MediaRoomToken> for MediaRoomTokenResponse {
    fn from(token: MediaRoomToken) -> Self {
        Self {
            provider: token.provider,
            server_url: token.server_url,
            region: token.region,
            room_type: token.room_type.as_str(),
            room_name: token.room_name,
            organization_id: token.organization_id.to_string(),
            space_id: token.space_id.to_string(),
            channel_id: token.channel_id.to_string(),
            participant_identity: token.participant_identity,
            participant_token: token.participant_token,
            expires_at: token.expires_at.to_rfc3339(),
            grants: MediaTokenGrantsResponse::from(token.grants),
        }
    }
}
