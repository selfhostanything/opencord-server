use serde::{Deserialize, Serialize};

use crate::domain::bot::{BotApplication, BotApplicationCreated, BotToken};
use crate::models::permission::SpaceMemberDetailResponse;

#[derive(Debug, Deserialize)]
pub struct CreateBotApplicationRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InviteBotToSpaceRequest {
    pub role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BotApplicationResponse {
    pub id: String,
    pub organization_id: String,
    pub bot_user_id: String,
    pub created_by_user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct BotTokenResponse {
    pub id: String,
    pub application_id: String,
    pub token: String,
    pub token_last_four: String,
}

#[derive(Debug, Serialize)]
pub struct BotApplicationCreatedResponse {
    pub bot_application: BotApplicationResponse,
    pub bot_token: BotTokenResponse,
}

#[derive(Debug, Serialize)]
pub struct BotApplicationDetailResponse {
    pub bot_application: BotApplicationResponse,
    pub active_token_last_four: Option<String>,
    pub space_memberships: Vec<SpaceMemberDetailResponse>,
}

#[derive(Debug, Serialize)]
pub struct BotApplicationListResponse {
    pub bot_applications: Vec<BotApplicationDetailResponse>,
}

#[derive(Debug, Serialize)]
pub struct BotTokenResourceResponse {
    pub bot_token: BotTokenResponse,
}

#[derive(Debug, Serialize)]
pub struct BotApplicationInviteResponse {
    pub bot_application: BotApplicationResponse,
    pub member: SpaceMemberDetailResponse,
}

impl From<BotApplicationCreated> for BotApplicationCreatedResponse {
    fn from(created: BotApplicationCreated) -> Self {
        Self {
            bot_application: BotApplicationResponse::from(created.application),
            bot_token: BotTokenResponse::from(created.token),
        }
    }
}

impl From<BotApplication> for BotApplicationResponse {
    fn from(application: BotApplication) -> Self {
        Self {
            id: application.id.to_string(),
            organization_id: application.organization_id.to_string(),
            bot_user_id: application.bot_user_id.to_string(),
            created_by_user_id: application.created_by_user_id.to_string(),
            name: application.name,
            description: application.description,
            status: application.status,
        }
    }
}

impl From<BotToken> for BotTokenResponse {
    fn from(token: BotToken) -> Self {
        Self {
            id: token.id.to_string(),
            application_id: token.application_id.to_string(),
            token: token.token,
            token_last_four: token.token_last_four,
        }
    }
}
