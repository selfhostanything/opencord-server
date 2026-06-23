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

async fn response_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    String::from_utf8(bytes.to_vec()).expect("response should be utf-8")
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

async fn register(app: &axum::Router, email: &str) -> String {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Calendar Test User",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["session"]["token"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create_organization(app: &axum::Router, token: &str, name: &str) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            token,
            json!({ "name": name }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["organization"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create_meeting(app: &axum::Router, token: &str, organization_id: &str) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/meetings"),
            token,
            json!({
                "title": "Google Sync Review",
                "description": "Calendar provider sync",
                "starts_at": "2026-06-24T09:00:00Z",
                "ends_at": "2026-06-24T09:30:00Z",
                "timezone": "Asia/Bangkok",
                "attendees": [
                    {
                        "email": "external@example.com",
                        "display_name": "External Guest",
                        "role": "required"
                    }
                ]
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["meeting"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn google_calendar_connected_user_can_create_and_update_provider_event() {
    let app = test_app();
    let token = register(&app, "calendar-owner@example.com").await;
    let organization_id = create_organization(&app, &token, "Calendar Parent").await;
    let meeting_id = create_meeting(&app, &token, &organization_id).await;

    let connected = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/calendar/accounts/google",
            &token,
            json!({
                "external_account_id": "google-user-123",
                "calendar_id": "primary",
                "access_token": "ya29.access-secret",
                "refresh_token": "1//refresh-secret"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(connected.status(), StatusCode::CREATED);
    let body = response_json(connected).await;
    assert_eq!(body["account"]["provider"], "google");
    assert_eq!(body["account"]["external_account_id"], "google-user-123");
    assert_eq!(body["account"]["calendar_id"], "primary");
    assert_eq!(body["account"]["sync_enabled"], true);
    assert!(body["account"].get("access_token").is_none());
    assert!(body["account"].get("refresh_token").is_none());

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/meetings/{meeting_id}/calendar/google/sync"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let body = response_json(created).await;
    assert_eq!(body["calendar_event"]["provider"], "google");
    assert_eq!(body["calendar_event"]["operation"], "created");
    assert_eq!(body["calendar_event"]["meeting_id"], meeting_id);
    assert_eq!(body["calendar_event"]["calendar_id"], "primary");
    assert_eq!(body["calendar_event"]["status"], "synced");
    let provider_event_id = body["calendar_event"]["provider_event_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(provider_event_id.starts_with("google-"));

    let updated_meeting = app
        .clone()
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/meetings/{meeting_id}"),
            &token,
            json!({
                "title": "Google Sync Review Updated",
                "ends_at": "2026-06-24T10:00:00Z"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(updated_meeting.status(), StatusCode::OK);

    let updated = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/meetings/{meeting_id}/calendar/google/sync"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let raw_body = response_text(updated).await;
    assert!(!raw_body.contains("ya29.access-secret"));
    assert!(!raw_body.contains("1//refresh-secret"));
    let body: Value =
        serde_json::from_str(&raw_body).expect("calendar sync response should be json");
    assert_eq!(body["calendar_event"]["operation"], "updated");
    assert_eq!(
        body["calendar_event"]["provider_event_id"],
        provider_event_id
    );
    assert_eq!(body["calendar_event"]["status"], "synced");
    assert!(
        body["calendar_event"]["provider_event_url"]
            .as_str()
            .is_some()
    );
}

#[tokio::test]
async fn google_calendar_sync_requires_connected_account() {
    let app = test_app();
    let token = register(&app, "calendar-missing-account@example.com").await;
    let organization_id = create_organization(&app, &token, "Calendar Missing Parent").await;
    let meeting_id = create_meeting(&app, &token, &organization_id).await;

    let response = app
        .oneshot(bearer_request(
            Method::POST,
            &format!("/meetings/{meeting_id}/calendar/google/sync"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(response).await["error"]["message"],
        "google calendar account is not connected"
    );
}

#[tokio::test]
async fn microsoft_calendar_connected_user_can_create_and_update_provider_event() {
    let app = test_app();
    let token = register(&app, "calendar-microsoft-owner@example.com").await;
    let organization_id = create_organization(&app, &token, "Microsoft Calendar Parent").await;
    let meeting_id = create_meeting(&app, &token, &organization_id).await;

    let connected = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/calendar/accounts/microsoft",
            &token,
            json!({
                "external_account_id": "microsoft-user-123",
                "calendar_id": "calendar",
                "access_token": "microsoft-access-secret",
                "refresh_token": "microsoft-refresh-secret"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(connected.status(), StatusCode::CREATED);
    let body = response_json(connected).await;
    assert_eq!(body["account"]["provider"], "microsoft");
    assert_eq!(body["account"]["external_account_id"], "microsoft-user-123");
    assert_eq!(body["account"]["calendar_id"], "calendar");
    assert!(body["account"].get("access_token").is_none());
    assert!(body["account"].get("refresh_token").is_none());

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/meetings/{meeting_id}/calendar/microsoft/sync"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let body = response_json(created).await;
    assert_eq!(body["calendar_event"]["provider"], "microsoft");
    assert_eq!(body["calendar_event"]["operation"], "created");
    assert_eq!(body["calendar_event"]["meeting_id"], meeting_id);
    assert_eq!(body["calendar_event"]["calendar_id"], "calendar");
    assert_eq!(body["calendar_event"]["status"], "synced");
    let provider_event_id = body["calendar_event"]["provider_event_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(provider_event_id.starts_with("microsoft-"));

    let updated_meeting = app
        .clone()
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/meetings/{meeting_id}"),
            &token,
            json!({
                "title": "Microsoft Sync Review Updated",
                "ends_at": "2026-06-24T10:00:00Z"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(updated_meeting.status(), StatusCode::OK);

    let updated = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/meetings/{meeting_id}/calendar/microsoft/sync"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let raw_body = response_text(updated).await;
    assert!(!raw_body.contains("microsoft-access-secret"));
    assert!(!raw_body.contains("microsoft-refresh-secret"));
    let body: Value =
        serde_json::from_str(&raw_body).expect("calendar sync response should be json");
    assert_eq!(body["calendar_event"]["operation"], "updated");
    assert_eq!(
        body["calendar_event"]["provider_event_id"],
        provider_event_id
    );
    assert_eq!(body["calendar_event"]["status"], "synced");
    assert!(
        body["calendar_event"]["provider_event_url"]
            .as_str()
            .unwrap()
            .contains("outlook.office.com")
    );
}

#[tokio::test]
async fn microsoft_calendar_sync_requires_connected_account() {
    let app = test_app();
    let token = register(&app, "calendar-microsoft-missing@example.com").await;
    let organization_id = create_organization(&app, &token, "Microsoft Missing Parent").await;
    let meeting_id = create_meeting(&app, &token, &organization_id).await;

    let response = app
        .oneshot(bearer_request(
            Method::POST,
            &format!("/meetings/{meeting_id}/calendar/microsoft/sync"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(response).await["error"]["message"],
        "microsoft calendar account is not connected"
    );
}
