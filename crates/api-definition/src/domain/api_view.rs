use crate::domain::error::ApiDefinitionError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiView {
    pub project_id: String,
    pub user_id: String,
    pub name: String,
    pub config: serde_json::Value,
    pub shared: bool,
}

impl NewApiView {
    pub fn new(
        project_id: &str,
        user_id: &str,
        name: &str,
        config: serde_json::Value,
        shared: bool,
    ) -> Result<Self, ApiDefinitionError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiDefinitionError::EmptyName);
        }
        let config = if config.is_object() { config } else { serde_json::json!({}) };
        Ok(Self {
            project_id: project_id.to_string(),
            user_id: user_id.to_string(),
            name: name.to_string(),
            config,
            shared,
        })
    }
}

/// Partial update of a view: `None` fields keep their current values.
///
/// name validation matches [`NewApiView::new`] (non-empty after trim); config, if given, must be a JSON object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApiViewPatch {
    pub name: Option<String>,
    pub config: Option<serde_json::Value>,
    pub shared: Option<bool>,
}

impl ApiViewPatch {
    pub fn new(
        name: Option<&str>,
        config: Option<serde_json::Value>,
        shared: Option<bool>,
    ) -> Result<Self, ApiDefinitionError> {
        let name = match name {
            Some(n) => {
                let n = n.trim();
                if n.is_empty() {
                    return Err(ApiDefinitionError::EmptyName);
                }
                Some(n.to_string())
            }
            None => None,
        };
        if let Some(c) = &config {
            if !c.is_object() {
                return Err(ApiDefinitionError::BadConfig);
            }
        }
        Ok(Self { name, config, shared })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiView {
    pub id: String,
    pub project_id: String,
    pub user_id: String,
    pub name: String,
    pub config: serde_json::Value,
    pub shared: bool,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_view_ok_and_defaults_config() {
        let v =
            NewApiView::new("p1", "u1", " 我的视图 ", serde_json::json!("bad"), true).expect("ok");
        assert_eq!(v.name, "我的视图");
        assert_eq!(v.config, serde_json::json!({}));
        assert!(v.shared);
    }

    #[test]
    fn new_view_rejects_blank_name() {
        let err = NewApiView::new("p1", "u1", "  ", serde_json::json!({}), true).unwrap_err();
        assert_eq!(err, ApiDefinitionError::EmptyName);
    }

    #[test]
    fn patch_trims_name_and_keeps_absent_fields() {
        let p = ApiViewPatch::new(Some(" 新名字 "), None, Some(false)).expect("ok");
        assert_eq!(p.name.as_deref(), Some("新名字"));
        assert_eq!(p.config, None);
        assert_eq!(p.shared, Some(false));
        let empty = ApiViewPatch::new(None, None, None).expect("ok");
        assert_eq!(empty, ApiViewPatch::default());
    }

    #[test]
    fn patch_rejects_blank_name() {
        let err = ApiViewPatch::new(Some("  "), None, None).unwrap_err();
        assert_eq!(err, ApiDefinitionError::EmptyName);
    }

    #[test]
    fn patch_rejects_non_object_config() {
        let err = ApiViewPatch::new(None, Some(serde_json::json!([1])), None).unwrap_err();
        assert_eq!(err, ApiDefinitionError::BadConfig);
    }
}
