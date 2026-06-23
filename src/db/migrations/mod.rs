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
        ]
    }
}
