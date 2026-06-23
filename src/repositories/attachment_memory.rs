use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::attachment::{
    Attachment, AttachmentContent, AttachmentError, AttachmentStatus, AttachmentStore,
};

#[derive(Default)]
pub struct MemoryAttachmentStore {
    state: Mutex<MemoryAttachmentState>,
}

#[derive(Default)]
struct MemoryAttachmentState {
    attachments_by_id: HashMap<Uuid, Attachment>,
    content_by_id: HashMap<Uuid, AttachmentContent>,
}

#[async_trait::async_trait]
impl AttachmentStore for MemoryAttachmentStore {
    async fn create_attachment(&self, attachment: Attachment) -> Result<(), AttachmentError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AttachmentError::StoreUnavailable)?;
        state.attachments_by_id.insert(attachment.id, attachment);
        Ok(())
    }

    async fn get_attachment(
        &self,
        attachment_id: Uuid,
    ) -> Result<Option<Attachment>, AttachmentError> {
        let state = self
            .state
            .lock()
            .map_err(|_| AttachmentError::StoreUnavailable)?;
        Ok(state.attachments_by_id.get(&attachment_id).cloned())
    }

    async fn upload_content(
        &self,
        mut attachment: Attachment,
        content: AttachmentContent,
    ) -> Result<Attachment, AttachmentError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AttachmentError::StoreUnavailable)?;
        if !state.attachments_by_id.contains_key(&attachment.id) {
            return Err(AttachmentError::NotFound);
        }

        attachment.status = AttachmentStatus::Uploaded;
        state.content_by_id.insert(attachment.id, content);
        state
            .attachments_by_id
            .insert(attachment.id, attachment.clone());
        Ok(attachment)
    }

    async fn content_for_attachment(
        &self,
        attachment_id: Uuid,
    ) -> Result<Option<AttachmentContent>, AttachmentError> {
        let state = self
            .state
            .lock()
            .map_err(|_| AttachmentError::StoreUnavailable)?;
        Ok(state.content_by_id.get(&attachment_id).cloned())
    }

    async fn link_attachments_to_message(
        &self,
        message_id: Uuid,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, AttachmentError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AttachmentError::StoreUnavailable)?;
        let mut attachments = Vec::with_capacity(attachment_ids.len());

        for attachment_id in attachment_ids {
            let Some(attachment) = state.attachments_by_id.get_mut(attachment_id) else {
                return Err(AttachmentError::NotFound);
            };
            attachment.message_id = Some(message_id);
            attachment.status = AttachmentStatus::Linked;
            attachments.push(attachment.clone());
        }

        Ok(attachments)
    }

    async fn list_for_message_ids(
        &self,
        message_ids: &[Uuid],
    ) -> Result<Vec<Attachment>, AttachmentError> {
        let state = self
            .state
            .lock()
            .map_err(|_| AttachmentError::StoreUnavailable)?;
        let message_ids = message_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let mut attachments = state
            .attachments_by_id
            .values()
            .filter(|attachment| {
                attachment
                    .message_id
                    .is_some_and(|message_id| message_ids.contains(&message_id))
            })
            .cloned()
            .collect::<Vec<_>>();

        attachments.sort_by_key(|attachment| attachment.id);
        Ok(attachments)
    }

    async fn stored_bytes_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<i64, AttachmentError> {
        let state = self
            .state
            .lock()
            .map_err(|_| AttachmentError::StoreUnavailable)?;

        Ok(state
            .attachments_by_id
            .values()
            .filter(|attachment| attachment.organization_id == organization_id)
            .filter(|attachment| {
                matches!(
                    attachment.status,
                    AttachmentStatus::Uploaded | AttachmentStatus::Linked
                )
            })
            .map(|attachment| attachment.size_bytes)
            .sum())
    }

    async fn purge_for_retention(
        &self,
        organization_id: Uuid,
        created_before: Option<String>,
        dry_run: bool,
    ) -> Result<usize, AttachmentError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AttachmentError::StoreUnavailable)?;
        let expired_ids = state
            .attachments_by_id
            .values()
            .filter(|attachment| attachment.organization_id == organization_id)
            .filter(|attachment| {
                created_before
                    .as_deref()
                    .is_some_and(|cutoff| attachment.created_at.as_str() < cutoff)
            })
            .map(|attachment| attachment.id)
            .collect::<Vec<_>>();

        if !dry_run {
            for attachment_id in &expired_ids {
                state.attachments_by_id.remove(attachment_id);
                state.content_by_id.remove(attachment_id);
            }
        }

        Ok(expired_ids.len())
    }
}
