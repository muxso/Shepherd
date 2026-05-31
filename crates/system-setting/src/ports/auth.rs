//! 鉴权端口:凭证仓储、密码哈希、会话存储。

use async_trait::async_trait;

pub use webauth::{AuthRepoError, SessionStore};

/// 登录校验所需的用户凭证。
#[derive(Debug, Clone)]
pub struct UserCredential {
    pub user_id: String,
    pub password_hash: String,
    /// 原始权限串,如 `"SYSTEM_USER:READ+ADD"`(交给 kernel 解析)。
    pub permissions: Vec<String>,
}

#[async_trait]
pub trait CredentialRepository: Send + Sync {
    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserCredential>, AuthRepoError>;
}

/// 密码哈希器(纯计算,同步)。生产实现见 `adapters::auth`(Argon2)。
pub trait PasswordHasher: Send + Sync {
    fn hash(&self, plain: &str) -> String;
    fn verify(&self, plain: &str, hash: &str) -> bool;
}

