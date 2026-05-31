//! 领域层:纯业务规则,零 IO,零框架。
pub mod auth;
pub mod oidc;
pub mod organization;
pub mod role;
pub mod user;
pub mod user_lookup;

pub use auth::{AuthError, AuthUser, Session};
pub use oidc::{ExternalIdentity, OidcError};
pub use organization::{NewOrganization, OrgError, Organization};
pub use role::{NewRole, Role, RoleError, RoleScope};
pub use user::{Email, NewUser, User, UserError};
pub use user_lookup::UserSource;
