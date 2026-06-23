use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::bot::AuthenticatedBot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatGatewaySession {
    pub session_id: String,
    pub application_id: Uuid,
    pub organization_id: Uuid,
    pub bot_user_id: Uuid,
    pub sequence: i64,
    pub intents: u64,
}

#[derive(Default)]
pub struct CompatGatewaySessions {
    sessions: Mutex<HashMap<String, CompatGatewaySession>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompatGatewayResumeResult {
    Resumed(CompatGatewaySession),
    InvalidSequence,
    NotFound,
}

impl CompatGatewaySessions {
    pub fn create(
        &self,
        session_id: String,
        bot: &AuthenticatedBot,
        sequence: i64,
        intents: u64,
    ) -> CompatGatewaySession {
        let session = CompatGatewaySession {
            session_id: session_id.clone(),
            application_id: bot.application_id,
            organization_id: bot.organization_id,
            bot_user_id: bot.bot_user_id,
            sequence,
            intents,
        };
        self.sessions
            .lock()
            .expect("compat gateway sessions mutex poisoned")
            .insert(session_id, session.clone());
        session
    }

    pub fn update_sequence(&self, session_id: &str, sequence: i64) {
        if let Some(session) = self
            .sessions
            .lock()
            .expect("compat gateway sessions mutex poisoned")
            .get_mut(session_id)
        {
            session.sequence = session.sequence.max(sequence);
        }
    }

    pub fn resume(
        &self,
        session_id: &str,
        bot: &AuthenticatedBot,
        client_sequence: i64,
    ) -> CompatGatewayResumeResult {
        let mut sessions = self
            .sessions
            .lock()
            .expect("compat gateway sessions mutex poisoned");
        let Some(session) = sessions.get_mut(session_id) else {
            return CompatGatewayResumeResult::NotFound;
        };
        if session.application_id != bot.application_id
            || session.organization_id != bot.organization_id
            || session.bot_user_id != bot.bot_user_id
        {
            return CompatGatewayResumeResult::NotFound;
        }
        if client_sequence > session.sequence {
            return CompatGatewayResumeResult::InvalidSequence;
        }

        session.sequence = session.sequence.max(client_sequence);
        CompatGatewayResumeResult::Resumed(session.clone())
    }
}
