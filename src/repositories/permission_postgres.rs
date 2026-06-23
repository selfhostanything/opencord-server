use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, Value};
use uuid::Uuid;

use crate::domain::permission::{
    AssignedRole, ChannelPermissionOverride, PermissionError, PermissionStore,
    PermissionTargetKind, Role, RoleAssignment,
};

#[derive(Clone)]
pub struct PostgresPermissionStore {
    db: DatabaseConnection,
}

impl PostgresPermissionStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl PermissionStore for PostgresPermissionStore {
    async fn create_role(&self, role: Role) -> Result<(), PermissionError> {
        let result = self
            .db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO roles (
                    id, organization_id, space_id, name, color, position, permissions_bitset
                )
                VALUES ($1::uuid, $2::uuid, $3::uuid, $4, NULLIF($5, ''), $6, $7)
                "#,
                role_values(&role)?,
            ))
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => Err(PermissionError::RoleAlreadyExists),
            Err(_) => Err(PermissionError::StoreUnavailable),
        }
    }

    async fn get_role(
        &self,
        space_id: Uuid,
        role_id: Uuid,
    ) -> Result<Option<Role>, PermissionError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                role_select_sql(
                    r#"
                    WHERE space_id = $1::uuid
                      AND id = $2::uuid
                    "#,
                ),
                vec![
                    Value::from(space_id.to_string()),
                    Value::from(role_id.to_string()),
                ],
            ))
            .await
            .map_err(|_| PermissionError::StoreUnavailable)?;

        row.map(role_from_row).transpose()
    }

    async fn list_roles_for_space(&self, space_id: Uuid) -> Result<Vec<Role>, PermissionError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                role_select_sql(
                    r#"
                    WHERE space_id = $1::uuid
                    ORDER BY position ASC, name ASC
                    "#,
                ),
                vec![Value::from(space_id.to_string())],
            ))
            .await
            .map_err(|_| PermissionError::StoreUnavailable)?;

        rows.into_iter()
            .map(role_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn assign_role(&self, assignment: RoleAssignment) -> Result<(), PermissionError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO role_assignments (
                    space_id, role_id, user_id, assigned_by_user_id
                )
                VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid)
                ON CONFLICT (role_id, user_id) DO NOTHING
                "#,
                vec![
                    Value::from(assignment.space_id.to_string()),
                    Value::from(assignment.role_id.to_string()),
                    Value::from(assignment.user_id.to_string()),
                    Value::from(assignment.assigned_by_user_id.to_string()),
                ],
            ))
            .await
            .map_err(|_| PermissionError::StoreUnavailable)?;

        Ok(())
    }

    async fn assigned_roles_for_user(
        &self,
        space_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<AssignedRole>, PermissionError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT roles.id::text, roles.permissions_bitset
                FROM role_assignments
                INNER JOIN roles ON roles.id = role_assignments.role_id
                WHERE role_assignments.space_id = $1::uuid
                  AND role_assignments.user_id = $2::uuid
                ORDER BY roles.position ASC, roles.name ASC
                "#,
                vec![
                    Value::from(space_id.to_string()),
                    Value::from(user_id.to_string()),
                ],
            ))
            .await
            .map_err(|_| PermissionError::StoreUnavailable)?;

        rows.into_iter()
            .map(assigned_role_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn upsert_channel_override(
        &self,
        permission_override: ChannelPermissionOverride,
    ) -> Result<(), PermissionError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO channel_permission_overrides (
                    channel_id, target_kind, target_id, allow_bitset, deny_bitset
                )
                VALUES ($1::uuid, $2, $3::uuid, $4, $5)
                ON CONFLICT (channel_id, target_kind, target_id)
                DO UPDATE SET
                    allow_bitset = EXCLUDED.allow_bitset,
                    deny_bitset = EXCLUDED.deny_bitset,
                    updated_at = now()
                "#,
                override_values(&permission_override)?,
            ))
            .await
            .map_err(|_| PermissionError::StoreUnavailable)?;

        Ok(())
    }

    async fn channel_overrides_for_user(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
        role_ids: &[Uuid],
    ) -> Result<Vec<ChannelPermissionOverride>, PermissionError> {
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT channel_id::text, target_kind, target_id::text,
                       allow_bitset, deny_bitset
                FROM channel_permission_overrides
                WHERE channel_id = $1::uuid
                "#,
                vec![Value::from(channel_id.to_string())],
            ))
            .await
            .map_err(|_| PermissionError::StoreUnavailable)?;

        rows.into_iter()
            .map(override_from_row)
            .filter_map(|result| match result {
                Ok(permission_override)
                    if permission_override.target_id == user_id
                        || role_ids.contains(&permission_override.target_id) =>
                {
                    Some(Ok(permission_override))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

fn role_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT id::text, organization_id::text, space_id::text, name,
               color, position, permissions_bitset
        FROM roles
        {where_clause}
        "#
    )
}

fn role_from_row(row: sea_orm::QueryResult) -> Result<Role, PermissionError> {
    Ok(Role {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
        space_id: parse_uuid(
            &row.try_get::<String>("", "space_id")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
        name: row
            .try_get::<String>("", "name")
            .map_err(|_| PermissionError::StoreUnavailable)?,
        color: row
            .try_get::<Option<String>>("", "color")
            .map_err(|_| PermissionError::StoreUnavailable)?,
        position: row
            .try_get::<i32>("", "position")
            .map_err(|_| PermissionError::StoreUnavailable)?,
        permissions_bitset: bitset_from_i64(
            row.try_get::<i64>("", "permissions_bitset")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
    })
}

fn assigned_role_from_row(row: sea_orm::QueryResult) -> Result<AssignedRole, PermissionError> {
    Ok(AssignedRole {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
        permissions_bitset: bitset_from_i64(
            row.try_get::<i64>("", "permissions_bitset")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
    })
}

fn override_from_row(
    row: sea_orm::QueryResult,
) -> Result<ChannelPermissionOverride, PermissionError> {
    Ok(ChannelPermissionOverride {
        channel_id: parse_uuid(
            &row.try_get::<String>("", "channel_id")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
        target_kind: PermissionTargetKind::parse(
            &row.try_get::<String>("", "target_kind")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
        target_id: parse_uuid(
            &row.try_get::<String>("", "target_id")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
        allow_bitset: bitset_from_i64(
            row.try_get::<i64>("", "allow_bitset")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
        deny_bitset: bitset_from_i64(
            row.try_get::<i64>("", "deny_bitset")
                .map_err(|_| PermissionError::StoreUnavailable)?,
        )?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, PermissionError> {
    Uuid::parse_str(value).map_err(|_| PermissionError::StoreUnavailable)
}

fn role_values(role: &Role) -> Result<Vec<Value>, PermissionError> {
    Ok(vec![
        Value::from(role.id.to_string()),
        Value::from(role.organization_id.to_string()),
        Value::from(role.space_id.to_string()),
        Value::from(role.name.clone()),
        Value::from(role.color.clone().unwrap_or_default()),
        Value::from(role.position),
        Value::from(bitset_to_i64(role.permissions_bitset)?),
    ])
}

fn override_values(
    permission_override: &ChannelPermissionOverride,
) -> Result<Vec<Value>, PermissionError> {
    Ok(vec![
        Value::from(permission_override.channel_id.to_string()),
        Value::from(permission_override.target_kind.as_str()),
        Value::from(permission_override.target_id.to_string()),
        Value::from(bitset_to_i64(permission_override.allow_bitset)?),
        Value::from(bitset_to_i64(permission_override.deny_bitset)?),
    ])
}

fn bitset_to_i64(bitset: u64) -> Result<i64, PermissionError> {
    i64::try_from(bitset).map_err(|_| PermissionError::StoreUnavailable)
}

fn bitset_from_i64(bitset: i64) -> Result<u64, PermissionError> {
    u64::try_from(bitset).map_err(|_| PermissionError::StoreUnavailable)
}

fn is_unique_violation(error: &DbErr) -> bool {
    error.to_string().contains("duplicate key")
}
