use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait, Value,
};
use uuid::Uuid;

use crate::domain::organization::{
    OrganizationError, OrganizationMembership, OrganizationStore, StoredOrganization,
    StoredOrganizationMember,
};

#[derive(Clone)]
pub struct PostgresOrganizationStore {
    db: DatabaseConnection,
}

impl PostgresOrganizationStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl OrganizationStore for PostgresOrganizationStore {
    async fn create_organization(
        &self,
        organization: StoredOrganization,
        owner_member: StoredOrganizationMember,
    ) -> Result<(), OrganizationError> {
        let txn = self
            .db
            .begin()
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        let result = txn
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO organizations (id, slug, name)
                VALUES ($1::uuid, $2, $3)
                "#,
                values(vec![
                    organization.id.to_string(),
                    organization.slug,
                    organization.name,
                ]),
            ))
            .await;

        match result {
            Ok(_) => {}
            Err(error) if is_unique_violation(&error) => {
                return Err(OrganizationError::SlugAlreadyExists);
            }
            Err(_) => return Err(OrganizationError::StoreUnavailable),
        }

        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r#"
            INSERT INTO organization_members (organization_id, user_id, role, status)
            VALUES ($1::uuid, $2::uuid, $3, $4)
            "#,
            values(vec![
                owner_member.organization_id.to_string(),
                owner_member.user_id.to_string(),
                owner_member.role,
                owner_member.status,
            ]),
        ))
        .await
        .map_err(|_| OrganizationError::StoreUnavailable)?;

        txn.commit()
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)
    }

    async fn list_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<OrganizationMembership>, OrganizationError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT organizations.id::text, organizations.slug, organizations.name,
                       organization_members.role
                FROM organization_members
                INNER JOIN organizations
                    ON organizations.id = organization_members.organization_id
                WHERE organization_members.user_id = $1::uuid
                  AND organization_members.status = 'active'
                  AND organizations.suspended_at IS NULL
                ORDER BY organizations.name ASC
                "#,
                values(vec![user_id.to_string()]),
            ))
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        rows.into_iter()
            .map(organization_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn get_for_user(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
    ) -> Result<Option<OrganizationMembership>, OrganizationError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT organizations.id::text, organizations.slug, organizations.name,
                       organization_members.role
                FROM organization_members
                INNER JOIN organizations
                    ON organizations.id = organization_members.organization_id
                WHERE organization_members.user_id = $1::uuid
                  AND organization_members.organization_id = $2::uuid
                  AND organization_members.status = 'active'
                  AND organizations.suspended_at IS NULL
                "#,
                values(vec![user_id.to_string(), organization_id.to_string()]),
            ))
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        row.map(organization_from_row).transpose()
    }
}

fn organization_from_row(
    row: sea_orm::QueryResult,
) -> Result<OrganizationMembership, OrganizationError> {
    let id = row
        .try_get::<String>("", "id")
        .map_err(|_| OrganizationError::StoreUnavailable)?;
    let id = Uuid::parse_str(&id).map_err(|_| OrganizationError::StoreUnavailable)?;

    Ok(OrganizationMembership {
        id,
        slug: row
            .try_get::<String>("", "slug")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        name: row
            .try_get::<String>("", "name")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        role: row
            .try_get::<String>("", "role")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
    })
}

fn values(values: Vec<String>) -> Vec<Value> {
    values.into_iter().map(Value::from).collect()
}

fn is_unique_violation(error: &DbErr) -> bool {
    error.to_string().contains("duplicate key")
}
