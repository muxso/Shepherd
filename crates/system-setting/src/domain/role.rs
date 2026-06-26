use kernel::permission::PermissionSet;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleScope {
    System,
    Organization,
    Project,
}

impl RoleScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            RoleScope::System => "SYSTEM",
            RoleScope::Organization => "ORGANIZATION",
            RoleScope::Project => "PROJECT",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "SYSTEM" => Some(RoleScope::System),
            "ORGANIZATION" => Some(RoleScope::Organization),
            "PROJECT" => Some(RoleScope::Project),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RoleError {
    #[error("role name must not be empty")]
    EmptyName,
    #[error("invalid permission string: {0}")]
    BadPermission(String),
}

fn validate_perms(perms: &[String]) -> Result<(), RoleError> {
    for p in perms {
        PermissionSet::from_raw([p]).map_err(|_| RoleError::BadPermission(p.clone()))?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRole {
    pub name: String,
    pub scope: RoleScope,
    pub permissions: Vec<String>,
}

impl NewRole {
    pub fn new(name: &str, scope: RoleScope, permissions: Vec<String>) -> Result<Self, RoleError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RoleError::EmptyName);
        }
        validate_perms(&permissions)?;
        Ok(Self { name: name.to_string(), scope, permissions })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub scope: RoleScope,
    pub permissions: Vec<String>,
}

impl Role {
    pub fn rename(&mut self, name: &str) -> Result<(), RoleError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RoleError::EmptyName);
        }
        self.name = name.to_string();
        Ok(())
    }

    pub fn set_permissions(&mut self, permissions: Vec<String>) -> Result<(), RoleError> {
        validate_perms(&permissions)?;
        self.permissions = permissions;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_name() {
        assert_eq!(NewRole::new("  ", RoleScope::System, vec![]), Err(RoleError::EmptyName));
    }

    #[test]
    fn rejects_bad_permission() {
        let err = NewRole::new("r", RoleScope::System, vec!["NOPE".into()]).unwrap_err();
        assert_eq!(err, RoleError::BadPermission("NOPE".into()));
    }

    #[test]
    fn accepts_valid() {
        let r = NewRole::new("admin", RoleScope::System, vec!["SYSTEM_USER:READ+ADD".into()])
            .expect("valid");
        assert_eq!(r.scope, RoleScope::System);
    }

    #[test]
    fn scope_roundtrip() {
        assert_eq!(RoleScope::parse("organization"), Some(RoleScope::Organization));
        assert_eq!(RoleScope::parse("x"), None);
    }
}
