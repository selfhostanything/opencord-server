use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::config::AppConfig;
use crate::domain::auth::{AuthService, AuthStore};
use crate::domain::organization::{OrganizationService, OrganizationStore};
use crate::repositories::auth_memory::MemoryAuthStore;
use crate::repositories::auth_postgres::PostgresAuthStore;
use crate::repositories::organization_memory::MemoryOrganizationStore;
use crate::repositories::organization_postgres::PostgresOrganizationStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub auth: Arc<AuthService>,
    pub organizations: Arc<OrganizationService>,
}

impl AppState {
    pub fn in_memory(config: AppConfig) -> Self {
        Self::with_stores(
            config,
            Arc::new(MemoryAuthStore::default()),
            Arc::new(MemoryOrganizationStore::default()),
        )
    }

    pub fn with_database(config: AppConfig, db: DatabaseConnection) -> Self {
        Self::with_stores(
            config,
            Arc::new(PostgresAuthStore::new(db.clone())),
            Arc::new(PostgresOrganizationStore::new(db)),
        )
    }

    pub fn with_stores(
        config: AppConfig,
        auth_store: Arc<dyn AuthStore>,
        organization_store: Arc<dyn OrganizationStore>,
    ) -> Self {
        Self {
            config,
            auth: Arc::new(AuthService::new(auth_store)),
            organizations: Arc::new(OrganizationService::new(organization_store)),
        }
    }
}
