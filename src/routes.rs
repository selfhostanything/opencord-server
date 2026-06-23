use axum::Router;
use axum::middleware;
use axum::routing::{get, patch, post, put};

use crate::config::AppConfig;
use crate::controllers::{
    attachment_controller, audit_controller, auth_controller, channel_controller,
    discovery_controller, health_controller, media_controller, meeting_controller,
    message_controller, metrics_controller, organization_controller, permission_controller,
    push_controller, realtime_controller, space_controller, voice_controller,
};
use crate::http::cors::browser_cors;
use crate::state::AppState;

pub fn api_router(config: AppConfig) -> Router {
    api_router_with_state(AppState::in_memory(config))
}

pub fn api_router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health_controller::health))
        .route("/metrics", get(metrics_controller::prometheus))
        .route("/ws", get(realtime_controller::websocket))
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
            "/media/rooms/token",
            post(media_controller::create_room_token),
        )
        .route(
            "/voice/channels/{channel_id}/join",
            post(voice_controller::join),
        )
        .route(
            "/push-tokens",
            post(push_controller::register).get(push_controller::list),
        )
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
        .route(
            "/organizations/{organization_id}/meetings",
            post(meeting_controller::create).get(meeting_controller::list),
        )
        .route(
            "/meetings/{meeting_id}",
            get(meeting_controller::get)
                .patch(meeting_controller::update)
                .delete(meeting_controller::cancel),
        )
        .route(
            "/spaces/{space_id}/channels",
            post(channel_controller::create).get(channel_controller::list),
        )
        .route(
            "/spaces/{space_id}/members",
            post(permission_controller::add_space_member),
        )
        .route(
            "/spaces/{space_id}/audit-events",
            get(audit_controller::list_for_space),
        )
        .route(
            "/spaces/{space_id}/roles",
            post(permission_controller::create_role),
        )
        .route(
            "/spaces/{space_id}/roles/{role_id}/assignments",
            post(permission_controller::assign_role),
        )
        .route("/channels/{channel_id}", patch(channel_controller::update))
        .route(
            "/channels/{channel_id}/permission-overrides",
            post(permission_controller::set_channel_override),
        )
        .route(
            "/channels/{channel_id}/messages",
            post(message_controller::create).get(message_controller::list),
        )
        .route("/attachments/presign", post(attachment_controller::presign))
        .route(
            "/attachments/{attachment_id}/content",
            put(attachment_controller::upload_content).get(attachment_controller::download_content),
        )
        .route(
            "/messages/{message_id}",
            patch(message_controller::update).delete(message_controller::delete),
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

pub fn realtime_router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health_controller::health))
        .route("/ws", get(realtime_controller::websocket))
        .layer(middleware::from_fn(browser_cors))
        .with_state(state)
}
