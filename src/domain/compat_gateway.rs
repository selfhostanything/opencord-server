use std::sync::Arc;

use serde_json::Value;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompatGatewayResumeResult {
    Resumed(CompatGatewaySession),
    InvalidSequence,
    NotFound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompatGatewayError {
    StoreUnavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompatGatewayReplayEvent {
    pub session_id: String,
    pub sequence: i64,
    pub event_type: String,
    pub payload: Value,
}

#[async_trait::async_trait]
pub trait CompatGatewaySessionStore: Send + Sync {
    async fn create_session(
        &self,
        session: CompatGatewaySession,
    ) -> Result<CompatGatewaySession, CompatGatewayError>;

    async fn update_sequence(
        &self,
        session_id: &str,
        sequence: i64,
    ) -> Result<(), CompatGatewayError>;

    async fn resume_session(
        &self,
        session_id: &str,
        bot: &AuthenticatedBot,
        client_sequence: i64,
    ) -> Result<CompatGatewayResumeResult, CompatGatewayError>;

    async fn append_replay_event(
        &self,
        event: CompatGatewayReplayEvent,
    ) -> Result<(), CompatGatewayError>;

    async fn list_replay_events_after(
        &self,
        session_id: &str,
        sequence: i64,
        limit: u32,
    ) -> Result<Vec<CompatGatewayReplayEvent>, CompatGatewayError>;
}

pub struct CompatGatewaySessions {
    store: Arc<dyn CompatGatewaySessionStore>,
}

impl CompatGatewaySessions {
    pub fn new(store: Arc<dyn CompatGatewaySessionStore>) -> Self {
        Self { store }
    }

    pub async fn create(
        &self,
        session_id: String,
        bot: &AuthenticatedBot,
        sequence: i64,
        intents: u64,
    ) -> Result<CompatGatewaySession, CompatGatewayError> {
        self.store
            .create_session(CompatGatewaySession {
                session_id,
                application_id: bot.application_id,
                organization_id: bot.organization_id,
                bot_user_id: bot.bot_user_id,
                sequence,
                intents,
            })
            .await
    }

    pub async fn update_sequence(
        &self,
        session_id: &str,
        sequence: i64,
    ) -> Result<(), CompatGatewayError> {
        self.store.update_sequence(session_id, sequence).await
    }

    pub async fn resume(
        &self,
        session_id: &str,
        bot: &AuthenticatedBot,
        client_sequence: i64,
    ) -> Result<CompatGatewayResumeResult, CompatGatewayError> {
        self.store
            .resume_session(session_id, bot, client_sequence)
            .await
    }

    pub async fn append_replay_event(
        &self,
        event: CompatGatewayReplayEvent,
    ) -> Result<(), CompatGatewayError> {
        self.store.append_replay_event(event).await
    }

    pub async fn list_replay_events_after(
        &self,
        session_id: &str,
        sequence: i64,
        limit: u32,
    ) -> Result<Vec<CompatGatewayReplayEvent>, CompatGatewayError> {
        self.store
            .list_replay_events_after(session_id, sequence, limit)
            .await
    }
}
