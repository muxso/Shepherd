//! 鉴权领域:会话与已认证用户。复用 `kernel::permission` 做 RBAC 判定。

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    /// 用户名或密码错误(刻意不区分"用户不存在"与"密码错",避免账号枚举)。
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("backend error: {0}")]
    Backend(String),
}

pub use webauth::{AuthUser, Session};
