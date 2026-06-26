use crate::domain::error::ApiDefinitionError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiModule {
    pub project_id: String,
    pub parent_id: Option<String>,
    pub name: String,
}

impl NewApiModule {
    pub fn new(
        project_id: &str,
        parent_id: Option<&str>,
        name: &str,
    ) -> Result<Self, ApiDefinitionError> {
        if project_id.trim().is_empty() {
            return Err(ApiDefinitionError::EmptyProject);
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiDefinitionError::EmptyName);
        }
        Ok(Self {
            project_id: project_id.to_string(),
            parent_id: parent_id.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
            name: name.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiModule {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_module_trims_and_normalizes_parent() {
        let m = NewApiModule::new("p1", Some(""), " 登录域 ").expect("ok");
        assert_eq!(m.name, "登录域");
        assert_eq!(m.parent_id, None);
        let m2 = NewApiModule::new("p1", Some("mod-1"), "子模块").expect("ok");
        assert_eq!(m2.parent_id.as_deref(), Some("mod-1"));
    }

    #[test]
    fn new_module_rejects_blank() {
        assert_eq!(NewApiModule::new("  ", None, "x"), Err(ApiDefinitionError::EmptyProject));
        assert_eq!(NewApiModule::new("p1", None, "  "), Err(ApiDefinitionError::EmptyName));
    }
}
