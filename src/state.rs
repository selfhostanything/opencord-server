use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::config::AppConfig;
use crate::domain::auth::{AuthService, AuthStore};
use crate::domain::channel::{ChannelService, ChannelStore};
use crate::domain::message::{MessageService, MessageStore};
use crate::domain::organization::{OrganizationService, OrganizationStore};
use crate::domain::permission::{PermissionService, PermissionStore};
use crate::domain::space::{SpaceService, SpaceStore};
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
    pub permissions: Arc<PermissionService>,
}

impl AppState {
    pub fn in_memory(config: AppConfig) -> Self {
        Self::with_stores(
            config,
            Arc::new(MemoryAuthStore::default()),
            Arc::new(MemoryOrganizationStore::default()),
            Arc::new(MemorySpaceStore::default()),
            Arc::new(MemoryChannelStore::default()),
            Arc::new(MemoryMessageStore::default()),
            Arc::new(MemoryPermissionStore::default()),
        )
    }

    pub fn with_database(config: AppConfig, db: DatabaseConnection) -> Self {
        Self::with_stores(
            config,
            Arc::new(PostgresAuthStore::new(db.clone())),
            Arc::new(PostgresOrganizationStore::new(db.clone())),
            Arc::new(PostgresSpaceStore::new(db.clone())),
            Arc::new(PostgresChannelStore::new(db.clone())),
            Arc::new(PostgresMessageStore::new(db.clone())),
            Arc::new(PostgresPermissionStore::new(db)),
        )
    }

    pub fn with_stores(
        config: AppConfig,
        auth_store: Arc<dyn AuthStore>,
        organization_store: Arc<dyn OrganizationStore>,
        space_store: Arc<dyn SpaceStore>,
        channel_store: Arc<dyn ChannelStore>,
        message_store: Arc<dyn MessageStore>,
        permission_store: Arc<dyn PermissionStore>,
    ) -> Self {
        Self {
            config,
            auth: Arc::new(AuthService::new(auth_store)),
            organizations: Arc::new(OrganizationService::new(organization_store)),
            spaces: Arc::new(SpaceService::new(space_store)),
            channels: Arc::new(ChannelService::new(channel_store)),
            messages: Arc::new(MessageService::new(message_store)),
            permissions: Arc::new(PermissionService::new(permission_store)),
        }
    }
}
