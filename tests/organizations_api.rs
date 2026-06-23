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

async fn register_token(app: &axum::Router, email: &str) -> String {
    register_user(app, email).await.0
}

async fn register_user(app: &axum::Router, email: &str) -> (String, String) {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Org Test User",
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

#[tokio::test]
async fn user_can_create_list_and_get_organization() {
    let app = test_app();
    let token = register_token(&app, "owner@example.com").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            &token,
            json!({ "name": "Acme Research" }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let organization_id = body["organization"]["id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(organization_id)
            .unwrap()
            .get_version_num(),
        7
    );
    assert_eq!(body["organization"]["name"], "Acme Research");
    assert_eq!(body["organization"]["slug"], "acme-research");
    assert_eq!(body["membership"]["role"], "owner");

    let list = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            "/organizations",
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body = response_json(list).await;
    assert_eq!(body["organizations"].as_array().unwrap().len(), 1);
    assert_eq!(body["organizations"][0]["id"], organization_id);
    assert_eq!(body["organizations"][0]["role"], "owner");

    let get = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let body = response_json(get).await;
    assert_eq!(body["organization"]["id"], organization_id);
}

#[tokio::test]
async fn cloud_can_provision_tenant_with_plan_region_and_owner_atomically() {
    let app = test_app();
    let (token, user_id) = register_user(&app, "tenant-owner@example.com").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/cloud/tenants",
            &token,
            json!({
                "name": "Acme Cloud",
                "plan": "team",
                "deployment_mode": "cloud",
                "primary_region": "vultr-sgp"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let tenant = &body["tenant"];
    let organization_id = tenant["organization_id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(organization_id)
            .unwrap()
            .get_version_num(),
        7
    );
    assert_eq!(tenant["owner_user_id"], user_id);
    assert_eq!(tenant["slug"], "acme-cloud");
    assert_eq!(tenant["name"], "Acme Cloud");
    assert_eq!(tenant["plan"], "team");
    assert_eq!(tenant["deployment_mode"], "cloud");
    assert_eq!(tenant["primary_region"], "vultr-sgp");

    let listed = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            "/organizations",
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let body = response_json(listed).await;
    assert_eq!(body["organizations"][0]["id"], organization_id);
    assert_eq!(body["organizations"][0]["plan"], "team");
    assert_eq!(body["organizations"][0]["deployment_mode"], "cloud");
    assert_eq!(body["organizations"][0]["primary_region"], "vultr-sgp");

    let duplicate = app
        .oneshot(bearer_request(
            Method::POST,
            "/cloud/tenants",
            &token,
            json!({
                "name": "Acme Cloud",
                "plan": "enterprise",
                "deployment_mode": "cloud",
                "primary_region": "vultr-ewr"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn organization_usage_shows_active_users_storage_and_calendar_accounts() {
    let app = test_app();
    let (token, _) = register_user(&app, "usage-owner@example.com").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/cloud/tenants",
            &token,
            json!({
                "name": "Usage Cloud",
                "plan": "team",
                "deployment_mode": "cloud",
                "primary_region": "vultr-sgp"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let organization_id = response_json(created).await["tenant"]["organization_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let initial_usage = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/usage"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(initial_usage.status(), StatusCode::OK);
    let body = response_json(initial_usage).await;
    assert_eq!(body["usage"]["organization_id"], organization_id);
    assert_eq!(body["usage"]["active_users"], 1);
    assert_eq!(body["usage"]["stored_file_bytes"], 0);
    assert_eq!(body["usage"]["calendar_connected_accounts"], 0);

    let connected_calendar = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/calendar/accounts/google",
            &token,
            json!({
                "external_account_id": "usage-google-user",
                "calendar_id": "primary",
                "access_token": "usage-access-secret",
                "refresh_token": "usage-refresh-secret"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(connected_calendar.status(), StatusCode::CREATED);

    let updated_usage = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/usage"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(updated_usage.status(), StatusCode::OK);
    let body = response_json(updated_usage).await;
    assert_eq!(body["usage"]["active_users"], 1);
    assert_eq!(body["usage"]["stored_file_bytes"], 0);
    assert_eq!(body["usage"]["calendar_connected_accounts"], 1);
}

#[tokio::test]
async fn organization_endpoints_require_bearer_auth() {
    let app = test_app();

    let response = app
        .oneshot(json_request(
            Method::POST,
            "/organizations",
            json!({ "name": "No Auth Org" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn organization_get_is_isolated_by_membership() {
    let app = test_app();
    let owner_token = register_token(&app, "isolated-owner@example.com").await;
    let outsider_token = register_token(&app, "isolated-outsider@example.com").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/organizations",
            &owner_token,
            json!({ "name": "Private Org" }),
        ))
        .await
        .unwrap();
    let organization_id = response_json(created).await["organization"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let outsider_list = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            "/organizations",
            &outsider_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_list.status(), StatusCode::OK);
    assert!(
        response_json(outsider_list).await["organizations"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let outsider_get = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}"),
            &outsider_token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(outsider_get.status(), StatusCode::NOT_FOUND);
}
