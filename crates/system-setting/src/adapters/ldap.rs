//! LDAP directory auth adapter (feature `ldap`). Simple bind: expand the username into a DN
//! via the DN template, then bind with that DN + password; a successful bind means the user
//! is authenticated. Authorization still comes from local roles.
//!
//! Config: `SHEPHERD_LDAP_URL` (e.g. `ldaps://dir.example.com:636`) +
//! `SHEPHERD_LDAP_DN_TEMPLATE` (must contain a `{username}` placeholder, e.g.
//! `uid={username},ou=people,dc=example,dc=com`).

use async_trait::async_trait;
use ldap3::LdapConnAsync;

use crate::ports::{AuthRepoError, DirectoryAuthenticator};

pub struct LdapAuthenticator {
    url: String,
    dn_template: String,
}

/// Substitute `{username}` into the DN template.
fn format_dn(template: &str, username: &str) -> String {
    template.replace("{username}", username)
}

impl LdapAuthenticator {
    pub fn new(url: impl Into<String>, dn_template: impl Into<String>) -> Self {
        Self { url: url.into(), dn_template: dn_template.into() }
    }

    /// Enabled only when both variables are set; otherwise returns None (no external directory).
    pub fn from_env(lookup: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let url = lookup("SHEPHERD_LDAP_URL").filter(|v| !v.trim().is_empty())?;
        let tpl = lookup("SHEPHERD_LDAP_DN_TEMPLATE").filter(|v| v.contains("{username}"))?;
        Some(Self::new(url, tpl))
    }
}

#[async_trait]
impl DirectoryAuthenticator for LdapAuthenticator {
    async fn authenticate(&self, username: &str, password: &str) -> Result<bool, AuthRepoError> {
        // An empty password is an anonymous bind in LDAP and would look like success — reject.
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
            _ => Ok(false),  // any other non-zero code is treated as auth failure
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
        // No placeholder → returned unchanged (cannot accidentally bind as someone else).
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
        // Missing {username} placeholder → rejected (prevents everyone binding to the same DN).
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
