use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    /// Does not distinguish unknown user from wrong password, to prevent account enumeration.
    #[error("invalid username or password")]
    InvalidCredentials,
    /// Too many consecutive failures for a username; everything is rejected during the
    /// lockout window, even the correct password.
    #[error("too many failed attempts")]
    LockedOut,
    #[error("backend error: {0}")]
    Backend(String),
}

pub use webauth::{AuthUser, Session};
