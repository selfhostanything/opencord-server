use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::config::AppConfig;
use crate::domain::auth::{AuthService, AuthStore};
use crate::repositories::auth_memory::MemoryAuthStore;
use crate::repositories::auth_postgres::PostgresAuthStore;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub auth: Arc<AuthService>,
}

impl AppState {
    pub fn in_memory(config: AppConfig) -> Self {
        Self::with_auth_store(config, Arc::new(MemoryAuthStore::default()))
    }

    pub fn with_database(config: AppConfig, db: DatabaseConnection) -> Self {
        Self::with_auth_store(config, Arc::new(PostgresAuthStore::new(db)))
    }

    pub fn with_auth_store(config: AppConfig, store: Arc<dyn AuthStore>) -> Self {
        Self {
            config,
            auth: Arc::new(AuthService::new(store)),
        }
    }
}
