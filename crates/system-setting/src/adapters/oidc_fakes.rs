//! 第三方登录的测试替身:可控提供方 + 内存外部用户映射。
//! 让 `OidcLoginUseCase` 的编排能脱离真实飞书/企业微信被穷举测试。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::{ExternalIdentity, OidcError};
use crate::ports::{ExternalIdentityProvider, ExternalUserRepository, LinkedUser};

/// 可控提供方:`exchange` 对约定的 code "bad" 报错,其余返回预置身份。
#[derive(Clone)]
pub struct FakeIdentityProvider {
    key: String,
    identity: ExternalIdentity,
}

impl FakeIdentityProvider {
    pub fn new(key: &str, identity: ExternalIdentity) -> Self {
        Self { key: key.to_string(), identity }
    }
}

#[async_trait]
impl ExternalIdentityProvider for FakeIdentityProvider {
    fn key(&self) -> &str {
        &self.key
    }

    fn authorize_url(&self, state: &str) -> String {
        format!("https://fake.example/{}/authorize?state={state}", self.key)
    }

    async fn exchange(&self, code: &str) -> Result<ExternalIdentity, OidcError> {
        if code == "bad" {
            return Err(OidcError::Exchange("invalid code".into()));
        }
        Ok(self.identity.clone())
    }
}

/// 内存外部用户映射:首次见到 (provider, open_id) 开通一个本地用户,之后复用。
#[derive(Clone)]
pub struct InMemoryExternalUserRepository {
    default_perms: Vec<String>,
    links: Arc<Mutex<HashMap<(String, String), LinkedUser>>>,
    provisioned: Arc<Mutex<usize>>,
}

impl InMemoryExternalUserRepository {
    pub fn new<I, S>(default_perms: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            default_perms: default_perms.into_iter().map(Into::into).collect(),
            links: Arc::new(Mutex::new(HashMap::new())),
            provisioned: Arc::new(Mutex::new(0)),
        }
    }

    /// 测试辅助:累计开通过几个新用户。
    pub fn provisioned_count(&self) -> usize {
        *self.provisioned.lock().expect("lock")
    }
}

#[async_trait]
impl ExternalUserRepository for InMemoryExternalUserRepository {
    async fn find_or_provision(
        &self,
        identity: &ExternalIdentity,
    ) -> Result<LinkedUser, OidcError> {
        let key = (identity.provider.clone(), identity.open_id.clone());
        let mut links = self.links.lock().expect("lock");
        if let Some(u) = links.get(&key) {
            return Ok(u.clone());
        }
        *self.provisioned.lock().expect("lock") += 1;
        let linked = LinkedUser {
            user_id: format!("ext-{}-{}", identity.provider, identity.open_id),
            permissions: self.default_perms.clone(),
        };
        links.insert(key, linked.clone());
        Ok(linked)
    }
}
