//! 适配器层。
//! - `in_memory` / `directory`:纯 test double(无 IO),始终编译,供用例单测复用。
//! - `pg`(feature=pg)、`http`(feature=http):真实 IO 适配器,仅在对应 feature 下编译。
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
