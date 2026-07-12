use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    pub resource: String,
    pub actions: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionParseError {
    Empty,
    MissingResource,
    MissingAction,
}

impl Permission {
    pub fn parse(raw: &str) -> Result<Self, PermissionParseError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(PermissionParseError::Empty);
        }
        let (resource, actions) = raw.split_once(':').ok_or(PermissionParseError::MissingAction)?;
        let resource = resource.trim();
        if resource.is_empty() {
            return Err(PermissionParseError::MissingResource);
        }
        let actions: BTreeSet<String> = actions
            .split('+')
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .map(|a| a.to_uppercase())
            .collect();
        if actions.is_empty() {
            return Err(PermissionParseError::MissingAction);
        }
        Ok(Self { resource: resource.to_uppercase(), actions })
    }
}

/// Permission set held by a principal (aggregated over user/roles).
#[derive(Debug, Default, Clone)]
pub struct PermissionSet {
    granted: Vec<Permission>,
}

impl PermissionSet {
    pub fn from_raw<I, S>(raws: I) -> Result<Self, PermissionParseError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut granted = Vec::new();
        for r in raws {
            granted.push(Permission::parse(r.as_ref())?);
        }
        Ok(Self { granted })
    }

    /// Whether the given action is allowed on the resource. Resource match is
    /// case-insensitive; the action must match exactly.
    pub fn allows(&self, resource: &str, action: &str) -> bool {
        let resource = resource.to_uppercase();
        let action = action.to_uppercase();
        self.granted.iter().any(|p| p.resource == resource && p.actions.contains(&action))
    }

    /// Serialize back to raw permission strings (`RESOURCE:A+B`), used to persist
    /// session permission snapshots. Round-trips with [`from_raw`](Self::from_raw)
    /// (actions come out in lexicographic order since the backing store is a BTreeSet).
    pub fn to_raw(&self) -> Vec<String> {
        self.granted
            .iter()
            .map(|p| {
                let actions: Vec<&str> = p.actions.iter().map(String::as_str).collect();
                format!("{}:{}", p.resource, actions.join("+"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_action() {
        let p = Permission::parse("SYSTEM_USER:READ").expect("valid");
        assert_eq!(p.resource, "SYSTEM_USER");
        assert!(p.actions.contains("READ"));
    }

    #[test]
    fn to_raw_roundtrips_through_from_raw() {
        let set = PermissionSet::from_raw(["SYSTEM_USER:READ+ADD", "PROJECT:READ"]).expect("valid");
        let raw = set.to_raw();
        let back = PermissionSet::from_raw(&raw).expect("valid");
        assert!(back.allows("SYSTEM_USER", "ADD"));
        assert!(back.allows("PROJECT", "READ"));
        assert!(!back.allows("PROJECT", "DELETE"));
    }

    #[test]
    fn parses_multi_action_set() {
        let p = Permission::parse("PROJECT_API:READ+ADD+DELETE").expect("valid");
        assert_eq!(p.actions.len(), 3);
        assert!(p.actions.contains("ADD"));
    }

    #[test]
    fn parse_is_case_insensitive() {
        let p = Permission::parse("system_user:read").expect("valid");
        assert_eq!(p.resource, "SYSTEM_USER");
        assert!(p.actions.contains("READ"));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Permission::parse("   "), Err(PermissionParseError::Empty));
    }

    #[test]
    fn rejects_missing_action() {
        assert_eq!(Permission::parse("SYSTEM_USER"), Err(PermissionParseError::MissingAction));
        assert_eq!(Permission::parse("SYSTEM_USER:"), Err(PermissionParseError::MissingAction));
    }

    #[test]
    fn rejects_missing_resource() {
        assert_eq!(Permission::parse(":READ"), Err(PermissionParseError::MissingResource));
    }

    #[test]
    fn set_allows_granted_action() {
        let set = PermissionSet::from_raw(["SYSTEM_USER:READ+ADD"]).expect("valid");
        assert!(set.allows("SYSTEM_USER", "READ"));
        assert!(set.allows("system_user", "add")); // case-insensitive
    }

    #[test]
    fn set_denies_ungranted_action() {
        let set = PermissionSet::from_raw(["SYSTEM_USER:READ"]).expect("valid");
        assert!(!set.allows("SYSTEM_USER", "DELETE"));
    }

    #[test]
    fn set_denies_unknown_resource() {
        let set = PermissionSet::from_raw(["SYSTEM_USER:READ"]).expect("valid");
        assert!(!set.allows("PROJECT_API", "READ"));
    }
}
