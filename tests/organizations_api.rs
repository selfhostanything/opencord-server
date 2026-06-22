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
    response_json(response).await["session"]["token"]
        .as_str()
        .unwrap()
        .to_owned()
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
