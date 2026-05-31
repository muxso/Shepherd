//! 第三方身份登录领域模型(飞书 / 企业微信)。
//!
//! 注:飞书/企业微信严格说是 **OAuth2**(非标准 OIDC,无标准 id_token),
//! 都要"用 code 换 token、再调各自的获取用户信息接口"。故抽象命名为 `ExternalIdentity*`。

use thiserror::Error;

/// 从第三方解析出的外部身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdentity {
    /// 提供方 key,如 "feishu" / "wecom"。
    pub provider: String,
    /// 提供方内的稳定用户标识(飞书 open_id / 企业微信 userid)。
    pub open_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OidcError {
    #[error("unknown identity provider: {0}")]
    UnknownProvider(String),
    /// code → token / userinfo 交换失败(网络、凭证、厂商返回错误码)。
    #[error("identity exchange failed: {0}")]
    Exchange(String),
    #[error("backend error: {0}")]
    Backend(String),
}
