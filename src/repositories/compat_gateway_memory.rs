use std::collections::HashMap;
use std::sync::Mutex;

use crate::domain::bot::AuthenticatedBot;
use crate::domain::compat_gateway::{
    CompatGatewayError, CompatGatewayReplayEvent, CompatGatewayResumeResult, CompatGatewaySession,
    CompatGatewaySessionStore,
};

#[derive(Default)]
pub struct MemoryCompatGatewaySessionStore {
    sessions: Mutex<HashMap<String, CompatGatewaySession>>,
    replay_events: Mutex<HashMap<String, Vec<CompatGatewayReplayEvent>>>,
}

#[async_trait::async_trait]
impl CompatGatewaySessionStore for MemoryCompatGatewaySessionStore {
    async fn create_session(
        &self,
        session: CompatGatewaySession,
    ) -> Result<CompatGatewaySession, CompatGatewayError> {
        self.sessions
            .lock()
            .map_err(|_| CompatGatewayError::StoreUnavailable)?
            .insert(session.session_id.clone(), session.clone());
        Ok(session)
    }

    async fn update_sequence(
        &self,
        session_id: &str,
        sequence: i64,
    ) -> Result<(), CompatGatewayError> {
        if let Some(session) = self
            .sessions
            .lock()
            .map_err(|_| CompatGatewayError::StoreUnavailable)?
            .get_mut(session_id)
        {
            session.sequence = session.sequence.max(sequence);
        }

        Ok(())
    }

    async fn resume_session(
        &self,
        session_id: &str,
        bot: &AuthenticatedBot,
        client_sequence: i64,
    ) -> Result<CompatGatewayResumeResult, CompatGatewayError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| CompatGatewayError::StoreUnavailable)?;
        let Some(session) = sessions.get_mut(session_id) else {
            return Ok(CompatGatewayResumeResult::NotFound);
        };
        if session.application_id != bot.application_id
            || session.organization_id != bot.organization_id
            || session.bot_user_id != bot.bot_user_id
        {
            return Ok(CompatGatewayResumeResult::NotFound);
        }
        if client_sequence > session.sequence {
            return Ok(CompatGatewayResumeResult::InvalidSequence);
        }

        session.sequence = session.sequence.max(client_sequence);
        Ok(CompatGatewayResumeResult::Resumed(session.clone()))
    }

    async fn append_replay_event(
        &self,
        event: CompatGatewayReplayEvent,
    ) -> Result<(), CompatGatewayError> {
        let mut replay_events = self
            .replay_events
            .lock()
            .map_err(|_| CompatGatewayError::StoreUnavailable)?;
        let events = replay_events
            .entry(event.session_id.clone())
            .or_insert_with(Vec::new);
        if let Some(existing) = events
            .iter_mut()
            .find(|existing| existing.sequence == event.sequence)
        {
            *existing = event;
        } else {
            events.push(event);
            events.sort_by_key(|event| event.sequence);
        }

        Ok(())
    }

    async fn list_replay_events_after(
        &self,
        session_id: &str,
        sequence: i64,
        limit: u32,
    ) -> Result<Vec<CompatGatewayReplayEvent>, CompatGatewayError> {
        let replay_events = self
            .replay_events
            .lock()
            .map_err(|_| CompatGatewayError::StoreUnavailable)?;

        Ok(replay_events
            .get(session_id)
            .map(|events| {
                events
                    .iter()
                    .filter(|event| event.sequence > sequence)
                    .take(limit as usize)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }
}
