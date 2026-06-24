//! 共享鉴权基元:`AuthUser`、`Session`、`SessionStore` 端口、`AuthRepoError`,
//! 以及(feature=http)从 `Authorization: Bearer` 还原 `AuthUser` 的 axum 提取器。

use async_trait::async_trait;
use thiserror::Error;

// 便于下游(如 server)铸造服务令牌而无需直接依赖 kernel;同时供本 crate 内部使用。
pub use kernel::permission::PermissionSet;

/// 已认证用户(从会话还原后注入请求)。
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub permissions: PermissionSet,
}

impl AuthUser {
    /// 是否拥有对某资源执行某动作的权限。
    pub fn can(&self, resource: &str, action: &str) -> bool {
        self.permissions.allows(resource, action)
    }
}

/// 一个会话(令牌 ↔ 用户 + 权限快照 + 过期)。
#[derive(Debug, Clone)]
pub struct Session {
    pub token: String,
    pub user_id: String,
    pub permissions: PermissionSet,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthRepoError {
    #[error("auth backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create(
        &self,
        user_id: &str,
        permissions: PermissionSet,
        ttl_secs: i64,
    ) -> Result<String, AuthRepoError>;
    /// 不存在或已过期均返回 None。
    async fn get(&self, token: &str) -> Result<Option<Session>, AuthRepoError>;
    /// 撤销令牌(登出)。幂等。
    async fn revoke(&self, token: &str) -> Result<(), AuthRepoError>;
}

#[cfg(feature = "http")]
mod extractor {
    use super::{AuthUser, SessionStore};
    use std::sync::Arc;

    use axum::extract::{FromRef, FromRequestParts};
    use axum::http::{header::AUTHORIZATION, request::Parts, StatusCode};

    /// 任何把 `Arc<dyn SessionStore>` 放进 state(via FromRef)的 Router 都能用 `AuthUser` 提取器。
    impl<S> FromRequestParts<S> for AuthUser
    where
        Arc<dyn SessionStore>: FromRef<S>,
        S: Send + Sync,
    {
        type Rejection = (StatusCode, &'static str);

        async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
            let store = Arc::<dyn SessionStore>::from_ref(state);
            let token = parts
                .headers
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .ok_or((StatusCode::UNAUTHORIZED, "missing bearer token"))?;
            let session = store
                .get(token)
                .await
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "auth backend error"))?
                .ok_or((StatusCode::UNAUTHORIZED, "invalid or expired token"))?;
            Ok(AuthUser { user_id: session.user_id, permissions: session.permissions })
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
pub mod testing {
    //! 测试用内存会话存储:无过期(`expires_at_ms = i64::MAX`),令牌自增可复现。
    use super::{AuthRepoError, Session, SessionStore};
    use async_trait::async_trait;
    use kernel::permission::PermissionSet;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct InMemorySessionStore {
        inner: Mutex<(u64, HashMap<String, Session>)>,
    }

    impl InMemorySessionStore {
        pub fn new() -> Self {
            Self::default()
        }
    }

    #[async_trait]
    impl SessionStore for InMemorySessionStore {
        async fn create(
            &self,
            user_id: &str,
            permissions: PermissionSet,
            _ttl_secs: i64,
        ) -> Result<String, AuthRepoError> {
            let mut g = self.inner.lock().expect("lock");
            g.0 += 1;
            let token = format!("test-token-{}", g.0);
            g.1.insert(
                token.clone(),
                Session {
                    token: token.clone(),
                    user_id: user_id.to_string(),
                    permissions,
                    expires_at_ms: i64::MAX,
                },
            );
            Ok(token)
        }
        async fn get(&self, token: &str) -> Result<Option<Session>, AuthRepoError> {
            Ok(self.inner.lock().expect("lock").1.get(token).cloned())
        }
        async fn revoke(&self, token: &str) -> Result<(), AuthRepoError> {
            self.inner.lock().expect("lock").1.remove(token);
            Ok(())
        }
    }
}
