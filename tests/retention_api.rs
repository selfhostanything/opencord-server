use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use opencord_server::config::AppConfig;
use opencord_server::domain::ids;
use opencord_server::domain::retention::{RetentionRun, RetentionStore};
use opencord_server::repositories::attachment_memory::MemoryAttachmentStore;
use opencord_server::repositories::audit_memory::MemoryAuditStore;
use opencord_server::repositories::auth_memory::MemoryAuthStore;
use opencord_server::repositories::billing_memory::MemoryBillingStore;
use opencord_server::repositories::bot_memory::MemoryBotStore;
use opencord_server::repositories::calendar_memory::MemoryCalendarStore;
use opencord_server::repositories::channel_memory::MemoryChannelStore;
use opencord_server::repositories::command_memory::MemoryCommandStore;
use opencord_server::repositories::compat_gateway_memory::MemoryCompatGatewaySessionStore;
use opencord_server::repositories::meeting_memory::MemoryMeetingStore;
use opencord_server::repositories::message_memory::MemoryMessageStore;
use opencord_server::repositories::organization_memory::MemoryOrganizationStore;
use opencord_server::repositories::permission_memory::MemoryPermissionStore;
use opencord_server::repositories::push_memory::MemoryPushTokenStore;
use opencord_server::repositories::retention_memory::MemoryRetentionStore;
use opencord_server::repositories::scim_memory::MemoryScimStore;
use opencord_server::repositories::space_memory::MemorySpaceStore;
use opencord_server::repositories::webhook_memory::MemoryIncomingWebhookStore;
use opencord_server::routes::{api_router, api_router_with_state};
use opencord_server::state::{AppState, AppStores};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn test_app() -> axum::Router {
    api_router(AppConfig {
        version: "test-version".to_owned(),
        public_url: "https://chat.example.com".to_owned(),
    })
}

fn test_app_with_retention_store() -> (axum::Router, Arc<MemoryRetentionStore>) {
    let retention = Arc::new(MemoryRetentionStore::default());
    let state = AppState::with_stores(
        AppConfig {
            version: "test-version".to_owned(),
            public_url: "https://chat.example.com".to_owned(),
        },
        AppStores {
            auth: Arc::new(MemoryAuthStore::default()),
            organizations: Arc::new(MemoryOrganizationStore::default()),
            spaces: Arc::new(MemorySpaceStore::default()),
            channels: Arc::new(MemoryChannelStore::default()),
            messages: Arc::new(MemoryMessageStore::default()),
            meetings: Arc::new(MemoryMeetingStore::default()),
            calendar: Arc::new(MemoryCalendarStore::default()),
            attachments: Arc::new(MemoryAttachmentStore::default()),
            audit: Arc::new(MemoryAuditStore::default()),
            permissions: Arc::new(MemoryPermissionStore::default()),
            push: Arc::new(MemoryPushTokenStore::default()),
            billing: Arc::new(MemoryBillingStore::default()),
            scim: Arc::new(MemoryScimStore::default()),
            retention: retention.clone(),
            bots: Arc::new(MemoryBotStore::default()),
            webhooks: Arc::new(MemoryIncomingWebhookStore::default()),
            commands: Arc::new(MemoryCommandStore::default()),
            compat_gateway_sessions: Arc::new(MemoryCompatGatewaySessionStore::default()),
        },
    );

    (api_router_with_state(state), retention)
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
                "display_name": "Retention Test User",
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

async fn create_space(app: &axum::Router, owner_token: &str, suffix: &str) -> (String, String) {
    let org = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            owner_token,
            json!({ "name": format!("Retention Org {suffix}") }),
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
            json!({ "name": format!("Retention Space {suffix}") }),
        ))
        .await
        .unwrap();
    assert_eq!(space.status(), StatusCode::CREATED);
    let space_id = response_json(space).await["space"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    (organization_id, space_id)
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
async fn organization_admin_can_configure_retention_policy() {
    let app = test_app();
    let (owner_token, _) = register(&app, "retention-owner@example.com").await;
    let (member_token, member_id) = register(&app, "retention-member@example.com").await;
    let (organization_id, space_id) = create_space(&app, &owner_token, "policy").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let configured = app
        .clone()
        .oneshot(bearer_request(
            Method::PUT,
            &format!("/organizations/{organization_id}/retention-policy"),
            &owner_token,
            json!({
                "messages_retain_days": 365,
                "files_retain_days": 90,
                "audit_logs_retain_days": 730,
                "deleted_message_purge_days": 30
            }),
        ))
        .await
        .unwrap();
    assert_eq!(configured.status(), StatusCode::OK);
    let body = response_json(configured).await;
    assert_eq!(body["policy"]["organization_id"], organization_id);
    assert_eq!(body["policy"]["messages_retain_days"], 365);
    assert_eq!(body["policy"]["files_retain_days"], 90);
    assert_eq!(body["policy"]["audit_logs_retain_days"], 730);
    assert_eq!(body["policy"]["deleted_message_purge_days"], 30);

    let fetched = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/retention-policy"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    let body = response_json(fetched).await;
    assert_eq!(body["policy"]["messages_retain_days"], 365);

    let forbidden = app
        .oneshot(bearer_request(
            Method::PUT,
            &format!("/organizations/{organization_id}/retention-policy"),
            &member_token,
            json!({
                "messages_retain_days": 1,
                "files_retain_days": 1,
                "audit_logs_retain_days": 1,
                "deleted_message_purge_days": 1
            }),
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn organization_admin_can_list_retention_run_history() {
    let (app, retention_store) = test_app_with_retention_store();
    let (owner_token, _) = register(&app, "retention-runs-owner@example.com").await;
    let (member_token, member_id) = register(&app, "retention-runs-member@example.com").await;
    let (organization_id, space_id) = create_space(&app, &owner_token, "runs").await;
    add_space_member(&app, &owner_token, &space_id, &member_id).await;
    let organization_uuid = Uuid::parse_str(&organization_id).unwrap();

    retention_store
        .record_run(RetentionRun {
            id: ids::new_uuid_v7(),
            organization_id: organization_uuid,
            dry_run: true,
            messages_purged: 3,
            files_purged: 2,
            audit_events_purged: 1,
            ran_at: "2026-06-23T10:00:00.000Z".to_owned(),
        })
        .await
        .expect("first run should save");
    retention_store
        .record_run(RetentionRun {
            id: ids::new_uuid_v7(),
            organization_id: organization_uuid,
            dry_run: false,
            messages_purged: 4,
            files_purged: 1,
            audit_events_purged: 0,
            ran_at: "2026-06-23T11:00:00.000Z".to_owned(),
        })
        .await
        .expect("second run should save");

    let listed = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/retention-runs"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    let runs = body["retention_runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["organization_id"], organization_id);
    assert_eq!(runs[0]["dry_run"], true);
    assert_eq!(runs[0]["messages_purged"], 3);
    assert_eq!(runs[0]["files_purged"], 2);
    assert_eq!(runs[0]["audit_events_purged"], 1);
    assert_eq!(runs[0]["ran_at"], "2026-06-23T10:00:00.000Z");
    assert_eq!(runs[1]["dry_run"], false);
    assert_eq!(runs[1]["messages_purged"], 4);

    let forbidden = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/retention-runs"),
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}
