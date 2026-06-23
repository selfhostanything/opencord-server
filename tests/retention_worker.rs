use std::sync::Arc;

use opencord_server::domain::attachment::{Attachment, AttachmentStatus, AttachmentStore};
use opencord_server::domain::audit::{AuditEvent, AuditStore};
use opencord_server::domain::ids;
use opencord_server::domain::message::{Message, MessageStore};
use opencord_server::domain::retention::{RetentionPolicy, RetentionStore, RetentionWorker};
use opencord_server::repositories::attachment_memory::MemoryAttachmentStore;
use opencord_server::repositories::audit_memory::MemoryAuditStore;
use opencord_server::repositories::message_memory::MemoryMessageStore;
use opencord_server::repositories::retention_memory::MemoryRetentionStore;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn retention_worker_dry_runs_and_purges_expired_messages_files_and_audit_events() {
    let organization_id = ids::new_uuid_v7();
    let other_organization_id = ids::new_uuid_v7();
    let space_id = ids::new_uuid_v7();
    let channel_id = ids::new_uuid_v7();
    let user_id = ids::new_uuid_v7();
    let fixture = MessageFixture {
        organization_id,
        space_id,
        channel_id,
        user_id,
    };
    let other_fixture = MessageFixture {
        organization_id: other_organization_id,
        space_id,
        channel_id,
        user_id,
    };

    let retention_store = Arc::new(MemoryRetentionStore::default());
    let message_store = Arc::new(MemoryMessageStore::default());
    let attachment_store = Arc::new(MemoryAttachmentStore::default());
    let audit_store = Arc::new(MemoryAuditStore::default());

    retention_store
        .upsert_policy(RetentionPolicy {
            organization_id,
            messages_retain_days: Some(30),
            files_retain_days: Some(30),
            audit_logs_retain_days: Some(60),
            deleted_message_purge_days: Some(7),
        })
        .await
        .expect("retention policy should save");

    let old_message_id = ids::new_uuid_v7();
    message_store
        .create_message(test_message(
            old_message_id,
            &fixture,
            "old message",
            "2026-05-01T00:00:00.000Z",
            None,
        ))
        .await
        .expect("old message should save");
    let deleted_message_id = ids::new_uuid_v7();
    message_store
        .create_message(test_message(
            deleted_message_id,
            &fixture,
            "deleted message",
            "2026-06-20T00:00:00.000Z",
            Some("2026-06-10T00:00:00.000Z"),
        ))
        .await
        .expect("deleted message should save");
    let fresh_message_id = ids::new_uuid_v7();
    message_store
        .create_message(test_message(
            fresh_message_id,
            &fixture,
            "fresh message",
            "2026-06-22T00:00:00.000Z",
            None,
        ))
        .await
        .expect("fresh message should save");
    message_store
        .create_message(test_message(
            ids::new_uuid_v7(),
            &other_fixture,
            "other org old message",
            "2026-05-01T00:00:00.000Z",
            None,
        ))
        .await
        .expect("other org message should save");

    attachment_store
        .create_attachment(test_attachment(
            organization_id,
            space_id,
            channel_id,
            Some(old_message_id),
            user_id,
            10,
            "2026-05-01T00:00:00.000Z",
        ))
        .await
        .expect("old attachment should save");
    attachment_store
        .create_attachment(test_attachment(
            organization_id,
            space_id,
            channel_id,
            Some(fresh_message_id),
            user_id,
            11,
            "2026-06-22T00:00:00.000Z",
        ))
        .await
        .expect("fresh attachment should save");
    attachment_store
        .create_attachment(test_attachment(
            other_organization_id,
            space_id,
            channel_id,
            None,
            user_id,
            12,
            "2026-05-01T00:00:00.000Z",
        ))
        .await
        .expect("other org attachment should save");

    audit_store
        .create_event(test_audit_event(
            organization_id,
            space_id,
            user_id,
            "2026-04-01T00:00:00.000Z",
        ))
        .await
        .expect("old audit event should save");
    audit_store
        .create_event(test_audit_event(
            organization_id,
            space_id,
            user_id,
            "2026-06-22T00:00:00.000Z",
        ))
        .await
        .expect("fresh audit event should save");
    audit_store
        .create_event(test_audit_event(
            other_organization_id,
            space_id,
            user_id,
            "2026-04-01T00:00:00.000Z",
        ))
        .await
        .expect("other org audit event should save");

    let worker = RetentionWorker::new(
        retention_store.clone(),
        message_store.clone(),
        attachment_store.clone(),
        audit_store.clone(),
    );
    let now = "2026-06-23T00:00:00Z";

    let dry_run = worker
        .run_once_at(now.to_owned(), true)
        .await
        .expect("retention dry-run should succeed");
    assert!(dry_run.dry_run);
    assert_eq!(dry_run.organizations_scanned, 1);
    assert_eq!(dry_run.messages_purged, 2);
    assert_eq!(dry_run.files_purged, 1);
    assert_eq!(dry_run.audit_events_purged, 1);
    assert_eq!(
        message_store
            .list_for_organization_between(
                organization_id,
                "2020-01-01T00:00:00.000Z".to_owned(),
                "2030-01-01T00:00:00.000Z".to_owned(),
            )
            .await
            .expect("messages should list after dry-run")
            .len(),
        3
    );

    let actual = worker
        .run_once_at(now.to_owned(), false)
        .await
        .expect("retention purge should succeed");
    assert!(!actual.dry_run);
    assert_eq!(actual.organizations_scanned, 1);
    assert_eq!(actual.messages_purged, 2);
    assert_eq!(actual.files_purged, 1);
    assert_eq!(actual.audit_events_purged, 1);

    let remaining_messages = message_store
        .list_for_organization_between(
            organization_id,
            "2020-01-01T00:00:00.000Z".to_owned(),
            "2030-01-01T00:00:00.000Z".to_owned(),
        )
        .await
        .expect("messages should list after purge");
    assert_eq!(remaining_messages.len(), 1);
    assert_eq!(remaining_messages[0].id, fresh_message_id);
    assert_eq!(
        attachment_store
            .stored_bytes_for_organization(organization_id)
            .await
            .expect("stored bytes should compute after purge"),
        11
    );
    assert_eq!(
        audit_store
            .list_for_organization_between(
                organization_id,
                "2020-01-01T00:00:00.000Z".to_owned(),
                "2030-01-01T00:00:00.000Z".to_owned(),
            )
            .await
            .expect("audit events should list after purge")
            .len(),
        1
    );

    let runs = retention_store
        .list_runs_for_organization(organization_id)
        .await
        .expect("retention runs should list");
    assert_eq!(runs.len(), 2);
    assert!(runs[0].dry_run);
    assert!(!runs[1].dry_run);
    assert_eq!(runs[1].messages_purged, 2);
}

struct MessageFixture {
    organization_id: Uuid,
    space_id: Uuid,
    channel_id: Uuid,
    user_id: Uuid,
}

fn test_message(
    id: Uuid,
    fixture: &MessageFixture,
    content: &str,
    created_at: &str,
    deleted_at: Option<&str>,
) -> Message {
    Message {
        id,
        organization_id: fixture.organization_id,
        space_id: Some(fixture.space_id),
        channel_id: fixture.channel_id,
        author_user_id: fixture.user_id,
        content: content.to_owned(),
        content_format: "plain".to_owned(),
        edited_at: None,
        deleted_at: deleted_at.map(str::to_owned),
        created_at: created_at.to_owned(),
    }
}

fn test_attachment(
    organization_id: Uuid,
    space_id: Uuid,
    channel_id: Uuid,
    message_id: Option<Uuid>,
    uploader_user_id: Uuid,
    size_bytes: i64,
    created_at: &str,
) -> Attachment {
    Attachment {
        id: ids::new_uuid_v7(),
        organization_id,
        space_id,
        channel_id,
        message_id,
        uploader_user_id,
        file_name: format!("{size_bytes}.txt"),
        content_type: "text/plain".to_owned(),
        size_bytes,
        status: AttachmentStatus::Uploaded,
        created_at: created_at.to_owned(),
    }
}

fn test_audit_event(
    organization_id: Uuid,
    space_id: Uuid,
    actor_user_id: Uuid,
    created_at: &str,
) -> AuditEvent {
    AuditEvent {
        id: ids::new_uuid_v7(),
        organization_id,
        space_id,
        actor_user_id,
        action: "retention.test".to_owned(),
        target_type: "message".to_owned(),
        target_id: ids::new_uuid_v7(),
        metadata: json!({}),
        created_at: created_at.to_owned(),
    }
}
