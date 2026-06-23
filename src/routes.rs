use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, patch, post, put};

use crate::config::AppConfig;
use crate::controllers::{
    attachment_controller, audit_controller, auth_controller, billing_controller, bot_controller,
    calendar_controller, channel_controller, command_controller, compat_controller,
    compat_gateway_controller, data_export_controller, discovery_controller, health_controller,
    media_controller, meeting_controller, message_controller, metrics_controller,
    organization_controller, permission_controller, push_controller, realtime_controller,
    retention_controller, scim_controller, space_controller, usage_controller, voice_controller,
    webhook_controller,
};
use crate::http::cors::browser_cors;
use crate::state::AppState;

pub fn api_router(config: AppConfig) -> Router {
    api_router_with_state(AppState::in_memory(config))
}

pub fn api_router_with_state(state: AppState) -> Router {
    let cors_state = state.clone();
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
        .route(
            "/api/webhooks/{webhook_id}/{webhook_token}",
            post(webhook_controller::execute),
        )
        .route(
            "/api/compat/discord/v10/users/@me",
            get(compat_controller::get_current_user),
        )
        .route(
            "/api/compat/discord/v10/guilds/{space_id}",
            get(compat_controller::get_guild),
        )
        .route(
            "/api/compat/discord/v10/guilds/{space_id}/channels",
            get(compat_controller::list_guild_channels),
        )
        .route(
            "/api/compat/discord/v10/guilds/{space_id}/roles",
            get(compat_controller::list_guild_roles),
        )
        .route(
            "/api/compat/discord/v10/channels/{channel_id}/messages",
            post(compat_controller::create_message).get(compat_controller::list_messages),
        )
        .route(
            "/api/compat/discord/v10/channels/{channel_id}/messages/{message_id}",
            patch(compat_controller::update_message).delete(compat_controller::delete_message),
        )
        .route(
            "/api/compat/discord/v10/applications/{application_id}/guilds/{space_id}/commands",
            post(command_controller::create_compat_space_command),
        )
        .route(
            "/api/compat/discord/v10/interactions/{interaction_id}/{interaction_token}/callback",
            post(command_controller::create_interaction_callback),
        )
        .route(
            "/api/compat/discord/v10/webhooks/{application_id}/{interaction_token}",
            post(command_controller::create_interaction_followup),
        )
        .route(
            "/api/compat/discord/v10/webhooks/{application_id}/{interaction_token}/messages/@original",
            patch(command_controller::update_original_interaction_response)
                .delete(command_controller::delete_original_interaction_response),
        )
        .route(
            "/api/compat/discord/gateway",
            get(compat_gateway_controller::gateway),
        )
        .route(
            "/webhooks/{webhook_id}/{webhook_token}",
            post(webhook_controller::execute),
        )
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
            "/organizations/{organization_id}/retention-policy",
            get(retention_controller::get_policy).put(retention_controller::upsert_policy),
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
            "/organizations/{organization_id}/bot-applications",
            post(bot_controller::create_application).get(bot_controller::list_applications),
        )
        .route(
            "/organizations/{organization_id}/bot-applications/{application_id}",
            get(bot_controller::get_application),
        )
        .route(
            "/organizations/{organization_id}/bot-applications/{application_id}/tokens/rotate",
            post(bot_controller::rotate_token),
        )
        .route(
            "/organizations/{organization_id}/bot-applications/{application_id}/spaces/{space_id}/invite",
            post(bot_controller::invite_to_space),
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
        .route("/spaces/{space_id}", patch(space_controller::update))
        .route(
            "/spaces/{space_id}/members",
            post(permission_controller::add_space_member),
        )
        .route(
            "/spaces/{space_id}/members/{user_id}",
            delete(permission_controller::remove_space_member),
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
        .route(
            "/channels/{channel_id}",
            patch(channel_controller::update).delete(channel_controller::delete),
        )
        .route(
            "/channels/{channel_id}/permission-overrides",
            post(permission_controller::set_channel_override),
        )
        .route(
            "/channels/{channel_id}/webhooks",
            post(webhook_controller::create).get(webhook_controller::list_for_channel),
        )
        .route(
            "/channels/{channel_id}/webhooks/{webhook_id}/token/rotate",
            post(webhook_controller::rotate_token),
        )
        .route(
            "/channels/{channel_id}/webhooks/{webhook_id}",
            delete(webhook_controller::delete),
        )
        .route(
            "/channels/{channel_id}/command-interactions",
            post(command_controller::create_channel_interaction),
        )
        .route(
            "/channels/{channel_id}/component-interactions",
            post(command_controller::create_component_interaction),
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
        .layer(middleware::from_fn_with_state(cors_state, browser_cors))
        .with_state(state)
}

pub fn health_router(config: AppConfig) -> Router {
    let state = AppState::in_memory(config);
    let cors_state = state.clone();
    Router::new()
        .route("/healthz", get(health_controller::health))
        .layer(middleware::from_fn_with_state(cors_state, browser_cors))
        .with_state(state)
}

pub fn realtime_router_with_state(state: AppState) -> Router {
    let cors_state = state.clone();
    Router::new()
        .route("/healthz", get(health_controller::health))
        .route("/ws", get(realtime_controller::websocket))
        .layer(middleware::from_fn_with_state(cors_state, browser_cors))
        .with_state(state)
}
