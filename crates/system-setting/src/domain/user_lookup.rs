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
