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

async fn register_owner(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "scim-owner@example.com",
                "display_name": "SCIM Owner",
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
async fn scim_token_can_create_read_and_deactivate_external_users() {
    let app = test_app();
    let owner_token = register_owner(&app).await;

    let tenant = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/cloud/tenants",
            &owner_token,
            json!({
                "name": "SCIM Cloud",
                "plan": "enterprise",
                "deployment_mode": "cloud",
                "primary_region": "vultr-sgp"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(tenant.status(), StatusCode::CREATED);
    let organization_id = response_json(tenant).await["tenant"]["organization_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let token_response = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            &format!("/organizations/{organization_id}/scim/token"),
            &owner_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(token_response.status(), StatusCode::CREATED);
    let body = response_json(token_response).await;
    let scim_token = body["scim_token"]["token"].as_str().unwrap().to_owned();
    assert!(scim_token.starts_with("opc-scim-"));

    let created = app
        .clone()
        .oneshot(bearer_request(
            Method::POST,
            "/scim/v2/Users",
            &scim_token,
            json!({
                "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
                "externalId": "idp-user-1",
                "userName": "Scim.Member@Example.com",
                "name": {
                    "formatted": "SCIM Member"
                },
                "active": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body = response_json(created).await;
    assert_eq!(body["externalId"], "idp-user-1");
    assert_eq!(body["userName"], "scim.member@example.com");
    assert_eq!(body["active"], true);
    assert_eq!(
        uuid::Uuid::parse_str(body["id"].as_str().unwrap())
            .unwrap()
            .get_version_num(),
        7
    );

    let fetched = app
        .clone()
        .oneshot(bearer_request(
            Method::GET,
            "/scim/v2/Users/idp-user-1",
            &scim_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    let body = response_json(fetched).await;
    assert_eq!(body["externalId"], "idp-user-1");
    assert_eq!(body["active"], true);

    let deactivated = app
        .clone()
        .oneshot(bearer_request(
            Method::PATCH,
            "/scim/v2/Users/idp-user-1",
            &scim_token,
            json!({
                "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
                "Operations": [
                    {
                        "op": "replace",
                        "path": "active",
                        "value": false
                    }
                ]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(deactivated.status(), StatusCode::OK);
    let body = response_json(deactivated).await;
    assert_eq!(body["active"], false);

    let unauthorized = app
        .oneshot(bearer_request(
            Method::POST,
            "/scim/v2/Users",
            "bad-token",
            json!({
                "externalId": "idp-user-2",
                "userName": "other@example.com",
                "active": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
}
