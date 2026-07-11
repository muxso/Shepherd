use async_trait::async_trait;

pub use webauth::{AuthRepoError, SessionStore};

#[derive(Debug, Clone)]
pub struct UserCredential {
    pub user_id: String,
    pub password_hash: String,
    pub permissions: Vec<String>,
}

#[async_trait]
pub trait CredentialRepository: Send + Sync {
    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserCredential>, AuthRepoError>;

    /// 会话只带 user_id,自助改密按 user_id 取凭据。
    /// 默认 None:凭据不在库里(如环境变量注入的内置账号),库侧改不动。
    async fn find_by_user_id(
        &self,
        _user_id: &str,
    ) -> Result<Option<UserCredential>, AuthRepoError> {
        Ok(None)
    }

    /// 更新密码哈希;`false` 表示库中无该用户凭据。
    async fn update_password(
        &self,
        _user_id: &str,
        _password_hash: &str,
    ) -> Result<bool, AuthRepoError> {
        Ok(false)
    }

    /// 默认 no-op,仅 PG 实现真正重置
    async fn reset_password(
        &self,
        _user_id: &str,
        _username: &str,
        _password_hash: &str,
    ) -> Result<(), AuthRepoError> {
        Ok(())
    }
}

#[async_trait]
pub trait UserRoleQuery: Send + Sync {
    async fn roles_for(&self, user_ids: &[String]) -> Result<Vec<(String, String)>, AuthRepoError>;
}

pub trait PasswordHasher: Send + Sync {
    fn hash(&self, plain: &str) -> String;
    fn verify(&self, plain: &str, hash: &str) -> bool;
}

/// 外部目录认证(LDAP 等):仅验证「用户名 + 密码」能否绑定成功。
/// 授权仍走本地角色/权限(**本地授权 + 外部认证**),用户须已在本地存在。
/// `Ok(true)` 绑定成功;`Ok(false)` 凭证被拒;`Err` 为目录后端不可用(区别于密码错)。
#[async_trait]
pub trait DirectoryAuthenticator: Send + Sync {
    async fn authenticate(&self, username: &str, password: &str) -> Result<bool, AuthRepoError>;
}
