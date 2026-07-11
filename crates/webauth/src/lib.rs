//! Web 认证共享件:Session/AuthUser 模型与 SessionStore 端口(内置 InMemorySessionStore),
//! http feature 下提供 axum 提取器,从 Bearer token 解析出携带 PermissionSet 的 AuthUser,
//! 供各上下文的 http 适配器统一做鉴权。

use async_trait::async_trait;
use thiserror::Error;

// 重导出:下游可铸造服务令牌而无需直接依赖 kernel。
pub use kernel::permission::PermissionSet;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub permissions: PermissionSet,
}

impl AuthUser {
    pub fn can(&self, resource: &str, action: &str) -> bool {
        self.permissions.allows(resource, action)
    }
}

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
    /// 幂等。
    async fn revoke(&self, token: &str) -> Result<(), AuthRepoError>;
}

#[cfg(feature = "http")]
mod extractor {
    use super::{AuthUser, SessionStore};
    use std::sync::Arc;

    use axum::extract::{FromRef, FromRequestParts};
    use axum::http::{header::AUTHORIZATION, request::Parts, StatusCode};

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

#[cfg(all(test, feature = "http"))]
mod extractor_tests {
    use super::{AuthUser, SessionStore};
    use axum::body::Body;
    use axum::extract::FromRef;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct St {
        sessions: Arc<dyn SessionStore>,
    }
    impl FromRef<St> for Arc<dyn SessionStore> {
        fn from_ref(s: &St) -> Self {
            s.sessions.clone()
        }
    }

    async fn whoami(user: AuthUser) -> String {
        format!("{}:{}", user.user_id, user.can("BUG", "READ"))
    }

    fn app(sessions: Arc<dyn SessionStore>) -> Router {
        Router::new().route("/whoami", get(whoami)).with_state(St { sessions })
    }

    fn req(auth: Option<&str>) -> Request<Body> {
        let mut b = Request::builder().uri("/whoami");
        if let Some(a) = auth {
            b = b.header("authorization", a);
        }
        b.body(Body::empty()).expect("req")
    }

    #[tokio::test]
    async fn valid_token_yields_user_with_permissions() {
        let store = Arc::new(super::testing::InMemorySessionStore::new());
        let perms = super::PermissionSet::from_raw(["BUG:READ".to_string()]).expect("perms");
        let token = store.create("u1", perms, 3600).await.expect("token");
        let resp = app(store).oneshot(req(Some(&format!("Bearer {token}")))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        assert_eq!(&body[..], b"u1:true");
    }

    #[tokio::test]
    async fn missing_header_is_401() {
        let store = Arc::new(super::testing::InMemorySessionStore::new());
        let resp = app(store).oneshot(req(None)).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn non_bearer_scheme_is_401() {
        let store = Arc::new(super::testing::InMemorySessionStore::new());
        let resp = app(store).oneshot(req(Some("Basic dXNlcjpwdw=="))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn unknown_and_revoked_tokens_are_401() {
        let store = Arc::new(super::testing::InMemorySessionStore::new());
        let resp = app(store.clone()).oneshot(req(Some("Bearer ghost"))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let perms = super::PermissionSet::from_raw(["BUG:READ".to_string()]).expect("perms");
        let token = store.create("u1", perms, 3600).await.expect("token");
        store.revoke(&token).await.expect("revoke");
        let resp = app(store).oneshot(req(Some(&format!("Bearer {token}")))).await.expect("resp");
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

#[cfg(any(test, feature = "test-util"))]
pub mod testing {
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
            let mut g = self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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
            Ok(self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .1
                .get(token)
                .cloned())
        }
        async fn revoke(&self, token: &str) -> Result<(), AuthRepoError> {
            self.inner.lock().unwrap_or_else(std::sync::PoisonError::into_inner).1.remove(token);
            Ok(())
        }
    }
}
