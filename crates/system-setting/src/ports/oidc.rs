use async_trait::async_trait;

use crate::domain::{ExternalIdentity, OidcError};

#[async_trait]
pub trait ExternalIdentityProvider: Send + Sync {
    fn key(&self) -> &str;

    fn authorize_url(&self, state: &str) -> String;

    async fn exchange(&self, code: &str) -> Result<ExternalIdentity, OidcError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedUser {
    pub user_id: String,
    pub permissions: Vec<String>,
}

#[async_trait]
pub trait ExternalUserRepository: Send + Sync {
    async fn find_or_provision(&self, identity: &ExternalIdentity)
        -> Result<LinkedUser, OidcError>;
}
