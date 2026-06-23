use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use hmac::{Hmac, KeyInit, Mac};
use opencord_server::config::AppConfig;
use opencord_server::routes::api_router;
use serde_json::{Value, json};
use sha2::Sha256;
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

fn bearer_json_request(method: Method, uri: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn oidc_signature(
    secret: &str,
    issuer: &str,
    subject: &str,
    email: &str,
    email_verified: bool,
) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{issuer}\n{subject}\n{email}\n{email_verified}").as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[tokio::test]
async fn register_creates_uuid_v7_user_session_and_me_returns_user() {
    let app = test_app();

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "Ada@Example.COM",
                "display_name": "Ada Lovelace",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response_json(response).await;
    let user_id = body["user"]["id"].as_str().unwrap();
    assert_eq!(uuid::Uuid::parse_str(user_id).unwrap().get_version_num(), 7);
    assert_eq!(body["user"]["email"], "ada@example.com");
    assert_eq!(body["user"]["display_name"], "Ada Lovelace");
    assert!(body["user"].get("password").is_none());

    let token = body["session"]["token"].as_str().unwrap();
    assert!(token.len() >= 32);

    let me = app
        .oneshot(bearer_json_request(Method::GET, "/me", token, json!({})))
        .await
        .unwrap();

    assert_eq!(me.status(), StatusCode::OK);
    let body = response_json(me).await;
    assert_eq!(body["user"]["id"], user_id);
    assert_eq!(body["user"]["email"], "ada@example.com");
}

#[tokio::test]
async fn register_rejects_duplicate_email_case_insensitively() {
    let app = test_app();
    let payload = json!({
        "email": "sam@example.com",
        "display_name": "Sam",
        "password": "correct horse battery staple"
    });

    let first = app
        .clone()
        .oneshot(json_request(Method::POST, "/auth/register", payload))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);

    let duplicate = app
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "SAM@example.com",
                "display_name": "Other Sam",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    let body = response_json(duplicate).await;
    assert_eq!(body["error"]["code"], "email_already_registered");
}

#[tokio::test]
async fn login_logout_and_session_check_enforce_bearer_token() {
    let app = test_app();

    app.clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "grace@example.com",
                "display_name": "Grace Hopper",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

    let bad_login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/login",
            json!({
                "email": "grace@example.com",
                "password": "wrong password"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(bad_login.status(), StatusCode::UNAUTHORIZED);

    let login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/login",
            json!({
                "email": "GRACE@example.com",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let token = response_json(login).await["session"]["token"]
        .as_str()
        .unwrap()
        .to_owned();

    let me = app
        .clone()
        .oneshot(bearer_json_request(Method::GET, "/me", &token, json!({})))
        .await
        .unwrap();
    assert_eq!(me.status(), StatusCode::OK);

    let logout = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            "/auth/logout",
            &token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);

    let me_after_logout = app
        .oneshot(bearer_json_request(Method::GET, "/me", &token, json!({})))
        .await
        .unwrap();
    assert_eq!(me_after_logout.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn register_and_login_attempts_are_rate_limited_by_email() {
    let app = test_app();

    for attempt in 0..5 {
        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/auth/register",
                json!({
                    "email": "limited-register@example.com",
                    "display_name": format!("Limited Register {attempt}"),
                    "password": "correct horse battery staple"
                }),
            ))
            .await
            .unwrap();

        if attempt == 0 {
            assert_eq!(response.status(), StatusCode::CREATED);
        } else {
            assert_eq!(response.status(), StatusCode::CONFLICT);
        }
    }

    let limited_register = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "limited-register@example.com",
                "display_name": "Limited Register Blocked",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(limited_register.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        limited_register
            .headers()
            .get("x-ratelimit-remaining")
            .unwrap()
            .to_str()
            .unwrap(),
        "0"
    );
    assert!(
        limited_register
            .headers()
            .get(header::RETRY_AFTER)
            .is_some()
    );
    let body = response_json(limited_register).await;
    assert_eq!(body["error"]["code"], "rate_limited");

    let registered = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "limited-login@example.com",
                "display_name": "Limited Login",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(registered.status(), StatusCode::CREATED);

    for _ in 0..5 {
        let bad_login = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/auth/login",
                json!({
                    "email": "limited-login@example.com",
                    "password": "wrong password"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(bad_login.status(), StatusCode::UNAUTHORIZED);
    }

    let limited_login = app
        .oneshot(json_request(
            Method::POST,
            "/auth/login",
            json!({
                "email": "LIMITED-LOGIN@example.com",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(limited_login.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited_login.headers().get(header::RETRY_AFTER).is_some());
    let body = response_json(limited_login).await;
    assert_eq!(body["error"]["code"], "rate_limited");
}

#[tokio::test]
async fn oidc_login_can_be_required_for_an_organization_domain() {
    let app = test_app();
    let owner = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "owner@company.example",
                "display_name": "Owner",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(owner.status(), StatusCode::CREATED);
    let owner_body = response_json(owner).await;
    let owner_token = owner_body["session"]["token"].as_str().unwrap().to_owned();

    let tenant = app
        .clone()
        .oneshot(bearer_json_request(
            Method::POST,
            "/cloud/tenants",
            &owner_token,
            json!({
                "name": "Company Cloud",
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

    let issuer = "https://idp.company.example";
    let client_secret = "super-secret-oidc";
    let configured = app
        .clone()
        .oneshot(bearer_json_request(
            Method::PUT,
            &format!("/organizations/{organization_id}/oidc"),
            &owner_token,
            json!({
                "issuer": issuer,
                "authorization_endpoint": "https://idp.company.example/oauth2/authorize",
                "token_endpoint": "https://idp.company.example/oauth2/token",
                "jwks_uri": "https://idp.company.example/oauth2/jwks",
                "client_id": "opencord",
                "client_secret": client_secret,
                "allowed_domains": ["company.example"],
                "require_sso": true,
                "auto_join_role": "member"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(configured.status(), StatusCode::OK);
    let body = response_json(configured).await;
    assert_eq!(body["provider"]["organization_id"], organization_id);
    assert_eq!(body["provider"]["issuer"], issuer);
    assert_eq!(body["provider"]["require_sso"], true);
    assert!(body["provider"].get("client_secret").is_none());

    let providers = app
        .clone()
        .oneshot(json_request(
            Method::GET,
            "/auth/oidc/providers?email=member@company.example",
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(providers.status(), StatusCode::OK);
    let body = response_json(providers).await;
    assert_eq!(body["providers"].as_array().unwrap().len(), 1);
    assert_eq!(body["providers"][0]["organization_id"], organization_id);
    assert_eq!(body["providers"][0]["require_sso"], true);

    let password_registration = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/register",
            json!({
                "email": "member@company.example",
                "display_name": "Member",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(password_registration.status(), StatusCode::FORBIDDEN);
    let body = response_json(password_registration).await;
    assert_eq!(body["error"]["code"], "sso_required");

    let subject = "company-idp-user-1";
    let signature = oidc_signature(
        client_secret,
        issuer,
        subject,
        "member@company.example",
        true,
    );
    let oidc_login = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/auth/oidc/callback",
            json!({
                "issuer": issuer,
                "subject": subject,
                "email": "Member@Company.Example",
                "display_name": "Member User",
                "email_verified": true,
                "signature": signature
            }),
        ))
        .await
        .unwrap();
    assert_eq!(oidc_login.status(), StatusCode::OK);
    let body = response_json(oidc_login).await;
    let member_id = body["user"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        uuid::Uuid::parse_str(&member_id).unwrap().get_version_num(),
        7
    );
    assert_eq!(body["user"]["email"], "member@company.example");
    let member_token = body["session"]["token"].as_str().unwrap().to_owned();

    let organizations = app
        .clone()
        .oneshot(bearer_json_request(
            Method::GET,
            "/organizations",
            &member_token,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(organizations.status(), StatusCode::OK);
    let body = response_json(organizations).await;
    assert_eq!(body["organizations"][0]["id"], organization_id);
    assert_eq!(body["organizations"][0]["role"], "member");

    let bad_signature = app
        .oneshot(json_request(
            Method::POST,
            "/auth/oidc/callback",
            json!({
                "issuer": issuer,
                "subject": "company-idp-user-2",
                "email": "other@company.example",
                "display_name": "Other User",
                "email_verified": true,
                "signature": "bad-signature"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(bad_signature.status(), StatusCode::UNAUTHORIZED);
}
