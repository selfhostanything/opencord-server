use sea_orm_migration::prelude::*;

mod m20260622173149_baseline;
mod m20260623012400_auth;
mod m20260623013800_organizations;
mod m20260623015000_spaces;
mod m20260623020400_channels;
mod m20260623022000_messages;
mod m20260623023500_permissions;
mod m20260623032000_attachments;
mod m20260623034000_audit_events;
mod m20260623040000_push_tokens;
mod m20260623041000_meetings;
mod m20260623043000_calendar_sync;
mod m20260623044000_microsoft_calendar_sync;
mod m20260623045000_caldav_calendar_sync;
mod m20260623050000_billing;
mod m20260623051000_custom_domains;
mod m20260623052000_oidc_identity;
mod m20260623053000_scim;
mod m20260623054000_retention;
mod m20260623055000_bots;
mod m20260623060000_incoming_webhooks;
mod m20260623061000_commands;
mod m20260623062000_message_embeds;
mod m20260623063000_message_mentions;
mod m20260623064000_message_components;
mod m20260623065000_component_interactions;
mod m20260623070000_deferred_interactions;
mod m20260623071000_interaction_response_messages;
mod m20260623072000_compat_gateway_sessions;
mod m20260623073000_compat_gateway_replay_events;
mod m20260623074000_message_webhook_overrides;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260622173149_baseline::Migration),
            Box::new(m20260623012400_auth::Migration),
            Box::new(m20260623013800_organizations::Migration),
            Box::new(m20260623015000_spaces::Migration),
            Box::new(m20260623020400_channels::Migration),
            Box::new(m20260623022000_messages::Migration),
            Box::new(m20260623023500_permissions::Migration),
            Box::new(m20260623032000_attachments::Migration),
            Box::new(m20260623034000_audit_events::Migration),
            Box::new(m20260623040000_push_tokens::Migration),
            Box::new(m20260623041000_meetings::Migration),
            Box::new(m20260623043000_calendar_sync::Migration),
            Box::new(m20260623044000_microsoft_calendar_sync::Migration),
            Box::new(m20260623045000_caldav_calendar_sync::Migration),
            Box::new(m20260623050000_billing::Migration),
            Box::new(m20260623051000_custom_domains::Migration),
            Box::new(m20260623052000_oidc_identity::Migration),
            Box::new(m20260623053000_scim::Migration),
            Box::new(m20260623054000_retention::Migration),
            Box::new(m20260623055000_bots::Migration),
            Box::new(m20260623060000_incoming_webhooks::Migration),
            Box::new(m20260623061000_commands::Migration),
            Box::new(m20260623062000_message_embeds::Migration),
            Box::new(m20260623063000_message_mentions::Migration),
            Box::new(m20260623064000_message_components::Migration),
            Box::new(m20260623065000_component_interactions::Migration),
            Box::new(m20260623070000_deferred_interactions::Migration),
            Box::new(m20260623071000_interaction_response_messages::Migration),
            Box::new(m20260623072000_compat_gateway_sessions::Migration),
            Box::new(m20260623073000_compat_gateway_replay_events::Migration),
            Box::new(m20260623074000_message_webhook_overrides::Migration),
        ]
    }
}
