use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    /// 不区分用户不存在与密码错,避免账号枚举
    #[error("invalid username or password")]
    InvalidCredentials,
    /// 同一用户名连续失败过多,锁定期内一律拒绝(含正确口令)
    #[error("too many failed attempts")]
    LockedOut,
    #[error("backend error: {0}")]
    Backend(String),
}

pub use webauth::{AuthUser, Session};
