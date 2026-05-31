//! 第三方身份登录端口:提供方(OAuth2 流)+ 外部用户映射。

use async_trait::async_trait;

use crate::domain::{ExternalIdentity, OidcError};

/// 外部身份提供方(飞书 / 企业微信各一个实现)。封装 OAuth2 授权码流。
#[async_trait]
pub trait ExternalIdentityProvider: Send + Sync {
    /// 提供方 key("feishu" / "wecom"),用于路由与注册。
    fn key(&self) -> &str;

    /// 构造授权页跳转 URL(带 `state` 防 CSRF)。
    fn authorize_url(&self, state: &str) -> String;

    /// 用授权码换取外部身份(内部完成 code→token→userinfo 多步)。
    async fn exchange(&self, code: &str) -> Result<ExternalIdentity, OidcError>;
}

/// 映射后的本地用户(由外部身份找到/开通)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedUser {
    pub user_id: String,
    /// 原始权限串(交给 kernel 解析)。
    pub permissions: Vec<String>,
}

/// 外部身份 ↔ 本地用户映射:已存在则返回,首次登录则开通。
#[async_trait]
pub trait ExternalUserRepository: Send + Sync {
    async fn find_or_provision(
        &self,
        identity: &ExternalIdentity,
    ) -> Result<LinkedUser, OidcError>;
}
