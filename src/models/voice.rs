use serde::{Deserialize, Serialize};

use crate::models::media::{MediaRoomTokenResponse, MediaTokenGrantsResponse};

#[derive(Debug, Deserialize)]
pub struct JoinVoiceChannelRequest {
    pub self_mute: Option<bool>,
    pub self_deaf: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VoiceParticipantResponse {
    pub channel_id: String,
    pub user_id: String,
    pub self_mute: bool,
    pub self_deaf: bool,
}

#[derive(Debug, Serialize)]
pub struct VoiceMediaEventResponse {
    pub provider: String,
    pub server_url: String,
    pub region: String,
    pub room_type: &'static str,
    pub room_name: String,
    pub organization_id: String,
    pub space_id: String,
    pub channel_id: String,
    pub participant_identity: String,
    pub participant_token: Option<String>,
    pub expires_at: String,
    pub grants: MediaTokenGrantsResponse,
}

#[derive(Debug, Serialize)]
pub struct VoiceJoinResponse {
    pub voice: VoiceParticipantResponse,
    pub media: MediaRoomTokenResponse,
}

impl JoinVoiceChannelRequest {
    pub fn self_mute(&self) -> bool {
        self.self_mute.unwrap_or(false)
    }

    pub fn self_deaf(&self) -> bool {
        self.self_deaf.unwrap_or(false)
    }
}

impl From<MediaRoomTokenResponse> for VoiceMediaEventResponse {
    fn from(media: MediaRoomTokenResponse) -> Self {
        Self {
            provider: media.provider,
            server_url: media.server_url,
            region: media.region,
            room_type: media.room_type,
            room_name: media.room_name,
            organization_id: media.organization_id,
            space_id: media.space_id.unwrap_or_default(),
            channel_id: media.channel_id.unwrap_or_default(),
            participant_identity: media.participant_identity,
            participant_token: None,
            expires_at: media.expires_at,
            grants: media.grants,
        }
    }
}
