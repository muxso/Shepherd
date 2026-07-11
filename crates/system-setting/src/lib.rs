//! 系统设置上下文:用户/组织/角色与认证。角色分 System/Organization/Project 三级作用域,
//! 登录支持本地口令(auth)、LDAP 目录(ldap)、OIDC 外部身份(oidc)三条链路,另含 API Key 管理。
//! domain/application/ports 不做 IO;pg/http 及各认证适配器由 feature 启用。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
