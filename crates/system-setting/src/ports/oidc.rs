use async_trait::async_trait;

use crate::domain::{ExternalIdentity, OidcError};
pub use crate::domain::{OidcProvider, OidcRepoError};

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
    /// Look up the local user linked to `identity`, provisioning a new one on
    /// first login. `default_permissions` is the granting provider's configured
    /// permission set, applied only when a new link is created; existing links
    /// keep their stored permissions.
    async fn find_or_provision(
        &self,
        identity: &ExternalIdentity,
        default_permissions: &[String],
    ) -> Result<LinkedUser, OidcError>;
}

/// Persistence for [`OidcProvider`] records. Implementations back the runtime
/// provider registry: the composition root loads enabled providers from here
/// and (re)builds the strategy objects on each mutation.
#[async_trait]
pub trait OidcProviderRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<OidcProvider>, OidcRepoError>;

    async fn list_enabled(&self) -> Result<Vec<OidcProvider>, OidcRepoError>;

    async fn get(&self, key: &str) -> Result<Option<OidcProvider>, OidcRepoError>;

    /// Insert or update a provider keyed by `provider_key`.
    async fn upsert(&self, provider: &OidcProvider) -> Result<(), OidcRepoError>;

    async fn delete(&self, key: &str) -> Result<(), OidcRepoError>;
}
