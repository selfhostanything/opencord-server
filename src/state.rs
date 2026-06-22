use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::config::AppConfig;
use crate::domain::attachment::{AttachmentService, AttachmentStore};
use crate::domain::auth::{AuthService, AuthStore};
use crate::domain::channel::{ChannelService, ChannelStore};
use crate::domain::message::{MessageService, MessageStore};
use crate::domain::organization::{OrganizationService, OrganizationStore};
use crate::domain::permission::{PermissionService, PermissionStore};
use crate::domain::realtime::RealtimeHub;
use crate::domain::space::{SpaceService, SpaceStore};
use crate::repositories::attachment_memory::MemoryAttachmentStore;
use crate::repositories::attachment_postgres::PostgresAttachmentStore;
use crate::repositories::auth_memory::MemoryAuthStore;
use crate::repositories::auth_postgres::PostgresAuthStore;
use crate::repositories::channel_memory::MemoryChannelStore;
use crate::repositories::channel_postgres::PostgresChannelStore;
use crate::repositories::message_memory::MemoryMessageStore;
use crate::repositories::message_postgres::PostgresMessageStore;
use crate::repositories::organization_memory::MemoryOrganizationStore;
use crate::repositories::organization_postgres::PostgresOrganizationStore;
use crate::repositories::permission_memory::MemoryPermissionStore;
use crate::repositories::permission_postgres::PostgresPermissionStore;
use crate::repositories::space_memory::MemorySpaceStore;
use crate::repositories::space_postgres::PostgresSpaceStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub auth: Arc<AuthService>,
    pub organizations: Arc<OrganizationService>,
    pub spaces: Arc<SpaceService>,
    pub channels: Arc<ChannelService>,
    pub messages: Arc<MessageService>,
    pub attachments: Arc<AttachmentService>,
    pub permissions: Arc<PermissionService>,
    pub realtime: Arc<RealtimeHub>,
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
                attachments: Arc::new(MemoryAttachmentStore::default()),
                permissions: Arc::new(MemoryPermissionStore::default()),
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
                attachments: Arc::new(PostgresAttachmentStore::new(db.clone())),
                permissions: Arc::new(PostgresPermissionStore::new(db)),
            },
        )
    }

    pub fn with_stores(config: AppConfig, stores: AppStores) -> Self {
        Self {
            config,
            auth: Arc::new(AuthService::new(stores.auth)),
            organizations: Arc::new(OrganizationService::new(stores.organizations)),
            spaces: Arc::new(SpaceService::new(stores.spaces)),
            channels: Arc::new(ChannelService::new(stores.channels)),
            messages: Arc::new(MessageService::new(stores.messages)),
            attachments: Arc::new(AttachmentService::new(stores.attachments)),
            permissions: Arc::new(PermissionService::new(stores.permissions)),
            realtime: Arc::new(RealtimeHub::default()),
        }
    }
}

pub struct AppStores {
    pub auth: Arc<dyn AuthStore>,
    pub organizations: Arc<dyn OrganizationStore>,
    pub spaces: Arc<dyn SpaceStore>,
    pub channels: Arc<dyn ChannelStore>,
    pub messages: Arc<dyn MessageStore>,
    pub attachments: Arc<dyn AttachmentStore>,
    pub permissions: Arc<dyn PermissionStore>,
}
