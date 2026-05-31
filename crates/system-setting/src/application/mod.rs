//! 应用层:用例编排。依赖 `ports` trait,不碰具体 IO。
pub mod create_user;
pub mod login;
pub mod oidc_login;
pub mod organization;
pub mod resolve_user_names;
pub mod role;
pub mod user_admin;

pub use create_user::{CreateUserError, CreateUserUseCase};
pub use login::LoginUseCase;
pub use oidc_login::OidcLoginUseCase;
pub use organization::{OrgCmdError, OrganizationService};
pub use resolve_user_names::ResolveUserNamesUseCase;
pub use role::{RoleCmdError, RoleService, UserRoleService};
pub use user_admin::{UserCmdError, UserService};
