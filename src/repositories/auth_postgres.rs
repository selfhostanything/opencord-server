use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, Value};
use uuid::Uuid;

use crate::domain::auth::{AuthError, AuthStore, StoredSession, StoredUser};

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

fn values(values: Vec<String>) -> Vec<Value> {
    values.into_iter().map(Value::from).collect()
}

fn is_unique_violation(error: &DbErr) -> bool {
    error.to_string().contains("duplicate key")
}
