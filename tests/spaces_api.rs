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
                "display_name": "Space Test User",
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

async fn register_token(app: &axum::Router, email: &str) -> String {
    register(app, email).await.0
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
async fn organization_member_can_create_and_list_spaces() {
    let app = test_app();
    let token = register_token(&app, "space-owner@example.com").await;
    let organization_id = create_organization(&app, &token, "Space Parent").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            &token,
            json!({ "name": "Engineering" }),
        ))
        .await
        .unwrap();

    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    let space_id = body["space"]["id"].as_str().unwrap();
    assert_eq!(
        uuid::Uuid::parse_str(space_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(body["space"]["organization_id"], organization_id);
    assert_eq!(body["space"]["name"], "Engineering");
    assert_eq!(body["space"]["slug"], "engineering");
    assert_eq!(body["membership"]["role"], "owner");

    let list = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/spaces"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(list.status(), StatusCode::OK);
    let body = response_json(list).await;
    assert_eq!(body["spaces"].as_array().unwrap().len(), 1);
    assert_eq!(body["spaces"][0]["id"], space_id);
    assert_eq!(body["spaces"][0]["role"], "owner");
}

#[tokio::test]
async fn space_owner_can_update_space_name_and_slug() {
    let app = test_app();
    let token = register_token(&app, "space-update-owner@example.com").await;
    let organization_id = create_organization(&app, &token, "Space Update Parent").await;

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            &token,
            json!({ "name": "Engineering Draft" }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let space_id = response_json(created).await["space"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let updated = app
        .clone()
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/spaces/{space_id}"),
            &token,
            json!({ "name": "Engineering Stable" }),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let body = response_json(updated).await;
    assert_eq!(body["space"]["id"], space_id);
    assert_eq!(body["space"]["organization_id"], organization_id);
    assert_eq!(body["space"]["name"], "Engineering Stable");
    assert_eq!(body["space"]["slug"], "engineering-stable");
    assert_eq!(body["membership"]["role"], "owner");

    let list = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/spaces"),
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let body = response_json(list).await;
    assert_eq!(body["spaces"].as_array().unwrap().len(), 1);
    assert_eq!(body["spaces"][0]["id"], space_id);
    assert_eq!(body["spaces"][0]["name"], "Engineering Stable");
    assert_eq!(body["spaces"][0]["slug"], "engineering-stable");
}

#[tokio::test]
async fn space_update_requires_manage_space() {
    let app = test_app();
    let owner_token = register_token(&app, "space-update-denied-owner@example.com").await;
    let (member_token, member_id) = register(&app, "space-update-denied-member@example.com").await;
    let organization_id =
        create_organization(&app, &owner_token, "Space Update Denied Parent").await;
    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            &owner_token,
            json!({ "name": "Member Managed" }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let space_id = response_json(created).await["space"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    add_space_member(&app, &owner_token, &space_id, &member_id).await;

    let denied = app
        .oneshot(bearer_request(
            Method::PATCH,
            &format!("/spaces/{space_id}"),
            &member_token,
            json!({ "name": "Member Should Not Rename" }),
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn space_endpoints_require_bearer_auth() {
    let app = test_app();
    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/organizations/{}/spaces", uuid::Uuid::now_v7()),
            json!({ "name": "No Auth Space" }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn spaces_are_isolated_by_organization_membership() {
    let app = test_app();
    let owner_token = register_token(&app, "space-isolated-owner@example.com").await;
    let outsider_token = register_token(&app, "space-isolated-outsider@example.com").await;
    let organization_id = create_organization(&app, &owner_token, "Isolated Space Parent").await;

    let outsider_create = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            &outsider_token,
            json!({ "name": "Outsider Space" }),
        ))
        .await
        .unwrap();
    assert_eq!(outsider_create.status(), StatusCode::NOT_FOUND);

    let owner_create = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/spaces"),
            &owner_token,
            json!({ "name": "Private Space" }),
        ))
        .await
        .unwrap();
    assert_eq!(owner_create.status(), StatusCode::CREATED);

    let outsider_list = app
        .oneshot(bearer_request(
            Method::GET,
            &format!("/organizations/{organization_id}/spaces"),
            &outsider_token,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(outsider_list.status(), StatusCode::NOT_FOUND);
}
