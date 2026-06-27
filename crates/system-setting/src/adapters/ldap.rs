//! LDAP 目录认证适配器(feature `ldap`)。简单绑定(simple bind):用 DN 模板把用户名
//! 拼成 DN,再以该 DN + 口令绑定;绑定成功即认证通过。授权仍走本地角色。
//!
//! 配置:`SHEPHERD_LDAP_URL`(如 `ldaps://dir.example.com:636`)+
//! `SHEPHERD_LDAP_DN_TEMPLATE`(含 `{username}` 占位,如 `uid={username},ou=people,dc=example,dc=com`)。

use async_trait::async_trait;
use ldap3::LdapConnAsync;

use crate::ports::{AuthRepoError, DirectoryAuthenticator};

pub struct LdapAuthenticator {
    url: String,
    dn_template: String,
}

/// 把 `{username}` 替换进 DN 模板。
fn format_dn(template: &str, username: &str) -> String {
    template.replace("{username}", username)
}

impl LdapAuthenticator {
    pub fn new(url: impl Into<String>, dn_template: impl Into<String>) -> Self {
        Self { url: url.into(), dn_template: dn_template.into() }
    }

    /// 两个变量都齐备才启用;否则返回 None(不接外部目录)。
    pub fn from_env(lookup: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let url = lookup("SHEPHERD_LDAP_URL").filter(|v| !v.trim().is_empty())?;
        let tpl = lookup("SHEPHERD_LDAP_DN_TEMPLATE").filter(|v| v.contains("{username}"))?;
        Some(Self::new(url, tpl))
    }
}

#[async_trait]
impl DirectoryAuthenticator for LdapAuthenticator {
    async fn authenticate(&self, username: &str, password: &str) -> Result<bool, AuthRepoError> {
        // 空口令在 LDAP 里是匿名绑定,会被误判为成功 —— 直接拒。
        if password.is_empty() {
            return Ok(false);
        }
        let backend = |e: ldap3::LdapError| AuthRepoError::Backend(e.to_string());
        let (conn, mut ldap) = LdapConnAsync::new(&self.url).await.map_err(backend)?;
        ldap3::drive!(conn);
        let dn = format_dn(&self.dn_template, username);
        let res = ldap.simple_bind(&dn, password).await.map_err(backend)?;
        let _ = ldap.unbind().await;
        match res.rc {
            0 => Ok(true),   // success
            49 => Ok(false), // invalidCredentials
            _ => Ok(false),  // 其他非零码一律按认证失败处理
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dn_template_substitutes_username() {
        assert_eq!(
            format_dn("uid={username},ou=people,dc=example,dc=com", "alice"),
            "uid=alice,ou=people,dc=example,dc=com"
        );
        // 无占位符 → 原样返回(不会误绑定到他人)。
        assert_eq!(format_dn("cn=fixed,dc=x", "alice"), "cn=fixed,dc=x");
    }

    #[test]
    fn from_env_requires_url_and_templated_dn() {
        let with = |pairs: &[(&str, &str)]| {
            let m: std::collections::HashMap<String, String> =
                pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            LdapAuthenticator::from_env(move |k| m.get(k).cloned())
        };
        assert!(with(&[]).is_none());
        // 缺 {username} 占位 → 拒绝(防全员绑定同一 DN)。
        assert!(with(&[
            ("SHEPHERD_LDAP_URL", "ldaps://h"),
            ("SHEPHERD_LDAP_DN_TEMPLATE", "uid=fixed,dc=x"),
        ])
        .is_none());
        let ok = with(&[
            ("SHEPHERD_LDAP_URL", "ldaps://h"),
            ("SHEPHERD_LDAP_DN_TEMPLATE", "uid={username},dc=x"),
        ])
        .expect("configured");
        assert_eq!(ok.url, "ldaps://h");
    }
}
