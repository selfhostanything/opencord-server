use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::auth::{AuthError, AuthStore, StoredSession, StoredUser};

#[derive(Default)]
pub struct MemoryAuthStore {
    state: Mutex<MemoryAuthState>,
}

#[derive(Default)]
struct MemoryAuthState {
    users_by_id: HashMap<Uuid, StoredUser>,
    user_id_by_email: HashMap<String, Uuid>,
    sessions_by_token_hash: HashMap<String, MemorySession>,
}

struct MemorySession {
    user_id: Uuid,
    revoked: bool,
}

#[async_trait::async_trait]
impl AuthStore for MemoryAuthStore {
    async fn create_user(&self, user: StoredUser) -> Result<(), AuthError> {
        let mut state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        if state.user_id_by_email.contains_key(&user.email) {
            return Err(AuthError::EmailAlreadyRegistered);
        }

        state.user_id_by_email.insert(user.email.clone(), user.id);
        state.users_by_id.insert(user.id, user);

        Ok(())
    }

    async fn find_user_by_email(&self, email: &str) -> Result<Option<StoredUser>, AuthError> {
        let state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        let Some(user_id) = state.user_id_by_email.get(email) else {
            return Ok(None);
        };

        Ok(state.users_by_id.get(user_id).cloned())
    }

    async fn create_session(&self, session: StoredSession) -> Result<(), AuthError> {
        let mut state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        state.sessions_by_token_hash.insert(
            session.token_hash,
            MemorySession {
                user_id: session.user_id,
                revoked: false,
            },
        );

        Ok(())
    }

    async fn find_user_by_session_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<StoredUser>, AuthError> {
        let state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        let Some(session) = state.sessions_by_token_hash.get(token_hash) else {
            return Ok(None);
        };

        if session.revoked {
            return Ok(None);
        }

        Ok(state.users_by_id.get(&session.user_id).cloned())
    }

    async fn revoke_session(&self, token_hash: &str) -> Result<(), AuthError> {
        let mut state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        let Some(session) = state.sessions_by_token_hash.get_mut(token_hash) else {
            return Err(AuthError::Unauthorized);
        };

        session.revoked = true;
        Ok(())
    }
}
