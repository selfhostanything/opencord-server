use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct WellKnownResponse {
    pub server: &'static str,
    pub version: String,
    pub api_base_url: String,
    pub realtime_url: String,
}

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct CapabilitiesResponse {
    pub capabilities: Vec<&'static str>,
}
