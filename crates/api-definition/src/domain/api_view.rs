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
}
