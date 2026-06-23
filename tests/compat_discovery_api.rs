mod support;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use support::compat::{CompatHarness, assert_uuid_v7_string};

#[tokio::test]
async fn bot_can_discover_current_user_guild_and_visible_channels() {
    let harness = CompatHarness::new();
    let owner = harness.register("compat-discovery-owner@example.com").await;
    let space = harness
        .create_space_with_channel(&owner.token, "discovery")
        .await;
    let bot = harness
        .create_bot_application(&owner.token, &space.organization_id, "Discovery Bot")
        .await;
    harness
        .add_space_member(&owner.token, &space.space_id, &bot.bot_user_id, "member")
        .await;

    let voice_channel_id = create_channel(
        &harness,
        &owner.token,
        &space.space_id,
        json!({
            "name": "compat voice",
            "kind": "voice",
            "topic": "daily standup"
        }),
    )
    .await;
    let hidden_channel_id = create_channel(
        &harness,
        &owner.token,
        &space.space_id,
        json!({
            "name": "compat hidden",
            "kind": "text"
        }),
    )
    .await;
    let (override_status, _) = harness
        .bearer_json(
            Method::POST,
            &format!("/channels/{hidden_channel_id}/permission-overrides"),
            &owner.token,
            json!({
                "target_kind": "member",
                "target_id": bot.bot_user_id,
                "allow": [],
                "deny": ["VIEW_CHANNEL"]
            }),
        )
        .await;
    assert_eq!(override_status, StatusCode::OK);

    let (me_status, me_body) = harness
        .bot_json(
            Method::GET,
            "/api/compat/discord/v10/users/@me",
            &bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(me_status, StatusCode::OK);
    let me = me_body.expect("current bot user response");
    assert_eq!(me["id"], bot.bot_user_id);
    assert_eq!(me["username"], "Discovery Bot");
    assert_eq!(me["bot"], true);
    assert_uuid_v7_string(me["id"].as_str().expect("current user id"));

    let (guild_status, guild_body) = harness
        .bot_json(
            Method::GET,
            &format!("/api/compat/discord/v10/guilds/{}", space.space_id),
            &bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(guild_status, StatusCode::OK);
    let guild = guild_body.expect("guild response");
    assert_eq!(guild["id"], space.space_id);
    assert_eq!(guild["name"], "Compat Contract Space discovery");
    assert_eq!(guild["unavailable"], false);
    assert_uuid_v7_string(guild["id"].as_str().expect("guild id"));

    let (channels_status, channels_body) = harness
        .bot_json(
            Method::GET,
            &format!("/api/compat/discord/v10/guilds/{}/channels", space.space_id),
            &bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(channels_status, StatusCode::OK);
    let channels = channels_body
        .expect("channels response")
        .as_array()
        .expect("channels array")
        .clone();
    assert_eq!(channels.len(), 2);

    let text = find_channel(&channels, &space.channel_id);
    assert_eq!(text["guild_id"], space.space_id);
    assert_eq!(text["name"], "compat-contract-channel-discovery");
    assert_eq!(text["type"], 0);
    assert_eq!(text["position"], 0);
    assert_eq!(text["nsfw"], false);
    assert_uuid_v7_string(text["id"].as_str().expect("text channel id"));

    let voice = find_channel(&channels, &voice_channel_id);
    assert_eq!(voice["guild_id"], space.space_id);
    assert_eq!(voice["name"], "compat voice");
    assert_eq!(voice["type"], 2);
    assert_eq!(voice["topic"], "daily standup");

    assert!(
        channels
            .iter()
            .all(|channel| channel["id"] != hidden_channel_id)
    );
}

#[tokio::test]
async fn bot_can_discover_guild_roles() {
    let harness = CompatHarness::new();
    let owner = harness.register("compat-roles-owner@example.com").await;
    let space = harness
        .create_space_with_channel(&owner.token, "roles")
        .await;
    let bot = harness
        .create_bot_application(&owner.token, &space.organization_id, "Role Bot")
        .await;
    harness
        .add_space_member(&owner.token, &space.space_id, &bot.bot_user_id, "member")
        .await;

    let moderator_role_id = create_role(
        &harness,
        &owner.token,
        &space.space_id,
        json!({
            "name": "Moderator",
            "color": "#5865f2",
            "position": 10,
            "permissions": ["VIEW_CHANNEL", "MANAGE_MESSAGES"]
        }),
    )
    .await;
    let voice_role_id = create_role(
        &harness,
        &owner.token,
        &space.space_id,
        json!({
            "name": "Voice",
            "position": 5,
            "permissions": ["CONNECT_VOICE", "SPEAK"]
        }),
    )
    .await;

    let (status, body) = harness
        .bot_json(
            Method::GET,
            &format!("/api/compat/discord/v10/guilds/{}/roles", space.space_id),
            &bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let roles = body
        .expect("roles response")
        .as_array()
        .expect("roles array")
        .clone();
    assert_eq!(roles.len(), 2);

    assert_eq!(roles[0]["id"], voice_role_id);
    assert_eq!(roles[0]["name"], "Voice");
    assert_eq!(roles[0]["color"], 0);
    assert_eq!(roles[0]["position"], 5);
    assert_eq!(roles[0]["permissions"], "384");
    assert_eq!(roles[0]["hoist"], false);
    assert_eq!(roles[0]["managed"], false);
    assert_eq!(roles[0]["mentionable"], true);
    assert_uuid_v7_string(roles[0]["id"].as_str().expect("voice role id"));

    assert_eq!(roles[1]["id"], moderator_role_id);
    assert_eq!(roles[1]["name"], "Moderator");
    assert_eq!(roles[1]["color"], 5793266);
    assert_eq!(roles[1]["position"], 10);
    assert_eq!(roles[1]["permissions"], "5");
    assert_eq!(roles[1]["hoist"], false);
    assert_eq!(roles[1]["managed"], false);
    assert_eq!(roles[1]["mentionable"], true);
    assert_uuid_v7_string(roles[1]["id"].as_str().expect("moderator role id"));
}

#[tokio::test]
async fn bot_compat_rest_routes_return_rate_limit_headers_and_429() {
    let harness = CompatHarness::new();
    let owner = harness
        .register("compat-rate-limit-owner@example.com")
        .await;
    let space = harness
        .create_space_with_channel(&owner.token, "rate-limit")
        .await;
    let bot = harness
        .create_bot_application(&owner.token, &space.organization_id, "Limited Bot")
        .await;
    harness
        .add_space_member(&owner.token, &space.space_id, &bot.bot_user_id, "member")
        .await;
    let bucket = format!("compat-rest:bot:{}", bot.application_id);

    for remaining in (0..10).rev() {
        let (status, headers, body) = harness
            .bot_json_with_headers(
                Method::GET,
                "/api/compat/discord/v10/users/@me",
                &bot.bot_token,
                json!({}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.expect("current user response")["id"], bot.bot_user_id);
        assert_eq!(headers["x-ratelimit-limit"].to_str().unwrap(), "10");
        assert_eq!(
            headers["x-ratelimit-remaining"].to_str().unwrap(),
            remaining.to_string()
        );
        assert_eq!(headers["x-ratelimit-bucket"].to_str().unwrap(), bucket);
        assert!(headers.contains_key("x-ratelimit-reset"));
    }

    let (status, headers, body) = harness
        .bot_json_with_headers(
            Method::GET,
            &format!("/api/compat/discord/v10/guilds/{}/channels", space.space_id),
            &bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers["x-ratelimit-limit"].to_str().unwrap(), "10");
    assert_eq!(headers["x-ratelimit-remaining"].to_str().unwrap(), "0");
    assert_eq!(headers["x-ratelimit-bucket"].to_str().unwrap(), bucket);
    assert!(headers.contains_key("x-ratelimit-reset"));
    assert!(headers.contains_key("retry-after"));
    let body = body.expect("rate limited response");
    assert_eq!(body["message"], "rate limit exceeded");
    assert_eq!(body["code"], 42900);
}

#[tokio::test]
async fn guild_discovery_requires_bot_space_membership() {
    let harness = CompatHarness::new();
    let owner = harness
        .register("compat-discovery-nonmember@example.com")
        .await;
    let space = harness
        .create_space_with_channel(&owner.token, "nonmember")
        .await;
    let bot = harness
        .create_bot_application(&owner.token, &space.organization_id, "Outsider Bot")
        .await;

    let (guild_status, guild_body) = harness
        .bot_json(
            Method::GET,
            &format!("/api/compat/discord/v10/guilds/{}", space.space_id),
            &bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(guild_status, StatusCode::NOT_FOUND);
    assert_eq!(
        guild_body.expect("guild error response")["message"],
        "space was not found"
    );

    let (channels_status, channels_body) = harness
        .bot_json(
            Method::GET,
            &format!("/api/compat/discord/v10/guilds/{}/channels", space.space_id),
            &bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(channels_status, StatusCode::NOT_FOUND);
    assert_eq!(
        channels_body.expect("channels error response")["message"],
        "space was not found"
    );

    let (roles_status, roles_body) = harness
        .bot_json(
            Method::GET,
            &format!("/api/compat/discord/v10/guilds/{}/roles", space.space_id),
            &bot.bot_token,
            json!({}),
        )
        .await;
    assert_eq!(roles_status, StatusCode::NOT_FOUND);
    assert_eq!(
        roles_body.expect("roles error response")["message"],
        "space was not found"
    );
}

async fn create_channel(
    harness: &CompatHarness,
    owner_token: &str,
    space_id: &str,
    body: Value,
) -> String {
    let (status, body) = harness
        .bearer_json(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            owner_token,
            body,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    body.expect("channel response")["channel"]["id"]
        .as_str()
        .expect("channel id")
        .to_owned()
}

async fn create_role(
    harness: &CompatHarness,
    owner_token: &str,
    space_id: &str,
    body: Value,
) -> String {
    let (status, body) = harness
        .bearer_json(
            Method::POST,
            &format!("/spaces/{space_id}/roles"),
            owner_token,
            body,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    body.expect("role response")["role"]["id"]
        .as_str()
        .expect("role id")
        .to_owned()
}

fn find_channel<'a>(channels: &'a [Value], channel_id: &str) -> &'a Value {
    channels
        .iter()
        .find(|channel| channel["id"] == channel_id)
        .unwrap_or_else(|| panic!("missing channel {channel_id}"))
}
