use axum::Router;
use axum::middleware;
use axum::routing::{get, patch, post, put};

use crate::config::AppConfig;
use crate::controllers::{
    attachment_controller, audit_controller, auth_controller, billing_controller,
    calendar_controller, channel_controller, data_export_controller, discovery_controller,
    health_controller, media_controller, meeting_controller, message_controller,
    metrics_controller, organization_controller, permission_controller, push_controller,
    realtime_controller, scim_controller, space_controller, usage_controller, voice_controller,
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
        .route("/join/{join_slug}", get(meeting_controller::resolve_join))
        .route(
            "/.well-known/opencord",
            get(discovery_controller::well_known),
        )
        .route("/api/version", get(discovery_controller::version))
        .route("/api/capabilities", get(discovery_controller::capabilities))
        .route("/auth/register", post(auth_controller::register))
        .route("/auth/login", post(auth_controller::login))
        .route("/auth/oidc/providers", get(auth_controller::oidc_providers))
        .route("/auth/oidc/callback", post(auth_controller::oidc_callback))
        .route("/auth/logout", post(auth_controller::logout))
        .route("/me", get(auth_controller::me))
        .route(
            "/cloud/tenants",
            post(organization_controller::provision_tenant),
        )
        .route(
            "/billing/provider-events",
            post(billing_controller::apply_provider_event),
        )
        .route(
            "/calendar/accounts",
            get(calendar_controller::list_accounts),
        )
        .route(
            "/calendar/accounts/google",
            post(calendar_controller::connect_google),
        )
        .route(
            "/calendar/accounts/caldav",
            post(calendar_controller::connect_caldav),
        )
        .route(
            "/calendar/accounts/microsoft",
            post(calendar_controller::connect_microsoft),
        )
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
            "/organizations/{organization_id}/usage",
            get(usage_controller::get),
        )
        .route(
            "/organizations/{organization_id}/audit-events/export",
            get(audit_controller::export_for_organization),
        )
        .route(
            "/organizations/{organization_id}/data-export",
            get(data_export_controller::export_for_organization),
        )
        .route(
            "/organizations/{organization_id}/oidc",
            get(organization_controller::get_oidc_provider)
                .put(organization_controller::configure_oidc_provider),
        )
        .route(
            "/organizations/{organization_id}/scim/token",
            post(scim_controller::rotate_token),
        )
        .route(
            "/organizations/{organization_id}/custom-domains",
            post(organization_controller::create_custom_domain)
                .get(organization_controller::list_custom_domains),
        )
        .route(
            "/organizations/{organization_id}/custom-domains/{custom_domain_id}/verify",
            post(organization_controller::verify_custom_domain),
        )
        .route(
            "/custom-domains/resolve",
            get(organization_controller::resolve_custom_domain),
        )
        .route("/scim/v2/Users", post(scim_controller::create_user))
        .route(
            "/scim/v2/Users/{external_id}",
            get(scim_controller::get_user).patch(scim_controller::patch_user),
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
            "/meetings/{meeting_id}/invite.ics",
            get(meeting_controller::invite_ics),
        )
        .route(
            "/meetings/{meeting_id}/calendar/google/sync",
            post(meeting_controller::sync_google_calendar),
        )
        .route(
            "/meetings/{meeting_id}/calendar/caldav/sync",
            post(meeting_controller::sync_caldav_calendar),
        )
        .route(
            "/meetings/{meeting_id}/calendar/microsoft/sync",
            post(meeting_controller::sync_microsoft_calendar),
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
