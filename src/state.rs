use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::config::AppConfig;
use crate::domain::attachment::{AttachmentService, AttachmentStore};
use crate::domain::audit::{AuditService, AuditStore};
use crate::domain::auth::{AuthService, AuthStore};
use crate::domain::billing::{BillingService, BillingStore};
use crate::domain::bot::{BotService, BotStore};
use crate::domain::calendar_sync::{
    CalendarStore, CalendarSyncService, LocalCaldavCalendarAdapter, LocalGoogleCalendarAdapter,
    LocalMicrosoftCalendarAdapter,
};
use crate::domain::channel::{ChannelService, ChannelStore};
use crate::domain::command::{CommandService, CommandStore};
use crate::domain::data_export::DataExportService;
use crate::domain::media::MediaControlService;
use crate::domain::meeting::{MeetingService, MeetingStore};
use crate::domain::message::{MessageService, MessageStore};
use crate::domain::metrics::MediaMetrics;
use crate::domain::organization::{OrganizationService, OrganizationStore};
use crate::domain::permission::{PermissionService, PermissionStore};
use crate::domain::push::{PushService, PushTokenStore};
use crate::domain::realtime::RealtimeHub;
use crate::domain::retention::{RetentionService, RetentionStore};
use crate::domain::scim::{ScimService, ScimStore};
use crate::domain::space::{SpaceService, SpaceStore};
use crate::domain::usage::UsageService;
use crate::domain::webhook::{IncomingWebhookService, IncomingWebhookStore};
use crate::repositories::attachment_memory::MemoryAttachmentStore;
use crate::repositories::attachment_postgres::PostgresAttachmentStore;
use crate::repositories::audit_memory::MemoryAuditStore;
use crate::repositories::audit_postgres::PostgresAuditStore;
use crate::repositories::auth_memory::MemoryAuthStore;
use crate::repositories::auth_postgres::PostgresAuthStore;
use crate::repositories::billing_memory::MemoryBillingStore;
use crate::repositories::billing_postgres::PostgresBillingStore;
use crate::repositories::bot_memory::MemoryBotStore;
use crate::repositories::bot_postgres::PostgresBotStore;
use crate::repositories::calendar_memory::MemoryCalendarStore;
use crate::repositories::calendar_postgres::PostgresCalendarStore;
use crate::repositories::channel_memory::MemoryChannelStore;
use crate::repositories::channel_postgres::PostgresChannelStore;
use crate::repositories::command_memory::MemoryCommandStore;
use crate::repositories::command_postgres::PostgresCommandStore;
use crate::repositories::meeting_memory::MemoryMeetingStore;
use crate::repositories::meeting_postgres::PostgresMeetingStore;
use crate::repositories::message_memory::MemoryMessageStore;
use crate::repositories::message_postgres::PostgresMessageStore;
use crate::repositories::organization_memory::MemoryOrganizationStore;
use crate::repositories::organization_postgres::PostgresOrganizationStore;
use crate::repositories::permission_memory::MemoryPermissionStore;
use crate::repositories::permission_postgres::PostgresPermissionStore;
use crate::repositories::push_memory::MemoryPushTokenStore;
use crate::repositories::push_postgres::PostgresPushTokenStore;
use crate::repositories::retention_memory::MemoryRetentionStore;
use crate::repositories::retention_postgres::PostgresRetentionStore;
use crate::repositories::scim_memory::MemoryScimStore;
use crate::repositories::scim_postgres::PostgresScimStore;
use crate::repositories::space_memory::MemorySpaceStore;
use crate::repositories::space_postgres::PostgresSpaceStore;
use crate::repositories::webhook_memory::MemoryIncomingWebhookStore;
use crate::repositories::webhook_postgres::PostgresIncomingWebhookStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub auth: Arc<AuthService>,
    pub organizations: Arc<OrganizationService>,
    pub spaces: Arc<SpaceService>,
    pub channels: Arc<ChannelService>,
    pub messages: Arc<MessageService>,
    pub meetings: Arc<MeetingService>,
    pub calendar_sync: Arc<CalendarSyncService>,
    pub attachments: Arc<AttachmentService>,
    pub data_exports: Arc<DataExportService>,
    pub audit: Arc<AuditService>,
    pub permissions: Arc<PermissionService>,
    pub push: Arc<PushService>,
    pub media: Arc<MediaControlService>,
    pub metrics: Arc<MediaMetrics>,
    pub realtime: Arc<RealtimeHub>,
    pub usage: Arc<UsageService>,
    pub billing: Arc<BillingService>,
    pub scim: Arc<ScimService>,
    pub retention: Arc<RetentionService>,
    pub bots: Arc<BotService>,
    pub webhooks: Arc<IncomingWebhookService>,
    pub commands: Arc<CommandService>,
}

impl AppState {
    pub fn in_memory(config: AppConfig) -> Self {
        Self::with_stores(
            config,
            AppStores {
                auth: Arc::new(MemoryAuthStore::default()),
                organizations: Arc::new(MemoryOrganizationStore::default()),
                spaces: Arc::new(MemorySpaceStore::default()),
                channels: Arc::new(MemoryChannelStore::default()),
                messages: Arc::new(MemoryMessageStore::default()),
                meetings: Arc::new(MemoryMeetingStore::default()),
                calendar: Arc::new(MemoryCalendarStore::default()),
                attachments: Arc::new(MemoryAttachmentStore::default()),
                audit: Arc::new(MemoryAuditStore::default()),
                permissions: Arc::new(MemoryPermissionStore::default()),
                push: Arc::new(MemoryPushTokenStore::default()),
                billing: Arc::new(MemoryBillingStore::default()),
                scim: Arc::new(MemoryScimStore::default()),
                retention: Arc::new(MemoryRetentionStore::default()),
                bots: Arc::new(MemoryBotStore::default()),
                webhooks: Arc::new(MemoryIncomingWebhookStore::default()),
                commands: Arc::new(MemoryCommandStore::default()),
            },
        )
    }

    pub fn with_database(config: AppConfig, db: DatabaseConnection) -> Self {
        Self::with_stores(
            config,
            AppStores {
                auth: Arc::new(PostgresAuthStore::new(db.clone())),
                organizations: Arc::new(PostgresOrganizationStore::new(db.clone())),
                spaces: Arc::new(PostgresSpaceStore::new(db.clone())),
                channels: Arc::new(PostgresChannelStore::new(db.clone())),
                messages: Arc::new(PostgresMessageStore::new(db.clone())),
                meetings: Arc::new(PostgresMeetingStore::new(db.clone())),
                calendar: Arc::new(PostgresCalendarStore::new(db.clone())),
                attachments: Arc::new(PostgresAttachmentStore::new(db.clone())),
                audit: Arc::new(PostgresAuditStore::new(db.clone())),
                permissions: Arc::new(PostgresPermissionStore::new(db.clone())),
                push: Arc::new(PostgresPushTokenStore::new(db.clone())),
                billing: Arc::new(PostgresBillingStore::new(db.clone())),
                scim: Arc::new(PostgresScimStore::new(db.clone())),
                retention: Arc::new(PostgresRetentionStore::new(db.clone())),
                bots: Arc::new(PostgresBotStore::new(db.clone())),
                webhooks: Arc::new(PostgresIncomingWebhookStore::new(db.clone())),
                commands: Arc::new(PostgresCommandStore::new(db.clone())),
            },
        )
    }

    pub fn with_stores(config: AppConfig, stores: AppStores) -> Self {
        let usage = Arc::new(UsageService::new(
            stores.organizations.clone(),
            stores.attachments.clone(),
            stores.calendar.clone(),
        ));
        let billing = Arc::new(BillingService::new(
            stores.billing.clone(),
            stores.organizations.clone(),
        ));
        let scim = Arc::new(ScimService::new(
            stores.scim.clone(),
            stores.auth.clone(),
            stores.organizations.clone(),
        ));
        let bots = Arc::new(BotService::new(stores.bots.clone(), stores.auth.clone()));
        let webhooks = Arc::new(IncomingWebhookService::new(
            stores.webhooks.clone(),
            stores.auth.clone(),
        ));
        let data_exports = Arc::new(DataExportService::new(
            stores.messages.clone(),
            stores.attachments.clone(),
        ));
        let commands = Arc::new(CommandService::new(stores.commands.clone()));

        Self {
            config,
            auth: Arc::new(AuthService::new(stores.auth)),
            organizations: Arc::new(OrganizationService::new(stores.organizations)),
            spaces: Arc::new(SpaceService::new(stores.spaces)),
            channels: Arc::new(ChannelService::new(stores.channels)),
            messages: Arc::new(MessageService::new(stores.messages)),
            meetings: Arc::new(MeetingService::new(stores.meetings)),
            calendar_sync: Arc::new(CalendarSyncService::new(
                stores.calendar,
                Arc::new(LocalGoogleCalendarAdapter),
                Arc::new(LocalMicrosoftCalendarAdapter),
                Arc::new(LocalCaldavCalendarAdapter),
            )),
            attachments: Arc::new(AttachmentService::new(stores.attachments)),
            data_exports,
            audit: Arc::new(AuditService::new(stores.audit)),
            permissions: Arc::new(PermissionService::new(stores.permissions)),
            push: Arc::new(PushService::new(stores.push)),
            media: Arc::new(MediaControlService::from_env()),
            metrics: Arc::new(MediaMetrics::default()),
            realtime: Arc::new(RealtimeHub::default()),
            usage,
            billing,
            scim,
            retention: Arc::new(RetentionService::new(stores.retention)),
            bots,
            webhooks,
            commands,
        }
    }
}

pub struct AppStores {
    pub auth: Arc<dyn AuthStore>,
    pub organizations: Arc<dyn OrganizationStore>,
    pub spaces: Arc<dyn SpaceStore>,
    pub channels: Arc<dyn ChannelStore>,
    pub messages: Arc<dyn MessageStore>,
    pub meetings: Arc<dyn MeetingStore>,
    pub calendar: Arc<dyn CalendarStore>,
    pub attachments: Arc<dyn AttachmentStore>,
    pub audit: Arc<dyn AuditStore>,
    pub permissions: Arc<dyn PermissionStore>,
    pub push: Arc<dyn PushTokenStore>,
    pub billing: Arc<dyn BillingStore>,
    pub scim: Arc<dyn ScimStore>,
    pub retention: Arc<dyn RetentionStore>,
    pub bots: Arc<dyn BotStore>,
    pub webhooks: Arc<dyn IncomingWebhookStore>,
    pub commands: Arc<dyn CommandStore>,
}
