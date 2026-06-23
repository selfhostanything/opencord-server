use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};
use uuid::Uuid;

use crate::domain::scim::{ScimError, ScimStore, StoredScimToken, StoredScimUser};

#[derive(Clone)]
pub struct PostgresScimStore {
    db: DatabaseConnection,
}

impl PostgresScimStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl ScimStore for PostgresScimStore {
    async fn rotate_token(&self, token: StoredScimToken) -> Result<(), ScimError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO organization_scim_tokens (organization_id, token_hash)
                VALUES ($1::uuid, $2)
                ON CONFLICT (organization_id) DO UPDATE SET
                    token_hash = EXCLUDED.token_hash,
                    updated_at = now()
                "#,
                values(vec![token.organization_id.to_string(), token.token_hash]),
            ))
            .await
            .map_err(|_| ScimError::StoreUnavailable)?;

        Ok(())
    }

    async fn token_by_hash(&self, token_hash: &str) -> Result<Option<StoredScimToken>, ScimError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT organization_id::text, token_hash
                FROM organization_scim_tokens
                WHERE token_hash = $1
                "#,
                values(vec![token_hash.to_owned()]),
            ))
            .await
            .map_err(|_| ScimError::StoreUnavailable)?;

        row.map(scim_token_from_row).transpose()
    }

    async fn upsert_user(&self, user: StoredScimUser) -> Result<StoredScimUser, ScimError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO scim_users (
                    id, organization_id, user_id, external_id, user_name, display_name, active
                )
                VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7)
                ON CONFLICT (organization_id, external_id) DO UPDATE SET
                    user_id = EXCLUDED.user_id,
                    user_name = EXCLUDED.user_name,
                    display_name = EXCLUDED.display_name,
                    active = EXCLUDED.active,
                    updated_at = now()
                RETURNING id::text, organization_id::text, user_id::text, external_id,
                          user_name, display_name, active
                "#,
                vec![
                    Value::from(user.id.to_string()),
                    Value::from(user.organization_id.to_string()),
                    Value::from(user.user_id.to_string()),
                    Value::from(user.external_id),
                    Value::from(user.user_name),
                    Value::from(user.display_name),
                    Value::from(user.active),
                ],
            ))
            .await
            .map_err(|_| ScimError::StoreUnavailable)?;

        row.map(scim_user_from_row)
            .transpose()?
            .ok_or(ScimError::StoreUnavailable)
    }

    async fn user_by_external_id(
        &self,
        organization_id: Uuid,
        external_id: &str,
    ) -> Result<Option<StoredScimUser>, ScimError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, user_id::text, external_id,
                       user_name, display_name, active
                FROM scim_users
                WHERE organization_id = $1::uuid
                  AND external_id = $2
                "#,
                values(vec![organization_id.to_string(), external_id.to_owned()]),
            ))
            .await
            .map_err(|_| ScimError::StoreUnavailable)?;

        row.map(scim_user_from_row).transpose()
    }

    async fn set_user_active(
        &self,
        organization_id: Uuid,
        external_id: &str,
        active: bool,
    ) -> Result<StoredScimUser, ScimError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE scim_users
                SET active = $3,
                    updated_at = now()
                WHERE organization_id = $1::uuid
                  AND external_id = $2
                RETURNING id::text, organization_id::text, user_id::text, external_id,
                          user_name, display_name, active
                "#,
                vec![
                    Value::from(organization_id.to_string()),
                    Value::from(external_id.to_owned()),
                    Value::from(active),
                ],
            ))
            .await
            .map_err(|_| ScimError::StoreUnavailable)?;

        row.map(scim_user_from_row)
            .transpose()?
            .ok_or(ScimError::NotFound)
    }
}

fn scim_token_from_row(row: sea_orm::QueryResult) -> Result<StoredScimToken, ScimError> {
    let organization_id = row
        .try_get::<String>("", "organization_id")
        .map_err(|_| ScimError::StoreUnavailable)?;
    let organization_id =
        Uuid::parse_str(&organization_id).map_err(|_| ScimError::StoreUnavailable)?;

    Ok(StoredScimToken {
        organization_id,
        token_hash: row
            .try_get::<String>("", "token_hash")
            .map_err(|_| ScimError::StoreUnavailable)?,
    })
}

fn scim_user_from_row(row: sea_orm::QueryResult) -> Result<StoredScimUser, ScimError> {
    let id = row
        .try_get::<String>("", "id")
        .map_err(|_| ScimError::StoreUnavailable)?;
    let id = Uuid::parse_str(&id).map_err(|_| ScimError::StoreUnavailable)?;
    let organization_id = row
        .try_get::<String>("", "organization_id")
        .map_err(|_| ScimError::StoreUnavailable)?;
    let organization_id =
        Uuid::parse_str(&organization_id).map_err(|_| ScimError::StoreUnavailable)?;
    let user_id = row
        .try_get::<String>("", "user_id")
        .map_err(|_| ScimError::StoreUnavailable)?;
    let user_id = Uuid::parse_str(&user_id).map_err(|_| ScimError::StoreUnavailable)?;

    Ok(StoredScimUser {
        id,
        organization_id,
        user_id,
        external_id: row
            .try_get::<String>("", "external_id")
            .map_err(|_| ScimError::StoreUnavailable)?,
        user_name: row
            .try_get::<String>("", "user_name")
            .map_err(|_| ScimError::StoreUnavailable)?,
        display_name: row
            .try_get::<String>("", "display_name")
            .map_err(|_| ScimError::StoreUnavailable)?,
        active: row
            .try_get::<bool>("", "active")
            .map_err(|_| ScimError::StoreUnavailable)?,
    })
}

fn values(values: Vec<String>) -> Vec<Value> {
    values.into_iter().map(Value::from).collect()
}
