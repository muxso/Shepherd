//! 功能用例领域模型(零 IO)。自定义字段以字符串映射承载,适配各团队模板差异。

use std::collections::BTreeMap;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CaseError {
    #[error("project id must not be empty")]
    EmptyProject,
    #[error("case name must not be empty")]
    EmptyName,
}

/// 一条功能用例。`custom_fields` 为 `{字段名:值}`,落 jsonb。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionalCase {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub module: String,
    pub priority: String,
    pub status: String,
    pub custom_fields: BTreeMap<String, String>,
}

/// 待创建用例(构造即校验:项目/名称非空;module/priority/status 缺省给默认)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFunctionalCase {
    pub project_id: String,
    pub name: String,
    pub module: String,
    pub priority: String,
    pub status: String,
    pub custom_fields: BTreeMap<String, String>,
}

impl NewFunctionalCase {
    pub fn new(
        project_id: &str,
        name: &str,
        module: &str,
        priority: &str,
        status: &str,
        custom_fields: BTreeMap<String, String>,
    ) -> Result<Self, CaseError> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(CaseError::EmptyProject);
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(CaseError::EmptyName);
        }
        let with_default = |v: &str, d: &str| {
            let v = v.trim();
            if v.is_empty() { d.to_string() } else { v.to_string() }
        };
        Ok(Self {
            project_id: project_id.to_string(),
            name: name.to_string(),
            module: module.trim().to_string(),
            priority: with_default(priority, "P2"),
            status: with_default(status, "PREPARED"),
            custom_fields,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> BTreeMap<String, String> {
        BTreeMap::from([("owner".into(), "alice".into())])
    }

    #[test]
    fn defaults_priority_and_status() {
        let c = NewFunctionalCase::new("p1", "登录成功", "", "", "", fields()).expect("ok");
        assert_eq!(c.priority, "P2");
        assert_eq!(c.status, "PREPARED");
        assert_eq!(c.custom_fields["owner"], "alice");
    }

    #[test]
    fn keeps_given_values_and_trims() {
        let c = NewFunctionalCase::new(" p1 ", " 用例 ", "登录", "P0", "REVIEWING", fields())
            .expect("ok");
        assert_eq!(c.project_id, "p1");
        assert_eq!(c.name, "用例");
        assert_eq!(c.priority, "P0");
        assert_eq!(c.status, "REVIEWING");
    }

    #[test]
    fn rejects_empty_project_and_name() {
        assert_eq!(
            NewFunctionalCase::new(" ", "x", "", "", "", fields()).unwrap_err(),
            CaseError::EmptyProject
        );
        assert_eq!(
            NewFunctionalCase::new("p1", "  ", "", "", "", fields()).unwrap_err(),
            CaseError::EmptyName
        );
    }
}
