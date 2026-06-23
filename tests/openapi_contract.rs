use std::fs;

use serde_yaml::Value;

const REQUIRED_CONTRACT_PATHS: &[&str] = &[
    "/healthz",
    "/metrics",
    "/.well-known/opencord",
    "/auth/login",
    "/me",
    "/organizations",
    "/organizations/{organization_id}/spaces",
    "/channels/{channel_id}/messages",
    "/voice/channels/{channel_id}/join",
    "/join/{join_slug}",
    "/organizations/{organization_id}/meetings",
    "/organizations/{organization_id}/bot-applications",
    "/channels/{channel_id}/webhooks",
    "/api/compat/discord/v10/channels/{channel_id}/messages",
];

#[test]
fn openapi_contract_covers_route_contracts_used_by_clients_and_bots() {
    let document = fs::read_to_string("openapi/openapi.yaml").expect("read OpenAPI contract");
    let document: Value = serde_yaml::from_str(&document).expect("parse OpenAPI YAML");
    let paths = document
        .get("paths")
        .and_then(Value::as_mapping)
        .expect("OpenAPI contract has paths map");

    for path in REQUIRED_CONTRACT_PATHS {
        assert!(
            paths.contains_key(Value::String((*path).to_owned())),
            "OpenAPI contract is missing {path}",
        );
    }
}
