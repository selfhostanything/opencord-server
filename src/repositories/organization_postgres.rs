use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait, Value,
};
use uuid::Uuid;

use crate::domain::organization::{
    CustomDomain, CustomDomainTenant, OrganizationError, OrganizationMembership, OrganizationStore,
    StoredCustomDomain, StoredOrganization, StoredOrganizationMember,
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
                INSERT INTO organizations (
                    id, slug, name, plan, deployment_mode, primary_region
                )
                VALUES ($1::uuid, $2, $3, $4, $5, $6)
                "#,
                values(vec![
                    organization.id.to_string(),
                    organization.slug,
                    organization.name,
                    organization.plan,
                    organization.deployment_mode,
                    organization.primary_region,
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
                       organizations.plan, organizations.deployment_mode,
                       organizations.primary_region, organization_members.role
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
                       organizations.plan, organizations.deployment_mode,
                       organizations.primary_region, organization_members.role
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

    async fn active_member_user_ids(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<Uuid>, OrganizationError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT user_id::text
                FROM organization_members
                WHERE organization_id = $1::uuid
                  AND status = 'active'
                ORDER BY user_id ASC
                "#,
                values(vec![organization_id.to_string()]),
            ))
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        rows.into_iter()
            .map(|row| {
                let user_id = row
                    .try_get::<String>("", "user_id")
                    .map_err(|_| OrganizationError::StoreUnavailable)?;
                Uuid::parse_str(&user_id).map_err(|_| OrganizationError::StoreUnavailable)
            })
            .collect()
    }

    async fn update_plan(
        &self,
        organization_id: Uuid,
        plan: String,
    ) -> Result<(), OrganizationError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE organizations
                SET plan = $2,
                    updated_at = now()
                WHERE id = $1::uuid
                "#,
                values(vec![organization_id.to_string(), plan]),
            ))
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        if result.rows_affected() == 0 {
            Err(OrganizationError::NotFound)
        } else {
            Ok(())
        }
    }

    async fn add_member_if_missing(
        &self,
        member: StoredOrganizationMember,
    ) -> Result<(), OrganizationError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO organization_members (organization_id, user_id, role, status)
                VALUES ($1::uuid, $2::uuid, $3, $4)
                ON CONFLICT (organization_id, user_id)
                DO UPDATE SET status = 'active'
                "#,
                values(vec![
                    member.organization_id.to_string(),
                    member.user_id.to_string(),
                    member.role,
                    member.status,
                ]),
            ))
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        Ok(())
    }

    async fn create_custom_domain(
        &self,
        custom_domain: StoredCustomDomain,
    ) -> Result<(), OrganizationError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO organization_custom_domains (
                    id, organization_id, hostname, verification_token, status
                )
                VALUES ($1::uuid, $2::uuid, $3, $4, $5)
                "#,
                values(vec![
                    custom_domain.id.to_string(),
                    custom_domain.organization_id.to_string(),
                    custom_domain.hostname,
                    custom_domain.verification_token,
                    custom_domain.status,
                ]),
            ))
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => {
                Err(OrganizationError::CustomDomainAlreadyExists)
            }
            Err(error) if is_foreign_key_violation(&error) => Err(OrganizationError::NotFound),
            Err(_) => Err(OrganizationError::StoreUnavailable),
        }
    }

    async fn list_custom_domains(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<CustomDomain>, OrganizationError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, hostname, verification_token, status
                FROM organization_custom_domains
                WHERE organization_id = $1::uuid
                ORDER BY hostname ASC
                "#,
                values(vec![organization_id.to_string()]),
            ))
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        rows.into_iter()
            .map(custom_domain_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn verify_custom_domain(
        &self,
        organization_id: Uuid,
        custom_domain_id: Uuid,
        verification_token: String,
    ) -> Result<CustomDomain, OrganizationError> {
        let custom_domain = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT id::text, organization_id::text, hostname, verification_token, status
                FROM organization_custom_domains
                WHERE id = $1::uuid
                  AND organization_id = $2::uuid
                "#,
                values(vec![
                    custom_domain_id.to_string(),
                    organization_id.to_string(),
                ]),
            ))
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?
            .map(custom_domain_from_row)
            .transpose()?
            .ok_or(OrganizationError::NotFound)?;

        if custom_domain.verification_token != verification_token {
            return Err(OrganizationError::InvalidInput(
                "custom domain verification token is invalid",
            ));
        }

        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE organization_custom_domains
                SET status = 'active',
                    verified_at = now(),
                    updated_at = now()
                WHERE id = $1::uuid
                  AND organization_id = $2::uuid
                  AND verification_token = $3
                RETURNING id::text, organization_id::text, hostname, verification_token, status
                "#,
                values(vec![
                    custom_domain_id.to_string(),
                    organization_id.to_string(),
                    custom_domain.verification_token,
                ]),
            ))
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        row.map(custom_domain_from_row)
            .transpose()?
            .ok_or(OrganizationError::NotFound)
    }

    async fn resolve_custom_domain(
        &self,
        hostname: String,
    ) -> Result<Option<CustomDomainTenant>, OrganizationError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT organizations.id::text AS organization_id,
                       organizations.slug,
                       organizations.name,
                       organizations.plan,
                       organizations.deployment_mode,
                       organizations.primary_region,
                       organization_custom_domains.hostname
                FROM organization_custom_domains
                INNER JOIN organizations
                    ON organizations.id = organization_custom_domains.organization_id
                WHERE organization_custom_domains.hostname = $1
                  AND organization_custom_domains.status = 'active'
                  AND organizations.suspended_at IS NULL
                "#,
                values(vec![hostname]),
            ))
            .await
            .map_err(|_| OrganizationError::StoreUnavailable)?;

        row.map(custom_domain_tenant_from_row).transpose()
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
        plan: row
            .try_get::<String>("", "plan")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        deployment_mode: row
            .try_get::<String>("", "deployment_mode")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        primary_region: row
            .try_get::<String>("", "primary_region")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
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

fn custom_domain_from_row(row: sea_orm::QueryResult) -> Result<CustomDomain, OrganizationError> {
    let id = row
        .try_get::<String>("", "id")
        .map_err(|_| OrganizationError::StoreUnavailable)?;
    let id = Uuid::parse_str(&id).map_err(|_| OrganizationError::StoreUnavailable)?;
    let organization_id = row
        .try_get::<String>("", "organization_id")
        .map_err(|_| OrganizationError::StoreUnavailable)?;
    let organization_id =
        Uuid::parse_str(&organization_id).map_err(|_| OrganizationError::StoreUnavailable)?;

    Ok(CustomDomain {
        id,
        organization_id,
        hostname: row
            .try_get::<String>("", "hostname")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        verification_token: row
            .try_get::<String>("", "verification_token")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        status: row
            .try_get::<String>("", "status")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
    })
}

fn custom_domain_tenant_from_row(
    row: sea_orm::QueryResult,
) -> Result<CustomDomainTenant, OrganizationError> {
    let organization_id = row
        .try_get::<String>("", "organization_id")
        .map_err(|_| OrganizationError::StoreUnavailable)?;
    let organization_id =
        Uuid::parse_str(&organization_id).map_err(|_| OrganizationError::StoreUnavailable)?;

    Ok(CustomDomainTenant {
        organization_id,
        slug: row
            .try_get::<String>("", "slug")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        name: row
            .try_get::<String>("", "name")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        plan: row
            .try_get::<String>("", "plan")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        deployment_mode: row
            .try_get::<String>("", "deployment_mode")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        primary_region: row
            .try_get::<String>("", "primary_region")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
        hostname: row
            .try_get::<String>("", "hostname")
            .map_err(|_| OrganizationError::StoreUnavailable)?,
    })
}
