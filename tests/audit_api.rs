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

async fn register(app: &axum::Router, email: &str) -> (String, String) {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Audit Test User",
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
    suffix: &str,
) -> (String, String, String) {
    let org = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            owner_token,
            json!({ "name": format!("Audit Org {suffix}") }),
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
            json!({ "name": format!("Audit Space {suffix}") }),
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
            json!({ "name": format!("audit-channel-{suffix}") }),
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
async fn admin_actions_write_space_audit_events() {
    let app = test_app();
    let (owner_token, owner_id) = register(&app, "audit-owner@example.com").await;
    let (member_token, member_id) = register(&app, "audit-member@example.com").await;
    let (organization_id, space_id, channel_id) =
        create_space_with_channel(&app, &owner_token, "events").await;

    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let role = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/roles"),
            &owner_token,
            json!({
                "name": "Auditors",
                "permissions": ["MANAGE_MESSAGES"]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(role.status(), StatusCode::CREATED);
    let role_id = response_json(role).await["role"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let assignment = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/roles/{role_id}/assignments"),
            &owner_token,
            json!({ "user_id": member_id }),
        ))
        .await
        .unwrap();
    assert_eq!(assignment.status(), StatusCode::CREATED);

    let permission_override = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/channels/{channel_id}/permission-overrides"),
            &owner_token,
            json!({
                "target_kind": "member",
                "target_id": member_id,
                "allow": [],
                "deny": ["SEND_MESSAGES"]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(permission_override.status(), StatusCode::OK);

    let audit = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/spaces/{space_id}/audit-events"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(audit.status(), StatusCode::OK);
    let body = response_json(audit).await;
    let events = body["audit_events"].as_array().unwrap();
    assert_eq!(events.len(), 4);

    let actions = events
        .iter()
        .map(|event| event["action"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        actions,
        vec![
            "space.member.added",
            "role.created",
            "role.assigned",
            "channel.permission_override.set"
        ]
    );

    let first = &events[0];
    assert_eq!(first["organization_id"], organization_id);
    assert_eq!(first["space_id"], space_id);
    assert_eq!(first["actor_user_id"], owner_id);
    assert_eq!(first["target_type"], "user");
    assert_eq!(first["target_id"], member_id);
    assert_eq!(first["metadata"]["role"], "member");
    assert_eq!(
        uuid::Uuid::parse_str(first["id"].as_str().unwrap())
            .unwrap()
            .get_version_num(),
        7
    );
    assert!(first["created_at"].as_str().is_some());

    let denied_member_read = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/spaces/{space_id}/audit-events"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(denied_member_read.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn organization_admin_can_export_audit_events_by_date_range() {
    let app = test_app();
    let (owner_token, _) = register(&app, "audit-export-owner@example.com").await;
    let (member_token, member_id) = register(&app, "audit-export-member@example.com").await;
    let (organization_id, space_id, _) =
        create_space_with_channel(&app, &owner_token, "export").await;

    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let exported = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!(
                "/organizations/{organization_id}/audit-events/export?from=2020-01-01T00:00:00Z&to=2030-01-01T00:00:00Z"
            ),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(exported.status(), StatusCode::OK);
    let body = response_json(exported).await;
    assert_eq!(body["export"]["organization_id"], organization_id);
    assert_eq!(body["export"]["format"], "json");
    assert_eq!(body["export"]["audit_events"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["export"]["audit_events"][0]["action"],
        "space.member.added"
    );

    let forbidden = app
        .oneshot(bearer_request(
            Method::GET,
            &format!(
                "/organizations/{organization_id}/audit-events/export?from=2020-01-01T00:00:00Z&to=2030-01-01T00:00:00Z"
            ),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn audit_events_require_bearer_auth() {
    let response = test_app()
        .oneshot(json_request(
            Method::GET,
            &format!("/spaces/{}/audit-events", uuid::Uuid::now_v7()),
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
