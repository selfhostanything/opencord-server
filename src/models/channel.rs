use serde::{Deserialize, Serialize};

use crate::domain::channel::ChannelPatch;

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub kind: Option<String>,
    pub name: String,
    pub topic: Option<String>,
    pub is_private: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PatchChannelRequest {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: Option<i32>,
    pub is_private: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChannelResponse {
    pub id: String,
    pub organization_id: String,
    pub space_id: String,
    pub kind: String,
    pub name: String,
    pub slug: String,
    pub position: i32,
    pub topic: Option<String>,
    pub is_private: bool,
}

#[derive(Debug, Serialize)]
pub struct ChannelResourceResponse {
    pub channel: ChannelResponse,
}

#[derive(Debug, Serialize)]
pub struct ChannelListResponse {
    pub channels: Vec<ChannelResponse>,
}

impl From<PatchChannelRequest> for ChannelPatch {
    fn from(request: PatchChannelRequest) -> Self {
        Self {
            name: request.name,
            topic: request.topic,
            position: request.position,
            is_private: request.is_private,
        }
    }
}
