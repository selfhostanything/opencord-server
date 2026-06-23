use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use opencord_server::config::AppConfig;
use opencord_server::routes::api_router;
use serde_json::{Value, json};
use tower::ServiceExt;

fn test_app() -> axum::Router {
    api_router(AppConfig {
        version: "test-version".to_owned(),
        public_url: "https://chat.example.com".to_owned(),
    })
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    serde_json::from_slice(&bytes).expect("response should be json")
}

fn json_request(method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn bearer_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn bot_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bot {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn register(app: &axum::Router, email: &str) -> (String, String) {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Command Test User",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    (
        body["session"]["token"].as_str().unwrap().to_owned(),
        body["user"]["id"].as_str().unwrap().to_owned(),
    )
}

async fn create_space_with_channel(
    app: &axum::Router,
    owner_token: &str,
) -> (String, String, String) {
    let org = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            owner_token,
            json!({ "name": "Command Org" }),
        ))
        .await
        .unwrap();
    assert_eq!(org.status(), StatusCode::CREATED);
    let organization_id = response_json(org).await["organization"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let space = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            owner_token,
            json!({ "name": "Command Space" }),
        ))
        .await
        .unwrap();
    assert_eq!(space.status(), StatusCode::CREATED);
    let space_id = response_json(space).await["space"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let channel = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            owner_token,
            json!({ "name": "commands" }),
        ))
        .await
        .unwrap();
    assert_eq!(channel.status(), StatusCode::CREATED);
    let channel_id = response_json(channel).await["channel"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    (organization_id, space_id, channel_id)
}

async fn create_bot(
    app: &axum::Router,
    owner_token: &str,
    organization_id: &str,
) -> (String, String, String) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/bot-applications"),
            owner_token,
            json!({
                "name": "Command Bot",
                "description": "Exercises slash command flows"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    (
        body["bot_application"]["id"].as_str().unwrap().to_owned(),
        body["bot_token"]["token"].as_str().unwrap().to_owned(),
        body["bot_application"]["bot_user_id"]
            .as_str()
            .unwrap()
            .to_owned(),
    )
}

async fn add_space_member(app: &axum::Router, owner_token: &str, space_id: &str, user_id: &str) {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/members"),
            owner_token,
            json!({
                "user_id": user_id,
                "role": "member"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn bot_registers_space_command_and_responds_to_interaction() {
    let app = test_app();
    let (owner_token, owner_id) = register(&app, "command-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token).await;
    let (application_id, bot_token, bot_user_id) =
        create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

    let created_command = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/applications/{application_id}/guilds/{space_id}/commands"
            ),
            &bot_token,
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
        ))
        .await
        .unwrap();

    assert_eq!(created_command.status(), StatusCode::CREATED);
    assert_eq!(
        created_command
            .headers()
            .get("x-ratelimit-limit")
            .expect("rate limit")
            .to_str()
            .unwrap(),
        "10"
    );
    assert_eq!(
        created_command
            .headers()
            .get("x-ratelimit-remaining")
            .expect("rate limit remaining")
            .to_str()
            .unwrap(),
        "9"
    );
    assert_eq!(
        created_command
            .headers()
            .get("x-ratelimit-bucket")
            .expect("rate limit bucket")
            .to_str()
            .unwrap(),
        format!("compat-rest:bot:{application_id}")
    );
    let command = response_json(created_command).await;
    let command_id = command["id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(command_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(command["application_id"], application_id);
    assert_eq!(command["guild_id"], space_id);
    assert_eq!(command["name"], "deploy");
    assert_eq!(command["description"], "Deploy a release");
    assert_eq!(command["type"], 1);
    assert_eq!(command["options"][0]["name"], "version");

    let interaction = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/command-interactions"),
            &owner_token,
            json!({
                "command_id": command_id,
                "options": [
                    {
                        "name": "version",
                        "value": "1.2.3"
                    }
                ]
            }),
        ))
        .await
        .unwrap();

    assert_eq!(interaction.status(), StatusCode::CREATED);
    let interaction = response_json(interaction).await["interaction"].clone();
    let interaction_id = interaction["id"].as_str().unwrap();
    let interaction_token = interaction["token"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(interaction_id)
            .unwrap()
            .get_version_num(),
        7
    );
    assert!(interaction_token.starts_with("oci_"));
    assert_eq!(interaction["application_id"], application_id);
    assert_eq!(interaction["command_id"], command_id);
    assert_eq!(interaction["channel_id"], channel_id);
    assert_eq!(interaction["invoking_user_id"], owner_id);
    assert_eq!(interaction["status"], "pending");

    let callback = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/interactions/{interaction_id}/{interaction_token}/callback"
            ),
            json!({
                "type": 4,
                "data": {
                    "content": "Deploying 1.2.3"
                }
            }),
        ))
        .await
        .unwrap();

    assert_eq!(callback.status(), StatusCode::NO_CONTENT);

    let messages = app
        .clone()
        .oneshot(bot_request(
            Method::GET,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(messages.status(), StatusCode::OK);
    let messages = response_json(messages).await;
    assert_eq!(messages.as_array().unwrap().len(), 1);
    assert_eq!(messages[0]["content"], "Deploying 1.2.3");
    assert_eq!(messages[0]["author"]["id"], bot_user_id);
    assert_eq!(messages[0]["author"]["bot"], true);
}

#[tokio::test]
async fn command_registration_requires_matching_bot_application_and_space_membership() {
    let app = test_app();
    let (owner_token, _) = register(&app, "command-private-owner@example.com").await;
    let (organization_id, space_id, _) = create_space_with_channel(&app, &owner_token).await;
    let (application_id, bot_token, _) = create_bot(&app, &owner_token, &organization_id).await;

    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/applications/{application_id}/guilds/{space_id}/commands"
            ),
            &bot_token,
            json!({
                "name": "deploy",
                "description": "Deploy a release",
                "type": 1
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/applications/{}/guilds/{space_id}/commands",
                uuid::Uuid::now_v7()
            ),
            &bot_token,
            json!({
                "name": "deploy",
                "description": "Deploy a release",
                "type": 1
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bot_defers_interaction_and_sends_followup_message() {
    let app = test_app();
    let (owner_token, _) = register(&app, "command-deferred-owner@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token).await;
    let (application_id, bot_token, bot_user_id) =
        create_bot(&app, &owner_token, &organization_id).await;
    add_space_member(&app, &owner_token, &space_id, &bot_user_id).await;

    let created_command = app
        .clone()
        .oneshot(bot_request(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/applications/{application_id}/guilds/{space_id}/commands"
            ),
            &bot_token,
            json!({
                "name": "report",
                "description": "Generate a report",
                "type": 1
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created_command.status(), StatusCode::CREATED);
    let command_id = response_json(created_command).await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let interaction = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/command-interactions"),
            &owner_token,
            json!({
                "command_id": command_id,
                "options": []
            }),
        ))
        .await
        .unwrap();
    assert_eq!(interaction.status(), StatusCode::CREATED);
    let interaction = response_json(interaction).await["interaction"].clone();
    let interaction_id = interaction["id"].as_str().unwrap();
    let interaction_token = interaction["token"].as_str().unwrap();

    let deferred = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!(
                "/api/compat/discord/v10/interactions/{interaction_id}/{interaction_token}/callback"
            ),
            json!({
                "type": 5
            }),
        ))
        .await
        .unwrap();
    assert_eq!(deferred.status(), StatusCode::NO_CONTENT);

    let followup = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/compat/discord/v10/webhooks/{application_id}/{interaction_token}"),
            json!({
                "content": "Report is ready"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(followup.status(), StatusCode::OK);
    let followup = response_json(followup).await;
    assert_eq!(followup["content"], "Report is ready");
    assert_eq!(followup["author"]["id"], bot_user_id);
    assert_eq!(followup["author"]["bot"], true);

    let second_followup = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/compat/discord/v10/webhooks/{application_id}/{interaction_token}"),
            json!({
                "content": "Report is ready again"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(second_followup.status(), StatusCode::CONFLICT);

    let messages = app
        .clone()
        .oneshot(bot_request(
            Method::GET,
            &format!("/api/compat/discord/v10/channels/{channel_id}/messages"),
            &bot_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(messages.status(), StatusCode::OK);
    let messages = response_json(messages).await;
    assert_eq!(messages.as_array().unwrap().len(), 1);
    assert_eq!(messages[0]["content"], "Report is ready");
    assert_eq!(messages[0]["author"]["id"], bot_user_id);
}
