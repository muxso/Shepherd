use async_trait::async_trait;

use crate::domain::{NewTemplate, Template};
use crate::ports::RepoError;

#[async_trait]
pub trait TemplateRepository: Send + Sync {
    async fn insert(&self, template: &NewTemplate) -> Result<Template, RepoError>;

    async fn find_by_id(&self, id: &str) -> Result<Option<Template>, RepoError>;

    /// (project_id, kind, name) is unique; used for duplicate-name checks before create/rename.
    async fn find_by_name(
        &self,
        project_id: &str,
        kind: &str,
        name: &str,
    ) -> Result<Option<Template>, RepoError>;

    /// Overwrites name/config and refreshes updated_at; returns `None` if the id does not exist.
    async fn update(&self, template: &Template) -> Result<Option<Template>, RepoError>;

    /// Deletes a template; returns whether a row was actually removed.
    async fn delete(&self, id: &str) -> Result<bool, RepoError>;

    /// Lists project templates ordered by creation time; when `kind` is given, only that kind.
    async fn list(&self, project_id: &str, kind: Option<&str>) -> Result<Vec<Template>, RepoError>;
}
