use async_trait::async_trait;

use crate::domain::{NewTemplate, Template};
use crate::ports::RepoError;

#[async_trait]
pub trait TemplateRepository: Send + Sync {
    async fn insert(&self, template: &NewTemplate) -> Result<Template, RepoError>;

    async fn find_by_id(&self, id: &str) -> Result<Option<Template>, RepoError>;

    /// (project_id, kind, name) 唯一,用于创建/改名前的重名检查。
    async fn find_by_name(
        &self,
        project_id: &str,
        kind: &str,
        name: &str,
    ) -> Result<Option<Template>, RepoError>;

    /// 盖写 name/config 并刷新 updated_at;id 不存在返回 `None`。
    async fn update(&self, template: &Template) -> Result<Option<Template>, RepoError>;

    /// 删除模板;返回是否确有一行被删。
    async fn delete(&self, id: &str) -> Result<bool, RepoError>;

    /// 按创建时间列出项目内模板;kind 给定时只列该场景。
    async fn list(&self, project_id: &str, kind: Option<&str>) -> Result<Vec<Template>, RepoError>;
}
