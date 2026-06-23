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

async fn register(app: &axum::Router, email: &str) -> String {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "display_name": "Billing Test User",
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

async fn provision_tenant(app: &axum::Router, token: &str) -> String {
    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/cloud/tenants",
            token,
            json!({
                "name": "Billing Cloud",
                "plan": "team",
                "deployment_mode": "cloud",
                "primary_region": "vultr-sgp"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    response_json(response).await["tenant"]["organization_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn billing_provider_event_updates_local_plan_entitlement() {
    let app = test_app();
    let token = register(&app, "billing-owner@example.com").await;
    let organization_id = provision_tenant(&app, &token).await;

    let response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/billing/provider-events",
            &token,
            json!({
                "organization_id": organization_id,
                "provider": "stripe",
                "event_type": "subscription.updated",
                "external_customer_id": "cus_opencord_123",
                "external_subscription_id": "sub_opencord_123",
                "plan": "enterprise",
                "status": "active",
                "current_period_end": "2026-07-23T00:00:00Z"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["billing"]["organization_id"], organization_id);
    assert_eq!(body["billing"]["provider"], "stripe");
    assert_eq!(body["billing"]["event_type"], "subscription.updated");
    assert_eq!(body["billing"]["plan"], "enterprise");
    assert_eq!(body["billing"]["status"], "active");

    let organization = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(organization.status(), StatusCode::OK);
    let body = response_json(organization).await;
    assert_eq!(body["organization"]["plan"], "enterprise");
}
