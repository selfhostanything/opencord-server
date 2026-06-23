use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ScimTokenEnvelope {
    pub scim_token: ScimTokenResponse,
}

#[derive(Debug, Serialize)]
pub struct ScimTokenResponse {
    pub organization_id: String,
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct ScimCreateUserRequest {
    #[serde(rename = "externalId")]
    pub external_id: String,
    #[serde(rename = "userName")]
    pub user_name: String,
    pub name: Option<ScimNameRequest>,
    #[serde(default = "default_active")]
    pub active: bool,
}

#[derive(Debug, Deserialize)]
pub struct ScimNameRequest {
    pub formatted: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ScimPatchUserRequest {
    #[serde(rename = "Operations")]
    pub operations: Vec<ScimPatchOperation>,
}

#[derive(Debug, Deserialize)]
pub struct ScimPatchOperation {
    pub op: String,
    pub path: Option<String>,
    pub value: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ScimUserResponse {
    pub schemas: Vec<&'static str>,
    pub id: String,
    #[serde(rename = "externalId")]
    pub external_id: String,
    #[serde(rename = "userName")]
    pub user_name: String,
    pub name: ScimNameResponse,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct ScimNameResponse {
    pub formatted: String,
}

fn default_active() -> bool {
    true
}
