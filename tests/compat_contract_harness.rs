mod support;

use std::path::PathBuf;

use axum::http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use support::compat::{CompatHarness, assert_uuid_v7_string};

#[tokio::test]
async fn harness_validates_rest_gateway_and_interaction_contracts() {
    let harness = CompatHarness::new();
    let owner = harness.register("compat-contract-owner@example.com").await;
    let space = harness
        .create_space_with_channel(&owner.token, "contract")
        .await;
    let bot = harness
        .create_bot_application(&owner.token, &space.organization_id, "Contract Bot")
        .await;
    harness
        .add_space_member(&owner.token, &space.space_id, &bot.bot_user_id, "member")
        .await;

    let mut gateway = harness.connect_compat_gateway().await;
    let hello = harness.next_gateway_json(&mut gateway).await;
    assert_eq!(hello["op"], 10);
    assert_eq!(hello["d"]["heartbeat_interval"], 45000);

    let ready = harness
        .identify_compat_gateway(&mut gateway, &bot.bot_token)
        .await;
    assert_dispatch(&ready, "READY", 1);
    assert_eq!(ready["d"]["user"]["id"], bot.bot_user_id);
    assert_eq!(ready["d"]["user"]["username"], "Contract Bot");
    assert_eq!(ready["d"]["user"]["bot"], true);
    assert_eq!(ready["d"]["application"]["id"], bot.application_id);
    assert!(ready["d"]["session_id"].as_str().is_some());
    assert_json_fixture("gateway_ready", &ready);

    let (message_status, message_body) = harness
        .bot_json(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/channels/{}/messages",
                space.channel_id
            ),
            &bot.bot_token,
            json!({ "content": "contract hello", "tts": false }),
        )
        .await;
    assert_eq!(message_status, StatusCode::OK);
    let message = message_body.expect("message create response");
    assert_message_contract(
        &message,
        &space.channel_id,
        &bot.bot_user_id,
        "contract hello",
        true,
        json!([]),
    );
    assert_json_fixture("rest_message_create", &message);

    let gateway_message = harness.next_gateway_json(&mut gateway).await;
    assert_dispatch(&gateway_message, "MESSAGE_CREATE", 2);
    assert_message_contract(
        &gateway_message["d"],
        &space.channel_id,
        &bot.bot_user_id,
        "contract hello",
        true,
        json!([]),
    );
    assert_json_fixture("gateway_message_create", &gateway_message);

    let embed = json!({
        "title": "Deploy ready",
        "description": "Release 1.2.3 passed checks",
        "color": 5793266
    });
    let expected_embed = json!({
        "title": "Deploy ready",
        "type": "rich",
        "description": "Release 1.2.3 passed checks",
        "color": 5793266
    });
    let (embed_status, embed_body) = harness
        .bot_json(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/channels/{}/messages",
                space.channel_id
            ),
            &bot.bot_token,
            json!({
                "content": "",
                "embeds": [embed.clone()],
                "allowed_mentions": {
                    "parse": []
                }
            }),
        )
        .await;
    assert_eq!(embed_status, StatusCode::OK);
    let embed_message = embed_body.expect("embed message create response");
    assert_message_contract(
        &embed_message,
        &space.channel_id,
        &bot.bot_user_id,
        "",
        true,
        json!([expected_embed.clone()]),
    );
    assert_json_fixture("rest_embed_message_create", &embed_message);

    let gateway_embed_message = harness.next_gateway_json(&mut gateway).await;
    assert_dispatch(&gateway_embed_message, "MESSAGE_CREATE", 3);
    assert_message_contract(
        &gateway_embed_message["d"],
        &space.channel_id,
        &bot.bot_user_id,
        "",
        true,
        json!([expected_embed]),
    );
    assert_json_fixture("gateway_embed_message_create", &gateway_embed_message);

    let (command_status, command_body) = harness
        .bot_json(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/applications/{}/guilds/{}/commands",
                bot.application_id, space.space_id
            ),
            &bot.bot_token,
            json!({
                "name": "deploy",
                "description": "Deploy a release",
                "type": 1,
                "options": [
                    {
                        "type": 3,
                        "name": "version",
                        "description": "Release version",
                        "required": true
                    }
                ]
            }),
        )
        .await;
    assert_eq!(command_status, StatusCode::CREATED);
    let command = command_body.expect("command create response");
    let command_id = command["id"].as_str().expect("command id");
    assert_uuid_v7_string(command_id);
    assert_eq!(command["application_id"], bot.application_id);
    assert_eq!(command["guild_id"], space.space_id);
    assert_eq!(command["name"], "deploy");
    assert_eq!(command["type"], 1);
    assert_eq!(command["options"][0]["name"], "version");
    assert_json_fixture("rest_command_create", &command);

    let (interaction_status, interaction_body) = harness
        .bearer_json(
            Method::POST,
            &format!("/channels/{}/command-interactions", space.channel_id),
            &owner.token,
            json!({
                "command_id": command_id,
                "options": [
                    {
                        "name": "version",
                        "value": "1.2.3"
                    }
                ]
            }),
        )
        .await;
    assert_eq!(interaction_status, StatusCode::CREATED);
    let interaction_body = interaction_body.expect("interaction create response");
    let interaction = &interaction_body["interaction"];
    let interaction_id = interaction["id"].as_str().expect("interaction id");
    let interaction_token = interaction["token"].as_str().expect("interaction token");
    assert_uuid_v7_string(interaction_id);
    assert!(interaction_token.starts_with("oci_"));
    assert_eq!(
        interaction["token_last_four"]
            .as_str()
            .expect("token last four"),
        &interaction_token[interaction_token.len() - 4..]
    );
    assert_eq!(interaction["application_id"], bot.application_id);
    assert_eq!(interaction["space_id"], space.space_id);
    assert_eq!(interaction["channel_id"], space.channel_id);
    assert_eq!(interaction["command_id"], command_id);
    assert_eq!(interaction["invoking_user_id"], owner.user_id);
    assert_eq!(interaction["status"], "pending");

    let gateway_interaction = harness.next_gateway_json(&mut gateway).await;
    assert_dispatch(&gateway_interaction, "INTERACTION_CREATE", 4);
    assert_eq!(gateway_interaction["d"]["id"], interaction_id);
    assert_eq!(
        gateway_interaction["d"]["application_id"],
        bot.application_id
    );
    assert_eq!(gateway_interaction["d"]["guild_id"], space.space_id);
    assert_eq!(gateway_interaction["d"]["channel_id"], space.channel_id);
    assert_eq!(
        gateway_interaction["d"]["member"]["user"]["id"],
        owner.user_id
    );
    assert_eq!(gateway_interaction["d"]["data"]["id"], command_id);
    assert_eq!(gateway_interaction["d"]["data"]["name"], "deploy");
    assert_eq!(gateway_interaction["d"]["data"]["type"], 1);
    assert_eq!(
        gateway_interaction["d"]["data"]["options"][0]["name"],
        "version"
    );
    assert_eq!(
        gateway_interaction["d"]["data"]["options"][0]["value"],
        "1.2.3"
    );
    assert_json_fixture("native_interaction_create", &interaction_body);
    assert_json_fixture("gateway_interaction_create", &gateway_interaction);

    let (callback_status, callback_body) = harness
        .json(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/interactions/{interaction_id}/{interaction_token}/callback"
            ),
            json!({
                "type": 4,
                "data": {
                    "content": "deploy queued"
                }
            }),
        )
        .await;
    assert_eq!(callback_status, StatusCode::NO_CONTENT);
    assert!(callback_body.is_none());

    let callback_message = harness.next_gateway_json(&mut gateway).await;
    assert_dispatch(&callback_message, "MESSAGE_CREATE", 5);
    assert_message_contract(
        &callback_message["d"],
        &space.channel_id,
        &bot.bot_user_id,
        "deploy queued",
        true,
        json!([]),
    );
    assert_json_fixture("gateway_callback_message_create", &callback_message);
}

#[tokio::test]
async fn harness_validates_error_and_rate_limit_contracts() {
    let harness = CompatHarness::new();
    let owner = harness.register("compat-contract-errors@example.com").await;
    let space = harness
        .create_space_with_channel(&owner.token, "contract-errors")
        .await;
    let bot = harness
        .create_bot_application(&owner.token, &space.organization_id, "Contract Error Bot")
        .await;
    harness
        .add_space_member(&owner.token, &space.space_id, &bot.bot_user_id, "member")
        .await;

    let (invalid_status, invalid_body) = harness
        .bot_json(
            Method::GET,
            "/api/compat/discord/v10/users/@me",
            "ocb_invalid_contract_token",
            json!({}),
        )
        .await;
    assert_eq!(invalid_status, StatusCode::UNAUTHORIZED);
    assert_json_fixture(
        "error_invalid_bot_token",
        &invalid_body.expect("invalid token error response"),
    );

    let missing_space_id = uuid::Uuid::now_v7().to_string();
    let (missing_space_status, missing_space_body) = harness
        .bot_json(
            Method::GET,
            &format!("/api/compat/discord/v10/guilds/{missing_space_id}"),
            &bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(missing_space_status, StatusCode::NOT_FOUND);
    assert_json_fixture(
        "error_missing_space",
        &missing_space_body.expect("missing space error response"),
    );

    let rate_limited_bot = harness
        .create_bot_application(
            &owner.token,
            &space.organization_id,
            "Contract Rate Limit Bot",
        )
        .await;
    harness
        .add_space_member(
            &owner.token,
            &space.space_id,
            &rate_limited_bot.bot_user_id,
            "member",
        )
        .await;

    let bucket = format!("compat-rest:bot:{}", rate_limited_bot.application_id);
    for expected_remaining in (0..10).rev() {
        let (status, headers, body) = harness
            .bot_json_with_headers(
                Method::GET,
                "/api/compat/discord/v10/users/@me",
                &rate_limited_bot.bot_token,
                json!({}),
            )
            .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body.expect("current user response")["id"],
            rate_limited_bot.bot_user_id
        );
        assert_compat_rate_limit_headers(&headers, "10", &expected_remaining.to_string(), &bucket);
    }

    let (limited_status, limited_headers, limited_body) = harness
        .bot_json_with_headers(
            Method::GET,
            &format!("/api/compat/discord/v10/guilds/{}/channels", space.space_id),
            &rate_limited_bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(limited_status, StatusCode::TOO_MANY_REQUESTS);
    assert_compat_rate_limit_headers(&limited_headers, "10", "0", &bucket);
    assert!(limited_headers.contains_key("retry-after"));
    assert_json_fixture(
        "error_rate_limited",
        &limited_body.expect("rate limit error response"),
    );
}

fn assert_dispatch(event: &Value, event_type: &str, sequence: i64) {
    assert_eq!(event["op"], 0);
    assert_eq!(event["t"], event_type);
    assert_eq!(event["s"], sequence);
}

fn assert_message_contract(
    message: &Value,
    channel_id: &str,
    author_id: &str,
    content: &str,
    author_is_bot: bool,
    expected_embeds: Value,
) {
    assert_uuid_v7_string(message["id"].as_str().expect("message id"));
    assert_eq!(message["channel_id"], channel_id);
    assert_eq!(message["author"]["id"], author_id);
    assert_eq!(message["author"]["bot"], author_is_bot);
    assert_eq!(message["content"], content);
    assert_eq!(message["type"], 0);
    assert_eq!(message["tts"], false);
    assert_eq!(message["mention_everyone"], false);
    assert!(message["mentions"].as_array().expect("mentions").is_empty());
    assert!(
        message["mention_roles"]
            .as_array()
            .expect("mention roles")
            .is_empty()
    );
    assert!(
        message["attachments"]
            .as_array()
            .expect("attachments")
            .is_empty()
    );
    assert_eq!(message["embeds"], expected_embeds);
    assert_eq!(message["pinned"], false);
    assert!(message["timestamp"].as_str().is_some());
}

fn assert_compat_rate_limit_headers(
    headers: &HeaderMap,
    expected_limit: &str,
    expected_remaining: &str,
    expected_bucket: &str,
) {
    assert_eq!(
        headers["x-ratelimit-limit"].to_str().unwrap(),
        expected_limit
    );
    assert_eq!(
        headers["x-ratelimit-remaining"].to_str().unwrap(),
        expected_remaining
    );
    assert_eq!(
        headers["x-ratelimit-bucket"].to_str().unwrap(),
        expected_bucket
    );
    assert!(headers.contains_key("x-ratelimit-reset"));
}

fn assert_json_fixture(name: &str, actual: &Value) {
    let actual = normalize_contract_json(actual);
    let actual_pretty =
        serde_json::to_string_pretty(&actual).expect("serialize normalized actual fixture");
    let path = fixture_path(name);

    if std::env::var_os("OPENCORD_UPDATE_COMPAT_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().expect("fixture parent directory"))
            .expect("create fixture directory");
        std::fs::write(&path, format!("{actual_pretty}\n")).expect("write compatibility fixture");
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "read compatibility fixture {}: {error}; run with OPENCORD_UPDATE_COMPAT_FIXTURES=1",
            path.display()
        )
    });
    let expected: Value = serde_json::from_str(&expected).expect("fixture should be valid JSON");
    let expected_pretty =
        serde_json::to_string_pretty(&expected).expect("serialize normalized expected fixture");

    assert_eq!(
        expected_pretty,
        actual_pretty,
        "compatibility fixture mismatch: {}",
        path.display()
    );
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("compat")
        .join(format!("{name}.json"))
}

fn normalize_contract_json(value: &Value) -> Value {
    normalize_contract_json_with_key(None, value)
}

fn normalize_contract_json_with_key(key: Option<&str>, value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| normalize_contract_json_with_key(None, value))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        normalize_contract_json_with_key(Some(key), value),
                    )
                })
                .collect(),
        ),
        Value::String(value) => Value::String(normalize_contract_string(key, value)),
        _ => value.clone(),
    }
}

fn normalize_contract_string(key: Option<&str>, value: &str) -> String {
    if matches!(
        key,
        Some("timestamp" | "edited_timestamp" | "created_at" | "updated_at")
    ) {
        return "<timestamp>".to_owned();
    }

    if matches!(key, Some("session_id")) {
        return "<session-id>".to_owned();
    }

    if matches!(key, Some("token")) && value.starts_with("oci_") {
        return "<interaction-token>".to_owned();
    }

    if matches!(key, Some("token_last_four")) {
        return "<token-last-four>".to_owned();
    }

    if uuid::Uuid::parse_str(value)
        .map(|id| id.get_version_num() == 7)
        .unwrap_or(false)
    {
        return "<uuid-v7>".to_owned();
    }

    value.to_owned()
}
