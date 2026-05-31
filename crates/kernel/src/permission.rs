//! 权限。对应 Java 版散落在切面 + `PermissionConstants` + `@RequiresPermissions` 的隐式逻辑。
//!
//! Java 版的权限判断埋在 AOP 拦截器里,不开 Spring 容器根本测不了。
//! 这里把"一个权限串怎么解析""一组权限是否覆盖某个目标"抽成纯函数,
//! 让权限规则变成**显式、可回归**的测试用例。
//!
//! 权限串形如 `SYSTEM_USER:READ` 或 `PROJECT_API:READ+ADD+DELETE`(冒号左边资源,右边动作集合)。

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

/// 一个主体(用户/角色聚合后)拥有的权限集合。
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

    /// 是否允许对某资源执行某动作。资源不区分大小写,动作要精确命中。
    pub fn allows(&self, resource: &str, action: &str) -> bool {
        let resource = resource.to_uppercase();
        let action = action.to_uppercase();
        self.granted
            .iter()
            .any(|p| p.resource == resource && p.actions.contains(&action))
    }

    /// 序列化回原始权限串(`RESOURCE:A+B`),用于持久化会话权限快照。
    /// 与 [`from_raw`](Self::from_raw) 往返一致(动作按字典序,因底层是 BTreeSet)。
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
        // 往返后语义不变
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
        assert!(set.allows("system_user", "add")); // 大小写无关
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
