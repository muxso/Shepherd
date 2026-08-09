use std::sync::Arc;

use serde_json::Value;
use thiserror::Error;

use crate::domain::{
    normalize_kind, normalize_name, validate_config, NewTemplate, Template, TemplateError,
};
use crate::ports::{RepoError, TemplateRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TemplateCmdError {
    #[error("validation: {0}")]
    Validation(#[from] TemplateError),
    #[error("template name already exists")]
    NameExists,
    #[error("template not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] RepoError),
}

#[derive(Clone)]
pub struct TemplateService {
    repo: Arc<dyn TemplateRepository>,
}

impl TemplateService {
    pub fn new(repo: Arc<dyn TemplateRepository>) -> Self {
        Self { repo }
    }

    /// Creates a template; returns `NameExists` if the same (project, kind, name) already exists.
    pub async fn create(
        &self,
        project_id: &str,
        kind: &str,
        name: &str,
        config: Value,
        created_by: &str,
    ) -> Result<Template, TemplateCmdError> {
        let new = NewTemplate::new(project_id, kind, name, config, created_by)?;
        if self.repo.find_by_name(&new.project_id, &new.kind, &new.name).await?.is_some() {
            return Err(TemplateCmdError::NameExists);
        }
        Ok(self.repo.insert(&new).await?)
    }

    /// Renames and/or replaces the config; `None` keeps the current value. Renaming onto an
    /// existing name within the same (project, kind) returns `NameExists`.
    pub async fn update(
        &self,
        id: &str,
        name: Option<&str>,
        config: Option<Value>,
    ) -> Result<Template, TemplateCmdError> {
        let mut cur = self.repo.find_by_id(id).await?.ok_or(TemplateCmdError::NotFound)?;
        if let Some(name) = name {
            let name = normalize_name(name)?;
            if name != cur.name {
                if self.repo.find_by_name(&cur.project_id, &cur.kind, &name).await?.is_some() {
                    return Err(TemplateCmdError::NameExists);
                }
                cur.name = name;
            }
        }
        if let Some(config) = config {
            validate_config(&config)?;
            cur.config = config;
        }
        self.repo.update(&cur).await?.ok_or(TemplateCmdError::NotFound)
    }

    /// Deletes a template; returns `NotFound` if it does not exist.
    pub async fn delete(&self, id: &str) -> Result<(), TemplateCmdError> {
        if self.repo.delete(id).await? {
            Ok(())
        } else {
            Err(TemplateCmdError::NotFound)
        }
    }

    /// Lists templates in a project; when `kind` is given, filters by the normalized kind.
    pub async fn list(
        &self,
        project_id: &str,
        kind: Option<&str>,
    ) -> Result<Vec<Template>, TemplateCmdError> {
        let kind = kind.map(normalize_kind).transpose()?;
        Ok(self.repo.list(project_id, kind.as_deref()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryTemplateRepository;
    use serde_json::json;

    fn svc() -> TemplateService {
        TemplateService::new(Arc::new(InMemoryTemplateRepository::new()))
    }

    #[tokio::test]
    async fn create_then_list() {
        let s = svc();
        let t = s
            .create("p1", "Requirement", "default", json!({"fields": []}), "admin")
            .await
            .expect("create");
        assert_eq!(t.kind, "requirement");
        assert_eq!(t.config, json!({"fields": []}));
        assert!(t.created_at_ms > 0);
        assert_eq!(t.created_at_ms, t.updated_at_ms);
        let list = s.list("p1", None).await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "default");
    }

    #[tokio::test]
    async fn duplicate_name_same_project_kind_conflicts() {
        let s = svc();
        s.create("p1", "requirement", "default", json!({}), "admin").await.expect("create");
        // kind/name are compared after normalization: case and whitespace cannot bypass uniqueness.
        assert_eq!(
            s.create("p1", " Requirement ", " default ", json!({}), "admin").await,
            Err(TemplateCmdError::NameExists)
        );
        // A different kind or a different project does not conflict.
        assert!(s.create("p1", "bug", "default", json!({}), "admin").await.is_ok());
        assert!(s.create("p2", "requirement", "default", json!({}), "admin").await.is_ok());
    }

    #[tokio::test]
    async fn update_name_and_config_independently() {
        let s = svc();
        let t = s.create("p1", "requirement", "default", json!({}), "admin").await.expect("create");
        let t2 = s.update(&t.id, Some("rename"), None).await.expect("rename");
        assert_eq!(t2.name, "rename");
        assert_eq!(t2.config, json!({})); // config untouched
        assert!(t2.updated_at_ms > t2.created_at_ms);
        let t3 = s.update(&t.id, None, Some(json!({"a": 1}))).await.expect("reconfig");
        assert_eq!(t3.name, "rename"); // name untouched
        assert_eq!(t3.config, json!({"a": 1}));
    }

    #[tokio::test]
    async fn update_rename_to_existing_conflicts() {
        let s = svc();
        s.create("p1", "requirement", "A", json!({}), "admin").await.expect("create");
        let b = s.create("p1", "requirement", "B", json!({}), "admin").await.expect("create");
        assert_eq!(s.update(&b.id, Some("A"), None).await, Err(TemplateCmdError::NameExists));
        // Renaming to the template's own current name is not a conflict.
        assert!(s.update(&b.id, Some(" B "), None).await.is_ok());
    }

    #[tokio::test]
    async fn update_unknown_id_not_found() {
        let s = svc();
        assert_eq!(s.update("nope", Some("x"), None).await, Err(TemplateCmdError::NotFound));
    }

    #[tokio::test]
    async fn update_rejects_non_object_config() {
        let s = svc();
        let t = s.create("p1", "requirement", "A", json!({}), "admin").await.expect("create");
        assert_eq!(
            s.update(&t.id, None, Some(json!([1]))).await,
            Err(TemplateCmdError::Validation(TemplateError::ConfigNotObject))
        );
    }

    #[tokio::test]
    async fn delete_then_not_found() {
        let s = svc();
        let t = s.create("p1", "requirement", "A", json!({}), "admin").await.expect("create");
        s.delete(&t.id).await.expect("delete");
        assert_eq!(s.list("p1", None).await.expect("list").len(), 0);
        assert_eq!(s.delete(&t.id).await, Err(TemplateCmdError::NotFound));
    }

    #[tokio::test]
    async fn list_filters_by_kind() {
        let s = svc();
        s.create("p1", "requirement", "R1", json!({}), "admin").await.expect("create");
        s.create("p1", "requirement", "R2", json!({}), "admin").await.expect("create");
        s.create("p1", "bug", "B1", json!({}), "admin").await.expect("create");
        s.create("p2", "requirement", "R1", json!({}), "admin").await.expect("create");
        assert_eq!(s.list("p1", None).await.expect("list").len(), 3);
        let reqs = s.list("p1", Some(" Requirement ")).await.expect("list");
        assert_eq!(reqs.len(), 2);
        assert!(reqs.iter().all(|t| t.kind == "requirement"));
        assert_eq!(s.list("p1", Some("bug")).await.expect("list").len(), 1);
    }

    #[tokio::test]
    async fn invalid_kind_is_validation_error() {
        let s = svc();
        assert!(matches!(
            s.create("p1", "  ", "A", json!({}), "admin").await,
            Err(TemplateCmdError::Validation(TemplateError::EmptyKind))
        ));
        assert!(matches!(
            s.list("p1", Some("  ")).await,
            Err(TemplateCmdError::Validation(TemplateError::EmptyKind))
        ));
    }
}
