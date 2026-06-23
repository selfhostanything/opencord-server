use std::collections::HashMap;
use std::sync::Mutex;

use crate::domain::bot::AuthenticatedBot;
use crate::domain::compat_gateway::{
    CompatGatewayError, CompatGatewayResumeResult, CompatGatewaySession, CompatGatewaySessionStore,
};

#[derive(Default)]
pub struct MemoryCompatGatewaySessionStore {
    sessions: Mutex<HashMap<String, CompatGatewaySession>>,
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
}
