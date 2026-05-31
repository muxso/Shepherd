//! 用户来源(创建途径)。配套 OIDC 绕过 CFT 的规则见 `application::resolve_user_names`:
//! 展示名解析与用户来源无关,绝不应被创建途径校验拦截。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserSource {
    Local,
    Oidc,
    Ldap,
}

impl UserSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserSource::Local => "LOCAL",
            UserSource::Oidc => "OIDC",
            UserSource::Ldap => "LDAP",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "LOCAL" => Some(UserSource::Local),
            "OIDC" => Some(UserSource::Oidc),
            "LDAP" => Some(UserSource::Ldap),
            _ => None,
        }
    }

    /// 是否为外部来源(非本地)。CFT/Liber 的创建途径校验正是对这类用户报异常。
    pub fn is_external(&self) -> bool {
        !matches!(self, UserSource::Local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_roundtrip() {
        for s in [UserSource::Local, UserSource::Oidc, UserSource::Ldap] {
            assert_eq!(UserSource::parse(s.as_str()), Some(s));
        }
        assert_eq!(UserSource::parse("oidc"), Some(UserSource::Oidc));
        assert_eq!(UserSource::parse("saml"), None);
    }

    #[test]
    fn external_classification() {
        assert!(UserSource::Oidc.is_external());
        assert!(UserSource::Ldap.is_external());
        assert!(!UserSource::Local.is_external());
    }
}
