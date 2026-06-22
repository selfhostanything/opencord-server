use axum::Router;
use axum::middleware;
use axum::routing::{get, post};

use crate::config::AppConfig;
use crate::controllers::{
    auth_controller, discovery_controller, health_controller, organization_controller,
    space_controller,
};
use crate::http::cors::browser_cors;
use crate::state::AppState;

pub fn api_router(config: AppConfig) -> Router {
    api_router_with_state(AppState::in_memory(config))
}

pub fn api_router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health_controller::health))
        .route(
            "/.well-known/opencord",
            get(discovery_controller::well_known),
        )
        .route("/api/version", get(discovery_controller::version))
        .route("/api/capabilities", get(discovery_controller::capabilities))
        .route("/auth/register", post(auth_controller::register))
        .route("/auth/login", post(auth_controller::login))
        .route("/auth/logout", post(auth_controller::logout))
        .route("/me", get(auth_controller::me))
        .route(
            "/organizations",
            post(organization_controller::create).get(organization_controller::list),
        )
        .route(
            "/organizations/{organization_id}",
            get(organization_controller::get),
        )
        .route(
            "/organizations/{organization_id}/spaces",
            post(space_controller::create).get(space_controller::list),
        )
        .layer(middleware::from_fn(browser_cors))
        .with_state(state)
}

pub fn health_router(config: AppConfig) -> Router {
    Router::new()
        .route("/healthz", get(health_controller::health))
        .layer(middleware::from_fn(browser_cors))
        .with_state(AppState::in_memory(config))
}
