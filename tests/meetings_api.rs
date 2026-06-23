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

async fn register(app: &axum::Router, email: &str) -> (String, String) {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Meeting Test User",
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

async fn create_space(
    app: &axum::Router,
    token: &str,
    organization_id: &str,
    name: &str,
) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            token,
            json!({ "name": name }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["space"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn create_channel(app: &axum::Router, token: &str, space_id: &str, name: &str) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/spaces/{space_id}/channels"),
            token,
            json!({ "name": name }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["channel"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn organization_member_can_create_list_update_and_cancel_meeting() {
    let app = test_app();
    let (token, user_id) = register(&app, "meeting-owner@example.com").await;
    let organization_id = create_organization(&app, &token, "Meeting Parent").await;
    let space_id = create_space(&app, &token, &organization_id, "Meeting Space").await;
    let channel_id = create_channel(&app, &token, &space_id, "Calendar Chat").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/meetings"),
            &token,
            json!({
                "space_id": space_id,
                "channel_id": channel_id,
                "title": "Roadmap Review",
                "description": "Discuss launch scope",
                "starts_at": "2026-06-24T09:00:00Z",
                "ends_at": "2026-06-24T09:30:00Z",
                "timezone": "Asia/Bangkok",
                "attendees": [
                    {
                        "email": "external@example.com",
                        "display_name": "External Guest",
                        "role": "required"
                    }
                ],
                "reminders": [
                    {
                        "recipient_email": "external@example.com",
                        "channel": "email",
                        "offset_minutes": 10
                    }
                ]
            }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let meeting_id = body["meeting"]["id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(meeting_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(body["meeting"]["organization_id"], organization_id);
    assert_eq!(body["meeting"]["space_id"], space_id);
    assert_eq!(body["meeting"]["channel_id"], channel_id);
    assert_eq!(body["meeting"]["created_by_user_id"], user_id);
    assert_eq!(body["meeting"]["title"], "Roadmap Review");
    assert_eq!(body["meeting"]["description"], "Discuss launch scope");
    assert_eq!(body["meeting"]["status"], "scheduled");
    assert_eq!(body["meeting"]["starts_at"], "2026-06-24T09:00:00Z");
    assert_eq!(body["meeting"]["ends_at"], "2026-06-24T09:30:00Z");
    assert_eq!(body["meeting"]["timezone"], "Asia/Bangkok");
    assert!(
        body["meeting"]["join_slug"]
            .as_str()
            .unwrap()
            .starts_with("mtg-")
    );
    assert_eq!(
        body["meeting"]["attendees"][0]["email"],
        "external@example.com"
    );
    assert_eq!(
        body["meeting"]["attendees"][0]["response_status"],
        "needs_action"
    );
    assert_eq!(body["meeting"]["reminders"][0]["channel"], "email");
    assert_eq!(body["meeting"]["reminders"][0]["offset_minutes"], 10);
    assert_eq!(
        body["meeting"]["reminders"][0]["scheduled_for"],
        "2026-06-24T08:50:00Z"
    );

    let listed = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/meetings"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    assert_eq!(body["meetings"].as_array().unwrap().len(), 1);
    assert_eq!(body["meetings"][0]["id"], meeting_id);

    let fetched = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/meetings/{meeting_id}"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    assert_eq!(response_json(fetched).await["meeting"]["id"], meeting_id);

    let updated = app
        .clone()
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/meetings/{meeting_id}"),
            &token,
            json!({
                "title": "Roadmap Review Updated",
                "description": "Updated scope",
                "ends_at": "2026-06-24T10:00:00Z"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let body = response_json(updated).await;
    assert_eq!(body["meeting"]["title"], "Roadmap Review Updated");
    assert_eq!(body["meeting"]["description"], "Updated scope");
    assert_eq!(body["meeting"]["ends_at"], "2026-06-24T10:00:00Z");

    let cancelled = app
        .oneshot(bearer_request(
            Method::DELETE,
            &format!("/meetings/{meeting_id}"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(cancelled.status(), StatusCode::OK);
    let body = response_json(cancelled).await;
    assert_eq!(body["meeting"]["status"], "cancelled");
    assert!(body["meeting"]["cancelled_at"].as_str().is_some());
}

#[tokio::test]
async fn meeting_endpoints_require_bearer_auth_and_membership() {
    let app = test_app();
    let (owner_token, _) = register(&app, "meeting-auth-owner@example.com").await;
    let (outsider_token, _) = register(&app, "meeting-auth-outsider@example.com").await;
    let organization_id = create_organization(&app, &owner_token, "Meeting Auth Parent").await;

    let missing_auth = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/organizations/{organization_id}/meetings"),
            json!({
                "title": "No Auth",
                "starts_at": "2026-06-24T09:00:00Z",
                "ends_at": "2026-06-24T09:30:00Z"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(missing_auth.status(), StatusCode::UNAUTHORIZED);

    let outsider_list = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/meetings"),
            &outsider_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_list.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn meeting_join_url_resolves_to_visible_meeting() {
    let app = test_app();
    let (token, _) = register(&app, "meeting-link-owner@example.com").await;
    let (outsider_token, _) = register(&app, "meeting-link-outsider@example.com").await;
    let organization_id = create_organization(&app, &token, "Meeting Link Parent").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/meetings"),
            &token,
            json!({
                "title": "Join Link Meeting",
                "starts_at": "2026-06-24T09:00:00Z",
                "ends_at": "2026-06-24T09:30:00Z"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let meeting_id = body["meeting"]["id"].as_str().unwrap();
    let join_slug = body["meeting"]["join_slug"].as_str().unwrap();
    assert_eq!(
        body["meeting"]["join_url"],
        format!("https://chat.example.com/join/{join_slug}")
    );

    let resolved = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/join/{join_slug}"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(resolved.status(), StatusCode::OK);
    assert_eq!(response_json(resolved).await["meeting"]["id"], meeting_id);

    let outsider = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/join/{join_slug}"),
            &outsider_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(outsider.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn meeting_invite_ics_contains_schedule_attendee_and_join_url() {
    let app = test_app();
    let (token, _) = register(&app, "meeting-ics-owner@example.com").await;
    let organization_id = create_organization(&app, &token, "Meeting ICS Parent").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/meetings"),
            &token,
            json!({
                "title": "Roadmap, Review",
                "description": "Discuss launch; scope",
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
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let meeting_id = body["meeting"]["id"].as_str().unwrap();
    let join_slug = body["meeting"]["join_slug"].as_str().unwrap();
    let join_url = format!("https://chat.example.com/join/{join_slug}");

    let response = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/meetings/{meeting_id}/invite.ics"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/calendar; charset=utf-8"
    );
    let body = response_text(response).await;
    assert!(body.starts_with("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n"));
    assert!(body.contains("PRODID:-//OpenCord//OpenCord Calendar//EN\r\n"));
    assert!(body.contains("BEGIN:VEVENT\r\n"));
    assert!(body.contains(&format!("UID:{meeting_id}@opencord\r\n")));
    assert!(body.contains("DTSTART:20260624T090000Z\r\n"));
    assert!(body.contains("DTEND:20260624T093000Z\r\n"));
    assert!(body.contains("SUMMARY:Roadmap\\, Review\r\n"));
    assert!(body.contains(&format!(
        "DESCRIPTION:Discuss launch\\; scope\\n\\nJoin: {join_url}\r\n"
    )));
    assert!(body.contains(&format!("LOCATION:{join_url}\r\n")));
    assert!(body.contains(&format!("URL:{join_url}\r\n")));
    assert!(body.contains(
        "ATTENDEE;ROLE=REQ-PARTICIPANT;CN=\"External Guest\":mailto:external@example.com\r\n"
    ));
    assert!(body.ends_with("END:VEVENT\r\nEND:VCALENDAR\r\n"));
}

#[tokio::test]
async fn meeting_create_validates_schedule() {
    let app = test_app();
    let (token, _) = register(&app, "meeting-validation-owner@example.com").await;
    let organization_id = create_organization(&app, &token, "Meeting Validation Parent").await;

    let response = app
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/meetings"),
            &token,
            json!({
                "title": "Invalid Meeting",
                "starts_at": "2026-06-24T10:00:00Z",
                "ends_at": "2026-06-24T09:00:00Z"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(response).await["error"]["message"],
        "meeting end time must be after start time"
    );
}
