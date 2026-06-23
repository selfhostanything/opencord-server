use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, Value};
use uuid::Uuid;

use crate::domain::auth::{
    AuthError, AuthStore, StoredOidcIdentity, StoredOidcProvider, StoredSession, StoredUser,
};

#[derive(Clone)]
pub struct PostgresAuthStore {
    db: DatabaseConnection,
}

impl PostgresAuthStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl AuthStore for PostgresAuthStore {
    async fn create_user(&self, user: StoredUser) -> Result<(), AuthError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO users (id, email, display_name, password_hash)
                VALUES ($1::uuid, $2, $3, $4)
                "#,
                values(vec![
                    user.id.to_string(),
                    user.email,
                    user.display_name,
                    user.password_hash,
                ]),
            ))
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => Err(AuthError::EmailAlreadyRegistered),
            Err(_) => Err(AuthError::StoreUnavailable),
        }
    }

    async fn find_user_by_id(&self, user_id: Uuid) -> Result<Option<StoredUser>, AuthError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, email, display_name, password_hash
                FROM users
                WHERE id = $1::uuid
                "#,
                values(vec![user_id.to_string()]),
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        row.map(stored_user_from_row).transpose()
    }

    async fn find_user_by_email(&self, email: &str) -> Result<Option<StoredUser>, AuthError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, email, display_name, password_hash
                FROM users
                WHERE email = $1
                "#,
                values(vec![email.to_owned()]),
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        row.map(stored_user_from_row).transpose()
    }

    async fn create_session(&self, session: StoredSession) -> Result<(), AuthError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO user_sessions (id, user_id, token_hash)
                VALUES ($1::uuid, $2::uuid, $3)
                "#,
                values(vec![
                    session.id.to_string(),
                    session.user_id.to_string(),
                    session.token_hash,
                ]),
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        Ok(())
    }

    async fn find_user_by_session_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<StoredUser>, AuthError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT users.id::text, users.email, users.display_name, users.password_hash
                FROM user_sessions
                INNER JOIN users ON users.id = user_sessions.user_id
                WHERE user_sessions.token_hash = $1
                  AND user_sessions.revoked_at IS NULL
                "#,
                values(vec![token_hash.to_owned()]),
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        row.map(stored_user_from_row).transpose()
    }

    async fn revoke_session(&self, token_hash: &str) -> Result<(), AuthError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE user_sessions
                SET revoked_at = now()
                WHERE token_hash = $1
                  AND revoked_at IS NULL
                "#,
                values(vec![token_hash.to_owned()]),
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        if result.rows_affected() == 0 {
            Err(AuthError::Unauthorized)
        } else {
            Ok(())
        }
    }

    async fn upsert_oidc_provider(&self, provider: StoredOidcProvider) -> Result<(), AuthError> {
        let allowed_domains_json = serde_json::to_string(&provider.allowed_domains)
            .map_err(|_| AuthError::StoreUnavailable)?;
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO organization_oidc_providers (
                    organization_id, issuer, authorization_endpoint, token_endpoint,
                    jwks_uri, client_id, client_secret, allowed_domains_json,
                    require_sso, auto_join_role
                )
                VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (organization_id) DO UPDATE SET
                    issuer = EXCLUDED.issuer,
                    authorization_endpoint = EXCLUDED.authorization_endpoint,
                    token_endpoint = EXCLUDED.token_endpoint,
                    jwks_uri = EXCLUDED.jwks_uri,
                    client_id = EXCLUDED.client_id,
                    client_secret = EXCLUDED.client_secret,
                    allowed_domains_json = EXCLUDED.allowed_domains_json,
                    require_sso = EXCLUDED.require_sso,
                    auto_join_role = EXCLUDED.auto_join_role,
                    updated_at = now()
                "#,
                vec![
                    Value::from(provider.organization_id.to_string()),
                    Value::from(provider.issuer),
                    Value::from(provider.authorization_endpoint),
                    Value::from(provider.token_endpoint),
                    Value::from(provider.jwks_uri),
                    Value::from(provider.client_id),
                    Value::from(provider.client_secret),
                    Value::from(allowed_domains_json),
                    Value::from(provider.require_sso),
                    Value::from(provider.auto_join_role),
                ],
            ))
            .await
            .map_err(|error| {
                if is_foreign_key_violation(&error) {
                    AuthError::InvalidInput("organization was not found")
                } else {
                    AuthError::StoreUnavailable
                }
            })?;

        Ok(())
    }

    async fn oidc_provider_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Option<StoredOidcProvider>, AuthError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT organization_id::text, issuer, authorization_endpoint, token_endpoint,
                       jwks_uri, client_id, client_secret, allowed_domains_json,
                       require_sso, auto_join_role
                FROM organization_oidc_providers
                WHERE organization_id = $1::uuid
                "#,
                values(vec![organization_id.to_string()]),
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        row.map(oidc_provider_from_row).transpose()
    }

    async fn oidc_providers_for_email_domain(
        &self,
        domain: &str,
    ) -> Result<Vec<StoredOidcProvider>, AuthError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT organization_id::text, issuer, authorization_endpoint, token_endpoint,
                       jwks_uri, client_id, client_secret, allowed_domains_json,
                       require_sso, auto_join_role
                FROM organization_oidc_providers
                ORDER BY issuer ASC
                "#,
                vec![],
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        rows.into_iter()
            .map(oidc_provider_from_row)
            .filter_map(|provider| match provider {
                Ok(provider)
                    if provider
                        .allowed_domains
                        .iter()
                        .any(|allowed| allowed == domain) =>
                {
                    Some(Ok(provider))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    async fn oidc_provider_for_issuer_and_domain(
        &self,
        issuer: &str,
        domain: &str,
    ) -> Result<Option<StoredOidcProvider>, AuthError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT organization_id::text, issuer, authorization_endpoint, token_endpoint,
                       jwks_uri, client_id, client_secret, allowed_domains_json,
                       require_sso, auto_join_role
                FROM organization_oidc_providers
                WHERE issuer = $1
                ORDER BY issuer ASC
                "#,
                values(vec![issuer.to_owned()]),
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        for row in rows {
            let provider = oidc_provider_from_row(row)?;
            if provider
                .allowed_domains
                .iter()
                .any(|allowed| allowed == domain)
            {
                return Ok(Some(provider));
            }
        }

        Ok(None)
    }

    async fn find_oidc_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<StoredOidcIdentity>, AuthError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, user_id::text, organization_id::text, issuer, subject, email
                FROM user_oidc_identities
                WHERE issuer = $1
                  AND subject = $2
                "#,
                values(vec![issuer.to_owned(), subject.to_owned()]),
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        row.map(oidc_identity_from_row).transpose()
    }

    async fn create_oidc_identity(&self, identity: StoredOidcIdentity) -> Result<(), AuthError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO user_oidc_identities (
                    id, user_id, organization_id, issuer, subject, email
                )
                VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6)
                "#,
                values(vec![
                    identity.id.to_string(),
                    identity.user_id.to_string(),
                    identity.organization_id.to_string(),
                    identity.issuer,
                    identity.subject,
                    identity.email,
                ]),
            ))
            .await
            .map_err(|_| AuthError::StoreUnavailable)?;

        Ok(())
    }
}

fn stored_user_from_row(row: sea_orm::QueryResult) -> Result<StoredUser, AuthError> {
    let id = row
        .try_get::<String>("", "id")
        .map_err(|_| AuthError::StoreUnavailable)?;
    let id = Uuid::parse_str(&id).map_err(|_| AuthError::StoreUnavailable)?;

    Ok(StoredUser {
        id,
        email: row
            .try_get::<String>("", "email")
            .map_err(|_| AuthError::StoreUnavailable)?,
        display_name: row
            .try_get::<String>("", "display_name")
            .map_err(|_| AuthError::StoreUnavailable)?,
        password_hash: row
            .try_get::<String>("", "password_hash")
            .map_err(|_| AuthError::StoreUnavailable)?,
    })
}

fn oidc_provider_from_row(row: sea_orm::QueryResult) -> Result<StoredOidcProvider, AuthError> {
    let organization_id = row
        .try_get::<String>("", "organization_id")
        .map_err(|_| AuthError::StoreUnavailable)?;
    let organization_id =
        Uuid::parse_str(&organization_id).map_err(|_| AuthError::StoreUnavailable)?;
    let allowed_domains_json = row
        .try_get::<String>("", "allowed_domains_json")
        .map_err(|_| AuthError::StoreUnavailable)?;
    let allowed_domains = serde_json::from_str::<Vec<String>>(&allowed_domains_json)
        .map_err(|_| AuthError::StoreUnavailable)?;

    Ok(StoredOidcProvider {
        organization_id,
        issuer: row
            .try_get::<String>("", "issuer")
            .map_err(|_| AuthError::StoreUnavailable)?,
        authorization_endpoint: row
            .try_get::<String>("", "authorization_endpoint")
            .map_err(|_| AuthError::StoreUnavailable)?,
        token_endpoint: row
            .try_get::<String>("", "token_endpoint")
            .map_err(|_| AuthError::StoreUnavailable)?,
        jwks_uri: row
            .try_get::<String>("", "jwks_uri")
            .map_err(|_| AuthError::StoreUnavailable)?,
        client_id: row
            .try_get::<String>("", "client_id")
            .map_err(|_| AuthError::StoreUnavailable)?,
        client_secret: row
            .try_get::<String>("", "client_secret")
            .map_err(|_| AuthError::StoreUnavailable)?,
        allowed_domains,
        require_sso: row
            .try_get::<bool>("", "require_sso")
            .map_err(|_| AuthError::StoreUnavailable)?,
        auto_join_role: row
            .try_get::<String>("", "auto_join_role")
            .map_err(|_| AuthError::StoreUnavailable)?,
    })
}

fn oidc_identity_from_row(row: sea_orm::QueryResult) -> Result<StoredOidcIdentity, AuthError> {
    let id = row
        .try_get::<String>("", "id")
        .map_err(|_| AuthError::StoreUnavailable)?;
    let id = Uuid::parse_str(&id).map_err(|_| AuthError::StoreUnavailable)?;
    let user_id = row
        .try_get::<String>("", "user_id")
        .map_err(|_| AuthError::StoreUnavailable)?;
    let user_id = Uuid::parse_str(&user_id).map_err(|_| AuthError::StoreUnavailable)?;
    let organization_id = row
        .try_get::<String>("", "organization_id")
        .map_err(|_| AuthError::StoreUnavailable)?;
    let organization_id =
        Uuid::parse_str(&organization_id).map_err(|_| AuthError::StoreUnavailable)?;

    Ok(StoredOidcIdentity {
        id,
        user_id,
        organization_id,
        issuer: row
            .try_get::<String>("", "issuer")
            .map_err(|_| AuthError::StoreUnavailable)?,
        subject: row
            .try_get::<String>("", "subject")
            .map_err(|_| AuthError::StoreUnavailable)?,
        email: row
            .try_get::<String>("", "email")
            .map_err(|_| AuthError::StoreUnavailable)?,
    })
}

fn values(values: Vec<String>) -> Vec<Value> {
    values.into_iter().map(Value::from).collect()
}

fn is_unique_violation(error: &DbErr) -> bool {
    error.to_string().contains("duplicate key")
}

fn is_foreign_key_violation(error: &DbErr) -> bool {
    error.to_string().contains("foreign key")
}
