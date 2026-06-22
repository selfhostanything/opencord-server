use axum::Router;
use axum::middleware;
use axum::routing::get;

use crate::config::AppConfig;
use crate::controllers::{discovery_controller, health_controller};
use crate::http::cors::browser_cors;

pub fn api_router(config: AppConfig) -> Router {
    Router::new()
        .route("/healthz", get(health_controller::health))
        .route(
            "/.well-known/opencord",
            get(discovery_controller::well_known),
        )
        .route("/api/version", get(discovery_controller::version))
        .route("/api/capabilities", get(discovery_controller::capabilities))
        .layer(middleware::from_fn(browser_cors))
        .with_state(config)
}

pub fn health_router(config: AppConfig) -> Router {
    Router::new()
        .route("/healthz", get(health_controller::health))
        .layer(middleware::from_fn(browser_cors))
        .with_state(config)
}
