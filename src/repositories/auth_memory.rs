use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::auth::{
    AuthError, AuthStore, StoredOidcIdentity, StoredOidcProvider, StoredSession, StoredUser,
};

#[derive(Default)]
pub struct MemoryAuthStore {
    state: Mutex<MemoryAuthState>,
}

#[derive(Default)]
struct MemoryAuthState {
    users_by_id: HashMap<Uuid, StoredUser>,
    user_id_by_email: HashMap<String, Uuid>,
    sessions_by_token_hash: HashMap<String, MemorySession>,
    oidc_providers_by_organization: HashMap<Uuid, StoredOidcProvider>,
    oidc_identity_by_issuer_subject: HashMap<(String, String), StoredOidcIdentity>,
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

    async fn find_user_by_id(&self, user_id: Uuid) -> Result<Option<StoredUser>, AuthError> {
        let state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        Ok(state.users_by_id.get(&user_id).cloned())
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

    async fn upsert_oidc_provider(&self, provider: StoredOidcProvider) -> Result<(), AuthError> {
        let mut state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        state
            .oidc_providers_by_organization
            .insert(provider.organization_id, provider);
        Ok(())
    }

    async fn oidc_provider_for_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Option<StoredOidcProvider>, AuthError> {
        let state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        Ok(state
            .oidc_providers_by_organization
            .get(&organization_id)
            .cloned())
    }

    async fn oidc_providers_for_email_domain(
        &self,
        domain: &str,
    ) -> Result<Vec<StoredOidcProvider>, AuthError> {
        let state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        let mut providers = state
            .oidc_providers_by_organization
            .values()
            .filter(|provider| {
                provider
                    .allowed_domains
                    .iter()
                    .any(|allowed_domain| allowed_domain == domain)
            })
            .cloned()
            .collect::<Vec<_>>();
        providers.sort_by(|left, right| left.issuer.cmp(&right.issuer));
        Ok(providers)
    }

    async fn oidc_provider_for_issuer_and_domain(
        &self,
        issuer: &str,
        domain: &str,
    ) -> Result<Option<StoredOidcProvider>, AuthError> {
        let state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        Ok(state
            .oidc_providers_by_organization
            .values()
            .find(|provider| {
                provider.issuer == issuer
                    && provider
                        .allowed_domains
                        .iter()
                        .any(|allowed_domain| allowed_domain == domain)
            })
            .cloned())
    }

    async fn find_oidc_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> Result<Option<StoredOidcIdentity>, AuthError> {
        let state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        Ok(state
            .oidc_identity_by_issuer_subject
            .get(&(issuer.to_owned(), subject.to_owned()))
            .cloned())
    }

    async fn create_oidc_identity(&self, identity: StoredOidcIdentity) -> Result<(), AuthError> {
        let mut state = self.state.lock().map_err(|_| AuthError::StoreUnavailable)?;
        let key = (identity.issuer.clone(), identity.subject.clone());
        state.oidc_identity_by_issuer_subject.insert(key, identity);
        Ok(())
    }
}
