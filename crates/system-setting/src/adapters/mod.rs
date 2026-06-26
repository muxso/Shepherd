pub mod directory;
pub mod in_memory;
pub mod in_memory_auth;
pub mod in_memory_org;
pub mod in_memory_role;
pub mod oidc_fakes;

pub use directory::SpyDirectory;
pub use in_memory::InMemoryUserRepository;
pub use in_memory_auth::{InMemoryCredentialRepository, InMemorySessionStore, PlainPasswordHasher};
pub use in_memory_org::InMemoryOrgRepository;
pub use in_memory_role::{InMemoryRoleRepository, InMemoryUserRoleRepository};
pub use oidc_fakes::{FakeIdentityProvider, InMemoryExternalUserRepository};

#[cfg(feature = "pg")]
pub mod pg;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "auth")]
pub mod auth;
#[cfg(feature = "oidc")]
pub mod oidc;
