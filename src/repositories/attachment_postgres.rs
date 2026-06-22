use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};
use uuid::Uuid;

use crate::domain::attachment::{
    Attachment, AttachmentContent, AttachmentError, AttachmentStatus, AttachmentStore,
};

#[derive(Clone)]
pub struct PostgresAttachmentStore {
    db: DatabaseConnection,
}

impl PostgresAttachmentStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl AttachmentStore for PostgresAttachmentStore {
    async fn create_attachment(&self, attachment: Attachment) -> Result<(), AttachmentError> {
        self.db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                INSERT INTO attachments (
                    id, organization_id, space_id, channel_id, message_id,
                    uploader_user_id, file_name, content_type, size_bytes, status
                )
                VALUES (
                    $1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid,
                    $6::uuid, $7, $8, $9, $10
                )
                "#,
                attachment_values(&attachment),
            ))
            .await
            .map_err(|_| AttachmentError::StoreUnavailable)?;

        Ok(())
    }

    async fn get_attachment(
        &self,
        attachment_id: Uuid,
    ) -> Result<Option<Attachment>, AttachmentError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                attachment_select_sql(
                    r#"
                    WHERE id = $1::uuid
                    "#,
                ),
                vec![Value::from(attachment_id.to_string())],
            ))
            .await
            .map_err(|_| AttachmentError::StoreUnavailable)?;

        row.map(attachment_from_row).transpose()
    }

    async fn upload_content(
        &self,
        attachment: Attachment,
        content: AttachmentContent,
    ) -> Result<Attachment, AttachmentError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                UPDATE attachments
                SET content = decode($3, 'hex'),
                    status = 'uploaded',
                    uploaded_at = now()
                WHERE id = $1::uuid
                  AND uploader_user_id = $2::uuid
                  AND message_id IS NULL
                RETURNING id::text, organization_id::text, space_id::text, channel_id::text,
                          message_id::text, uploader_user_id::text, file_name, content_type,
                          size_bytes, status
                "#,
                vec![
                    Value::from(attachment.id.to_string()),
                    Value::from(attachment.uploader_user_id.to_string()),
                    Value::from(hex::encode(content.bytes)),
                ],
            ))
            .await
            .map_err(|_| AttachmentError::StoreUnavailable)?;

        row.map(attachment_from_row)
            .transpose()?
            .ok_or(AttachmentError::NotFound)
    }

    async fn content_for_attachment(
        &self,
        attachment_id: Uuid,
    ) -> Result<Option<AttachmentContent>, AttachmentError> {
        let row = self
            .db
            .query_one(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                r#"
                SELECT content_type, encode(content, 'hex') AS content_hex
                FROM attachments
                WHERE id = $1::uuid
                  AND content IS NOT NULL
                "#,
                vec![Value::from(attachment_id.to_string())],
            ))
            .await
            .map_err(|_| AttachmentError::StoreUnavailable)?;

        row.map(|row| {
            let content_type = row
                .try_get::<String>("", "content_type")
                .map_err(|_| AttachmentError::StoreUnavailable)?;
            let content_hex = row
                .try_get::<String>("", "content_hex")
                .map_err(|_| AttachmentError::StoreUnavailable)?;
            let bytes = hex::decode(content_hex).map_err(|_| AttachmentError::StoreUnavailable)?;
            Ok(AttachmentContent {
                content_type,
                bytes,
            })
        })
        .transpose()
    }

    async fn link_attachments_to_message(
        &self,
        message_id: Uuid,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, AttachmentError> {
        let mut attachments = Vec::with_capacity(attachment_ids.len());

        for attachment_id in attachment_ids {
            let row = self
                .db
                .query_one(Statement::from_sql_and_values(
                    DatabaseBackend::Postgres,
                    r#"
                    UPDATE attachments
                    SET message_id = $2::uuid,
                        status = 'linked'
                    WHERE id = $1::uuid
                      AND message_id IS NULL
                      AND status = 'uploaded'
                    RETURNING id::text, organization_id::text, space_id::text, channel_id::text,
                              message_id::text, uploader_user_id::text, file_name, content_type,
                              size_bytes, status
                    "#,
                    vec![
                        Value::from(attachment_id.to_string()),
                        Value::from(message_id.to_string()),
                    ],
                ))
                .await
                .map_err(|_| AttachmentError::StoreUnavailable)?;

            attachments.push(
                row.map(attachment_from_row)
                    .transpose()?
                    .ok_or(AttachmentError::NotFound)?,
            );
        }

        Ok(attachments)
    }

    async fn list_for_message_ids(
        &self,
        message_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, AttachmentError> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = (1..=message_ids.len())
            .map(|index| format!("${index}::uuid"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = message_ids
            .iter()
            .map(|message_id| Value::from(message_id.to_string()))
            .collect::<Vec<_>>();
        let rows = self
            .db
            .query_all(Statement::from_sql_and_values(
                DatabaseBackend::Postgres,
                attachment_select_sql(&format!(
                    r#"
                    WHERE message_id IN ({placeholders})
                    ORDER BY id ASC
                    "#
                )),
                values,
            ))
            .await
            .map_err(|_| AttachmentError::StoreUnavailable)?;

        rows.into_iter()
            .map(attachment_from_row)
            .collect::<Result<Vec<_>, _>>()
    }
}

fn attachment_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT id::text, organization_id::text, space_id::text, channel_id::text,
               message_id::text, uploader_user_id::text, file_name, content_type,
               size_bytes, status
        FROM attachments
        {where_clause}
        "#
    )
}

fn attachment_from_row(row: sea_orm::QueryResult) -> Result<Attachment, AttachmentError> {
    let message_id = row
        .try_get::<Option<String>>("", "message_id")
        .map_err(|_| AttachmentError::StoreUnavailable)?
        .map(|id| Uuid::parse_str(&id).map_err(|_| AttachmentError::StoreUnavailable))
        .transpose()?;

    let status = row
        .try_get::<String>("", "status")
        .map_err(|_| AttachmentError::StoreUnavailable)?;

    Ok(Attachment {
        id: parse_uuid(
            &row.try_get::<String>("", "id")
                .map_err(|_| AttachmentError::StoreUnavailable)?,
        )?,
        organization_id: parse_uuid(
            &row.try_get::<String>("", "organization_id")
                .map_err(|_| AttachmentError::StoreUnavailable)?,
        )?,
        space_id: parse_uuid(
            &row.try_get::<String>("", "space_id")
                .map_err(|_| AttachmentError::StoreUnavailable)?,
        )?,
        channel_id: parse_uuid(
            &row.try_get::<String>("", "channel_id")
                .map_err(|_| AttachmentError::StoreUnavailable)?,
        )?,
        message_id,
        uploader_user_id: parse_uuid(
            &row.try_get::<String>("", "uploader_user_id")
                .map_err(|_| AttachmentError::StoreUnavailable)?,
        )?,
        file_name: row
            .try_get::<String>("", "file_name")
            .map_err(|_| AttachmentError::StoreUnavailable)?,
        content_type: row
            .try_get::<String>("", "content_type")
            .map_err(|_| AttachmentError::StoreUnavailable)?,
        size_bytes: row
            .try_get::<i64>("", "size_bytes")
            .map_err(|_| AttachmentError::StoreUnavailable)?,
        status: AttachmentStatus::parse(&status)?,
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, AttachmentError> {
    Uuid::parse_str(value).map_err(|_| AttachmentError::StoreUnavailable)
}

fn attachment_values(attachment: &Attachment) -> Vec<Value> {
    vec![
        Value::from(attachment.id.to_string()),
        Value::from(attachment.organization_id.to_string()),
        Value::from(attachment.space_id.to_string()),
        Value::from(attachment.channel_id.to_string()),
        Value::from(attachment.message_id.map(|id| id.to_string())),
        Value::from(attachment.uploader_user_id.to_string()),
        Value::from(attachment.file_name.clone()),
        Value::from(attachment.content_type.clone()),
        Value::from(attachment.size_bytes),
        Value::from(attachment.status.as_str()),
    ]
}
