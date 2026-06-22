use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait, Value,
};
use uuid::Uuid;

use crate::domain::space::{
    SpaceError, SpaceMembership, SpaceStore, StoredSpace, StoredSpaceMember,
};

#[derive(Clone)]
pub struct PostgresSpaceStore {
    db: DatabaseConnection,
}

impl PostgresSpaceStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl SpaceStore for PostgresSpaceStore {
    async fn create_space(
        &self,
        space: StoredSpace,
        owner_member: StoredSpaceMember,
    ) -> Result<(), SpaceError> {
        let txn = self
            .db
            .begin()
            .await
            .map_err(|_| SpaceError::StoreUnavailable)?;

        let result = txn
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO spaces (id, organization_id, slug, name, owner_user_id)
                VALUES ($1::uuid, $2::uuid, $3, $4, $5::uuid)
                "#,
                values(vec![
                    space.id.to_string(),
                    space.organization_id.to_string(),
                    space.slug,
                    space.name,
                    space.owner_user_id.to_string(),
                ]),
            ))
            .await;

        match result {
            Ok(_) => {}
            Err(error) if is_unique_violation(&error) => {
                return Err(SpaceError::SlugAlreadyExists);
            }
            Err(_) => return Err(SpaceError::StoreUnavailable),
        }

        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r#"
            INSERT INTO space_members (space_id, user_id, role, status)
            VALUES ($1::uuid, $2::uuid, $3, $4)
            "#,
            values(vec![
                owner_member.space_id.to_string(),
                owner_member.user_id.to_string(),
                owner_member.role,
                owner_member.status,
            ]),
        ))
        .await
        .map_err(|_| SpaceError::StoreUnavailable)?;

        txn.commit().await.map_err(|_| SpaceError::StoreUnavailable)
    }

    async fn list_for_user(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<Vec<SpaceMembership>, SpaceError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT spaces.id::text, spaces.organization_id::text, spaces.slug,
                       spaces.name, space_members.role
                FROM space_members
                INNER JOIN spaces ON spaces.id = space_members.space_id
                WHERE space_members.user_id = $1::uuid
                  AND space_members.status = 'active'
                  AND spaces.organization_id = $2::uuid
                  AND spaces.archived_at IS NULL
                ORDER BY spaces.name ASC
                "#,
                values(vec![user_id.to_string(), organization_id.to_string()]),
            ))
            .await
            .map_err(|_| SpaceError::StoreUnavailable)?;

        rows.into_iter()
            .map(space_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn get_for_user(
        &self,
        user_id: Uuid,
        space_id: Uuid,
    ) -> Result<Option<SpaceMembership>, SpaceError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT spaces.id::text, spaces.organization_id::text, spaces.slug,
                       spaces.name, space_members.role
                FROM space_members
                INNER JOIN spaces ON spaces.id = space_members.space_id
                WHERE space_members.user_id = $1::uuid
                  AND space_members.status = 'active'
                  AND spaces.id = $2::uuid
                  AND spaces.archived_at IS NULL
                "#,
                values(vec![user_id.to_string(), space_id.to_string()]),
            ))
            .await
            .map_err(|_| SpaceError::StoreUnavailable)?;

        row.map(space_from_row).transpose()
    }

    async fn add_member(&self, member: StoredSpaceMember) -> Result<SpaceMembership, SpaceError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO space_members (space_id, user_id, role, status)
                VALUES ($1::uuid, $2::uuid, $3, $4)
                ON CONFLICT (space_id, user_id)
                DO UPDATE SET
                    role = CASE
                        WHEN space_members.role = 'owner' THEN space_members.role
                        ELSE EXCLUDED.role
                    END,
                    status = 'active'
                "#,
                values(vec![
                    member.space_id.to_string(),
                    member.user_id.to_string(),
                    member.role,
                    member.status,
                ]),
            ))
            .await
            .map_err(|_| SpaceError::StoreUnavailable)?;

        self.get_for_user(member.user_id, member.space_id)
            .await?
            .ok_or(SpaceError::NotFound)
    }
}

fn space_from_row(row: sea_orm::QueryResult) -> Result<SpaceMembership, SpaceError> {
    let id = row
        .try_get::<String>("", "id")
        .map_err(|_| SpaceError::StoreUnavailable)?;
    let id = Uuid::parse_str(&id).map_err(|_| SpaceError::StoreUnavailable)?;

    let organization_id = row
        .try_get::<String>("", "organization_id")
        .map_err(|_| SpaceError::StoreUnavailable)?;
    let organization_id =
        Uuid::parse_str(&organization_id).map_err(|_| SpaceError::StoreUnavailable)?;

    Ok(SpaceMembership {
        id,
        organization_id,
        slug: row
            .try_get::<String>("", "slug")
            .map_err(|_| SpaceError::StoreUnavailable)?,
        name: row
            .try_get::<String>("", "name")
            .map_err(|_| SpaceError::StoreUnavailable)?,
        role: row
            .try_get::<String>("", "role")
            .map_err(|_| SpaceError::StoreUnavailable)?,
    })
}

fn values(values: Vec<String>) -> Vec<Value> {
    values.into_iter().map(Value::from).collect()
}

fn is_unique_violation(error: &DbErr) -> bool {
    error.to_string().contains("duplicate key")
}
