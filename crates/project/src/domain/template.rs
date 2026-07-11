//! 项目内模板:kind 区分使用场景(requirement / functional-case / bug ...),
//! config 对后端不透明,只要求是 JSON 对象,结构由各前端场景自定义。

use serde_json::Value;
use thiserror::Error;

pub const MAX_KIND_LEN: usize = 32;
pub const MAX_NAME_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TemplateError {
    #[error("project id must not be empty")]
    EmptyProject,
    #[error("template kind must not be empty")]
    EmptyKind,
    #[error("template kind must be at most 32 chars")]
    KindTooLong,
    #[error("template name must not be empty")]
    EmptyName,
    #[error("template name must be at most 64 chars")]
    NameTooLong,
    #[error("template config must be a JSON object")]
    ConfigNotObject,
}

/// kind 归一:trim + 小写,空或超长报错。写入与查询过滤共用同一套规则。
pub fn normalize_kind(kind: &str) -> Result<String, TemplateError> {
    let kind = kind.trim().to_lowercase();
    if kind.is_empty() {
        return Err(TemplateError::EmptyKind);
    }
    if kind.chars().count() > MAX_KIND_LEN {
        return Err(TemplateError::KindTooLong);
    }
    Ok(kind)
}

/// name 归一:trim,空或超长报错(大小写保留,唯一性按原样比较)。
pub fn normalize_name(name: &str) -> Result<String, TemplateError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(TemplateError::EmptyName);
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(TemplateError::NameTooLong);
    }
    Ok(name.to_string())
}

/// config 只做最小约束:必须是 JSON 对象,内部结构后端不解释。
pub fn validate_config(config: &Value) -> Result<(), TemplateError> {
    if config.is_object() {
        Ok(())
    } else {
        Err(TemplateError::ConfigNotObject)
    }
}

/// 入参校验后的模板(尚未落库)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTemplate {
    pub project_id: String,
    pub kind: String,
    pub name: String,
    pub config: Value,
    pub created_by: String,
}

impl NewTemplate {
    pub fn new(
        project_id: &str,
        kind: &str,
        name: &str,
        config: Value,
        created_by: &str,
    ) -> Result<Self, TemplateError> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err(TemplateError::EmptyProject);
        }
        validate_config(&config)?;
        Ok(Self {
            project_id: project_id.to_string(),
            kind: normalize_kind(kind)?,
            name: normalize_name(name)?,
            config,
            created_by: created_by.trim().to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub id: String,
    pub project_id: String,
    pub kind: String,
    pub name: String,
    pub config: Value,
    pub created_by: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kind_is_trimmed_and_lowercased() {
        assert_eq!(normalize_kind("  Requirement ").expect("ok"), "requirement");
        assert_eq!(normalize_kind("Functional-Case").expect("ok"), "functional-case");
    }

    #[test]
    fn kind_rejects_blank_and_too_long() {
        assert_eq!(normalize_kind("   "), Err(TemplateError::EmptyKind));
        assert_eq!(normalize_kind(&"k".repeat(33)), Err(TemplateError::KindTooLong));
        assert!(normalize_kind(&"k".repeat(32)).is_ok());
    }

    #[test]
    fn name_is_trimmed_and_length_checked() {
        assert_eq!(normalize_name("  默认模板 ").expect("ok"), "默认模板");
        assert_eq!(normalize_name("  "), Err(TemplateError::EmptyName));
        assert_eq!(normalize_name(&"n".repeat(65)), Err(TemplateError::NameTooLong));
        assert!(normalize_name(&"n".repeat(64)).is_ok());
    }

    #[test]
    fn config_must_be_object() {
        assert!(validate_config(&json!({})).is_ok());
        assert!(validate_config(&json!({"fields": [1, 2]})).is_ok());
        assert_eq!(validate_config(&json!([1])), Err(TemplateError::ConfigNotObject));
        assert_eq!(validate_config(&json!("x")), Err(TemplateError::ConfigNotObject));
        assert_eq!(validate_config(&Value::Null), Err(TemplateError::ConfigNotObject));
    }

    #[test]
    fn new_template_normalizes_and_validates() {
        let t = NewTemplate::new(" p1 ", " Requirement ", " 默认 ", json!({}), " admin ")
            .expect("valid");
        assert_eq!(t.project_id, "p1");
        assert_eq!(t.kind, "requirement");
        assert_eq!(t.name, "默认");
        assert_eq!(t.created_by, "admin");
    }

    #[test]
    fn new_template_rejects_bad_input() {
        assert_eq!(
            NewTemplate::new(" ", "requirement", "n", json!({}), ""),
            Err(TemplateError::EmptyProject)
        );
        assert_eq!(NewTemplate::new("p", " ", "n", json!({}), ""), Err(TemplateError::EmptyKind));
        assert_eq!(
            NewTemplate::new("p", "requirement", " ", json!({}), ""),
            Err(TemplateError::EmptyName)
        );
        assert_eq!(
            NewTemplate::new("p", "requirement", "n", json!(1), ""),
            Err(TemplateError::ConfigNotObject)
        );
    }
}
