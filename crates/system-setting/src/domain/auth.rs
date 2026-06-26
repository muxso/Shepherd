use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    /// 不区分用户不存在与密码错,避免账号枚举
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("backend error: {0}")]
    Backend(String),
}

pub use webauth::{AuthUser, Session};
