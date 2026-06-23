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

    /// 重置密码:按 user_id 改哈希;无凭证则按用户名(邮箱)新建。默认 no-op(内存实现继承)。
    async fn reset_password(
        &self,
        _user_id: &str,
        _username: &str,
        _password_hash: &str,
    ) -> Result<(), AuthRepoError> {
        Ok(())
    }
}

/// 用户→用户组(角色名)只读查询。供用户列表附带「用户组」列。
#[async_trait]
pub trait UserRoleQuery: Send + Sync {
    /// 批量取多个用户的用户组名,返回 (user_id, role_name) 列表。
    async fn roles_for(&self, user_ids: &[String]) -> Result<Vec<(String, String)>, AuthRepoError>;
}

/// 密码哈希器(纯计算,同步)。生产实现见 `adapters::auth`(Argon2)。
pub trait PasswordHasher: Send + Sync {
    fn hash(&self, plain: &str) -> String;
    fn verify(&self, plain: &str, hash: &str) -> bool;
}

